//! Peace (docs/WAR.md, section 10): the war score, the terms it buys, who
//! offers and who accepts (the `PeaceTable` hook for phase 6's
//! diplomacy), and carrying a deal out.

use super::{Faction, Goal, Wars};
use crate::econ::{EconData, Economy};

/// What a peace deal says.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct PeaceTerms {
    /// The side that gets its way, or none (white peace).
    pub winner: Option<usize>,
    pub provinces: Vec<usize>,
    /// The loser pays this share of its GDP a year, for `years`.
    pub reparations: f64,
    pub years: f64,
    pub vassal: bool,
    pub ideology: bool,
    /// Prisoners go home (false: the winner keeps them, a bargaining chip).
    pub prisoners: bool,
    /// The war score it was made at (side 0's view).
    pub score: f64,
}

impl PeaceTerms {
    pub fn white(score: f64) -> PeaceTerms {
        PeaceTerms { winner: None, provinces: Vec::new(), reparations: 0.0, years: 0.0, vassal: false, ideology: false, prisoners: true, score }
    }

    pub fn words(&self, w: &Wars) -> String {
        let Some(_) = self.winner else { return "white peace".to_string() };
        let mut parts = Vec::new();
        if !self.provinces.is_empty() {
            parts.push(format!("{} handed over", self.provinces.iter().map(|&q| w.geo.names[q].clone()).collect::<Vec<_>>().join(", ")));
        }
        if self.reparations > 0.0 {
            parts.push(format!("reparations of {:.0}% of GDP a year for {:.0} years", self.reparations * 100.0, self.years));
        }
        if self.vassal {
            parts.push("the loser becomes a vassal".to_string());
        }
        if self.ideology {
            parts.push("the loser changes its government".to_string());
        }
        if parts.is_empty() {
            parts.push("the loser backs down".to_string());
        }
        parts.join("; ")
    }
}

/// The hook for diplomacy: who answers a peace offer, and how.
pub trait PeaceTable {
    /// Would `who` accept these terms to end war `war`?
    fn accepts(&self, w: &Wars, e: &Economy, war: usize, who: Faction, terms: &PeaceTerms) -> bool;
}

/// Phase 5's answer: accept when the terms cost less than fighting on.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ByExhaustion;

impl PeaceTable for ByExhaustion {
    fn accepts(&self, w: &Wars, e: &Economy, war: usize, who: Faction, terms: &PeaceTerms) -> bool {
        let wr = &w.wars[war];
        let Some(side) = wr.side_of(who) else { return false };
        let score = score(w, e, war) * if side == 0 { 1.0 } else { -1.0 };
        let loses = terms.winner.is_some_and(|s| s != side);
        if !loses {
            return true;
        }
        let stake = w.data.peace.stake[wr.level.min(4)];
        w.forces[who].exhaustion + (-score).max(0.0) / 100.0 > w.data.peace.accept * stake || wr.pending.is_some_and(|p| p.0 != side)
    }
}

/// Side 0's war score, −100 to 100 (docs/WAR.md, section 10).
pub fn score(w: &Wars, e: &Economy, war: usize) -> f64 {
    let wr = &w.wars[war];
    let held_share = |side: usize, by: usize| -> f64 {
        let mut own = 0usize;
        let mut lost = 0usize;
        for q in 0..w.control.len() {
            let Some(o) = w.owner[q] else { continue };
            if wr.sides[side].contains(&o) {
                own += 1;
                if w.control[q].is_some_and(|c| wr.sides[by].contains(&c)) {
                    lost += 1;
                }
            }
        }
        if own > 0 { lost as f64 / own as f64 } else { 0.0 }
    };
    let _ = e;
    let taken = held_share(1, 0) - held_share(0, 1);
    let (l0, l1) = (&wr.losses[0], &wr.losses[1]);
    let lost0 = l0.killed + l0.captured;
    let lost1 = l1.killed + l1.captured;
    // Losses count against the size of both armies: a tenth of them is the whole 30 points.
    let armies: f64 = wr.sides.iter().flatten().map(|&f| w.strength(f).0).sum::<f64>() + lost0 + lost1;
    let losses = ((lost1 - lost0) / (0.1 * armies).max(1.0)).clamp(-1.0, 1.0);
    let ex = |s: usize| wr.sides[s].iter().map(|&f| w.forces[f].exhaustion).fold(0.0, f64::max);
    (40.0 * taken + 30.0 * losses + 15.0 * (ex(1) - ex(0)).clamp(-1.0, 1.0)).clamp(-100.0, 100.0)
}

