//! Diplomacy and spies (docs/DIPLOMACY.md): relations and their reasons,
//! reputation, treaties with terms that can be broken, calls to arms,
//! negotiated peace, vassals' loyalty, and secret services.
//!
//! Nothing here draws. `Diplomacy::day` runs once a day after the war's
//! day. The same world and seed always give the same diplomacy.

pub mod alliances;
pub mod data;
pub mod report;
pub mod spies;
pub mod talks;
pub mod treaties;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use duckforge::prelude::Rng;

use crate::ai::Brains;
use crate::econ::{EconData, Economy};
use crate::war::Wars;
pub use data::DiploData;

/// A kind of treaty (docs/DIPLOMACY.md, section 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Trade,
    NonAggression,
    Defensive,
    Alliance,
    Access,
    Guarantee,
    Arms,
    Loan,
    Sanctions,
}

impl Kind {
    pub const ALL: [Kind; 9] = [Kind::Trade, Kind::NonAggression, Kind::Defensive, Kind::Alliance, Kind::Access, Kind::Guarantee, Kind::Arms, Kind::Loan, Kind::Sanctions];
    /// The kinds a player proposes from the Propose page.
    pub const PROPOSE: [Kind; 8] = [Kind::Trade, Kind::NonAggression, Kind::Defensive, Kind::Alliance, Kind::Access, Kind::Guarantee, Kind::Arms, Kind::Loan];

    pub fn key(self) -> &'static str {
        match self {
            Kind::Trade => "trade",
            Kind::NonAggression => "non_aggression",
            Kind::Defensive => "defensive",
            Kind::Alliance => "alliance",
            Kind::Access => "access",
            Kind::Guarantee => "guarantee",
            Kind::Arms => "arms",
            Kind::Loan => "loan",
            Kind::Sanctions => "sanctions",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Trade => "Trade agreement",
            Kind::NonAggression => "Non-aggression pact",
            Kind::Defensive => "Defensive pact",
            Kind::Alliance => "Alliance",
            Kind::Access => "Military access",
            Kind::Guarantee => "Guarantee of independence",
            Kind::Arms => "Arms deal",
            Kind::Loan => "Loan",
            Kind::Sanctions => "Sanctions",
        }
    }

    /// Lasts for years (arms deals are done at once; loans run their own term).
    pub fn lasts(self) -> bool {
        !matches!(self, Kind::Arms | Kind::Loan)
    }

    /// Attacking the other party breaks it.
    pub fn binding(self) -> bool {
        matches!(self, Kind::NonAggression | Kind::Defensive | Kind::Alliance | Kind::Guarantee)
    }
}

/// Where a treaty stands.
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Active,
    /// It ran its course (or a loan was repaid).
    Expired,
    /// Ended by agreement, or because a war between them ended it.
    Ended,
    Broken { by: usize, day: u32, how: String },
}

/// A treaty and its terms.
#[derive(Debug, Clone, PartialEq)]
pub struct Treaty {
    pub id: usize,
    pub kind: Kind,
    /// The parties: `a` proposed it (guarantor, lender, seller), `b` the
    /// other (guaranteed, borrower, buyer).
    pub a: usize,
    pub b: usize,
    /// Sanctions: every member, and the target.
    pub members: Vec<usize>,
    pub target: Option<usize>,
    pub start: u32,
    pub end: u32,
    pub state: State,
    /// Loan: W$ lent and the yearly rate; arms: units and the W$ paid.
    pub amount: f64,
    pub rate: f64,
    /// Loan: W$ still owed; months missed in a row.
    pub owed: f64,
    pub missed: u32,
    /// Arms: the good.
    pub good: Option<usize>,
}

impl Treaty {
    pub fn active(&self) -> bool {
        self.state == State::Active
    }

    pub fn involves(&self, i: usize) -> bool {
        self.a == i || self.b == i || self.members.contains(&i)
    }

    /// Between exactly these two (either way round).
    pub fn between(&self, i: usize, j: usize) -> bool {
        (self.a == i && self.b == j) || (self.a == j && self.b == i)
    }

