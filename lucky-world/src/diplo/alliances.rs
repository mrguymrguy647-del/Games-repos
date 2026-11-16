//! Alliances in war, vassals, and the hooks diplomacy gives the war
//! (docs/DIPLOMACY.md, sections 6, 8 and 11).

use super::treaties::{break_treaty, end_treaty, sign};
use super::{Diplomacy, EventKind, Inbox, Kind, Proposal};
use crate::ai::Brains;
use crate::econ::{EconData, Economy};
use crate::war::{escalation, Wars};

/// What changed in the wars since yesterday: new wars break pacts and call
/// allies; new members fight side by side; ended wars are remembered.
pub fn watch_wars(d: &mut Diplomacy, e: &mut Economy, ed: &EconData, w: &mut Wars, b: Option<&Brains>) {
    let n = d.n;
    let start = d.seen_wars;
    for k in start..w.wars.len() {
        d.war_sides.push([0, 0]);
        d.war_ended.push(false);
        new_war(d, e, ed, w, b, k);
    }
    d.seen_wars = w.wars.len();
    for k in 0..w.wars.len() {
        // New members: they fight beside their side, against the other.
        for s in 0..2 {
            let now = w.wars[k].sides[s].len();
            let was = d.war_sides[k][s];
            if now > was {
                for idx in was..now {
                    let x = w.wars[k].sides[s][idx];
                    if x >= n {
                        continue;
                    }
                    for &y in &w.wars[k].sides[s] {
                        if y < n && y != x {
                            d.remember(y, x, EventKind::SideBySide);
                            d.remember(x, y, EventKind::SideBySide);
                        }
                    }
                    if idx > 0 {
                        for &z in &w.wars[k].sides[1 - s] {
                            if z < n {
                                d.remember(z, x, EventKind::FoughtUs);
                            }
                        }
                    }
                }
                d.war_sides[k][s] = now;
            }
        }
        if !d.war_ended[k] && !w.wars[k].active() {
            d.war_ended[k] = true;
            let (a, v) = (w.wars[k].sides[0][0], w.wars[k].sides[1][0]);
            if a < n && v < n {
                d.remember(a, v, EventKind::MadePeace);
                d.remember(v, a, EventKind::MadePeace);
            }
            d.talks.state.remove(&k);
        }
    }
}

/// A war begins: it breaks every pact between the two, and both sides'
/// allies are called (docs/DIPLOMACY.md, sections 5 and 6).
fn new_war(d: &mut Diplomacy, e: &mut Economy, ed: &EconData, w: &mut Wars, b: Option<&Brains>, k: usize) {
    let n = d.n;
    let wr = w.wars[k].clone();
    d.war_sides[k] = [wr.sides[0].len(), wr.sides[1].len()];
    if wr.max_level == 0 {
        return;
    }
    let (a, v) = (wr.sides[0][0], wr.sides[1][0]);
    if a >= n || v >= n {
        return; // a civil war: rebels have no treaties
    }
    d.remember(v, a, EventKind::InvadedUs);
    d.remember(a, v, EventKind::FoughtUs);
    // The attacker breaks every binding pact with its victim; the rest end.
    let ids: Vec<usize> = d.live().filter(|t| t.between(a, v) && t.kind != super::Kind::Loan).map(|t| t.id).collect();
    for id in ids {
        if d.treaties[id].kind.binding() {
            break_treaty(d, e, id, a, &format!("attacked {}", d.names[v]));
        } else {
            end_treaty(d, e, id);
        }
    }
    d.count("wars", 1.0);
    // A coalition against a hegemon becomes a formal alliance.
    if wr.motive == "coalition" {
        let members: Vec<usize> = wr.sides[0].iter().copied().filter(|&x| x < n).collect();
        for (ix, &x) in members.iter().enumerate() {
            for &y in &members[ix + 1..] {
                if !d.has(x, y, Kind::Alliance) {
                    let p = Proposal { from: x, to: y, kind: Kind::Alliance, day: d.day, good: None, amount: 0.0, target: None };
                    let id = sign(d, e, ed, w, &p);
                    d.treaties[id].end = d.day + (d.data.war.alliance_years * 360.0) as u32;
                    d.count("coalition alliances", 1.0);
                }
            }
        }
    }
    // Calls to arms: the victim's defenders, then the attacker's allies.
    if wr.civil {
        return;
    }
    let mut calls: Vec<(usize, usize, Kind, bool)> = Vec::new();
    for t in d.live().filter(|t| t.involves(v)) {
        match t.kind {
            Kind::Alliance | Kind::Defensive => calls.push((t.other(v), 1, t.kind, false)),
            Kind::Guarantee if t.b == v => calls.push((t.a, 1, Kind::Guarantee, false)),
            _ => {}
        }
    }
    if let Some(o) = e.nations[v].overlord {
        calls.push((o, 1, Kind::Guarantee, true));
    }
    for t in d.live().filter(|t| t.involves(a) && t.kind == Kind::Alliance) {
        calls.push((t.other(a), 0, Kind::Alliance, false));
    }
    for (x, side, kind, vassal) in calls {
        if x != a && x != v && x < n {
            call(d, e, ed, w, b, x, k, side, kind, vassal);
        }
    }
}

