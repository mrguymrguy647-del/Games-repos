//! The sea (docs/WAR.md, section 7): sea zones, navies, control,
//! blockades, safe sea lanes for supply, and invasions by transport.

use super::{normal, Faction, WarData, Wars};
use crate::econ::{EconData, Economy};
use crate::hex;
use crate::mapdata::{Atlas, TerrainKind};
use crate::world::World;

/// The zones a map's sea is cut into: (each province's zones, zone links,
/// zone middles, each zone's coastal provinces).
#[allow(clippy::type_complexity)]
pub fn zones(wd: &WarData, atlas: &Atlas, world: &World) -> (Vec<Vec<usize>>, Vec<Vec<usize>>, Vec<(f32, f32)>, Vec<Vec<usize>>) {
    let map = atlas.map(world.map);
    let (w, h) = (map.width, map.height);
    let sea: Vec<bool> = map.cells.iter().map(|&t| atlas.terrains[t].kind == TerrainKind::Sea).collect();
    let step = wd.sea.zone_cells.sqrt().round().max(3.0) as usize;
    // Seeds on a grid, and one in every body of water the grid missed.
    let mut zone_of: Vec<Option<usize>> = vec![None; sea.len()];
    let mut seeds: Vec<usize> = Vec::new();
    for r in 0..h {
        for c in 0..w {
            let i = map.index(c, r);
            if sea[i] && c % step == step / 2 && r % step == step / 2 {
                zone_of[i] = Some(seeds.len());
                seeds.push(i);
            }
        }
    }
    let fill = |zone_of: &mut Vec<Option<usize>>, start: Vec<usize>| {
        let mut queue: std::collections::VecDeque<usize> = start.into_iter().collect();
        while let Some(i) = queue.pop_front() {
            let (c, r) = map.cell(i);
            for (nc, nr) in hex::neighbours(c, r, w, h) {
                let j = map.index(nc, nr);
                if sea[j] && zone_of[j].is_none() {
                    zone_of[j] = zone_of[i];
                    queue.push_back(j);
                }
            }
        }
    };
    fill(&mut zone_of, seeds.clone());
    for i in 0..sea.len() {
        if sea[i] && zone_of[i].is_none() {
            zone_of[i] = Some(seeds.len());
            seeds.push(i);
            fill(&mut zone_of, vec![i]);
        }
    }
    let count = seeds.len();
    let mut links: Vec<Vec<usize>> = vec![Vec::new(); count];
    let mut sum = vec![(0.0f32, 0.0f32, 0usize); count];
    for i in 0..sea.len() {
        let Some(z) = zone_of[i] else { continue };
        let (c, r) = map.cell(i);
        let (x, y) = hex::center(c, r);
        sum[z] = (sum[z].0 + x, sum[z].1 + y, sum[z].2 + 1);
        for (nc, nr) in hex::neighbours(c, r, w, h) {
            if let Some(o) = zone_of[map.index(nc, nr)] {
                if o != z && !links[z].contains(&o) {
                    links[z].push(o);
                }
            }
        }
    }
    let middle: Vec<(f32, f32)> = sum.iter().map(|&(x, y, n)| (x / n.max(1) as f32, y / n.max(1) as f32)).collect();
    let mut of_province: Vec<Vec<usize>> = vec![Vec::new(); map.provinces.len()];
    let mut coast: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (p, prov) in map.provinces.iter().enumerate() {
        for &cell in &prov.cells {
            let (c, r) = map.cell(cell);
            for (nc, nr) in hex::neighbours(c, r, w, h) {
                if let Some(z) = zone_of[map.index(nc, nr)] {
                    if !of_province[p].contains(&z) {
                        of_province[p].push(z);
                        coast[z].push(p);
                    }
                }
            }
        }
    }
    (of_province, links, middle, coast)
}

/// Whether zone `z` is held against `f`: its enemies there outgun it and
/// its friends by `control_ratio` (neutral ships don't count).
pub fn hostile(w: &Wars, f: Faction, z: usize) -> bool {
    let Some(here) = w.zone_fleets.get(z) else { return false };
    let mut theirs = 0.0;
    let mut ours = 0.0;
    for &(g, p) in here {
        if w.at_war(f, g).is_some() {
            theirs += p;
        } else if g == f || w.friends(f).contains(&g) {
            ours += p;
        }
    }
    theirs > 0.0 && theirs >= w.data.sea.control_ratio * ours
}

