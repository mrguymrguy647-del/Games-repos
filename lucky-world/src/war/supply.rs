//! Supply and manpower (docs/WAR.md, sections 2 and 5): hospitals,
//! recruits, food and fuel, desertion, supply lines from the capital, and
//! the generals' requests (the player's job).

use super::{whole, Division, Faction, Kind, Request, WarData, Wars, FRONTS};
use crate::econ::{EconData, Economy};

/// The rounds a division wants to carry: `ammo_days` battle days.
pub fn ammo_target(wd: &WarData, d: &Division) -> f64 {
    let c = &wd.combat;
    let rps = c.rounds_per_ammo / c.shells_per_ammo;
    let per_hour = d.soldiers * c.rounds_per_hour + (d.soldiers * c.shells_per_soldier_hour + d.tanks * c.shells_per_tank_hour) * rps;
    per_hour * 24.0 * wd.supply.ammo_days
}

/// The supply line from a faction's capital: for each province, the share
/// of supplies that survive the way there, and the weakest road + rail on it.
pub fn lines(w: &Wars, e: &Economy, f: Faction) -> Vec<Option<(f64, f64)>> {
    let s = &w.data.supply;
    let friends = w.friends(f);
    let Some(start) = w.base(e, f) else { return vec![None; w.control.len()] };
    let ok = |q: usize| w.control[q].is_some_and(|c| friends.contains(&c));
    if !ok(start) {
        return vec![None; w.control.len()];
    }
    let mut out: Vec<Option<(f64, f64)>> = vec![None; w.control.len()];
    let roads = |q: usize| e.provinces[q].road + e.provinces[q].rail;
    out[start] = Some((1.0, roads(start)));
    let mut queue = std::collections::VecDeque::from([start]);
    while let Some(q) = queue.pop_front() {
        let (surv, weak) = out[q].unwrap();
        let mut next: Vec<(usize, f64)> = w.geo.neighbours[q].iter().map(|&r| (r, 1.0)).collect();
        // By sea: from a port to another friendly port across safe zones (layer 5b).
        if w.geo.coastal[q] && e.provinces[q].port >= 1.0 {
            for (r, k) in super::sea::sea_links(w, f, q) {
                next.push((r, k));
            }
        }
        for (r, k) in next {
            if out[r].is_some() || !ok(r) {
                continue;
            }
            let hop = (s.hop_base + s.hop_roads * roads(r)).min(s.hop_max) * (0.95 + 0.05 * e.provinces[r].transport.clamp(0.0, 1.0)) * k;
            out[r] = Some((surv * hop, weak.min(roads(r))));
            queue.push_back(r);
        }
    }
    out
}

/// A day of supply and manpower.
pub fn day(w: &mut Wars, e: &mut Economy, ed: &EconData) {
    hospitals(w, e);
    for f in 0..w.forces.len() {
        manpower(w, e, ed, f);
    }
    for f in 0..w.forces.len() {
        if w.in_any_war(f) {
            upkeep(w, e, ed, f);
            requests(w, f);
        }
        deliver(w, e, ed, f);
    }
    desertion(w, e, ed);
    disband(w, e, ed);
}

/// The wounded heal, die or go home.
fn hospitals(w: &mut Wars, e: &mut Economy) {
    let h = w.data.hospital.clone();
    for f in 0..w.nations {
        let out = w.forces[f].hospital / h.heal_days;
        if out < 1.0 {
            continue;
        }
        let back = (out * h.back).floor();
        let die = (out * h.die).floor();
        let home = (out - back - die).max(0.0).floor();
        w.forces[f].hospital -= back + die + home;
        w.forces[f].dead += die;
        e.nations[f].population = (e.nations[f].population - die).max(1.0);
        join(w, f, back);
    }
}

/// Soldiers join a faction's weakest divisions (back from hospital, or recruits).
fn join(w: &mut Wars, f: Faction, mut soldiers: f64) -> f64 {
    let size = w.data.army.division_size;
    let recruit = w.data.army.recruit_training;
    let mut order: Vec<usize> = (0..w.divisions.len()).filter(|&d| w.divisions[d].alive && w.divisions[d].faction == f && w.divisions[d].soldiers < size).collect();
    order.sort_by(|&a, &b| w.divisions[a].soldiers.total_cmp(&w.divisions[b].soldiers).then(a.cmp(&b)));
    for d in order {
        if soldiers < 1.0 {
            break;
        }
        let dv = &mut w.divisions[d];
        let add = (size - dv.soldiers).min(soldiers).floor();
        dv.training = (dv.training * dv.soldiers + recruit * add) / (dv.soldiers + add).max(1.0);
        dv.soldiers += add;
        dv.tally.joined += add;
        soldiers -= add;
    }
    soldiers
}

