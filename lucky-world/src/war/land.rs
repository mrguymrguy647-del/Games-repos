//! Land war (docs/WAR.md, sections 3 and 4): marching, the hourly battle,
//! breaking, retreat and surrender, occupation, and the generals' AI.

use super::{hash3, normal, whole, Battle, BattleReport, Faction, Kind, Order, SideReport, Style, Wars};
use crate::econ::{EconData, Economy};

/// Days to march into province `r`.
pub fn move_hours(w: &Wars, e: &Economy, r: usize) -> f64 {
    let p = &e.provinces[r];
    24.0 * w.geo.terrain[r].move_days / (1.0 + 0.15 * (p.road + p.rail))
}

/// Whether faction `f` may take province `r` (its holder is at war with
/// `f`, and the war's level allows it).
pub fn may_take(w: &Wars, f: Faction, r: usize) -> bool {
    let Some(c) = w.control[r] else { return false };
    let Some(k) = w.at_war(f, c) else { return false };
    let war = &w.wars[k];
    match war.level {
        0 => false,
        1 => true, // a skirmish fights over the border, but takes nothing (finish_battle)
        // A military operation takes only its target provinces.
        2 => match &war.goal {
            super::Goal::Provinces(ps) => ps.contains(&r),
            _ => true,
        },
        _ => true,
    }
}

/// An hour: marches go on, battles are fought.
pub fn hour(w: &mut Wars, e: &mut Economy, ed: &EconData) {
    for d in 0..w.divisions.len() {
        if !w.divisions[d].alive {
            continue;
        }
        if w.divisions[d].landing > 0.0 {
            w.divisions[d].landing -= 1.0;
        }
        if let Some((to, left, by_sea)) = w.divisions[d].moving {
            if left > 1.0 {
                w.divisions[d].moving = Some((to, left - 1.0, by_sea));
            } else {
                w.divisions[d].moving = None;
                arrive(w, e, ed, d, to, by_sea);
            }
        }
    }
    // Who may be in each battle: its attackers, and everyone in its province.
    // (The lists are reused hour to hour, so nothing is allocated.)
    let mut lists = std::mem::take(&mut w.in_battle);
    lists.resize_with(w.battles.len(), Vec::new);
    lists.iter_mut().for_each(|l| l.clear());
    let mut at = std::mem::take(&mut w.battle_at);
    at.clear();
    at.resize(w.control.len(), usize::MAX);
    // (A province can have more than one battle: the second and later go in `more`.)
    let mut more: Vec<(usize, usize)> = Vec::new();
    for (b, bt) in w.battles.iter().enumerate() {
        if !bt.over {
            if at[bt.province] == usize::MAX {
                at[bt.province] = b;
            } else {
                more.push((bt.province, b));
            }
        }
    }
    for (d, dv) in w.divisions.iter().enumerate() {
        if !dv.alive {
            continue;
        }
        let mine = dv.battle.filter(|&b| b < lists.len());
        if let Some(b) = mine {
            lists[b].push(d);
        }
        let b = at[dv.province];
        if b != usize::MAX {
            if mine != Some(b) {
                lists[b].push(d);
            }
            for &(p, b2) in &more {
                if p == dv.province && mine != Some(b2) {
                    lists[b2].push(d);
                }
            }
        }
    }
    w.battle_at = at;
    w.in_battle = lists;
    w.in_battle_live = true;
    for b in 0..w.battles.len() {
        if !w.battles[b].over {
            battle_hour(w, e, ed, b);
        }
    }
    for l in w.in_battle.iter_mut() {
        l.clear();
    }
    w.in_battle_live = false;
}

/// A division reaches a province: it moves in, takes it, or fights for it.
pub fn arrive(w: &mut Wars, e: &mut Economy, _ed: &EconData, d: usize, to: usize, by_sea: bool) {
    let f = w.divisions[d].faction;
    if by_sea {
        let need = transports_for(w, &w.divisions[d]);
        w.forces[f].transports_busy = (w.forces[f].transports_busy - need).max(0.0);
    }
    let enemies_there: Vec<usize> = (0..w.divisions.len())
        .filter(|&x| w.divisions[x].alive && w.divisions[x].province == to && w.divisions[x].moving.is_none() && w.at_war(f, w.divisions[x].faction).is_some())
        .collect();
    let holder = w.control[to];
    let friendly = holder.is_none_or(|c| w.friends(f).contains(&c));
    if enemies_there.is_empty() {
        if friendly {
            let dv = &mut w.divisions[d];
            dv.province = to;
            dv.entrench = 0.0;
            return;
        }
        if may_take(w, f, to) && w.level_between(f, holder.unwrap()) >= 2 {
            // Nobody defends it: taken without a fight.
            let name = w.geo.names[to].clone();
            let who = w.names[f].clone();
            let from = holder.map_or(String::new(), |c| w.names[c].clone());
            w.set_control(e, to, Some(f));
            w.divisions[d].province = to;
            w.divisions[d].entrench = 0.0;
            w.say(vec![f], format!("{who} takes {name} from {from} unopposed"));
            return;
        }
        return; // not allowed in: it stays where it was (by sea: still in port)
    }
    // A battle: join the one under way here, or start one.
    let war = w.at_war(f, w.divisions[enemies_there[0]].faction).unwrap();
    if by_sea {
        w.divisions[d].landing = 48.0;
    }
    if let Some(b) = w.battles.iter().position(|b| !b.over && b.province == to) {
        if w.battles[b].sides[0].contains(&f) {
            w.battles[b].attackers.push(w.divisions[d].id);
            let from = w.divisions[d].province;
            w.battles[b].from.push(from);
            w.divisions[d].battle = Some(b);
            add_to_report(w, b, 0, d);
        }
        return;
    }
    let sides = [w.wars[war].sides[w.wars[war].side_of(f).unwrap()].clone(), w.wars[war].sides[1 - w.wars[war].side_of(f).unwrap()].clone()];
    let id = w.battles.len();
    let terrain = w.geo.terrain_name[to].clone();
    w.battles.push(Battle {
        id,
        province: to,
        war,
        sides,
        attackers: vec![w.divisions[d].id],
        from: vec![w.divisions[d].province],
        over: false,
        report: BattleReport {
            province: to,
            province_name: w.geo.names[to].clone(),
            war,
            day_start: w.day,
            day_end: w.day,
            hours: 0,
            terrain,
            weather: Vec::new(),
            attacker: SideReport::default(),
            defender: SideReport::default(),
            winner: 1,
        },
    });
    w.divisions[d].battle = Some(id);
    add_to_report(w, id, 0, d);
    for x in enemies_there {
        w.divisions[x].battle = Some(id);
        add_to_report(w, id, 1, x);
    }
    let (a, dname) = (w.names[f].clone(), w.geo.names[to].clone());
    w.say(w.battles[id].sides[0].iter().chain(w.battles[id].sides[1].iter()).copied().collect(), format!("Battle of {dname}: {a} attacks"));
}