    pub fn other(&self, i: usize) -> usize {
        if self.a == i { self.b } else { self.a }
    }

    /// The treaty in words.
    pub fn words(&self, names: &[String]) -> String {
        let (a, b) = (&names[self.a], &names[self.b]);
        match self.kind {
            Kind::Guarantee => format!("{a} guarantees {b}'s independence"),
            Kind::Loan => format!("{a} lends {b} {} at {:.1}%", crate::diplo::money(self.amount), self.rate * 100.0),
            Kind::Arms => format!("{a} sells {b} {:.0} {} for {}", self.rate, self.good.map_or("weapons".to_string(), |_| "units".to_string()), crate::diplo::money(self.amount)),
            Kind::Sanctions => format!(
                "{} against {}",
                self.members.iter().map(|&m| names[m].clone()).collect::<Vec<_>>().join(", "),
                self.target.map_or(String::new(), |t| names[t].clone())
            ),
            _ => format!("{a} and {b}"),
        }
    }
}

/// W$ in words (as the economy window writes it).
pub fn money(w: f64) -> String {
    let a = w.abs();
    let sign = if w < 0.0 { "-" } else { "" };
    if a >= 1e12 {
        format!("{sign}${:.2}T", a / 1e12)
    } else if a >= 1e9 {
        format!("{sign}${:.1}B", a / 1e9)
    } else {
        format!("{sign}${:.0}M", a / 1e6)
    }
}

/// Something a nation remembers about another (docs/DIPLOMACY.md, section 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    InvadedUs,
    FoughtUs,
    SideBySide,
    MadePeace,
    BrokeTreatyWithUs,
    BrokeTreatyWithAlly,
    RefusedCall,
    CameToAid,
    CaughtSpy,
    EmbargoedUs,
    LiftedEmbargo,
    ArmedEnemy,
    SoldArms,
    LentMoney,
    Defaulted,
    Protected,
    Neglected,
}

impl EventKind {
    pub fn weight(self, ev: &data::Events) -> f64 {
        match self {
            EventKind::InvadedUs => ev.invaded_us,
            EventKind::FoughtUs => ev.fought_us,
            EventKind::SideBySide => ev.side_by_side,
            EventKind::MadePeace => ev.made_peace,
            EventKind::BrokeTreatyWithUs => ev.broke_treaty_with_us,
            EventKind::BrokeTreatyWithAlly => ev.broke_treaty_with_ally,
            EventKind::RefusedCall => ev.refused_call,
            EventKind::CameToAid => ev.came_to_aid,
            EventKind::CaughtSpy => ev.caught_spy,
            EventKind::EmbargoedUs => ev.embargoed_us,
            EventKind::LiftedEmbargo => ev.lifted_embargo,
            EventKind::ArmedEnemy => ev.armed_enemy,
            EventKind::SoldArms => ev.sold_arms,
            EventKind::LentMoney => ev.lent_money,
            EventKind::Defaulted => ev.defaulted,
            EventKind::Protected => ev.protected,
            EventKind::Neglected => ev.neglected,
        }
    }

    pub fn words(self) -> &'static str {
        match self {
            EventKind::InvadedUs => "they invaded us",
            EventKind::FoughtUs => "we fought each other",
            EventKind::SideBySide => "we fought side by side",
            EventKind::MadePeace => "they made peace with us",
            EventKind::BrokeTreatyWithUs => "they broke a treaty with us",
            EventKind::BrokeTreatyWithAlly => "they broke a treaty with our ally",
            EventKind::RefusedCall => "they refused our call to arms",
            EventKind::CameToAid => "they came to our aid",
            EventKind::CaughtSpy => "we caught their spy",
            EventKind::EmbargoedUs => "they embargoed us",
            EventKind::LiftedEmbargo => "they lifted their embargo",
            EventKind::ArmedEnemy => "they armed our enemy",
            EventKind::SoldArms => "they sold us arms",
            EventKind::LentMoney => "they lent us money",
            EventKind::Defaulted => "they defaulted on our loan",
            EventKind::Protected => "they protected us",
            EventKind::Neglected => "they left us to our fate",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Event {
    pub day: u32,
    pub kind: EventKind,
}

/// A deal offered (to the player, in the inbox; or the terms of any proposal).
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub from: usize,
    pub to: usize,
    pub kind: Kind,
    pub day: u32,
    /// Arms: the good and units (the buyer is `to` when the seller offers);
    /// loan: W$. Sanctions: the target.
    pub good: Option<usize>,
    pub amount: f64,
    pub target: Option<usize>,
}

/// Something waiting for the player's answer.
#[derive(Debug, Clone, PartialEq)]
pub enum Inbox {
    Proposal(Proposal),
    /// An ally calls the player to war: (the ally, the war, the side, the treaty kind, the deadline).
    Call { ally: usize, war: usize, side: usize, kind: Kind, deadline: u32 },
}

/// An answer to a proposal, with its reason (the log).
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub day: u32,
    pub from: usize,
    pub to: usize,
    pub kind: Kind,
    pub yes: bool,
    pub reason: String,
}