/// Raises recruits towards the war's mobilisation, or sends soldiers home in peace.
fn manpower(w: &mut Wars, e: &mut Economy, ed: &EconData, f: Faction) {
    if f >= w.nations {
        return;
    }
    let a = w.data.army.clone();
    let level = w.level_of(f);
    let n = &e.nations[f];
    let target = (n.population * n.duty * w.data.levels.mobilisation[level]).min(n.population * a.max_share);
    let (have, _) = w.strength(f);
    let have = have + w.forces[f].hospital;
    if have < target - 1.0 {
        let add = (target - have).min(a.recruit_per_day * target).max(0.0).floor();
        let left = join(w, f, add);
        if left >= 0.5 * a.division_size {
            w.raise(e, ed, f, left, false);
        }
    } else if have > target * 1.05 + a.division_size && !w.in_any_war(f) {
        // Peace: the extra soldiers go home, biggest divisions first.
        let mut release = (have - target).min(a.recruit_per_day * 2.0 * target).floor();
        let mut order: Vec<usize> = (0..w.divisions.len()).filter(|&d| w.divisions[d].alive && w.divisions[d].faction == f).collect();
        order.sort_by(|&x, &y| w.divisions[y].soldiers.total_cmp(&w.divisions[x].soldiers).then(x.cmp(&y)));
        for d in order {
            if release < 1.0 {
                break;
            }
            let dv = &mut w.divisions[d];
            let r = (dv.soldiers * 0.5).min(release).floor();
            dv.soldiers -= r;
            dv.tally.released += r;
            release -= r;
        }
    }
}

/// A day at war: food, fuel to idle and move, spare parts; hunger kills.
fn upkeep(w: &mut Wars, e: &mut Economy, ed: &EconData, f: Faction) {
    let s = w.data.supply.clone();
    let mut parts = 0.0;
    for d in 0..w.divisions.len() {
        if !w.divisions[d].alive || w.divisions[d].faction != f {
            continue;
        }
        let eaten = w.divisions[d].soldiers;
        let tanks = w.divisions[d].tanks;
        let dv = &mut w.divisions[d];
        dv.food -= eaten;
        let fuel = (tanks * s.fuel_tank_day).min(dv.fuel);
        dv.fuel -= fuel;
        parts += tanks * s.parts_tank_day;
        if dv.food < 0.0 {
            dv.food = 0.0;
            let starved = whole(dv.soldiers * w.data.morale.starving_deaths, &mut w.rng);
            dv.morale = (dv.morale - 0.02).max(0.0);
            super::land::casualties(w, e, ed, d, starved, 0.0, 0.0, 0.0, None);
        }
        // Repairs: a broken tank takes `repair_days`.
        let dv = &mut w.divisions[d];
        dv.tanks_broken = (dv.tanks_broken * (1.0 - 1.0 / w.data.combat.repair_days)).max(0.0);
    }
    // Spare parts come out of the steel the nation holds.
    if f < w.nations {
        let n = &mut e.nations[f];
        let q = parts.min(n.stock[ed.steel]);
        n.stock[ed.steel] -= q;
        n.ledger.consumed[ed.steel] += q;
        let cost = q * n.price[ed.steel];
        n.treasury -= cost;
        n.budget_month.army += cost;
    }
}

/// The generals ask, each front once every `request_days`.
fn requests(w: &mut Wars, f: Faction) {
    let s = w.data.supply.clone();
    if w.day % s.request_days != (f as u32) % s.request_days {
        return;
    }
    let mut need = [[0.0f64; 3]; 5];
    let mut fuel_rate = [0.0f64; 5];
    for d in w.divisions.iter().filter(|d| d.alive && d.faction == f) {
        let fr = w.front_of(f, d.province);
        if d.kind == Kind::Armoured {
            need[fr][0] += (w.data.army.armour_tanks - d.tanks).max(0.0);
            need[fr][2] += (d.tanks * s.fuel_tank_day * s.fuel_days - d.fuel).max(0.0);
            fuel_rate[fr] += d.tanks * s.fuel_tank_day;
        }
        need[fr][1] += (d.soldiers - d.rifles).max(0.0);
    }
    let auto = w.forces[f].auto || f != 0;
    for fr in 0..5 {
        let owed = w.forces[f].granted[fr];
        let ask = [(need[fr][0] - owed[0]).max(0.0).floor(), (need[fr][1] - owed[1]).max(0.0).floor(), (need[fr][2] - owed[2]).max(0.0)];
        if ask[0] < 1.0 && ask[1] < 100.0 && ask[2] < fuel_rate[fr] * 3.0 {
            continue;
        }
        let r = Request { front: fr, day: w.day, tanks: ask[0], rifles: ask[1], fuel: ask[2], fuel_weeks: if fuel_rate[fr] > 0.0 { ask[2] / (fuel_rate[fr] * 7.0) } else { 0.0 }, jets: 0.0, answered: None };
        w.forces[f].requests.retain(|x| !(x.front == fr && x.answered.is_none()));
        w.forces[f].requests.push(r);
        if auto {
            let k = w.forces[f].requests.len() - 1;
            answer(w, f, k, w.forces[f].priority[fr] > 0);
        }
    }
    // Unanswered requests: the general grumbles, and his soldiers lose heart.
    let day = w.day;
    let late: Vec<usize> = w.forces[f].requests.iter().filter(|r| r.answered.is_none() && day >= r.day + s.request_wait).map(|r| r.front).collect();
    for fr in late {
        for d in w.divisions.iter_mut().filter(|d| d.alive && d.faction == f) {
            if w.capital.len() > 0 {
                let _ = d;
            }
        }
        for k in 0..w.divisions.len() {
            if w.divisions[k].alive && w.divisions[k].faction == f && w.front_of(f, w.divisions[k].province) == fr {
                w.divisions[k].morale = (w.divisions[k].morale - 0.002).max(0.0);
            }
        }
    }
    if w.forces[f].requests.len() > 40 {
        w.forces[f].requests.drain(..20);
    }
}