fn add_to_report(w: &mut Wars, b: usize, side: usize, d: usize) {
    let dv = &w.divisions[d];
    let general = w.generals[dv.general].name.clone();
    let r = if side == 0 { &mut w.battles[b].report.attacker } else { &mut w.battles[b].report.defender };
    if !r.factions.contains(&dv.faction) {
        r.factions.push(dv.faction);
    }
    r.divisions.push(dv.name.clone());
    r.generals.push(general);
    r.soldiers_start += dv.soldiers;
    r.tanks_start += dv.tanks;
}

/// A division's equipped share (docs/WAR.md, section 2).
pub fn equipped(w: &Wars, d: &super::Division) -> f64 {
    let s = &w.data.supply;
    if d.soldiers <= 0.0 {
        return 0.0;
    }
    let rifles = (d.rifles / d.soldiers).min(1.0);
    let ammo = (d.ammo / super::supply::ammo_target(&w.data, d).max(1.0)).min(1.0);
    let food = (d.food / (d.soldiers * s.food_days)).min(1.0);
    let armour = if d.kind == Kind::Armoured {
        let tanks = ((d.tanks - d.tanks_broken) / w.data.army.armour_tanks).clamp(0.0, 1.0);
        let fuel = (d.fuel / (d.tanks * s.fuel_tank_day * s.fuel_days).max(1.0)).min(1.0);
        tanks * fuel
    } else {
        1.0
    };
    0.45 * rifles + 0.25 * ammo + 0.15 * food + 0.15 * armour
}

/// The divisions fighting on each side of a battle now.
fn sides_now(w: &Wars, b: usize) -> [Vec<usize>; 2] {
    let bt = &w.battles[b];
    // In the hour's loop only the battle's own list is looked at.
    let all: Vec<usize>;
    let pool: &[usize] = match w.in_battle.get(b).filter(|_| w.in_battle_live) {
        Some(l) => l,
        None => {
            all = (0..w.divisions.len()).collect();
            &all
        }
    };
    let a: Vec<usize> = pool.iter().copied().filter(|&x| w.divisions[x].alive && w.divisions[x].battle == Some(b) && bt.attackers.contains(&w.divisions[x].id)).collect();
    let d: Vec<usize> = pool
        .iter()
        .copied()
        .filter(|&x| {
            let dv = &w.divisions[x];
            dv.alive && dv.province == bt.province && dv.moving.is_none() && bt.sides[1].contains(&dv.faction) && !bt.attackers.contains(&dv.id)
        })
        .collect();
    [a, d]
}

