//! Spies (docs/DIPLOMACY.md, section 9): secret services, training,
//! counter-intelligence, missions whose odds are agent skill against the
//! target's counter-intelligence plus luck, caught agents and incidents.

use super::{Diplomacy, EventKind};
use crate::ai::Brains;
use crate::econ::{EconData, Economy};
use crate::war::{escalation, Wars};

/// A kind of mission.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Intel,
    Sabotage,
    Steal,
    Unrest,
    Revolt,
    Fund,
    Meddle,
    Kill,
    Bribe,
}

impl Kind {
    pub const ALL: [Kind; 9] = [Kind::Intel, Kind::Sabotage, Kind::Steal, Kind::Unrest, Kind::Revolt, Kind::Fund, Kind::Meddle, Kind::Kill, Kind::Bribe];

    pub fn key(self) -> &'static str {
        match self {
            Kind::Intel => "intel",
            Kind::Sabotage => "sabotage",
            Kind::Steal => "steal",
            Kind::Unrest => "unrest",
            Kind::Revolt => "revolt",
            Kind::Fund => "fund",
            Kind::Meddle => "meddle",
            Kind::Kill => "kill",
            Kind::Bribe => "bribe",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Intel => "Gather intel",
            Kind::Sabotage => "Sabotage industry",
            Kind::Steal => "Steal research",
            Kind::Unrest => "Stir unrest",
            Kind::Revolt => "Start a revolt",
            Kind::Fund => "Fund rebels or a proxy",
            Kind::Meddle => "Meddle in politics",
            Kind::Kill => "Kill a general",
            Kind::Bribe => "Bribe a general",
        }
    }
}

/// A nation's secret service.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct Service {
    pub agents: u32,
    pub skill: f64,
    /// The share of agents kept at home on counter-intelligence.
    pub ci_share: f64,
    /// Agents in training: the day each is ready.
    pub training: Vec<u32>,
    pub max: u32,
    pub ci_base: f64,
}

/// A mission under way.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct Mission {
    pub by: usize,
    pub target: usize,
    pub kind: Kind,
    pub agents: u32,
    pub start: u32,
    pub end: u32,
}

/// A mission's result, for the log.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct Report {
    pub day: u32,
    pub by: usize,
    pub target: usize,
    pub kind: Kind,
    pub success: bool,
    pub caught: u32,
    /// The true odds it ran at.
    pub p_success: f64,
    pub p_caught: f64,
    pub words: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct Spies {
    pub services: Vec<Service>,
    pub missions: Vec<Mission>,
    pub reports: Vec<Report>,
    /// `intel_until[i][j]`: the day `i`'s intel on `j` runs out.
    pub intel_until: Vec<Vec<u32>>,
    /// Bribed generals: (general, until, the skill to give back).
    pub bribed: Vec<(usize, u32, f64)>,
}

/// A steady uniform number from a hash (the same month, the same answer).
fn hash01(a: usize, b: usize, c: u32) -> f64 {
    let mut h = (a as u32).wrapping_mul(0x9e37_79b9) ^ (b as u32).wrapping_mul(0x85eb_ca6b) ^ c.wrapping_mul(0xc2b2_ae35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0xffff) as f64 / 65535.0
}