/// `x` is called to war `k` on `side`, by a treaty of `kind` (or as an
/// overlord): it honours it or not (docs/DIPLOMACY.md, section 6).
#[allow(clippy::too_many_arguments)]
fn call(d: &mut Diplomacy, e: &mut Economy, ed: &EconData, w: &mut Wars, b: Option<&Brains>, x: usize, k: usize, side: usize, kind: Kind, vassal: bool) {
    let wr = &w.wars[k];
    if wr.side_of(x).is_some() || !wr.active() {
        return;
    }
    let ally = wr.sides[side][0];
    let enemy = wr.sides[1 - side][0];
    // Bound to both sides: it stays out (and breaks nothing).
    if d.allied(x, enemy) || w.at_war(x, ally).is_some() {
        return;
    }
    if Some(x) == d.player {
        let deadline = d.day + d.data.war.player_days;
        d.inbox.push(Inbox::Call { ally, war: k, side, kind, deadline });
        let text = format!("{} calls you to war against {}", d.names[ally], d.names[enemy]);
        d.say(vec![x], text);
        return;
    }
    let c = &d.data.war;
    let power = |f: usize| if f < d.n { d.power(e, f) } else { 60_000.0 * w.strength(f).0 };
    let mine: f64 = wr.sides[side].iter().filter(|&&f| f < d.n).map(|&f| power(f)).sum::<f64>() + power(x);
    let theirs: f64 = wr.sides[1 - side].iter().filter(|&&f| f < d.n).map(|&f| power(f)).sum::<f64>().max(1.0);
    let honour = d.traits[x].honour + d.relations[x][ally] / 100.0 - d.relations[x][enemy] / 200.0 + 0.5 * (mine / theirs).min(3.0) - 0.5
        - c.exhaustion_cost * w.forces[x].exhaustion
        - if w.in_any_war(x) { c.busy_cost } else { 0.0 };
    let offensive = side == 0 && kind == Kind::Alliance;
    let threshold = if offensive { c.offensive_threshold } else { c.honour_threshold };
    let _ = (b, ed);
    if honour > threshold {
        escalation::join(w, k, side, x);
        if w.wars[k].side_of(x).is_some() {
            honoured(d, x, ally, enemy, vassal);
        }
        return;
    }
    refused(d, e, x, ally, enemy, kind, offensive, vassal);
}

fn honoured(d: &mut Diplomacy, x: usize, ally: usize, enemy: usize, vassal: bool) {
    d.remember(ally, x, if vassal { EventKind::Protected } else { EventKind::CameToAid });
    d.reputation[x] = (d.reputation[x] + d.data.reputation.honour_gain).min(100.0);
    d.count("calls to arms honoured", 1.0);
    let text = format!("{} honours its word and joins {} against {}", d.names[x], d.names[ally], d.names[enemy]);
    d.say(vec![x, ally, enemy], text);
}

#[allow(clippy::too_many_arguments)]
fn refused(d: &mut Diplomacy, e: &mut Economy, x: usize, ally: usize, enemy: usize, kind: Kind, offensive: bool, vassal: bool) {
    if offensive {
        d.count("offensive calls declined", 1.0);
        return;
    }
    d.count("calls to arms refused", 1.0);
    if vassal {
        d.remember(ally, x, EventKind::Neglected);
        let text = format!("{} leaves its vassal {} to its fate against {}", d.names[x], d.names[ally], d.names[enemy]);
        d.say(vec![x, ally], text);
        return;
    }
    d.remember(ally, x, EventKind::RefusedCall);
    let found = d.live().find(|t| t.kind == kind && t.between(x, ally)).map(|t| t.id);
    if let Some(id) = found {
        break_treaty(d, e, id, x, &format!("refused the call to arms against {}", d.names[enemy]));
    }
}