/// One hour of a battle.
fn battle_hour(w: &mut Wars, e: &mut Economy, ed: &EconData, b: usize) {
    let sides = sides_now(w, b);
    for &x in &sides[1] {
        if w.divisions[x].battle != Some(b) {
            w.divisions[x].battle = Some(b);
            add_to_report(w, b, 1, x);
        }
    }
    if sides[0].is_empty() || sides[1].is_empty() {
        let winner = if sides[1].is_empty() && !sides[0].is_empty() { 0 } else { 1 };
        end_battle(w, e, ed, b, winner);
        return;
    }
    let c = w.data.combat.clone();
    let p = w.battles[b].province;
    let rps = c.rounds_per_ammo / c.shells_per_ammo; // rounds a shell is worth
    let terrain = w.geo.terrain[p];
    let city = w.geo.city[p];
    let weather = w.weather[p];
    let wfire = match weather {
        1 => w.data.weather.rain_fire,
        2 => w.data.weather.snow_fire,
        3 => w.data.weather.storm_fire,
        _ => 1.0,
    };
    if w.battles[b].report.hours % 24 == 0 {
        let name = Wars::weather_name(weather).to_string();
        w.battles[b].report.weather.push(name);
    }
    let frontage = c.frontage_base * w.geo.hexes[p].sqrt() * terrain.frontage;
    let river = w.geo.river[p];
    let tank_terrain = terrain.tanks * if city { c.city_tanks } else { 1.0 };
    // Fire: each side's hits on the other.
    let mut hits = [0.0f64; 2];
    let mut tank_hits = [0.0f64; 2];
    let mut engaged_of: [Vec<f64>; 2] = [Vec::new(), Vec::new()];
    let totals: [f64; 2] = [0, 1].map(|s| sides[s].iter().map(|&x| w.divisions[x].soldiers).sum::<f64>());
    // An attacker that outnumbers the defenders spreads round their flanks:
    // its line is wider, up to `flank_max` (the defenders' can't be).
    let ratio = totals[0] / totals[1].max(1.0);
    let flank = if ratio > 1.0 { (1.0 + c.flank * ratio.ln()).min(c.flank_max) } else { 1.0 };
    for s in 0..2 {
        let total = totals[s];
        let line = if s == 0 { frontage * flank } else { frontage };
        let share = (line / total.max(1.0)).min(1.0);
        let luck = (c.luck_sd * normal(&mut w.rng)).exp();
        let faction = w.divisions[sides[s][0]].faction;
        let air = 1.0 + w.data.air.support * w.air_over(faction, p).max(0.0) - w.data.air.support_enemy * (-w.air_over(faction, p)).max(0.0);
        let (mut h, mut th) = (0.0, 0.0);
        for &x in &sides[s] {
            let eq = equipped(w, &w.divisions[x]);
            let dv = &w.divisions[x];
            let engaged = dv.soldiers * share;
            engaged_of[s].push(engaged);
            let stance = if s == 0 { 1.0 } else { c.defend_stance };
            let armed = if dv.soldiers > 0.0 { (dv.rifles / dv.soldiers).min(1.0) } else { 0.0 };
            let shooters = engaged * (armed + (1.0 - armed) * c.no_rifle_fire);
            let tanks_up = (dv.tanks - dv.tanks_broken).max(0.0) * share;
            let tanks_fuelled = if dv.fuel > tanks_up * w.data.supply.fuel_tank_hour { tanks_up } else { (dv.fuel / w.data.supply.fuel_tank_hour.max(1e-9)).min(tanks_up) };
            let jam_share = (dv.rifles_faulty * c.jam_per_hour).min(1.0);
            let mut rounds = shooters * c.rounds_per_hour * stance * (1.0 - jam_share);
            let jams = whole(shooters * armed * jam_share, &mut w.rng);
            let mut shells = engaged * c.shells_per_soldier_hour * stance;
            let mut tank_shells = tanks_fuelled * c.shells_per_tank_hour;
            let need = rounds + (shells + tank_shells) * rps;
            let mut out_of_ammo = 0.0;
            if need > dv.ammo {
                let k = if need > 0.0 { dv.ammo / need } else { 0.0 };
                out_of_ammo = 1.0 - k;
                rounds *= k;
                shells *= k;
                tank_shells *= k;
            }
            let (rounds, shells, tank_shells) = (rounds.floor(), shells.floor(), tank_shells.floor());
            let misfires = whole(rounds * dv.ammo_faulty, &mut w.rng);
            let duds = whole((shells + tank_shells) * dv.ammo_faulty, &mut w.rng);
            let good_shell_share = if shells + tank_shells > 0.0 { 1.0 - duds / (shells + tank_shells) } else { 1.0 };
            let general = w.generals[dv.general].skill;
            let accuracy = (0.5 + dv.training) * (1.0 + 0.5 * dv.experience);
            let mut m = accuracy * general * wfire * (0.4 + 0.6 * eq) * (0.5 + 0.5 * dv.morale) * air * luck;
            if s == 0 {
                if river {
                    m *= c.river_penalty;
                }
                if dv.landing > 0.0 {
                    m *= c.amphibious;
                }
            }
            let bayonets = shooters * out_of_ammo * c.no_ammo_fire * c.rounds_per_hour;
            h += ((rounds - misfires).max(0.0) * c.p_round + bayonets * c.p_round + shells * good_shell_share * c.p_shell + tank_shells * good_shell_share * c.p_tank_vs_men * tank_terrain) * m;
            th += (tank_shells * good_shell_share * c.p_tank_vs_tank * tank_terrain + engaged * c.p_at) * m;
            // What it used.
            let used = rounds + (shells + tank_shells) * rps;
            let fuel = (tanks_fuelled * w.data.supply.fuel_tank_hour).min(dv.fuel);
            let f = dv.faction;
            let dvm = &mut w.divisions[x];
            dvm.ammo = (dvm.ammo - used).max(0.0);
            dvm.fuel -= fuel;
            dvm.entrench = if s == 0 { 0.0 } else { dvm.entrench };
            dvm.experience = (dvm.experience + w.data.army.experience_gain / 24.0).min(1.0);
            dvm.idle_hours = 0.0;
            w.forces[f].flows.used[ed.ammo] += used / c.rounds_per_ammo;
            let r = if s == 0 { &mut w.battles[b].report.attacker } else { &mut w.battles[b].report.defender };
            r.rounds_fired += rounds;
            r.misfires += misfires;
            r.jams += jams;
            r.shells_fired += shells + tank_shells;
            r.duds += duds;
            r.ammo_used += used / c.rounds_per_ammo;
            r.fuel_used += fuel;
        }
        hits[s] = h;
        tank_hits[s] = th;
    }
    // The defenders' cover: terrain, city, digging in (the attackers stand in the open).
    let entrench: f64 = {
        let total: f64 = sides[1].iter().map(|&x| w.divisions[x].soldiers).sum::<f64>().max(1.0);
        sides[1].iter().map(|&x| w.divisions[x].entrench * w.divisions[x].soldiers).sum::<f64>() / total
    };
    let cover = terrain.defence * if city { c.city_defence } else { 1.0 } * (1.0 - c.entrench_defence * entrench);
    hits[0] *= cover;
    tank_hits[0] *= cover;
    // Hits land.
    let total_hits = hits[0] + hits[1];
    for s in 0..2 {
        let target = 1 - s;
        let on_target = hits[s];
        let losing = if total_hits > 0.0 { on_target / total_hits } else { 0.5 };
        let captured_frac = c.captured * ((losing * 2.0 - 0.5).clamp(0.0, 1.0));
        let wounded_frac = c.wounded + c.captured - captured_frac;
        let engaged_total: f64 = engaged_of[target].iter().sum::<f64>().max(1.0);
        let captor = w.divisions[sides[s][0]].faction;
        for (k, &x) in sides[target].iter().enumerate() {
            let share = engaged_of[target][k] / engaged_total;
            let before = w.divisions[x].soldiers;
            let h = whole(on_target * share, &mut w.rng).min(before);
            let killed = whole(h * c.killed, &mut w.rng).min(h);
            let captured = whole(h * captured_frac, &mut w.rng).min(h - killed);
            let wounded = h - killed - captured;
            let _ = wounded_frac;
            let tanks_lost = whole(tank_hits[s] * share, &mut w.rng).min(w.divisions[x].tanks);
            casualties(w, e, ed, x, killed, wounded, captured, 0.0, Some(captor));
            w.count("captured in battle", captured);
            let f = w.divisions[x].faction;
            {
                let dv = &mut w.divisions[x];
                let broken_share = if dv.tanks > 0.0 { dv.tanks_broken / dv.tanks } else { 0.0 };
                dv.tanks -= tanks_lost;
                dv.tanks_broken = (dv.tanks_broken - tanks_lost * broken_share).max(0.0).min(dv.tanks);
            }
            w.forces[f].flows.used[ed.tanks] += tanks_lost;
            for war in w.wars.iter_mut().filter(|war| war.active()) {
                if let Some(s) = war.side_of(f) {
                    war.losses[s].tanks += tanks_lost;
                }
            }
            let skill = w.generals[w.divisions[x].general].skill;
            let exhaustion = w.forces[f].exhaustion;
            let eq = equipped(w, &w.divisions[x]);
            let m = &w.data.morale;
            let drop = m.shock * (h / before.max(1.0)) * (2.0 - skill) + m.shortage * (1.0 - eq) + m.exhaustion * exhaustion;
            w.divisions[x].morale = (w.divisions[x].morale - drop).clamp(0.0, 1.0);
            let r = if target == 0 { &mut w.battles[b].report.attacker } else { &mut w.battles[b].report.defender };
            r.hits += h;
            r.tanks_lost += tanks_lost;
        }
    }
    // Breakdowns: a faulty tank, or one without spare parts, breaks down more.
    let parts_ok = |w: &Wars, e: &Economy, f: Faction| if f < w.nations { e.nations[f].stock[ed.steel] > 1.0 } else { w.forces[f].stores[ed.steel] > 1.0 };
    for s in 0..2 {
        for &x in &sides[s] {
            let f = w.divisions[x].faction;
            let ok = parts_ok(w, e, f);
            let dv = &w.divisions[x];
            let up = (dv.tanks - dv.tanks_broken).max(0.0);
            if up <= 0.0 {
                continue;
            }
            let chance = c.breakdown * (1.0 + c.fault_breakdown * dv.tanks_faulty) * if ok { 1.0 } else { c.no_parts_breakdown };
            let broke = whole(up * chance, &mut w.rng).min(up);
            w.divisions[x].tanks_broken += broke;
            let r = if s == 0 { &mut w.battles[b].report.attacker } else { &mut w.battles[b].report.defender };
            r.tanks_broke_down += broke;
        }
    }
    // Who breaks?
    for s in 0..2 {
        for &x in &sides[s] {
            if w.divisions[x].alive && (w.divisions[x].morale < w.data.morale.break_point || w.divisions[x].soldiers < 200.0) {
                let enemy = w.battles[b].sides[1 - s][0];
                let enemy = sides[1 - s].first().map_or(enemy, |&y| w.divisions[y].faction);
                break_division(w, e, ed, x, enemy, Some((b, s)));
            }
        }
    }
    w.battles[b].report.hours += 1;
    // The attack is called off: its soldiers' morale is spent, or it has
    // gone on for `attack_days` without the upper hand. They pull back in
    // order (no stragglers): the defenders hold.
    {
        let a = sides_now(w, b);
        let soldiers: f64 = a[0].iter().map(|&x| w.divisions[x].soldiers).sum::<f64>().max(1.0);
        let morale = a[0].iter().map(|&x| w.divisions[x].morale * w.divisions[x].soldiers).sum::<f64>() / soldiers;
        let r = &w.battles[b].report;
        // The upper hand is attrition: who lost the bigger share of what it brought.
        let share = |s: &super::SideReport| (s.killed + s.wounded + s.captured + s.surrendered + s.deserted) / s.soldiers_start.max(1.0);
        let losing = share(&r.attacker) > share(&r.defender);
        let stuck = r.hours as f64 >= 24.0 * w.data.combat.attack_days && losing;
        let long = r.hours as f64 >= 24.0 * 3.0 * w.data.combat.attack_days;
        // A long battle the attackers are winning by attrition: the
        // defenders who can pull back do (in order: no stragglers).
        if !a[0].is_empty() && long && !losing && morale >= w.data.combat.call_off_morale {
            let d = sides_now(w, b);
            let mut left = 0;
            for &x in &d[1] {
                if !withdraw(w, x) {
                    left += 1;
                }
            }
            w.count("defenders worn down and withdrew", (d[1].len() - left) as f64);
            if left == 0 {
                end_battle(w, e, ed, b, 0);
                return;
            }
        }
        if !a[0].is_empty() && (morale < w.data.combat.call_off_morale || stuck || long && losing) {
            for &x in &a[0] {
                w.divisions[x].battle = None;
                w.divisions[x].entrench = 0.0;
            }
            end_battle(w, e, ed, b, 1);
            return;
        }
    }
    let after = sides_now(w, b);
    if after[0].is_empty() || after[1].is_empty() {
        let winner = if after[1].is_empty() && !after[0].is_empty() { 0 } else { 1 };
        end_battle(w, e, ed, b, winner);
    }
}