/// News from diplomacy (for the toasts and the report).
#[derive(Debug, Clone, PartialEq)]
pub struct News {
    pub day: u32,
    pub nations: Vec<usize>,
    pub text: String,
}

/// The whole diplomatic world.
#[derive(Debug, Clone)]
pub struct Diplomacy {
    pub data: DiploData,
    pub n: usize,
    pub names: Vec<String>,
    pub ideology: Vec<String>,
    pub leader: Vec<String>,
    pub traits: Vec<data::Trait>,
    /// How wrong each nation's spies are without intel (its secret service).
    pub intel_error: Vec<f64>,
    pub treaties: Vec<Treaty>,
    /// The active treaties' ids (kept in step by `reindex`).
    pub active: Vec<usize>,
    /// The last refusal of each (from, to, kind): no asking again for a year.
    pub refusals: std::collections::HashMap<(usize, usize, Kind), u32>,
    /// Every broken treaty: (who broke it, the day).
    pub broken: Vec<(usize, u32)>,
    /// Each nation's soldiers and tanks, counted once a day (the deals and
    /// calls weigh them thousands of times).
    pub armies: Vec<(f64, f64)>,
    /// `memory[i][j]`: what `i` remembers of `j`.
    pub memory: Vec<Vec<Vec<Event>>>,
    /// `relations[i][j]`: `i`'s view of `j`, −100 to 100, and why.
    pub relations: Vec<Vec<f64>>,
    pub reasons: Vec<Vec<Vec<(String, f64)>>>,
    pub reputation: Vec<f64>,
    /// Each vassal's loyalty to its overlord (0-100).
    pub loyalty: Vec<f64>,
    pub inbox: Vec<Inbox>,
    pub answers: Vec<Answer>,
    pub talks: talks::Talks,
    pub spies: spies::Spies,
    pub news: Vec<News>,
    pub stats: BTreeMap<String, f64>,
    pub player: Option<usize>,
    /// NPCs decide (false in tests that drive it by hand).
    pub ai: bool,
    pub rng: Rng,
    pub day: u32,
    seen_wars: usize,
    war_sides: Vec<[usize; 2]>,
    war_ended: Vec<bool>,
    embargo_seen: Vec<Vec<bool>>,
}

