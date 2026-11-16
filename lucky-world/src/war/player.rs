//! What the player can do about a war (the war room calls these): declare,
//! escalate, match or back down, offer or answer peace, give a front an
//! order. The NPCs reach the same functions through `escalation`, so the
//! rules are the same for everyone.

use super::escalation;
use super::peace::{self, PeaceTable};
use super::{Faction, Goal, Order, Wars};
use crate::econ::{EconData, Economy};

/// The player's side in a war, if the player is in it.
pub fn side(w: &Wars, war: usize) -> Option<usize> {
    w.player.and_then(|p| w.wars[war].side_of(p))
}

/// The wars the player is in (newest first), then everyone else's.
pub fn wars_in_view(w: &Wars) -> Vec<usize> {
    let mut mine: Vec<usize> = (0..w.wars.len()).rev().filter(|&k| w.wars[k].active() && side(w, k).is_some()).collect();
    let others: Vec<usize> = (0..w.wars.len()).rev().filter(|&k| w.wars[k].active() && side(w, k).is_none()).collect();
    let ended: Vec<usize> = (0..w.wars.len()).rev().filter(|&k| !w.wars[k].active()).take(12).collect();
    mine.extend(others);
    mine.extend(ended);
    mine
}

/// Declares a war on `target` at `level` (1 skirmish, 2 operation, 3 declared, 4 total).
pub fn declare(w: &mut Wars, e: &mut Economy, ed: &EconData, target: usize, level: usize) -> Option<usize> {
    let me = w.player?;
    if target == me || w.at_war(me, target).is_some() || w.forces[me].truces.iter().any(|&(f, until)| f == target && until > w.day) {
        return None;
    }
    let mine = w.held(me);
    let mut border: Vec<usize> = w.held(target).into_iter().filter(|&q| w.geo.neighbours[q].iter().any(|r| mine.contains(r))).collect();
    if border.is_empty() {
        border = w.held(target).into_iter().filter(|&q| w.geo.coastal[q]).take(1).collect();
    }
    border.sort_by_key(|&q| (w.geo.city[q], q));
    border.retain(|q| !w.capital.contains(q));
    let goal = if border.is_empty() { Goal::Reparations } else { Goal::Provinces(border.into_iter().take(if level >= 3 { 3 } else { 1 }).collect()) };
    Some(escalation::declare(w, e, ed, me, target, level.clamp(1, 4), goal, "the player's choice".to_string()))
}

/// Climbs one rung of the ladder; the other side must match or back down.
pub fn escalate(w: &mut Wars, war: usize) -> bool {
    let Some(s) = side(w, war) else { return false };
    let before = w.wars[war].level;
    escalation::escalate(w, war, s);
    w.wars[war].level > before
}

/// Answers the enemy's escalation: match it, or back down (the enemy gets its goal).
pub fn answer_escalation(w: &mut Wars, e: &mut Economy, ed: &EconData, war: usize, matches: bool) {
    let Some(s) = side(w, war) else { return };
    if w.wars[war].pending.is_some_and(|p| p.0 != s) {
        escalation::respond(w, e, ed, war, matches);
    }
}

/// The terms the war score buys now, and whether the enemy would sign them.
pub fn peace_on_offer(w: &Wars, e: &Economy, war: usize) -> Option<(peace::PeaceTerms, bool)> {
    let s = side(w, war)?;
    let t = peace::terms(w, war, peace::score(w, e, war));
    let enemy: Faction = w.wars[war].sides[1 - s][0];
    let yes = peace::ByExhaustion.accepts(w, e, war, enemy, &t);
    Some((t, yes))
}

/// Offers peace at the terms the war score buys; true if the enemy signs.
pub fn offer_peace(w: &mut Wars, e: &mut Economy, ed: &EconData, war: usize) -> bool {
    let Some((t, yes)) = peace_on_offer(w, e, war) else { return false };
    if yes {
        let how = if t.winner.is_none() { "a white peace" } else { "a peace deal" };
        peace::make_peace(w, e, ed, war, t, how);
    } else {
        let names = w.names[w.wars[war].sides[1 - side(w, war).unwrap_or(0)][0]].clone();
        let f = w.wars[war].sides.concat();
        w.say(f, format!("{names} turns down the peace offer"));
    }
    yes
}

/// Signs (or turns down) an offer the enemy made.
pub fn answer_offer(w: &mut Wars, e: &mut Economy, ed: &EconData, k: usize, accept: bool) {
    if k >= w.offers.len() {
        return;
    }
    let (war, _, t, _) = w.offers.remove(k);
    w.offers.retain(|o| o.0 != war || accept);
    if accept && w.wars[war].active() {
        let how = if t.winner.is_none() { "a white peace" } else { "a peace deal" };
        peace::make_peace(w, e, ed, war, t, how);
    }
}

/// Offers still worth answering (their war is on).
pub fn live_offers(w: &mut Wars) {
    let wars = &w.wars;
    w.offers.retain(|o| wars[o.0].active());
}

/// Gives every division of the player's on one front an order (Auto hands it back to the general).
pub fn order_front(w: &mut Wars, front: usize, order: Order) -> usize {
    let Some(me) = w.player else { return 0 };
    let mut n = 0;
    for d in 0..w.divisions.len() {
        let dv = &w.divisions[d];
        if dv.alive && dv.faction == me && w.front_of(me, dv.province) == front {
            w.divisions[d].order = order;
            n += 1;
        }
    }
    n
}

/// The enemy province nearest a front's divisions (an attack's target).
pub fn front_target(w: &Wars, front: usize) -> Option<usize> {
    let me = w.player?;
    let enemies = w.enemies(me);
    let mut best: Option<(usize, usize)> = None;
    for dv in w.divisions.iter().filter(|d| d.alive && d.faction == me && w.front_of(me, d.province) == front) {
        for &r in &w.geo.neighbours[dv.province] {
            if w.control[r].is_some_and(|c| enemies.contains(&c)) {
                let count = w.divisions.iter().filter(|x| x.alive && x.faction == me && x.province == dv.province).count();
                if best.is_none_or(|b| count > b.1) {
                    best = Some((r, count));
                }
            }
        }
    }
    best.map(|b| b.0)
}