/// Takes casualties off a division: the dead leave the population, the
/// wounded go to hospital, the captured become the captor's prisoners
/// (and their rifles its booty), deserters go home.
#[allow(clippy::too_many_arguments)]
pub fn casualties(w: &mut Wars, e: &mut Economy, ed: &EconData, x: usize, killed: f64, wounded: f64, captured: f64, deserted: f64, captor: Option<Faction>) {
    let total = killed + wounded + captured + deserted;
    if total <= 0.0 {
        return;
    }
    let f = w.divisions[x].faction;
    let nation = w.nation_of(f);
    let (armed, rifles_faulty) = {
        let dv = &w.divisions[x];
        (if dv.soldiers > 0.0 { (dv.rifles / dv.soldiers).min(1.0) } else { 0.0 }, dv.rifles_faulty)
    };
    {
        let dv = &mut w.divisions[x];
        dv.soldiers -= total;
        dv.tally.killed += killed;
        dv.tally.wounded += wounded;
        dv.tally.captured += captured;
        dv.tally.deserted += deserted;
        // The dead's and the captured's rifles are lost with them.
        dv.rifles = (dv.rifles - (killed + captured) * armed).max(0.0);
    }
    w.forces[f].flows.used[ed.rifles] += killed * armed;
    w.forces[f].flows.captured_from[ed.rifles] += captured * armed;
    if let Some(c) = captor {
        let booty = (captured * armed).floor();
        w.store(e, c, ed.rifles, booty, rifles_faulty);
        w.forces[c].flows.captured_by[ed.rifles] += booty;
        w.prisoners[c][f] += captured;
    }
    // The dead and the captured leave their nation's people.
    let n = &mut e.nations[nation];
    n.population = (n.population - killed - captured).max(1.0);
    w.forces[f].dead += killed;
    w.forces[f].casualties += killed + wounded + captured;
    if f < w.nations {
        w.forces[f].hospital += wounded;
    }
    // The battle's report and the war's totals.
    if let Some(b) = w.divisions[x].battle {
        let side = if w.battles[b].attackers.contains(&w.divisions[x].id) { 0 } else { 1 };
        let r = if side == 0 { &mut w.battles[b].report.attacker } else { &mut w.battles[b].report.defender };
        r.killed += killed;
        r.wounded += wounded;
        r.captured += captured;
        r.deserted += deserted;
    }
    for war in w.wars.iter_mut().filter(|war| war.active()) {
        if let Some(s) = war.side_of(f) {
            let l = &mut war.losses[s];
            l.killed += killed;
            l.wounded += wounded;
            l.captured += captured;
            l.deserted += deserted;
        }
    }
}