/// The terms a war score buys (side 0's score).
pub fn terms(w: &Wars, war: usize, score: f64) -> PeaceTerms {
    let p = &w.data.peace;
    let wr = &w.wars[war];
    if score.abs() < p.white {
        return PeaceTerms::white(score);
    }
    let winner = if score > 0.0 { 0 } else { 1 };
    let s = score.abs();
    let mut t = PeaceTerms::white(score);
    t.winner = Some(winner);
    if s >= p.white {
        t.reparations = p.reparations_share;
        t.years = p.reparations_years;
    }
    if s >= p.reparations {
        // The provinces the winner holds that its goal asked for.
        let asked: Vec<usize> = match (&wr.goal, winner) {
            (Goal::Provinces(ps), 0) => ps.clone(),
            _ => Vec::new(),
        };
        // Never the loser's capital, and never its last province.
        let mut taken: Vec<usize> = asked.into_iter().filter(|&q| w.control[q].is_some_and(|c| wr.sides[winner].contains(&c)) && !w.capital.contains(&q)).collect();
        if let Some(&loser) = wr.sides[1 - winner].first() {
            let left = (0..w.owner.len()).filter(|&q| w.owner[q] == Some(w.nation_of(loser)) && !taken.contains(&q)).count();
            if left == 0 {
                taken.pop();
            }
        }
        t.provinces = taken;
    }
    // No vassal for a winner that is itself a vassal, or already holds
    // `max_vassals`: empires don't snowball (reparations instead).
    let lead = w.nation_of(wr.sides[winner][0]);
    let can_vassal = lead < w.nations && w.overlord_of(lead).is_none() && w.vassals_of(lead).len() < w.data.ai.max_vassals;
    if s >= p.provinces && winner == 0 {
        match wr.goal {
            Goal::Vassal if can_vassal => t.vassal = true,
            Goal::Ideology => t.ideology = true,
            _ => {}
        }
    }
    t
}