/// Approve (or deny) a request: approved, it is owed to its front.
pub fn answer(w: &mut Wars, f: Faction, k: usize, yes: bool) {
    let r = &mut w.forces[f].requests[k];
    if r.answered.is_some() {
        return;
    }
    r.answered = Some(yes);
    if yes {
        let (fr, t, ri, fu) = (r.front, r.tanks, r.rifles, r.fuel);
        let g = &mut w.forces[f].granted[fr];
        g[0] += t;
        g[1] += ri;
        g[2] += fu;
    }
}

/// Sends supplies from the stockpile (or stores) to the divisions that can be reached.
fn deliver(w: &mut Wars, e: &mut Economy, ed: &EconData, f: Faction) {
    let at_war = w.in_any_war(f);
    let s = w.data.supply.clone();
    let ours: Vec<usize> = (0..w.divisions.len()).filter(|&d| w.divisions[d].alive && w.divisions[d].faction == f && w.divisions[d].moving.is_none_or(|m| !m.2)).collect();
    if ours.is_empty() {
        return;
    }
    // In peace only recruits' rifles go out (the economy feeds and trains the army).
    if !at_war && ours.iter().all(|&d| w.divisions[d].rifles >= w.divisions[d].soldiers) {
        return;
    }
    let line = lines(w, e, f);
    let rpa = w.data.combat.rounds_per_ammo;
    // What each division wants, by good: [ammo units, food units, fuel t, rifles, tanks].
    let mut want: Vec<[f64; 5]> = Vec::with_capacity(ours.len());
    for &d in &ours {
        let dv = &w.divisions[d];
        let fr = w.front_of(f, dv.province);
        let granted = w.forces[f].granted[fr];
        let mut x = [0.0; 5];
        x[3] = (dv.soldiers - dv.rifles).max(0.0).min(if at_war { granted[1].max(0.0) } else { f64::MAX });
        if at_war {
            x[0] = ((ammo_target(&w.data, dv) - dv.ammo) / rpa).max(0.0);
            x[1] = ((dv.soldiers * s.food_days - dv.food) * s.food_per_soldier).max(0.0);
            let routine = dv.tanks * s.fuel_tank_day * 3.0;
            let full = dv.tanks * s.fuel_tank_day * s.fuel_days;
            x[2] = (routine - dv.fuel).max(0.0) + ((full - dv.fuel.max(routine)).max(0.0)).min(granted[2].max(0.0));
            if dv.kind == Kind::Armoured {
                x[4] = (w.data.army.armour_tanks - dv.tanks).max(0.0).min(granted[0].max(0.0)).floor();
            }
        }
        want.push(x);
    }
    // What there is: the stockpile (weapons) and the stocks (food, fuel).
    let goods = [ed.ammo, ed.food, ed.fuel, ed.rifles, ed.tanks];
    let military = [true, false, false, true, true];
    let avail: Vec<f64> = goods
        .iter()
        .zip(military.iter())
        .map(|(&g, &m)| if f < w.nations { if m { e.nations[f].stockpile[g] } else { e.nations[f].stock[g] } } else { w.forces[f].stores[g] })
        .collect();
    let weight = |d: usize| [0.0, 0.5, 1.0, 2.0][w.forces[f].priority[w.front_of(f, w.divisions[d].province)].min(3) as usize];
    for k in 0..5 {
        let total: f64 = ours.iter().enumerate().map(|(i, &d)| want[i][k] * weight(d)).sum();
        if total <= 0.0 {
            continue;
        }
        let scale = (avail[k] / total).min(1.0);
        for (i, &d) in ours.iter().enumerate() {
            want[i][k] *= (scale * weight(d)).min(1.0);
        }
    }
    // Ship it: capacity and losses on the way.
    for (i, &d) in ours.iter().enumerate() {
        let p = w.divisions[d].province;
        let Some((surv, weak)) = line[p] else { continue };
        let fr = w.front_of(f, p);
        let interdict = (w.data.supply.interdiction * (-w.forces[f].air[fr]).max(0.0)).min(0.9);
        let cap = s.capacity * (1.0 + s.capacity_roads * weak);
        let tonnes = want[i][0] * ed.goods[ed.ammo].weight + want[i][1] * ed.goods[ed.food].weight + want[i][2] * ed.goods[ed.fuel].weight + want[i][3] * ed.goods[ed.rifles].weight + want[i][4] * ed.goods[ed.tanks].weight;
        let fit = if tonnes > cap { cap / tonnes } else { 1.0 };
        let arrive = surv * (1.0 - interdict);
        for k in 0..5 {
            let mut sent = want[i][k] * fit;
            if k >= 3 {
                sent = sent.floor();
            }
            if sent <= 0.0 {
                continue;
            }
            let g = goods[k];
            let faulty = if military[k] {
                let (q, fq) = w.take(e, ed, f, g, sent);
                sent = q;
                fq
            } else if f < w.nations {
                let n = &mut e.nations[f];
                let q = sent.min(n.stock[g]);
                n.stock[g] -= q;
                n.ledger.consumed[g] += q;
                let cost = q * n.price[g];
                n.treasury -= cost;
                n.budget_month.army += cost;
                sent = q;
                0.0
            } else {
                let q = sent.min(w.forces[f].stores[g]);
                w.forces[f].stores[g] -= q;
                sent = q;
                0.0
            };
            let got = if k >= 3 { (sent * arrive).floor() } else { sent * arrive };
            w.forces[f].flows.lost[g] += sent - got;
            let fr_g = &mut w.forces[f].granted[fr];
            match k {
                3 => fr_g[1] = (fr_g[1] - got).max(0.0),
                4 => fr_g[0] = (fr_g[0] - got).max(0.0),
                2 => fr_g[2] = (fr_g[2] - got).max(0.0),
                _ => {}
            }
            let dv = &mut w.divisions[d];
            match k {
                0 => {
                    dv.ammo_faulty = crate::econ::mix_faulty(dv.ammo_faulty, dv.ammo / rpa, faulty, got);
                    dv.ammo += got * rpa;
                }
                1 => dv.food += got / s.food_per_soldier,
                2 => dv.fuel += got,
                3 => {
                    dv.rifles_faulty = crate::econ::mix_faulty(dv.rifles_faulty, dv.rifles, faulty, got);
                    dv.rifles += got;
                }
                _ => {
                    dv.tanks_faulty = crate::econ::mix_faulty(dv.tanks_faulty, dv.tanks, faulty, got);
                    dv.tanks += got;
                }
            }
        }
    }
    let _ = FRONTS;
}