/// A division breaks: it retreats to a friendly neighbour (losing
/// stragglers and the tanks it can't take), or, with nowhere to go,
/// surrenders.
pub fn break_division(w: &mut Wars, e: &mut Economy, ed: &EconData, x: usize, enemy: Faction, battle: Option<(usize, usize)>) {
    let f = w.divisions[x].faction;
    let friends = w.friends(f);
    let here = w.divisions[x].province;
    let in_battle_here = battle.is_some_and(|(b, _)| w.battles[b].province == here);
    let retreat = if !in_battle_here && w.control[here].is_some_and(|c| friends.contains(&c)) {
        Some(here) // an attacker falls back where it came from
    } else {
        w.geo.neighbours[here].iter().copied().filter(|&r| w.control[r].is_some_and(|c| friends.contains(&c))).min_by_key(|&r| {
            let enemies = w.divisions.iter().filter(|d| d.alive && d.province == r && w.at_war(f, d.faction).is_some()).count();
            (enemies, r)
        })
    };
    // Encircled: break out through an empty neighbour the enemy holds
    // (not across the sea), back towards its own lines.
    let breakout = retreat.is_none();
    let retreat = retreat.or_else(|| {
        w.geo.neighbours[here]
            .iter()
            .copied()
            .filter(|&r| w.control[r].is_some() && !w.divisions.iter().any(|d| d.alive && d.province == r && w.at_war(f, d.faction).is_some()))
            .min()
    });
    if breakout && retreat.is_some() {
        w.count("pockets broke out", 1.0);
    }
    let m = &w.data.morale;
    let morale = w.divisions[x].morale;
    let straggle = m.stragglers_min + (m.stragglers_max - m.stragglers_min) * ((m.break_point - morale) / m.break_point).clamp(0.0, 1.0);
    // Tanks it can't take: broken ones, and all of them without fuel.
    let (abandon, faulty) = {
        let dv = &w.divisions[x];
        let fuel_need = dv.tanks * w.data.supply.fuel_tank_day;
        (if dv.fuel < fuel_need { dv.tanks } else { dv.tanks_broken }, dv.tanks_faulty)
    };
    // Nowhere to go: a pocket fights on unless its morale is gone or it has
    // nothing left to fight with.
    if retreat.is_none() {
        let eq = equipped(w, &w.divisions[x]);
        if w.divisions[x].soldiers >= 200.0 && morale >= m.surrender_morale && eq >= m.surrender_equipped {
            w.count("pockets fought on", 1.0 / 24.0);
            return;
        }
    }
    match retreat {
        Some(to) => {
            let lost = whole(w.divisions[x].soldiers * straggle, &mut w.rng);
            casualties(w, e, ed, x, 0.0, 0.0, lost, 0.0, Some(enemy));
            w.count("captured as stragglers", lost);
            w.count("divisions broke and retreated", 1.0);
            {
                let dv = &mut w.divisions[x];
                dv.tanks -= abandon;
                dv.tanks_broken = (dv.tanks_broken - abandon).max(0.0);
                dv.province = to;
                dv.moving = None;
                dv.battle = None;
                dv.entrench = 0.0;
                dv.order = Order::Auto;
            }
            moved_in(w, x);
            w.forces[f].flows.captured_from[ed.tanks] += abandon;
            for war in w.wars.iter_mut().filter(|war| war.active()) {
                if let Some(s) = war.side_of(f) {
                    war.losses[s].tanks += abandon;
                }
            }
            w.store(e, enemy, ed.tanks, abandon, faulty);
            w.forces[enemy].flows.captured_by[ed.tanks] += abandon;
            if let Some((b, s)) = battle {
                let r = if s == 0 { &mut w.battles[b].report.attacker } else { &mut w.battles[b].report.defender };
                r.tanks_abandoned += abandon;
                let r = if s == 0 { &mut w.battles[b].report.defender } else { &mut w.battles[b].report.attacker };
                r.tanks_captured += abandon;
            }
        }
        None => {
            // The pocket collapses: some slip away in small groups and make
            // their way home (counted as deserted); the rest surrender, and
            // everything they had is the enemy's.
            let escape = whole(w.divisions[x].soldiers * w.data.morale.pocket_escape, &mut w.rng);
            casualties(w, e, ed, x, 0.0, 0.0, 0.0, escape, None);
            w.count("slipped out of collapsing pockets", escape);
            let soldiers = w.divisions[x].soldiers;
            w.count("captured in surrenders", soldiers);
            w.count("divisions surrendered", 1.0);
            {
                let dv = &mut w.divisions[x];
                dv.tally.surrendered += soldiers;
                dv.soldiers = 0.0;
                dv.alive = false;
            }
            let nation = w.nation_of(f);
            e.nations[nation].population = (e.nations[nation].population - soldiers).max(1.0);
            w.prisoners[enemy][f] += soldiers;
            let (rifles, rf, tanks, tf, ammo, af) = {
                let dv = &w.divisions[x];
                (dv.rifles, dv.rifles_faulty, dv.tanks, dv.tanks_faulty, dv.ammo / w.data.combat.rounds_per_ammo, dv.ammo_faulty)
            };
            for (g, q, fq) in [(ed.rifles, rifles, rf), (ed.tanks, tanks, tf), (ed.ammo, ammo, af)] {
                w.forces[f].flows.captured_from[g] += q;
                w.store(e, enemy, g, q, fq);
                w.forces[enemy].flows.captured_by[g] += q;
            }
            {
                let dv = &mut w.divisions[x];
                dv.rifles = 0.0;
                dv.tanks = 0.0;
                dv.ammo = 0.0;
            }
            let general = w.divisions[x].general;
            w.generals[general].alive = false;
            for war in w.wars.iter_mut().filter(|war| war.active()) {
                if let Some(s) = war.side_of(f) {
                    war.losses[s].captured += soldiers;
                    war.losses[s].tanks += tanks;
                }
            }
            if let Some((b, s)) = battle {
                let r = if s == 0 { &mut w.battles[b].report.attacker } else { &mut w.battles[b].report.defender };
                r.surrendered += soldiers;
                r.tanks_abandoned += tanks;
                let r = if s == 0 { &mut w.battles[b].report.defender } else { &mut w.battles[b].report.attacker };
                r.tanks_captured += tanks;
            }
            let (name, who) = (w.divisions[x].name.clone(), w.names[f].clone());
            let place = w.geo.names[here].clone();
            w.say(vec![f, enemy], format!("{who}'s {name} surrenders at {place}: {} prisoners", super::report::big(soldiers)));
        }
    }
}

/// A defender pulls back in order to a friendly neighbour (no stragglers,
/// its tanks with it if it has the fuel). False if it has nowhere to go.
fn withdraw(w: &mut Wars, x: usize) -> bool {
    let f = w.divisions[x].faction;
    let friends = w.friends(f);
    let here = w.divisions[x].province;
    let Some(to) = w.geo.neighbours[here].iter().copied().filter(|&r| w.control[r].is_some_and(|c| friends.contains(&c))).min_by_key(|&r| (w.divisions.iter().filter(|d| d.alive && d.province == r && w.at_war(f, d.faction).is_some()).count(), r)) else {
        return false;
    };
    let dv = &mut w.divisions[x];
    dv.province = to;
    dv.moving = None;
    dv.battle = None;
    dv.entrench = 0.0;
    moved_in(w, x);
    true
}