impl Diplomacy {
    pub fn new(data: DiploData, w: &Wars, e: &Economy, ai: &crate::ai::AiData, seed: u32) -> Diplomacy {
        let n = w.nations;
        let rep = &data.reputation;
        let reputation = (0..n).map(|i| (rep.start + rep.leader.get(&w.leader[i]).copied().unwrap_or(0.0)).clamp(0.0, 100.0)).collect();
        let traits = (0..n).map(|i| data.traits(&w.leader[i], &w.ideology[i])).collect();
        let intel_error = (0..n).map(|i| ai.intel.get(&w.secret_service[i]).copied().unwrap_or(0.2)).collect();
        let spies = spies::Spies::new(&data, w, e);
        let mut d = Diplomacy {
            n,
            names: w.names[..n].to_vec(),
            ideology: w.ideology[..n].to_vec(),
            leader: w.leader[..n].to_vec(),
            traits,
            intel_error,
            treaties: Vec::new(),
            active: Vec::new(),
            refusals: std::collections::HashMap::new(),
            broken: Vec::new(),
            armies: (0..n).map(|i| w.strength(i)).collect(),
            memory: vec![vec![Vec::new(); n]; n],
            relations: vec![vec![0.0; n]; n],
            reasons: vec![vec![Vec::new(); n]; n],
            reputation,
            loyalty: vec![data.vassals.base; n],
            inbox: Vec::new(),
            answers: Vec::new(),
            talks: talks::Talks::default(),
            spies,
            news: Vec::new(),
            stats: BTreeMap::new(),
            player: w.player,
            ai: true,
            rng: Rng::new(seed ^ 0xd1_9b0a),
            day: e.day,
            seen_wars: w.wars.len(),
            war_sides: w.wars.iter().map(|x| [x.sides[0].len(), x.sides[1].len()]).collect(),
            war_ended: w.wars.iter().map(|x| !x.active()).collect(),
            embargo_seen: e.nations.iter().map(|x| x.embargo.clone()).collect(),
            data,
        };
        d.rebuild_relations(e, w, None);
        d
    }

    /// Adds to a report counter.
    pub fn count(&mut self, key: &str, x: f64) {
        *self.stats.entry(key.to_string()).or_insert(0.0) += x;
    }

    pub fn say(&mut self, nations: Vec<usize>, text: String) {
        self.news.push(News { day: self.day, nations, text });
        if self.news.len() > 400 {
            self.news.drain(..100);
        }
    }

    /// `i` remembers something `j` did.
    pub fn remember(&mut self, i: usize, j: usize, kind: EventKind) {
        if i >= self.n || j >= self.n || i == j {
            return;
        }
        let log = &mut self.memory[i][j];
        log.push(Event { day: self.day, kind });
        if log.len() > 24 {
            log.remove(0);
        }
    }

    /// The active treaties.
    pub fn live(&self) -> impl Iterator<Item = &Treaty> {
        self.active.iter().map(|&k| &self.treaties[k]).filter(|t| t.active())
    }

    /// Rebuilds the index of active treaties (after any treaty changes).
    pub fn reindex(&mut self) {
        self.active = self.treaties.iter().filter(|t| t.active()).map(|t| t.id).collect();
    }

    /// Active treaties between `i` and `j` (either way round).
    pub fn between(&self, i: usize, j: usize) -> impl Iterator<Item = &Treaty> {
        self.live().filter(move |t| t.between(i, j))
    }

    pub fn has(&self, i: usize, j: usize, kind: Kind) -> bool {
        self.between(i, j).any(|t| t.kind == kind)
    }

    /// Allies: an alliance or defensive pact, either way.
    pub fn allied(&self, i: usize, j: usize) -> bool {
        self.between(i, j).any(|t| matches!(t.kind, Kind::Alliance | Kind::Defensive))
    }

