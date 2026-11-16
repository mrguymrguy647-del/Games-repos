//! The air (docs/WAR.md, section 6): pilots, airbases, the fight for air
//! superiority, and what the side that has it does with it.

use super::{normal, whole, Faction, Wars};
use crate::econ::{EconData, Economy};

/// Whether a faction has an airbase: its capital, or a province it holds
/// with road + rail of at least `airbase_roads`.
fn has_airbase(w: &Wars, e: &Economy, f: Faction) -> bool {
    let cap = w.capital[w.nation_of(f)];
    if w.control[cap] == Some(f) {
        return true;
    }
    w.held(f).iter().any(|&q| e.provinces[q].road + e.provinces[q].rail >= w.data.air.airbase_roads)
}

/// The jets a faction can put in the air today, and their quality.
fn ready(w: &Wars, e: &Economy, ed: &EconData, f: Faction) -> f64 {
    if f >= w.nations || !has_airbase(w, e, f) {
        return 0.0;
    }
    let n = &e.nations[f];
    let jets = n.stockpile[ed.jets];
    if jets < 1.0 {
        return 0.0;
    }
    let pilots = (w.forces[f].pilots / jets).min(1.0);
    let fuel = if n.stock[ed.fuel] > jets * w.data.air.jet_fuel_per_day { 1.0 } else { 0.3 };
    let weather = match w.weather[w.capital[f]] {
        1 => w.data.weather.rain_air,
        2 => w.data.weather.snow_air,
        3 => w.data.weather.storm_air,
        _ => 1.0,
    };
    jets * (1.0 - n.stockpile_faulty[ed.jets] * w.data.air.fault_grounding) * pilots * fuel * weather * (0.5 + w.training_cap(e, f))
}

/// The share of a faction's soldiers facing an enemy (on its border with them).
fn facing(w: &Wars, f: Faction, g: Faction) -> f64 {
    let mut total = 0.0;
    let mut there = 0.0;
    for d in w.divisions.iter().filter(|d| d.alive && d.faction == f) {
        total += d.soldiers;
        if w.geo.neighbours[d.province].iter().any(|&r| w.control[r] == Some(g)) || w.control[d.province] == Some(g) {
            there += d.soldiers;
        }
    }
    if total > 0.0 { there / total } else { 0.0 }
}

pub fn day(w: &mut Wars, e: &mut Economy, ed: &EconData) {
    let a = w.data.air.clone();
    // Pilots train towards `pilots_per_jet` a jet.
    for f in 0..w.nations {
        let want = e.nations[f].stockpile[ed.jets] * a.pilots_per_jet;
        let p = &mut w.forces[f].pilots;
        *p += (want - *p) / a.pilot_training_days;
        w.forces[f].air = [0.0; 5];
    }
    for f in w.nations..w.forces.len() {
        w.forces[f].air = [0.0; 5];
    }
    // Air war from a military operation up (a skirmish stays on the ground).
    let fighting: Vec<Faction> = (0..w.nations).filter(|&f| w.in_any_war(f) && w.level_of(f) >= 2).collect();
    let power: Vec<f64> = (0..w.forces.len()).map(|f| if fighting.contains(&f) { ready(w, e, ed, f) } else { 0.0 }).collect();
    for &f in &fighting {
        // Fuel for the sorties.
        let jets = e.nations[f].stockpile[ed.jets];
        let n = &mut e.nations[f];
        let fuel = (jets * a.jet_fuel_per_day).min(n.stock[ed.fuel]);
        n.stock[ed.fuel] -= fuel;
        n.ledger.consumed[ed.fuel] += fuel;
        let cost = fuel * n.price[ed.fuel];
        n.treasury -= cost;
        n.budget_month.army += cost;
    }
    for &f in &fighting {
        let enemies = w.enemies(f);
        if enemies.is_empty() {
            continue;
        }
        let mine: f64 = enemies.iter().map(|&g| power[f] * facing(w, f, g)).sum();
        let theirs: f64 = enemies.iter().map(|&g| power[g] * facing(w, g, f)).sum();
        let s = if mine + theirs > 0.0 { (mine - theirs) / (mine + theirs) } else { 0.0 };
        // The fronts that face an enemy.
        for d in 0..w.divisions.len() {
            let dv = &w.divisions[d];
            if dv.alive && dv.faction == f && w.geo.neighbours[dv.province].iter().any(|&r| w.control[r].is_some_and(|c| enemies.contains(&c))) {
                let fr = w.front_of(f, dv.province);
                w.forces[f].air[fr] = s;
            }
        }
        // Losses: each side's jets fall to the other's air power.
        if theirs > 0.0 {
            let luck = (0.25 * normal(&mut w.rng)).exp();
            let lost = whole((a.air_kill * theirs * theirs / (mine + theirs) * luck).min(e.nations[f].stockpile[ed.jets]), &mut w.rng);
            lose_jets(w, e, ed, f, lost);
        }
        // Strategic bombing, from a declared war up, with the upper hand.
        let level = w.level_of(f);
        if s > 0.0 && level >= 3 && mine > 0.0 {
            for &g in &enemies {
                let gn = w.nation_of(g);
                let near: Vec<usize> = w.held(g).into_iter().filter(|&q| w.geo.neighbours[q].iter().any(|&r| w.control[r] == Some(f))).collect();
                for &q in &near {
                    if w.owner[q] == Some(gn) {
                        for b in e.nations[gn].buildings.iter_mut().filter(|b| b.province == q) {
                            b.levels *= 1.0 - a.bomb_factories * s;
                        }
                    }
                    let p = &mut e.provinces[q];
                    p.rail = (p.rail - a.bomb_rail * s).max(0.0);
                    p.road = (p.road - 0.5 * a.bomb_rail * s).max(0.0);
                }
                if level >= 4 {
                    let st = &mut e.nations[gn].stability;
                    *st = (*st - a.bomb_city_stability * s).max(0.0);
                }
                // Anti-aircraft fire: a share of the bombers that fly.
                let (soldiers, _) = w.strength(g);
                let bombers = e.nations[f].stockpile[ed.jets] * s;
                let lost = whole((a.aa_loss * bombers * (soldiers / 1_000_000.0).min(1.0)).min(e.nations[f].stockpile[ed.jets]), &mut w.rng);
                lose_jets(w, e, ed, f, lost);
            }
        }
    }
}

fn lose_jets(w: &mut Wars, e: &mut Economy, ed: &EconData, f: Faction, lost: f64) {
    if lost <= 0.0 {
        return;
    }
    let n = &mut e.nations[f];
    let lost = lost.min(n.stockpile[ed.jets]);
    n.stockpile[ed.jets] -= lost;
    w.forces[f].pilots = (w.forces[f].pilots - lost * 0.5).max(0.0);
    w.forces[f].flows.used[ed.jets] += lost;
    for war in w.wars.iter_mut().filter(|x| x.active()) {
        if let Some(s) = war.side_of(f) {
            war.losses[s].jets += lost;
        }
    }
}