/// A division moved mid-hour: the battles in its new province count it
/// from now on (as they would looking through every division).
fn moved_in(w: &mut Wars, x: usize) {
    if !w.in_battle_live {
        return;
    }
    let p = w.divisions[x].province;
    for b in 0..w.in_battle.len().min(w.battles.len()) {
        if !w.battles[b].over && w.battles[b].province == p && !w.in_battle[b].contains(&x) {
            // Kept in division order, as a full look would find them.
            let at = w.in_battle[b].partition_point(|&d| d < x);
            w.in_battle[b].insert(at, x);
        }
    }
}

/// A battle ends: the province changes hands (or not), morale moves, and
/// the report is filed.
fn end_battle(w: &mut Wars, e: &mut Economy, ed: &EconData, b: usize, winner: usize) {
    let _ = ed;
    w.battles[b].over = true;
    let p = w.battles[b].province;
    let war = w.battles[b].war;
    let attackers: Vec<usize> = (0..w.divisions.len()).filter(|&x| w.divisions[x].alive && w.divisions[x].battle == Some(b) && w.battles[b].attackers.contains(&w.divisions[x].id)).collect();
    let defenders: Vec<usize> = (0..w.divisions.len()).filter(|&x| w.divisions[x].alive && w.divisions[x].battle == Some(b) && !w.battles[b].attackers.contains(&w.divisions[x].id)).collect();
    let level = w.wars[war].level;
    let (win, lose) = (w.data.morale.win, w.data.morale.lose);
    if winner == 0 && !attackers.is_empty() {
        let f = w.divisions[attackers[0]].faction;
        if level >= 2 && may_take(w, f, p) {
            w.set_control(e, p, Some(f));
            for &x in &attackers {
                w.divisions[x].province = p;
                w.divisions[x].entrench = 0.0;
            }
        }
        for &x in &attackers {
            w.divisions[x].morale = (w.divisions[x].morale + win).min(1.0);
        }
    } else {
        for &x in &attackers {
            w.divisions[x].morale = (w.divisions[x].morale - lose).max(0.0);
        }
        for &x in &defenders {
            w.divisions[x].morale = (w.divisions[x].morale + win).min(1.0);
        }
    }
    for x in attackers.into_iter().chain(defenders) {
        w.divisions[x].battle = None;
    }
    // The fighting left its mark.
    let days = (w.battles[b].report.hours as f64 / 24.0).max(0.1);
    if let Some(o) = w.owner[p] {
        let k = (1.0 - w.data.combat.battle_damage).powf(days);
        for bl in e.nations[o].buildings.iter_mut().filter(|bl| bl.province == p) {
            bl.levels *= k;
        }
    }
    let pe = &mut e.provinces[p];
    pe.road = (pe.road - w.data.combat.road_damage * days).max(0.0);
    let mut report = w.battles[b].report.clone();
    report.day_end = w.day;
    report.winner = winner;
    // A battle where no shot was fired (one side was gone before the first hour) files no report.
    if report.hours == 0 && report.attacker.rounds_fired + report.defender.rounds_fired + report.attacker.shells_fired + report.defender.shells_fired <= 0.0 {
        return;
    }
    let wr = &mut w.wars[war];
    wr.battles += 1;
    if let Some(&f) = report.attacker.factions.first() {
        if let Some(s) = wr.side_of(f) {
            wr.losses[if winner == 0 { s } else { 1 - s }].battles_won += 1;
            wr.losses[s].ammo += report.attacker.ammo_used;
            wr.losses[1 - s].ammo += report.defender.ammo_used;
        }
    }
    let text = format!(
        "Battle of {} ends: {} {} ({} hours; {} and {} dead)",
        report.province_name,
        report.attacker.factions.first().map_or(String::new(), |&f| w.names[f].clone()),
        if winner == 0 { "wins" } else { "is thrown back" },
        report.hours,
        super::report::big(report.attacker.killed),
        super::report::big(report.defender.killed)
    );
    let factions = report.attacker.factions.iter().chain(report.defender.factions.iter()).copied().collect();
    w.reports.push(report);
    w.say(factions, text);
}

/// Daily: battles that ran out of soldiers on one side are closed.
pub fn finish_battles(w: &mut Wars, e: &mut Economy, ed: &EconData) {
    for b in 0..w.battles.len() {
        if w.battles[b].over {
            continue;
        }
        let s = sides_now(w, b);
        if s[0].is_empty() || s[1].is_empty() {
            end_battle(w, e, ed, b, if s[1].is_empty() && !s[0].is_empty() { 0 } else { 1 });
        }
    }
    // Drop old finished battles (their reports are kept).
    if w.battles.len() > 64 && w.battles.iter().all(|b| b.over) {
        w.battles.clear();
        for d in w.divisions.iter_mut() {
            d.battle = None;
        }
    }
}

/// The estimated power of the enemy divisions in a province, as `f`'s
/// scouts and spies see it.
fn seen_power(w: &mut Wars, f: Faction, r: usize) -> f64 {
    let truth: f64 = w
        .divisions
        .iter()
        .filter(|d| d.alive && d.province == r && d.moving.is_none() && w.at_war(f, d.faction).is_some())
        .map(|d| d.soldiers * (0.5 + d.training) * (0.4 + 0.6 * equipped(w, d)) * (1.0 + 0.002 * (d.tanks - d.tanks_broken).max(0.0)))
        .sum();
    if truth <= 0.0 {
        return 0.0;
    }
    truth * (w.intel_error[f] * normal(&mut w.rng)).exp()
}

/// A division's own power, as its general counts it.
fn own_power(w: &Wars, d: &super::Division) -> f64 {
    d.soldiers * (0.5 + d.training) * (0.4 + 0.6 * equipped(w, d)) * (1.0 + 0.002 * (d.tanks - d.tanks_broken).max(0.0)) * (0.5 + 0.5 * d.morale)
}

/// Transports a division needs (1,000 soldiers or 10 tanks each).
fn transports_for(w: &Wars, d: &super::Division) -> f64 {
    (d.soldiers / w.data.sea.transport_soldiers + d.tanks / w.data.sea.transport_tanks).ceil()
}