    /// Everyone bound to defend `j`: defensive and alliance partners,
    /// guarantors, and its overlord.
    pub fn defenders(&self, e: &Economy, j: usize) -> Vec<usize> {
        let mut out: Vec<usize> = self
            .treaties
            .iter()
            .filter(|t| t.active() && t.involves(j))
            .filter_map(|t| match t.kind {
                Kind::Alliance | Kind::Defensive => Some(t.other(j)),
                Kind::Guarantee if t.b == j => Some(t.a),
                _ => None,
            })
            .collect();
        if let Some(o) = e.nations[j].overlord {
            out.push(o);
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// A nation's power as the world counts it (GDP and soldiers).
    pub fn power(&self, e: &Economy, i: usize) -> f64 {
        e.nations[i].gdp + 60_000.0 * self.armies.get(i).map_or(0.0, |a| a.0)
    }

    /// Counts every nation's soldiers and tanks (once a day).
    pub fn count_armies(&mut self, w: &Wars) {
        let mut a = vec![(0.0, 0.0); self.n];
        for d in w.divisions.iter().filter(|d| d.alive && d.faction < self.n) {
            a[d.faction].0 += d.soldiers;
            a[d.faction].1 += d.tanks;
        }
        self.armies = a;
    }

    /// The day's diplomacy.
    pub fn day(&mut self, e: &mut Economy, ed: &EconData, w: &mut Wars, mut brains: Option<&mut Brains>) {
        self.day = e.day;
        self.count_armies(w);
        self.watch_embargoes(e);
        alliances::watch_wars(self, e, ed, w, brains.as_deref());
        spies::day(self, e, ed, w, brains.as_deref_mut());
        if self.day % 7 == 3 {
            talks::week(self, e, ed, w, brains.as_deref());
        }
        if self.day % 30 == 0 {
            self.month(e, ed, w, brains.as_deref_mut());
        }
        if self.ai {
            if let Some(b) = brains.as_deref_mut() {
                for i in 0..self.n {
                    if Some(i) != self.player && (self.day + i as u32 * 7) % 30 == 3 && b.brain(i).is_some() {
                        treaties::think(self, e, ed, w, b, i);
                        spies::think(self, e, ed, w, b, i);
                    }
                }
            }
        }
        alliances::player_deadlines(self, e, ed, w);
        // Offers to the player lapse after 90 days unanswered.
        let day = self.day;
        let before = self.inbox.len();
        self.inbox.retain(|x| !matches!(x, Inbox::Proposal(p) if day >= p.day + 90));
        if self.inbox.len() < before {
            self.count("offers to the player that lapsed", (before - self.inbox.len()) as f64);
        }
    }

    /// The month: treaties run on, relations are rebuilt, the hooks for
    /// the war and the economy are set.
    fn month(&mut self, e: &mut Economy, ed: &EconData, w: &mut Wars, mut brains: Option<&mut Brains>) {
        treaties::month(self, e, ed, w);
        alliances::vassals(self, e, ed, w);
        if self.day % 360 == 0 {
            let r = self.data.reputation.recover;
            let start = self.data.reputation.start;
            for x in self.reputation.iter_mut() {
                *x += r * (start - *x);
            }
        }
        self.rebuild_relations(e, w, brains.as_deref());
        if let Some(b) = brains.as_deref_mut() {
            for i in 0..self.n {
                if let Some(Some(br)) = b.brains.get_mut(i) {
                    for j in 0..self.n {
                        br.relations[j].regard = self.relations[i][j] / 100.0;
                    }
                }
            }
        }
        alliances::war_hooks(self, e, w);
    }

    /// The economy's embargoes: remembered as they start and end.
    fn watch_embargoes(&mut self, e: &Economy) {
        for i in 0..self.n {
            for j in 0..self.n {
                let on = e.nations[i].embargo[j];
                if on != self.embargo_seen[i][j] {
                    self.embargo_seen[i][j] = on;
                    self.remember(j, i, if on { EventKind::EmbargoedUs } else { EventKind::LiftedEmbargo });
                }
            }
        }
    }

    /// Every pair's relations, from their reasons (docs/DIPLOMACY.md, section 1).
    pub fn rebuild_relations(&mut self, e: &Economy, w: &Wars, brains: Option<&Brains>) {
        let r = self.data.relations.clone();
        let ev = self.data.events.clone();
        let n = self.n;
        let year = 360.0;
        // Who shares a land border.
        let mut border = vec![vec![false; n]; n];
        for q in 0..w.owner.len() {
            let Some(a) = w.owner[q] else { continue };
            for &p in &w.geo.neighbours[q] {
                if let Some(b) = w.owner[p] {
                    if a != b && a < n && b < n {
                        border[a][b] = true;
                    }
                }
            }
        }
        // Active treaties by pair.
        let mut pact: Vec<Vec<Vec<Kind>>> = vec![vec![Vec::new(); n]; n];
        for t in self.live() {
            match t.kind {
                Kind::Sanctions => {
                    if let Some(tg) = t.target {
                        for &m in &t.members {
                            pact[tg][m].push(Kind::Sanctions);
                        }
                    }
                }
                Kind::Guarantee => pact[t.b][t.a].push(Kind::Guarantee),
                Kind::Arms => {}
                k => {
                    pact[t.a][t.b].push(k);
                    pact[t.b][t.a].push(k);
                }
            }
        }
        let opposed = |a: &str, b: &str| r.opposed.iter().any(|p| (p[0] == a && p[1] == b) || (p[0] == b && p[1] == a));
        for i in 0..n {
            let ni = &e.nations[i];
            let trade_total: f64 = ni.imports_from_last.iter().sum::<f64>() + ni.exports_to_last.iter().sum::<f64>();
            let brain = brains.and_then(|b| b.brain(i));
            for j in 0..n {
                if i == j {
                    continue;
                }
                let mut why: Vec<(String, f64)> = Vec::new();
                // Ideology.
                let (ii, ij) = (&self.ideology[i], &self.ideology[j]);
                if ii == ij {
                    let friends = r.ideological_friends.contains(ii);
                    let v = r.same_ideology + if friends { r.friends_bonus } else { 0.0 };
                    why.push((format!("both {}", ii.replace('_', " ")), v));
                } else if opposed(ii, ij) {
                    why.push((format!("{} against {}", ii.replace('_', " "), ij.replace('_', " ")), r.opposed_ideology));
                } else {
                    why.push(("another ideology".to_string(), r.other_ideology));
                }
                // The border.
                let allied = pact[i][j].iter().any(|k| matches!(k, Kind::Alliance | Kind::Defensive));
                if border[i][j] && !allied {
                    why.push(("a shared border".to_string(), r.border));
                }
                // Trade.
                if trade_total > 0.0 {
                    let share = (ni.imports_from_last[j] + ni.exports_to_last[j]) / trade_total;
                    let v = (r.trade * share).min(r.trade_max);
                    if v >= 1.0 {
                        why.push(("trade partners".to_string(), v));
                    }
                }
                // Treaties.
                for &k in &pact[i][j] {
                    let v = self.data.treaty_relations(k);
                    if v != 0.0 {
                        let words = match k {
                            Kind::Sanctions => "their sanctions on us".to_string(),
                            Kind::Guarantee => "they guarantee us".to_string(),
                            _ => format!("our {}", k.name().to_lowercase()),
                        };
                        why.push((words, v));
                    }
                }
                // Reputation.
                let rep = (self.reputation[j] - self.data.reputation.start) * r.reputation;
                if rep.abs() >= 1.0 {
                    why.push((if rep > 0.0 { "they keep their word".to_string() } else { "they break their word".to_string() }, rep));
                }
                // Power.
                if let Some(b) = brain {
                    if b.threat == Some(j) && b.threat_level > 0.2 {
                        why.push(("our biggest threat".to_string(), r.threat));
                    }
                    if b.hegemon == Some(j) {
                        why.push(("too strong for comfort".to_string(), r.hegemon));
                    }
                }
                // History: each kind once, its events summed, newest named.
                let mut by: Vec<(EventKind, f64, u32)> = Vec::new();
                for m in &self.memory[i][j] {
                    let age = (self.day.saturating_sub(m.day)) as f64 / year;
                    let v = m.kind.weight(&ev) * 0.5f64.powf(age / r.half_life);
                    match by.iter_mut().find(|x| x.0 == m.kind) {
                        Some(x) => {
                            x.1 += v;
                            x.2 = x.2.max(m.day);
                        }
                        None => by.push((m.kind, v, m.day)),
                    }
                }
                for (k, v, d) in by {
                    if v.abs() >= 1.0 {
                        why.push((format!("{} in Year {}", k.words(), d / 360 + 1), v));
                    }
                }
                why.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
                let total: f64 = why.iter().map(|x| x.1).sum();
                self.relations[i][j] = total.clamp(-100.0, 100.0);
                self.reasons[i][j] = why;
            }
        }
    }

    /// The reasons in one line: "+12 trade partners · −30 they invaded us in Year 14".
    pub fn why(&self, i: usize, j: usize, most: usize) -> String {
        self.reasons[i][j].iter().take(most).map(|(w, v)| format!("{:+.0} {w}", v)).collect::<Vec<_>>().join(" · ")
    }
}