/// The player answers a call to arms.
#[allow(clippy::too_many_arguments)]
pub fn player_call(d: &mut Diplomacy, e: &mut Economy, ed: &EconData, w: &mut Wars, b: Option<&Brains>, ally: usize, k: usize, side: usize, kind: Kind, yes: bool) -> String {
    let _ = (ed, b);
    let Some(x) = d.player else { return String::new() };
    if k >= w.wars.len() || !w.wars[k].active() {
        return "That war is over".to_string();
    }
    let enemy = w.wars[k].sides[1 - side][0];
    let vassal = e.nations[ally].overlord == Some(x);
    if yes {
        escalation::join(w, k, side, x);
        if w.wars[k].side_of(x).is_some() {
            honoured(d, x, ally, enemy, vassal);
            return format!("You join {} against {}", d.names[ally], d.names[enemy]);
        }
        return "That side is full: you stay out".to_string();
    }
    refused(d, e, x, ally, enemy, kind, side == 0 && kind == Kind::Alliance, vassal);
    format!("You refuse {}'s call to arms", d.names[ally])
}

/// A call to the player left unanswered past its deadline is honoured.
pub fn player_deadlines(d: &mut Diplomacy, e: &mut Economy, ed: &EconData, w: &mut Wars) {
    let day = d.day;
    let due: Vec<usize> = d.inbox.iter().enumerate().filter(|(_, x)| matches!(x, Inbox::Call { deadline, .. } if *deadline <= day)).map(|(k, _)| k).collect();
    for k in due.into_iter().rev() {
        super::treaties::answer_inbox(d, e, ed, w, None, k, true);
    }
}

/// Vassals' loyalty (docs/DIPLOMACY.md, section 8), monthly.
pub fn vassals(d: &mut Diplomacy, e: &mut Economy, ed: &EconData, w: &Wars) {
    let v = d.data.vassals.clone();
    for i in 0..d.n {
        let Some(o) = e.nations[i].overlord else {
            d.loyalty[i] = v.base;
            continue;
        };
        let recent = |k: EventKind| d.memory[i][o].iter().any(|m| m.kind == k && d.day.saturating_sub(m.day) < 3600);
        let target = v.base + v.relations * d.relations[i][o] - v.tribute * ed.state.tribute
            + if recent(EventKind::Protected) { v.protected } else { 0.0 }
            + if recent(EventKind::Neglected) { v.neglected } else { 0.0 };
        let before = d.loyalty[i];
        d.loyalty[i] += v.speed * (target.clamp(0.0, 100.0) - d.loyalty[i]);
        if d.loyalty[i] < v.movement {
            e.nations[i].stability = (e.nations[i].stability - v.movement_stability).max(0.0);
            if before >= v.movement {
                d.count("independence movements", 1.0);
                let text = format!("An independence movement grows in {}, against {}", d.names[i], d.names[o]);
                d.say(vec![i, o], text);
            }
        }
    }
    let _ = w;
}

/// What diplomacy tells the war each month (docs/DIPLOMACY.md, section 11).
pub fn war_hooks(d: &Diplomacy, e: &Economy, w: &mut Wars) {
    let n = d.n;
    let c = &d.data.war;
    let mut cost = vec![vec![0.0; n]; n];
    let power: Vec<f64> = (0..n).map(|i| d.power(e, i).max(1.0)).collect();
    let defenders: Vec<Vec<usize>> = (0..n).map(|j| d.defenders(e, j)).collect();
    for i in 0..n {
        for j in 0..n {
            if i == j {
                continue;
            }
            let pacts: f64 = d.between(i, j).filter(|t| t.kind.binding()).map(|t| d.data.break_cost(t.kind)).sum();
            let deter: f64 = defenders[j].iter().filter(|&&z| z != i).map(|&z| power[z] * d.traits[z].honour).sum::<f64>() / power[i];
            cost[i][j] = d.traits[i].honour * pacts / c.break_desire + c.deterrence * deter.min(3.0);
        }
    }
    w.diplo_cost = cost;
    let mut access = vec![vec![false; n]; n];
    for t in d.live().filter(|t| matches!(t.kind, Kind::Access | Kind::Alliance)) {
        access[t.a][t.b] = true;
        access[t.b][t.a] = true;
    }
    w.access = access;
    w.loyalty = (0..n).map(|i| if e.nations[i].overlord.is_some() { d.loyalty[i] } else { 50.0 }).collect();
    w.intel_seen = d.spies.intel_factors(d.day, n, &d.data);
    w.peace_talks = true;
}