/// Invasions by sea (docs/WAR.md, section 7): against an enemy with no land
/// border, divisions gather at a home port, and sail for an enemy coast
/// when the sea on the way is safe and they are strong enough to land.
fn invasions(w: &mut Wars, e: &mut Economy, ed: &EconData, f: Faction, border: &[(usize, f64)]) {
    if f >= w.nations || e.nations[f].landlocked {
        return;
    }
    let enemies = w.enemies(f);
    let friends = w.friends(f);
    // Enemies we can't reach by land.
    let by_sea: Vec<Faction> = enemies
        .iter()
        .copied()
        .filter(|&g| !border.iter().any(|&(q, _)| w.geo.neighbours[q].iter().any(|&r| w.control[r] == Some(g))))
        .filter(|&g| w.level_between(f, g) >= 2)
        .collect();
    if by_sea.is_empty() {
        return;
    }
    let ports: Vec<usize> = w.held(f).into_iter().filter(|&q| w.geo.coastal[q] && e.provinces[q].port >= 1.0).collect();
    let Some(&home) = ports.iter().max_by(|&&a, &&b| e.provinces[a].people.total_cmp(&e.provinces[b].people).then(b.cmp(&a))) else { return };
    // Bring idle divisions to the port (a third stay home).
    let mine: Vec<usize> = (0..w.divisions.len())
        .filter(|&d| {
            let dv = &w.divisions[d];
            dv.alive && dv.faction == f && dv.moving.is_none() && dv.battle.is_none() && dv.order == Order::Auto
        })
        .collect();
    let at_ports: Vec<usize> = mine.iter().copied().filter(|&d| ports.contains(&w.divisions[d].province)).collect();
    let away = mine.len() - at_ports.len();
    if at_ports.len() < (2 * mine.len()) / 3 && away > 0 {
        let dist = w.geo.hops(home, |q| w.control[q].is_some_and(|c| friends.contains(&c)));
        let go: Vec<usize> = mine.iter().copied().filter(|&d| !ports.contains(&w.divisions[d].province)).take((2 * mine.len()) / 3).collect();
        for d in go {
            let here = w.divisions[d].province;
            if let Some((_, prev)) = dist[here] {
                if prev != here {
                    let h = move_hours(w, e, prev);
                    w.divisions[d].moving = Some((prev, h, false));
                }
            }
        }
    }
    // Sail: from each port with troops, to the weakest reachable enemy coast.
    let free = super::sea::transports(w, e, ed, f);
    if free < 1.0 {
        return;
    }
    let mut budget = free;
    for &port in &ports {
        let idle: Vec<usize> = at_ports.iter().copied().filter(|&d| w.divisions[d].province == port && w.divisions[d].morale > 0.5 && equipped(w, &w.divisions[d]) > 0.5).collect();
        if idle.is_empty() {
            continue;
        }
        let power: f64 = idle.iter().map(|&d| own_power(w, &w.divisions[d])).sum();
        let mut best: Option<(usize, f64, f64)> = None;
        for &g in &by_sea {
            for q in w.held(g) {
                if !w.geo.coastal[q] || !may_take(w, f, q) {
                    continue;
                }
                let Some(hours) = super::sea::crossing(w, f, port, q) else { continue };
                let theirs = seen_power(w, f, q) / w.geo.terrain[q].defence;
                let ratio = if theirs <= 0.0 { 99.0 } else { power * w.data.combat.amphibious / theirs };
                if ratio >= 1.5 && best.is_none_or(|(_, r, _)| ratio > r) {
                    best = Some((q, ratio, hours));
                }
            }
        }
        let Some((target, _, hours)) = best else { continue };
        let keep = (idle.len() as f64 / 3.0).floor() as usize;
        for &d in idle.iter().skip(keep) {
            let need = transports_for(w, &w.divisions[d]);
            if need > budget {
                break;
            }
            budget -= need;
            w.forces[f].transports_busy += need;
            // Contested water off the landing beaches costs men and ships.
            let contested = w.geo.zones_of[target].iter().any(|&z| w.zone_contested.get(z).copied().unwrap_or(false));
            let lost = if contested { whole(w.divisions[d].soldiers * w.data.sea.convoy_loss, &mut w.rng) } else { 0.0 };
            casualties(w, e, ed, d, lost, 0.0, 0.0, 0.0, None);
            w.divisions[d].moving = Some((target, hours, true));
            w.divisions[d].entrench = 0.0;
        }
        let (who, there) = (w.names[f].clone(), w.geo.names[target].clone());
        w.say(vec![f], format!("{who} sails to invade {there}"));
    }
}

/// The most soldiers an attack on `r` sends: `reserve_frontages` lines' worth.
fn attack_cap(w: &Wars, r: usize) -> f64 {
    let c = &w.data.combat;
    w.data.generals.reserve_frontages * c.frontage_base * w.geo.hexes[r].sqrt() * w.geo.terrain[r].frontage * c.flank_max
}

/// `f`'s soldiers fighting in `r` or marching there.
fn attacking(w: &Wars, f: Faction, r: usize) -> f64 {
    w.divisions.iter().filter(|d| d.alive && d.faction == f && (d.moving.is_some_and(|m| m.0 == r) || d.battle.is_some_and(|b| w.battles.get(b).is_some_and(|x| x.province == r)))).map(|d| d.soldiers).sum()
}