/// Carries a deal out and ends the war.
pub fn make_peace(w: &mut Wars, e: &mut Economy, ed: &EconData, war: usize, t: PeaceTerms, how: &str) {
    let wr = w.wars[war].clone();
    let lead = |s: usize| wr.sides[s][0];
    // Provinces change hands (rebels can't own land: a rebel win is handled by the civil war).
    if let Some(win) = t.winner {
        let to = w.nation_of(lead(win));
        for &q in &t.provinces {
            if let Some(from) = w.owner[q] {
                if from != to && !w.is_rebel(lead(win)) {
                    e.transfer_province(ed, q, from, to);
                    w.owner[q] = Some(to);
                    w.lost.retain(|l| !(l.0 == to && l.1 == q));
                    w.lost.push((from, q, w.day));
                    if w.capital[from] == q {
                        w.capital[from] = e.nations[from].capital;
                    }
                }
            }
        }
        let loser = w.nation_of(lead(1 - win));
        // Losing is a humiliation at home.
        if !w.is_rebel(lead(1 - win)) {
            let hit = w.data.peace.lost_war_stability * if t.vassal || t.ideology { 1.5 } else if t.provinces.is_empty() { 0.5 } else { 1.0 };
            e.nations[loser].stability = (e.nations[loser].stability - hit).max(0.0);
            w.forces[loser].lost_wars += 1;
        }
        if t.reparations > 0.0 && !w.is_rebel(lead(win)) {
            w.reparations.push((loser, to, t.reparations, (t.years * ed.days_per_year as f64) as u32));
        }
        if t.vassal {
            e.nations[loser].overlord = Some(to);
            w.overlords[loser] = Some(to);
            // A new vassal's own vassals go free: the empire doesn't nest.
            for v in 0..w.nations {
                if w.overlords[v] == Some(loser) {
                    e.nations[v].overlord = None;
                    w.overlords[v] = None;
                }
            }
        }
        if t.ideology {
            let new = w.ideology[to].clone();
            set_ideology(w, e, ed, loser, &new);
        }
    }
    // A war of independence the overlord didn't win sets the vassals free.
    if wr.goal == Goal::Independence && t.winner != Some(1) {
        for &f in &wr.sides[0] {
            let v = w.nation_of(f);
            if v < w.nations {
                e.nations[v].overlord = None;
                w.overlords[v] = None;
            }
        }
    }
    // Everything else goes back to its owner; armies go home.
    for q in 0..w.control.len() {
        let owner = w.owner[q];
        if w.control[q] != owner && (wr.sides[0].iter().chain(wr.sides[1].iter()).any(|&f| w.control[q] == Some(f) || owner == Some(w.nation_of(f)))) {
            w.set_control(e, q, owner);
        }
    }
    for d in 0..w.divisions.len() {
        let dv = &w.divisions[d];
        if !dv.alive || !(wr.sides[0].contains(&dv.faction) || wr.sides[1].contains(&dv.faction)) {
            continue;
        }
        let f = dv.faction;
        if w.owner[dv.province] != Some(w.nation_of(f)) || dv.moving.is_some() {
            let home = w.capital[w.nation_of(f)];
            let dv = &mut w.divisions[d];
            dv.province = home;
            dv.moving = None;
            dv.battle = None;
            dv.order = super::Order::Auto;
        }
    }
    for b in w.battles.iter_mut() {
        if b.war == war {
            b.over = true;
        }
    }
    // Prisoners go home (unless kept).
    if t.prisoners {
        for &a in wr.sides[0].iter().chain(wr.sides[1].iter()) {
            for &b in wr.sides[0].iter().chain(wr.sides[1].iter()) {
                let q = w.prisoners[a][b];
                if q > 0.0 {
                    w.prisoners[a][b] = 0.0;
                    let home = w.nation_of(b);
                    e.nations[home].population += q;
                }
            }
        }
    }
    let day = w.day;
    let words = t.words(w);
    let names = |s: usize| wr.sides[s].iter().map(|&f| w.names[f].clone()).collect::<Vec<_>>().join(" and ");
    let text = format!("Peace between {} and {} ({how}): {words}", names(0), names(1));
    {
        let x = &mut w.wars[war];
        x.end = Some(day);
        x.pending = None;
        x.result = Some(format!("{how}: {words}"));
        x.peace = Some(t);
    }
    for &a in wr.sides[0].iter() {
        for &b in wr.sides[1].iter() {
            let until = day + (w.data.ai.truce_years * ed.days_per_year as f64) as u32;
            w.forces[a].truces.push((b, until));
            w.forces[b].truces.push((a, until));
        }
    }
    w.refresh_relations();
    w.borders_changed = true;
    let factions = wr.sides[0].iter().chain(wr.sides[1].iter()).copied().collect();
    w.say(factions, text);
}

/// A nation takes a new government: its ideology, and the economic model
/// that goes with it.
pub fn set_ideology(w: &mut Wars, e: &mut Economy, ed: &EconData, nation: usize, ideology: &str) {
    if let Some((Some(model), _)) = ed.wheel_effects.get(&("ideology".to_string(), ideology.to_string())) {
        if let Some(m) = ed.models.iter().position(|x| &x.id == model) {
            e.nations[nation].model = m;
        }
    }
    w.ideology[nation] = ideology.to_string();
    e.nations[nation].stability = 50.0;
}

/// Reparations are paid every day, from the loser's treasury to the winner's.
pub fn pay_reparations(w: &mut Wars, e: &mut Economy) {
    for k in 0..w.reparations.len() {
        let (from, to, share, left) = w.reparations[k];
        if left == 0 {
            continue;
        }
        let pay_w = share * e.nations[from].gdp / 360.0;
        let nf = &mut e.nations[from];
        nf.treasury -= pay_w / nf.exchange;
        nf.budget_month.tribute_out += pay_w / nf.exchange;
        let nt = &mut e.nations[to];
        nt.treasury += pay_w / nt.exchange;
        nt.budget_month.tribute_in += pay_w / nt.exchange;
        w.reparations[k].3 -= 1;
    }
    w.reparations.retain(|r| r.3 > 0);
}