/// Coastal provinces reachable by sea from port `q` without crossing an
/// enemy-held zone: (province, the share of supplies that survive).
pub fn sea_links(w: &Wars, f: Faction, q: usize) -> Vec<(usize, f64)> {
    let zones = w.geo.zone_links.len();
    let mut dist: Vec<Option<u32>> = vec![None; zones];
    let mut queue = std::collections::VecDeque::new();
    for &z in &w.geo.zones_of[q] {
        if !hostile(w, f, z) {
            dist[z] = Some(1);
            queue.push_back(z);
        }
    }
    while let Some(z) = queue.pop_front() {
        let d = dist[z].unwrap();
        for &o in &w.geo.zone_links[z] {
            if dist[o].is_none() && !hostile(w, f, o) {
                dist[o] = Some(d + 1);
                queue.push_back(o);
            }
        }
    }
    let mut out: Vec<(usize, f64)> = Vec::new();
    for z in 0..zones {
        let Some(d) = dist[z] else { continue };
        let contested = w.zone_contested.get(z).copied().unwrap_or(false);
        let k = 0.99f64.powi(d as i32) * if contested { 1.0 - w.data.sea.convoy_loss } else { 1.0 };
        for &p in &w.geo.zone_coast[z] {
            if p != q && !out.iter().any(|x| x.0 == p) {
                out.push((p, k));
            }
        }
    }
    out
}

/// A navy's fighting power in a zone.
fn naval_power(w: &Wars, e: &Economy, ed: &EconData, f: Faction, ships: f64, z: usize) -> f64 {
    if ships <= 0.0 || f >= w.nations {
        return 0.0;
    }
    let n = &e.nations[f];
    let fuel = if n.stock[ed.fuel] > ships * w.data.sea.ship_fuel_per_day { 1.0 } else { 0.3 };
    let storm = w.geo.zone_coast[z].first().map_or(false, |&p| w.weather[p] == 3);
    let weather = if storm { w.data.weather.storm_sea } else { 1.0 };
    let training = w.training_cap(e, f);
    ships * (0.5 + training) * fuel * weather * (1.0 - n.stockpile_faulty[ed.warships] * 0.5)
}

/// A day at sea: fleets take station, fight for their zones, blockade and
/// carry invasions.
pub fn day(w: &mut Wars, e: &mut Economy, ed: &EconData) {
    let zones = w.geo.zone_links.len();
    station(w, e, ed);
    // Each zone: who is there, how strong.
    let mut present: Vec<Vec<(Faction, f64)>> = vec![Vec::new(); zones];
    for f in 0..w.nations {
        for &(z, ships) in &w.forces[f].fleets {
            if ships > 0.0 && z < zones {
                let p = naval_power(w, e, ed, f, ships, z);
                present[z].push((f, p));
            }
        }
    }
    let mut control: Vec<Option<Faction>> = vec![None; zones];
    let mut contested = vec![false; zones];
    for z in 0..zones {
        let here = &present[z];
        if here.is_empty() {
            continue;
        }
        // Battles between enemies: Lanchester at sea.
        let mut losses: Vec<f64> = vec![0.0; here.len()];
        for (a, &(fa, pa)) in here.iter().enumerate() {
            for (b, &(fb, _)) in here.iter().enumerate() {
                if a != b && w.at_war(fa, fb).is_some() && pa > 0.0 {
                    let luck = (0.25 * normal(&mut w.rng)).exp();
                    losses[b] += pa * w.data.sea.sea_kill * luck / (0.5 + w.training_cap(e, fb));
                }
            }
        }
        for (k, &(f, _)) in here.iter().enumerate() {
            if losses[k] <= 0.0 {
                continue;
            }
            let fleet = w.forces[f].fleets.iter_mut().find(|x| x.0 == z).unwrap();
            let sunk = super::whole(losses[k].min(fleet.1), &mut w.rng).min(fleet.1);
            fleet.1 -= sunk;
            e.nations[f].stockpile[ed.warships] = (e.nations[f].stockpile[ed.warships] - sunk).max(0.0);
            w.forces[f].flows.used[ed.warships] += sunk;
            for war in w.wars.iter_mut().filter(|x| x.active()) {
                if let Some(s) = war.side_of(f) {
                    war.losses[s].warships += sunk;
                }
            }
            if sunk >= 1.0 {
                let (who, n) = (w.names[f].clone(), sunk);
                w.say(vec![f], format!("{who} loses {n:.0} warships at sea"));
            }
        }
        // Control: the strongest, by `control_ratio` over every enemy of it.
        let mut best: Option<(Faction, f64)> = None;
        for &(f, p) in here {
            if best.is_none_or(|(_, bp)| p > bp) {
                best = Some((f, p));
            }
        }
        if let Some((f, p)) = best {
            let rival: f64 = here.iter().filter(|&&(g, _)| w.at_war(f, g).is_some()).map(|x| x.1).fold(0.0, f64::max);
            if p > 0.0 && p >= w.data.sea.control_ratio * rival {
                control[z] = Some(f);
            } else if rival > 0.0 {
                contested[z] = true;
            }
        }
    }
    w.zone_control = control;
    w.zone_contested = contested;
    w.zone_fleets = present;
    blockades(w, e);
    for f in 0..w.nations {
        w.forces[f].fleets.retain(|x| x.1 > 0.0);
    }
}