/// The generals' day: fronts, marches, attacks, digging in.
pub fn generals(w: &mut Wars, e: &mut Economy, ed: &EconData) {
    let _ = ed;
    let factions: Vec<Faction> = (0..w.forces.len()).filter(|&f| w.in_any_war(f)).collect();
    // Out of war: dig in, rest, drill.
    for d in 0..w.divisions.len() {
        let dv = &w.divisions[d];
        if !dv.alive || dv.battle.is_some() || dv.moving.is_some() {
            continue;
        }
        let gain = w.data.army.entrench_gain * if dv.order == Order::DigIn { 2.0 } else { 1.0 };
        let target = w.data.morale.target_base + w.data.morale.target_training * dv.training + w.data.morale.target_equipped * equipped(w, dv);
        let cap = w.training_cap(e, dv.faction);
        let dv = &mut w.divisions[d];
        dv.entrench = (dv.entrench + gain).min(1.0);
        dv.morale += w.data.morale.recover * (target - dv.morale);
        dv.training += w.data.army.training_gain * (cap - dv.training).max(0.0) * if dv.training < 0.4 { 10.0 } else { 1.0 };
    }
    for f in factions {
        let friends = w.friends(f);
        let held_by_friend = |w: &Wars, q: usize| w.control[q].is_some_and(|c| friends.contains(&c));
        // Border provinces and what each one faces.
        let mut border: Vec<(usize, f64)> = Vec::new();
        for q in 0..w.control.len() {
            if !held_by_friend(w, q) {
                continue;
            }
            let mut threat: f64 = 0.0;
            let mut touches = false;
            for k in 0..w.geo.neighbours[q].len() {
                let r = w.geo.neighbours[q][k];
                if w.control[r].is_some_and(|c| w.at_war(f, c).is_some()) {
                    touches = true;
                    threat = threat.max(seen_power(w, f, r));
                }
            }
            if touches {
                let own: f64 = w.divisions.iter().filter(|d| d.alive && d.faction == f && d.province == q && d.moving.is_none()).map(|d| own_power(w, d)).sum();
                border.push((q, 1.2 * threat - own));
            }
        }
        invasions(w, e, ed, f, &border);
        if border.is_empty() {
            continue;
        }
        // Attacks from each border province.
        let skirmish_cap = w.data.levels.skirmish_divisions;
        for &(q, _) in &border.clone() {
            let idle: Vec<usize> = (0..w.divisions.len())
                .filter(|&d| {
                    let dv = &w.divisions[d];
                    dv.alive && dv.faction == f && dv.province == q && dv.moving.is_none() && dv.battle.is_none() && dv.morale > 0.45 && matches!(dv.order, Order::Auto | Order::Attack(_))
                })
                .collect();
            if idle.is_empty() {
                continue;
            }
            let mine: f64 = idle.iter().map(|&d| own_power(w, &w.divisions[d])).sum();
            let ratio_needed = idle
                .iter()
                .map(|&d| match w.generals[w.divisions[d].general].style {
                    Style::Aggressive => w.data.generals.aggressive_ratio,
                    Style::Balanced => w.data.generals.balanced_ratio,
                    Style::Cautious => w.data.generals.cautious_ratio,
                })
                .fold(f64::MAX, f64::min);
            let mut best: Option<(usize, f64)> = None;
            for k in 0..w.geo.neighbours[q].len() {
                let r = w.geo.neighbours[q][k];
                let Some(c) = w.control[r] else { continue };
                if w.at_war(f, c).is_none() {
                    continue;
                }
                let level = w.level_between(f, c);
                if level == 2 && !may_take(w, f, r) {
                    continue;
                }
                let ordered = idle.iter().any(|&d| w.divisions[d].order == Order::Attack(r));
                // The side that started the war has a plan: it attacks at a lower ratio.
                let starter = w.at_war(f, c).is_some_and(|k| w.wars[k].side_of(f) == Some(0));
                let needed = ratio_needed * if starter { w.data.generals.initiative } else { 1.0 };
                let theirs = seen_power(w, f, r);
                // A border skirmish takes no land: it goes where the enemy stands.
                if level <= 1 && theirs <= 0.0 && !ordered {
                    continue;
                }
                // An attack that already has its reserves needs no more.
                if !ordered && attacking(w, f, r) >= attack_cap(w, r) {
                    continue;
                }
                let cover = w.geo.terrain[r].defence * if w.geo.city[r] { w.data.combat.city_defence } else { 1.0 };
                let river = if w.geo.river[r] { w.data.combat.river_penalty } else { 1.0 };
                let ratio = if theirs <= 0.0 { 99.0 } else { mine * river / (theirs / cover) };
                let ratio = if ordered { 99.0 } else { ratio };
                if ratio >= needed && best.is_none_or(|(_, b)| ratio > b) {
                    best = Some((r, ratio));
                }
            }
            if let Some((r, _)) = best {
                let level = w.level_between(f, w.control[r].unwrap());
                // A garrison stays behind: a third of the divisions (the
                // province mustn't be left empty for the enemy to walk into).
                let ordered = idle.iter().filter(|&&d| w.divisions[d].order == Order::Attack(r)).count();
                let keep = if idle.len() >= 2 { (idle.len() as f64 / 3.0).ceil() as usize } else { 0 };
                let mut take = if level <= 1 { skirmish_cap.min(idle.len().saturating_sub(keep).max(1)) } else { (idle.len() - keep).max(ordered).max(1) };
                let mut idle = idle.clone();
                idle.sort_by_key(|&d| (w.divisions[d].order != Order::Attack(r), std::cmp::Reverse((own_power(w, &w.divisions[d]) as u64) / 1000), d));
                // No more than the line can use, with reserves to rotate in:
                // the rest stay for the next attack (or another front).
                let room = attack_cap(w, r) - attacking(w, f, r);
                let mut sum = 0.0;
                let mut fits = 0;
                for &d in &idle {
                    if sum >= room {
                        break;
                    }
                    sum += w.divisions[d].soldiers;
                    fits += 1;
                }
                take = take.min(fits.max(ordered).max(1));
                let hours = 0.5 * move_hours(w, e, r);
                for &d in idle.iter().take(take) {
                    w.divisions[d].moving = Some((r, hours, false));
                    w.divisions[d].entrench = 0.0;
                    if w.divisions[d].order == Order::Attack(r) {
                        w.divisions[d].order = Order::Auto;
                    }
                }
            }
        }
        // Everyone else marches towards the border that needs them most.
        let hops_from: Vec<usize> = (0..w.divisions.len())
            .filter(|&d| {
                let dv = &w.divisions[d];
                dv.alive && dv.faction == f && dv.moving.is_none() && dv.battle.is_none() && matches!(dv.order, Order::Auto | Order::Reinforce(_) | Order::Retreat)
            })
            .collect();
        for d in hops_from {
            let here = w.divisions[d].province;
            let order = w.divisions[d].order;
            if order == Order::Retreat {
                let back = w.geo.neighbours[here].iter().copied().filter(|&r| held_by_friend(w, r)).max_by_key(|&r| {
                    let dist = w.geo.hops(w.capital[w.nation_of(f)], |q| held_by_friend(w, q));
                    std::cmp::Reverse(dist[r].map_or(99, |x| x.0))
                });
                if let Some(to) = back {
                    let h = move_hours(w, e, to);
                    w.divisions[d].moving = Some((to, h, false));
                }
                w.divisions[d].order = Order::Hold;
                continue;
            }
            let at_border = border.iter().any(|&(q, _)| q == here);
            if at_border && order == Order::Auto && w.divisions[d].morale >= 0.35 {
                continue;
            }
            let dist = w.geo.hops(here, |q| held_by_friend(w, q));
            let goal = match order {
                Order::Reinforce(p) => Some(p),
                _ => border.iter().filter(|&&(q, _)| dist[q].is_some() && q != here).max_by(|a, b| (a.1 / (1.0 + dist[a.0].unwrap().0 as f64)).total_cmp(&(b.1 / (1.0 + dist[b.0].unwrap().0 as f64)))).map(|x| x.0),
            };
            let Some(goal) = goal else { continue };
            if goal == here {
                w.divisions[d].order = Order::Auto;
                continue;
            }
            // One step along the way.
            let mut step = goal;
            while let Some((_, prev)) = dist[step] {
                if prev == here {
                    break;
                }
                step = prev;
            }
            if dist[step].is_some() && step != here {
                let h = move_hours(w, e, step);
                w.divisions[d].moving = Some((step, h, false));
                w.divisions[d].entrench = 0.0;
            }
        }
    }
    let _ = hash3;
    let _ = Kind::Infantry;
}