/// Low morale: soldiers slip away home.
fn desertion(w: &mut Wars, e: &mut Economy, ed: &EconData) {
    let m = w.data.morale.clone();
    for d in 0..w.divisions.len() {
        let dv = &w.divisions[d];
        if !dv.alive || dv.morale >= m.desert_below {
            continue;
        }
        let rate = m.desertion * (m.desert_below - dv.morale) / m.desert_below;
        let gone = whole(dv.soldiers * rate, &mut w.rng).min(dv.soldiers);
        super::land::casualties(w, e, ed, d, 0.0, 0.0, 0.0, gone, None);
    }
}

/// Divisions too small to fight are disbanded: their soldiers go home and
/// their weapons back into store.
fn disband(w: &mut Wars, e: &mut Economy, ed: &EconData) {
    for d in 0..w.divisions.len() {
        let dv = &w.divisions[d];
        if !dv.alive || dv.battle.is_some() || dv.moving.is_some() || dv.soldiers >= 300.0 {
            continue;
        }
        let f = dv.faction;
        let (rifles, rf, tanks, tf, ammo, af) = (dv.rifles, dv.rifles_faulty, dv.tanks, dv.tanks_faulty, dv.ammo / w.data.combat.rounds_per_ammo, dv.ammo_faulty);
        for (g, q, fq) in [(ed.rifles, rifles, rf), (ed.tanks, tanks, tf), (ed.ammo, ammo, af)] {
            w.store(e, f, g, q, fq);
            w.forces[f].flows.returned[g] += q;
        }
        let dv = &mut w.divisions[d];
        dv.tally.released += dv.soldiers;
        dv.soldiers = 0.0;
        dv.rifles = 0.0;
        dv.tanks = 0.0;
        dv.ammo = 0.0;
        dv.alive = false;
        let general = dv.general;
        w.generals[general].alive = false;
    }
}