/// Each navy chooses its zones: at war, the enemy's coasts and its own;
/// in peace, home waters. Landlocked nations have no navy.
fn station(w: &mut Wars, e: &mut Economy, ed: &EconData) {
    for f in 0..w.nations {
        let ships = e.nations[f].stockpile[ed.warships];
        if e.nations[f].landlocked || ships < 1.0 {
            w.forces[f].fleets.clear();
            continue;
        }
        // Fuel for the fleet.
        let n = &mut e.nations[f];
        let fuel = (ships * w.data.sea.ship_fuel_per_day * if w.in_any_war(f) { 1.0 } else { 0.2 }).min(n.stock[ed.fuel]);
        n.stock[ed.fuel] -= fuel;
        n.ledger.consumed[ed.fuel] += fuel;
        let cost = fuel * n.price[ed.fuel];
        n.treasury -= cost;
        n.budget_month.army += cost;
        let home: Vec<usize> = e.nations[f].provinces.iter().flat_map(|&q| w.geo.zones_of[q].iter().copied()).collect();
        let Some(&home_zone) = w.geo.zones_of[w.capital[f]].first().or(home.first()) else {
            w.forces[f].fleets.clear();
            continue;
        };
        let enemies = w.enemies(f);
        let mut targets: Vec<usize> = Vec::new();
        if !enemies.is_empty() {
            // The zones off enemy coasts: those that touch our own first
            // (they guard our trade), then the rest (a blockade).
            let mut theirs: Vec<(bool, usize)> = Vec::new();
            for &g in &enemies {
                for q in w.held(g) {
                    for &z in &w.geo.zones_of[q] {
                        if !theirs.iter().any(|x| x.1 == z) {
                            theirs.push((!home.contains(&z), z));
                        }
                    }
                }
            }
            theirs.sort();
            targets = theirs.into_iter().map(|x| x.1).collect();
        }
        if targets.is_empty() {
            targets.push(home_zone);
        }
        // A squadron needs a few ships: at most one zone per 4 warships.
        targets.truncate(((ships / 4.0).floor() as usize).max(1));
        let per = (ships / targets.len() as f64).floor();
        let mut fleets: Vec<(usize, f64)> = targets.iter().map(|&z| (z, per)).collect();
        fleets[0].1 += ships - per * targets.len() as f64;
        w.forces[f].fleets = fleets;
    }
}

/// A nation whose coastal zones the enemy holds loses its sea trade.
fn blockades(w: &mut Wars, e: &mut Economy) {
    for i in 0..w.nations {
        let n = &e.nations[i];
        if n.landlocked {
            e.nations[i].blockade = 0.0;
            w.forces[i].blockaded = 0.0;
            continue;
        }
        let mut total = 0usize;
        let mut held = 0usize;
        for &q in &n.provinces {
            for &z in &w.geo.zones_of[q] {
                total += 1;
                if hostile(w, i, z) {
                    held += 1;
                }
            }
        }
        let share = if total > 0 { held as f64 / total as f64 } else { 0.0 };
        let cut = if share >= 0.999 { w.data.sea.blockade_cut } else { share * w.data.sea.blockade_cut };
        if (cut - e.nations[i].blockade).abs() > 0.01 {
            e.nations[i].blockade = cut;
            if cut >= w.data.sea.blockade_cut - 1e-9 {
                let who = w.names[i].clone();
                w.say(vec![i], format!("{who} is blockaded: its ports are closed"));
            }
        }
        w.forces[i].blockaded = cut;
    }
}

/// Transports free to carry troops (each carries `transport_soldiers`).
pub fn transports(w: &Wars, e: &Economy, ed: &EconData, f: Faction) -> f64 {
    if f >= w.nations || e.nations[f].landlocked {
        return 0.0;
    }
    (e.nations[f].stockpile[ed.transports] - w.forces[f].transports_busy).max(0.0)
}

/// Whether `f` can sail from its port `from` to the coast of `to` (no
/// enemy-held zone on the way), and in how many hours.
pub fn crossing(w: &Wars, f: Faction, from: usize, to: usize) -> Option<f64> {
    sea_links(w, f, from).iter().find(|x| x.0 == to).map(|x| {
        let hops = (x.1.ln() / 0.99f64.ln()).abs().round().max(1.0);
        24.0 * (0.5 + 0.5 * hops)
    })
}