fn logistic(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

impl Spies {
    pub fn new(data: &super::DiploData, w: &Wars, e: &Economy) -> Spies {
        let n = w.nations;
        let sp = &data.spies;
        let services = (0..n)
            .map(|i| {
                let wheel = w.secret_service[i].as_str();
                let agents = sp.agents.get(wheel).copied().unwrap_or(0);
                Service {
                    agents,
                    skill: sp.skill.get(wheel).copied().unwrap_or(0.3),
                    ci_share: sp.ci_share,
                    training: Vec::new(),
                    max: ((agents as f64 * sp.max_factor) as u32).max(sp.min_max),
                    ci_base: sp.ci_base.get(wheel).copied().unwrap_or(0.2),
                }
            })
            .collect();
        let _ = e;
        Spies { services, missions: Vec::new(), reports: Vec::new(), intel_until: vec![vec![0; n]; n], bribed: Vec::new() }
    }

    pub fn busy(&self, i: usize) -> u32 {
        self.missions.iter().filter(|m| m.by == i).map(|m| m.agents).sum()
    }

    pub fn ci_agents(&self, i: usize) -> u32 {
        let s = &self.services[i];
        ((s.agents.saturating_sub(self.busy(i))) as f64 * s.ci_share).round() as u32
    }

    /// Agents free for a mission.
    pub fn idle(&self, i: usize) -> u32 {
        self.services[i].agents.saturating_sub(self.busy(i)).saturating_sub(self.ci_agents(i))
    }

    /// `C(j)`: the counter-intelligence a mission against `j` meets.
    pub fn counter_intel(&self, data: &super::DiploData, j: usize) -> f64 {
        let s = &self.services[j];
        s.ci_base + data.spies.ci_log * (1.0 + self.ci_agents(j) as f64).ln() * s.skill
    }

    /// The factor on `i`'s intel error about `j` (intel missions narrow it).
    pub fn intel_factor(&self, day: u32, i: usize, j: usize, data: &super::DiploData) -> f64 {
        match self.intel_until.get(i).and_then(|r| r.get(j)) {
            Some(&until) if until > day => data.spies.intel_factor,
            _ => 1.0,
        }
    }

    pub fn intel_factors(&self, day: u32, n: usize, data: &super::DiploData) -> Vec<Vec<f64>> {
        (0..n).map(|i| (0..n).map(|j| self.intel_factor(day, i, j, data)).collect()).collect()
    }
}

/// The odds of a mission: success and getting caught (docs/DIPLOMACY.md, section 9).
pub fn chances(data: &super::DiploData, kind: Kind, skill: f64, agents: u32, c: f64) -> (f64, f64) {
    let m = data.mission(kind);
    let k = data.spies.k;
    let s = skill + data.spies.per_agent * (agents.max(1) - 1) as f64;
    (logistic(k * (s - c + m.ease)), logistic(k * (c - s + m.exposure)))
}

/// `by`'s estimate of `target`'s counter-intelligence: the truth, with its
/// secret service's error (narrowed by intel, never under the floor).
pub fn estimate_ci(d: &Diplomacy, by: usize, target: usize) -> f64 {
    let truth = d.spies.counter_intel(&d.data, target);
    if by == target {
        return truth;
    }
    let err = (d.intel_error[by] * d.spies.intel_factor(d.day, by, target, &d.data)).max(d.data.spies.intel_floor);
    // A normal number from two steady uniforms (the same within a month).
    let (u1, u2) = (hash01(by, target, d.day / 30).max(1e-6), hash01(target, by, d.day / 30 + 7));
    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
    truth * (err * 2.0 * z).exp()
}

/// The odds as `by`'s own service sees them (what the spy screen shows).
pub fn seen_chances(d: &Diplomacy, by: usize, target: usize, kind: Kind, agents: u32) -> (f64, f64) {
    chances(&d.data, kind, d.spies.services[by].skill, agents, estimate_ci(d, by, target))
}

/// Why a mission can't be sent, if it can't.
pub fn check(d: &Diplomacy, e: &Economy, w: &Wars, by: usize, target: usize, kind: Kind, agents: u32) -> Result<(), String> {
    if by == target || target >= d.n {
        return Err("Choose another nation".to_string());
    }
    if agents == 0 || d.spies.idle(by) < agents {
        return Err(format!("Not enough free agents ({} free)", d.spies.idle(by)));
    }
    if d.spies.missions.iter().any(|m| m.by == by && m.target == target && m.kind == kind) {
        return Err("That mission is already under way".to_string());
    }
    match kind {
        Kind::Meddle if d.ideology[target] != "democracy" => Err("Only a democracy has politics to meddle in".to_string()),
        Kind::Kill | Kind::Bribe if !w.divisions.iter().any(|x| x.alive && x.faction == target) => Err("They have no generals".to_string()),
        Kind::Fund if e.nations[by].stockpile.iter().sum::<f64>() <= 0.0 => Err("We have no weapons to send".to_string()),
        _ => Ok(()),
    }
}

/// Sends a mission.
pub fn launch(d: &mut Diplomacy, e: &Economy, w: &Wars, by: usize, target: usize, kind: Kind, agents: u32) -> Result<String, String> {
    check(d, e, w, by, target, kind, agents)?;
    let m = d.data.mission(kind);
    let day = d.day;
    d.spies.missions.push(Mission { by, target, kind, agents, start: day, end: day + m.days });
    d.count(&format!("missions sent: {}", kind.key()), 1.0);
    Ok(format!("{} agent{} sent to {}: {} ({} days)", agents, if agents == 1 { "" } else { "s" }, d.names[target], kind.name().to_lowercase(), m.days))
}

/// Trains one agent (if there's room and money).
pub fn train(d: &mut Diplomacy, e: &mut Economy, i: usize) -> Result<String, String> {
    let s = &d.spies.services[i];
    if s.agents + s.training.len() as u32 >= s.max {
        return Err(format!("The service is full ({} agents at most)", s.max));
    }
    let cost = d.data.spies.train_cost * e.nations[i].gdp;
    let ex = e.nations[i].exchange.max(1e-12);
    if e.nations[i].treasury * ex < cost {
        return Err("The treasury can't pay for training".to_string());
    }
    e.nations[i].treasury -= cost / ex;
    let ready = d.day + d.data.spies.train_days;
    d.spies.services[i].training.push(ready);
    d.count("agents trained", 1.0);
    Ok(format!("An agent is in training ({} days, {})", d.data.spies.train_days, super::money(cost)))
}

/// The day: training ends, missions end, the service is paid for.
pub fn day(d: &mut Diplomacy, e: &mut Economy, ed: &EconData, w: &mut Wars, mut b: Option<&mut Brains>) {
    let day = d.day;
    for s in d.spies.services.iter_mut() {
        let done = s.training.iter().filter(|&&t| t <= day).count() as u32;
        s.training.retain(|&t| t > day);
        s.agents += done;
    }
    // Missions that end today.
    let mut k = 0;
    while k < d.spies.missions.len() {
        if d.spies.missions[k].end <= day {
            let m = d.spies.missions.remove(k);
            resolve(d, e, ed, w, b.as_deref_mut(), m);
        } else {
            k += 1;
        }
    }
    if day % 30 == 0 {
        // Upkeep, and the brains learn what their spies know.
        for i in 0..d.n {
            let cost = d.data.spies.upkeep * e.nations[i].gdp * d.spies.services[i].agents as f64 / 12.0;
            e.nations[i].treasury -= cost / e.nations[i].exchange.max(1e-12);
        }
        if let Some(b) = b.as_deref_mut() {
            for i in 0..d.n {
                if let Some(Some(br)) = b.brains.get_mut(i) {
                    for j in 0..d.n {
                        br.intel_factor[j] = d.spies.intel_factor(day, i, j, &d.data);
                    }
                }
            }
        }
        // Bribes run out.
        let back: Vec<(usize, f64)> = d.spies.bribed.iter().filter(|x| x.1 <= day).map(|x| (x.0, x.2)).collect();
        d.spies.bribed.retain(|x| x.1 > day);
        for (g, skill) in back {
            if g < w.generals.len() {
                w.generals[g].skill = skill;
            }
        }
    }
}

/// A mission ends: did it work, was anyone caught, what happened.
fn resolve(d: &mut Diplomacy, e: &mut Economy, ed: &EconData, w: &mut Wars, b: Option<&mut Brains>, m: Mission) {
    let (by, target) = (m.by, m.target);
    let c = d.spies.counter_intel(&d.data, target);
    let skill = d.spies.services[by].skill;
    let (ps, pc) = chances(&d.data, m.kind, skill, m.agents, c);
    let success = (d.rng.next_f32() as f64) < ps;
    let caught = (0..m.agents).filter(|_| (d.rng.next_f32() as f64) < pc).count() as u32;
    let sp = d.data.spies.clone();
    let s = &mut d.spies.services[by];
    s.agents = s.agents.saturating_sub(caught);
    if success {
        s.skill = (s.skill + sp.skill_gain).min(sp.skill_max);
    }
    let mut words = String::new();
    if success {
        words = effect(d, e, ed, w, b, &m);
    }
    d.count(&format!("missions {}: {}", if success { "succeeded" } else { "failed" }, m.kind.key()), 1.0);
    if caught > 0 {
        d.remember(target, by, EventKind::CaughtSpy);
        d.reputation[by] = (d.reputation[by] - d.data.reputation.incident).max(0.0);
        d.count("agents caught", caught as f64);
        d.count("spy incidents", 1.0);
        let text = format!("{} catches {} agent{} of {}: a diplomatic incident", d.names[target], caught, if caught == 1 { "" } else { "s" }, d.names[by]);
        d.say(vec![target, by], text);
    }
    let words = if success { words } else { "it failed".to_string() };
    d.spies.reports.push(Report { day: d.day, by, target, kind: m.kind, success, caught, p_success: ps, p_caught: pc, words });
    if d.spies.reports.len() > 800 {
        d.spies.reports.drain(..200);
    }
}

/// What a successful mission does (docs/DIPLOMACY.md, section 9).
fn effect(d: &mut Diplomacy, e: &mut Economy, ed: &EconData, w: &mut Wars, b: Option<&mut Brains>, m: &Mission) -> String {
    let sp = d.data.spies.clone();
    let (by, t) = (m.by, m.target);
    let day = d.day;
    match m.kind {
        Kind::Intel => {
            d.spies.intel_until[by][t] = day + (sp.intel_years * 360.0) as u32;
            if let Some(br) = b.and_then(|b| b.brains.get_mut(by)).and_then(|x| x.as_mut()) {
                br.intel_factor[t] = sp.intel_factor;
            }
            format!("we see {} clearly for {:.0} years", d.names[t], sp.intel_years)
        }
        Kind::Sabotage => {
            let n = &mut e.nations[t];
            let power: Vec<usize> = n.buildings.iter().enumerate().filter(|(_, x)| ed.buildings[x.kind].name.contains("Power")).map(|(k, _)| k).collect();
            if !power.is_empty() {
                let q = n.buildings[power[0]].province;
                for k in power {
                    if n.buildings[k].province == q {
                        n.buildings[k].levels *= 1.0 - sp.sabotage_power;
                    }
                }
                format!("power plants in {} wrecked", w.geo.names[q])
            } else {
                let mut by_prov: Vec<(usize, f64)> = Vec::new();
                for x in &n.buildings {
                    match by_prov.iter_mut().find(|p| p.0 == x.province) {
                        Some(p) => p.1 += x.levels,
                        None => by_prov.push((x.province, x.levels)),
                    }
                }
                let Some(&(q, _)) = by_prov.iter().max_by(|a, b| a.1.total_cmp(&b.1)) else { return "nothing worth wrecking".to_string() };
                for x in n.buildings.iter_mut().filter(|x| x.province == q) {
                    x.levels *= 1.0 - sp.sabotage_buildings;
                }
                let pe = &mut e.provinces[q];
                pe.rail = (pe.rail - sp.sabotage_rail).max(0.0);
                format!("factories and rail in {} wrecked", w.geo.names[q])
            }
        }
        Kind::Steal => {
            let gap = (e.nations[t].knowledge - e.nations[by].knowledge).max(0.0);
            e.nations[by].knowledge += sp.steal_share * gap;
            if gap > 0.0 { "their research is ours".to_string() } else { "they knew nothing we didn't".to_string() }
        }
        Kind::Unrest => {
            e.nations[t].stability = (e.nations[t].stability - sp.unrest).max(0.0);
            format!("unrest in {}", d.names[t])
        }
        Kind::Revolt => {
            let rebels = w.nations + t;
            let civil = w.wars.iter().any(|x| x.active() && x.civil && x.side_of(rebels).is_some());
            if e.nations[t].stability < sp.revolt_stability && !civil {
                escalation::rise(w, e, ed, t);
                d.count("revolts started by spies", 1.0);
                format!("a revolt breaks out in {}", d.names[t])
            } else {
                e.nations[t].stability = (e.nations[t].stability - sp.revolt_fallback).max(0.0);
                format!("riots in {}", d.names[t])
            }
        }
        Kind::Fund => {
            let rebels = w.nations + t;
            let civil = w.wars.iter().any(|x| x.active() && x.civil && x.side_of(rebels).is_some());
            let enemy = (0..d.n).find(|&z| z != by && w.at_war(t, z).is_some());
            let to = if civil { Some(rebels) } else { enemy };
            match to {
                Some(to) => {
                    for g in [ed.ammo, ed.rifles] {
                        let q = (e.nations[by].stockpile[g] * sp.fund_share).floor();
                        if q <= 0.0 {
                            continue;
                        }
                        let fq = e.nations[by].stockpile_faulty[g];
                        e.nations[by].stockpile[g] -= q;
                        w.forces[by].flows.given[g] += q;
                        w.store(e, to, g, q, fq);
                        w.forces[to].flows.received[g] += q;
                    }
                    if civil { format!("weapons reach the rebels in {}", d.names[t]) } else { format!("weapons reach {}'s enemy", d.names[t]) }
                }
                None => {
                    e.nations[t].stability = (e.nations[t].stability - 3.0).max(0.0);
                    format!("dissidents in {} are paid", d.names[t])
                }
            }
        }
        Kind::Meddle => {
            e.nations[t].stability = (e.nations[t].stability - sp.meddle).max(0.0);
            w.forces[t].war_unrest_days += sp.meddle_days;
            format!("{}'s government is shaken", d.names[t])
        }
        Kind::Kill | Kind::Bribe => {
            let best = w.divisions.iter().filter(|x| x.alive && x.faction == t).map(|x| x.general).max_by(|&a, &b| w.generals[a].skill.total_cmp(&w.generals[b].skill));
            let Some(g) = best else { return "no general found".to_string() };
            let name = w.generals[g].name.clone();
            if m.kind == Kind::Kill {
                w.generals[g].alive = false;
                let new = w.new_general(t);
                for x in w.divisions.iter_mut().filter(|x| x.general == g) {
                    x.general = new;
                }
                format!("{name} is dead")
            } else {
                let old = w.generals[g].skill;
                w.generals[g].skill = old * sp.bribe_skill;
                d.spies.bribed.push((g, day + sp.bribe_days, old));
                format!("{name} is ours")
            }
        }
    }
}

/// A brain's secret service for the month (docs/DIPLOMACY.md, section 10).
pub fn think(d: &mut Diplomacy, e: &mut Economy, ed: &EconData, w: &mut Wars, b: &Brains, i: usize) {
    let _ = ed;
    let Some(brain) = b.brain(i) else { return };
    let trait_spies = d.traits[i].spies;
    let threat_level = brain.threat_level;
    // Counter-intelligence: more at home when threatened.
    d.spies.services[i].ci_share = (d.data.spies.ci_share + 0.2 * threat_level - 0.2 * trait_spies).clamp(0.2, 0.8);
    // Training, while there is room and the treasury is in the black.
    let s = &d.spies.services[i];
    let want = (s.max as f64 * (0.5 + 0.5 * (threat_level + trait_spies).clamp(0.0, 1.0))).round() as u32;
    if s.agents + (s.training.len() as u32) < want && e.nations[i].treasury > 0.0 {
        let _ = train(d, e, i);
    }
    // One mission at a time, and only in some months (more for a spying
    // personality, more at war): a service plans before it acts.
    let idle = d.spies.idle(i);
    let running = d.spies.missions.iter().filter(|m| m.by == i).count();
    let eager = (d.data.spies.launch_chance + trait_spies + if w.in_any_war(i) { 0.2 } else { 0.0 }).clamp(0.05, 1.0);
    if idle == 0 || running >= 1 || (d.rng.next_f32() as f64) >= eager {
        return;
    }
    // Targets: its threat, its enemies, its opportunities, a research leader, a democratic rival.
    let mut targets: Vec<usize> = Vec::new();
    if let Some(t) = brain.threat {
        targets.push(t);
    }
    for j in 0..d.n {
        if j != i && (w.at_war(i, j).is_some() || brain.opportunities.iter().any(|o| o.target == j) || d.relations[i][j] < -30.0) && !targets.contains(&j) {
            targets.push(j);
        }
    }
    let leader = (0..d.n).filter(|&j| j != i).max_by(|&a, &b| e.nations[a].knowledge.total_cmp(&e.nations[b].knowledge));
    if let Some(l) = leader.filter(|&l| e.nations[l].knowledge > 1.2 * e.nations[i].knowledge.max(1e-9) && !targets.contains(&l)) {
        targets.push(l);
    }
    let mut best: Option<(f64, usize, Kind)> = None;
    for &j in &targets {
        let at_war = w.at_war(i, j).is_some();
        let stab = brain.estimates[j].stability;
        let rebels = w.nations + j;
        let civil = w.wars.iter().any(|x| x.active() && x.civil && x.side_of(rebels).is_some());
        let honour = d.traits[i].honour;
        for kind in Kind::ALL {
            let gain = match kind {
                Kind::Intel if d.spies.intel_factor(d.day, i, j, &d.data) >= 1.0 && (at_war || brain.threat == Some(j)) => 0.5,
                Kind::Sabotage if at_war => 0.8,
                Kind::Sabotage if brain.threat == Some(j) => 0.3,
                Kind::Steal if e.nations[j].knowledge > 1.3 * e.nations[i].knowledge.max(1e-9) => 0.5,
                Kind::Unrest if at_war => 0.4,
                Kind::Revolt if at_war && stab < d.data.spies.revolt_stability => 1.0,
                Kind::Fund if civil => 0.6,
                Kind::Meddle if d.ideology[j] == "democracy" && d.relations[i][j] < -20.0 => 0.4,
                Kind::Kill if at_war => 0.7,
                Kind::Bribe if at_war => 0.6,
                _ => 0.0,
            };
            if gain <= 0.0 || check(d, e, w, i, j, kind, 1).is_err() {
                continue;
            }
            let agents = idle.min(2);
            let (ps, pc) = seen_chances(d, i, j, kind, agents);
            let cost = pc * (0.6 * honour + if d.relations[i][j] > 0.0 { 0.3 } else { 0.0 });
            let ev = ps * gain - cost + 0.2 * trait_spies;
            if ev > 0.15 && best.is_none_or(|x| ev > x.0) {
                best = Some((ev, j, kind));
            }
        }
    }
    if let Some((_, j, kind)) = best {
        let _ = launch(d, e, w, i, j, kind, idle.min(2));
    }
}
