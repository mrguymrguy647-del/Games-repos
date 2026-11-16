//! War tests (docs/WAR.md, section 13).

use super::report::{rivals, WarWorld};
use super::{escalation, Goal, Wars};

/// A world where the two biggest neighbours go to war (no brains, no other wars).
fn forced(map: usize, seed: u32, level: usize) -> (WarWorld, usize, usize, usize) {
    let mut ww = WarWorld::new(map, seed, false).unwrap_or_else(|e| panic!("{e}"));
    ww.wars.ai = false;
    let (a, b) = rivals(&ww.wars).expect("two neighbours");
    let border: Vec<usize> = ww.wars.held(b).into_iter().filter(|&q| ww.wars.geo.neighbours[q].iter().any(|&r| ww.wars.control[r] == Some(a))).collect();
    let k = escalation::declare(&mut ww.wars, &mut ww.e, &ww.ed, a, b, level, Goal::Provinces(border), "a test".into());
    (ww, a, b, k)
}

#[test]
fn the_war_file_loads_and_a_typo_is_one_clear_sentence() {
    let wd = super::WarData::load().unwrap_or_else(|e| panic!("{e}"));
    assert!(wd.army.division_size > 0.0);
    let text = std::fs::read_to_string("data/war/war.toml").unwrap();
    let err = super::WarData::parse(&text.replace("rounds_per_hour", "rounds_per_hr")).unwrap_err();
    assert!(err.contains("data/war/war.toml") && err.contains("rounds_per_hr"), "{err}");
}

#[test]
fn the_same_seed_gives_the_same_battle() {
    let run = || {
        let (mut ww, ..) = forced(0, 3, 3);
        for _ in 0..60 {
            ww.day();
        }
        (ww.wars.reports.clone(), ww.wars.divisions.iter().map(|d| (d.soldiers, d.morale, d.province)).collect::<Vec<_>>())
    };
    let (a, b) = (run(), run());
    assert!(!a.0.is_empty(), "a battle was fought");
    assert_eq!(a.0, b.0, "same seed, same battle reports, hour by hour");
    assert_eq!(a.1, b.1);
}

/// Every soldier is accounted for, in every division, every day:
/// start + joined − released = alive + killed + wounded + captured + surrendered + deserted.
#[test]
fn every_soldier_is_accounted_for() {
    let (mut ww, a, b, _) = forced(2, 5, 3);
    for day in 0..150 {
        ww.day();
        for d in &ww.wars.divisions {
            let t = &d.tally;
            assert!((t.expected() - d.soldiers).abs() < 1e-6, "day {day}: {} has {} soldiers, its books say {}", d.name, d.soldiers, t.expected());
            assert!(d.soldiers >= 0.0 && d.soldiers.fract() == 0.0, "{}: {} soldiers", d.name, d.soldiers);
        }
    }
    // And the people: the dead and the prisoners left the population.
    let lost: f64 = [a, b].iter().map(|&f| ww.wars.forces[f].dead).sum();
    assert!(lost > 0.0, "people died");
    let reports: f64 = ww.wars.reports.iter().map(|r| r.attacker.killed + r.attacker.wounded + r.attacker.captured + r.defender.killed + r.defender.wounded + r.defender.captured).sum();
    assert!(reports > 0.0);
}

/// Every shell, rifle and tank is accounted for, and none comes from
/// nothing: in store + in the field + fired or destroyed + lost on the way
/// + taken by the enemy = at the start + bought + taken from the enemy.
#[test]
fn every_shell_rifle_and_tank_is_accounted_for() {
    let (mut ww, a, b, _) = forced(0, 2, 3);
    let ed = ww.ed.clone();
    let rpa = ww.wars.data.combat.rounds_per_ammo;
    let field = |w: &Wars, f: usize, g: usize| -> f64 {
        w.divisions.iter().filter(|d| d.alive && d.faction == f).map(|d| if g == ed.ammo { d.ammo / rpa } else if g == ed.rifles { d.rifles } else { d.tanks }).sum()
    };
    let flows0: Vec<_> = [a, b].iter().map(|&f| ww.wars.forces[f].flows.clone()).collect();
    let start: Vec<Vec<(f64, f64)>> = [a, b].iter().map(|&f| [ed.ammo, ed.rifles, ed.tanks].iter().map(|&g| (ww.e.nations[f].stockpile[g], field(&ww.wars, f, g))).collect()).collect();
    for _ in 0..120 {
        ww.day();
    }
    for (k, &f) in [a, b].iter().enumerate() {
        let fl = &ww.wars.forces[f].flows;
        let f0 = &flows0[k];
        for (j, &g) in [ed.ammo, ed.rifles, ed.tanks].iter().enumerate() {
            let d = |x: &Vec<f64>, y: &Vec<f64>| x[g] - y[g];
            let now = ww.e.nations[f].stockpile[g] + field(&ww.wars, f, g);
            let (s0, f0v) = start[k][j];
            let came = s0 + f0v + d(&fl.bought, &f0.bought) + d(&fl.captured_by, &f0.captured_by) + d(&fl.received, &f0.received);
            let went = d(&fl.used, &f0.used) + d(&fl.lost, &f0.lost) + d(&fl.captured_from, &f0.captured_from) + d(&fl.given, &f0.given);
            let gap = (now + went - came).abs();
            assert!(gap <= 1e-6 * came.max(1.0), "{} {}: now {now} + went {went} vs came {came} (gap {gap})", ww.wars.names[f], ed.goods[g].id);
        }
        assert!(fl.used[ed.ammo] > f0.used[ed.ammo], "ammo was fired");
    }
}

/// A war fought without ammo or food is lost: the same battle, the same
/// dice, but one side's supplies emptied.
#[test]
fn supply_shortages_really_weaken_armies() {
    let hits = |starve: bool| {
        let (mut ww, a, b, _) = forced(0, 4, 3);
        if starve {
            for g in [ww.ed.ammo, ww.ed.food] {
                ww.e.nations[a].stockpile[g] = 0.0;
                ww.e.nations[a].stock[g] = 0.0;
            }
            for d in ww.wars.divisions.iter_mut().filter(|d| d.faction == a) {
                d.ammo = 0.0;
                d.food = 0.0;
            }
        }
        // The army's equipped share, over the 40 days (the economy refills
        // the stockpile day by day, so the gap closes as it goes).
        let mut equipped = 0.0;
        for _ in 0..40 {
            ww.day();
            let alive: Vec<&super::Division> = ww.wars.divisions.iter().filter(|d| d.alive && d.faction == a).collect();
            equipped += alive.iter().map(|d| super::land::equipped(&ww.wars, d)).sum::<f64>() / alive.len().max(1) as f64 / 40.0;
        }
        let dealt: f64 = ww.wars.reports.iter().map(|r| if r.attacker.factions.contains(&a) { r.defender.hits } else if r.defender.factions.contains(&a) { r.attacker.hits } else { 0.0 }).sum();
        let _ = b;
        (dealt, equipped, ww.wars.forces[a].dead)
    };
    let (fed, fed_eq, fed_dead) = hits(false);
    let (starved, starved_eq, starved_dead) = hits(true);
    assert!(fed > 0.0);
    assert!(starved < 0.5 * fed, "without ammo the army hits far less: {starved} against {fed}");
    println!("hits {starved:.0} against {fed:.0}; equipped {starved_eq:.3} against {fed_eq:.3}");
    assert!(starved_eq < fed_eq - 0.05, "and is worse equipped: {starved_eq:.2} against {fed_eq:.2}");
    let _ = (fed_dead, starved_dead);
}

/// Landlocked nations can't build or hold a navy.
#[test]
fn landlocked_nations_have_no_navy() {
    let mut found = 0;
    for (map, seed) in [(0, 1), (2, 2), (0, 3)] {
        let mut ww = WarWorld::new(map, seed, false).unwrap();
        let ed = ww.ed.clone();
        let shipyard = ed.buildings.iter().position(|b| b.output == ed.warships).unwrap();
        for i in 0..ww.e.nations.len() {
            if !ww.e.nations[i].landlocked {
                continue;
            }
            found += 1;
            assert_eq!(ww.e.nations[i].stockpile[ed.warships], 0.0, "{} starts with warships", ww.wars.names[i]);
            assert!(ww.e.player_build(&ed, i, shipyard).is_err(), "{} could build a shipyard", ww.wars.names[i]);
        }
        for _ in 0..200 {
            ww.day();
        }
        for i in 0..ww.e.nations.len() {
            if ww.e.nations[i].landlocked {
                let n = &ww.e.nations[i];
                assert!(n.buildings.iter().all(|b| !ed.buildings[b.kind].coastal), "{} has a shipyard", n.name);
                assert!(n.construction.iter().all(|c| !ed.buildings[c.kind].coastal));
                assert!(ww.wars.forces[i].fleets.is_empty(), "{} has a fleet", n.name);
            }
        }
    }
    assert!(found > 0, "some nation is landlocked");
}

/// Layer 5b at sea: on the Thousand Isles a strong navy sinks the enemy's,
/// blockades its island and carries an invasion ashore.
#[test]
fn a_navy_blockades_and_invades_an_island() {
    let mut ww = WarWorld::new(1, 1, false).unwrap();
    ww.wars.ai = false;
    let (a, b) = super::report::rivals_by_sea(&ww.wars, &ww.e, &ww.ed).expect("two island nations");
    let ed = ww.ed.clone();
    let ships_b = ww.e.nations[b].stockpile[ed.warships];
    let goal: Vec<usize> = ww.wars.held(b).into_iter().filter(|&q| ww.wars.geo.coastal[q]).take(3).collect();
    escalation::declare(&mut ww.wars, &mut ww.e, &ww.ed, a, b, 3, Goal::Provinces(goal), "a test".into());
    let trade_before = ww.e.nations[b].trade_capacity;
    for _ in 0..200 {
        ww.day();
    }
    assert!(ww.e.nations[b].stockpile[ed.warships] < ships_b, "the weaker navy lost ships");
    assert!(ww.e.nations[b].blockade >= 0.5, "blockaded: {}", ww.e.nations[b].blockade);
    assert!(ww.e.nations[b].trade_capacity < 0.8 * trade_before, "its sea trade is cut");
    let taken = ww.wars.held(a).into_iter().filter(|&q| ww.wars.owner[q] == Some(b)).count();
    assert!(taken > 0, "an invasion took ground on the island");
}

/// Layer 5b in the air: the side with the jets (and pilots) wins the sky.
#[test]
fn the_side_with_more_jets_wins_the_sky() {
    let (mut ww, a, b, _) = forced(0, 1, 3);
    let ed = ww.ed.clone();
    let (ja, jb) = (ww.e.nations[a].stockpile[ed.jets], ww.e.nations[b].stockpile[ed.jets]);
    assert!(ja != jb);
    let (strong, weak) = if ja > jb { (a, b) } else { (b, a) };
    let mut best = 0.0f64;
    for _ in 0..30 {
        ww.day();
        best = best.max(ww.wars.forces[strong].air.iter().copied().fold(f64::MIN, f64::max));
        assert!(ww.wars.forces[weak].air.iter().all(|&s| s <= 0.0), "the weaker air force never has the upper hand");
    }
    assert!(best > 0.2, "air superiority: {best}");
}

/// Ideology sets how easily a nation climbs the ladder, and the leader how
/// much it wants war (docs/WAR.md, sections 8 and 11).
#[test]
fn military_rule_escalates_easily_and_democracy_does_not() {
    let wd = super::WarData::load().unwrap();
    let es = &wd.escalation;
    assert!(es.cost("military_rule") < es.cost("monarchy") && es.cost("monarchy") < es.cost("democracy"));
    let (ww, ..) = forced(0, 1, 1);
    let mut w = ww.wars.clone();
    w.leader[0] = "warlord".into();
    w.ideology[0] = "military_rule".into();
    w.leader[1] = "coward".into();
    w.ideology[1] = "democracy".into();
    assert!(escalation::aggression(&w, 0, None) > escalation::aggression(&w, 1, None) + 0.8);
}

/// A peace deal moves a province: its people, buildings and port go to the
/// winner; prisoners go home.
#[test]
fn a_peace_deal_moves_provinces_and_returns_prisoners() {
    let (mut ww, a, b, k) = forced(0, 6, 3);
    let ed = ww.ed.clone();
    let q = ww.wars.held(b).into_iter().find(|&q| q != ww.wars.capital[b] && ww.wars.geo.neighbours[q].iter().any(|&r| ww.wars.control[r] == Some(a))).unwrap();
    ww.wars.prisoners[a][b] = 1000.0;
    ww.e.nations[b].population -= 1000.0;
    let pop = (ww.e.nations[a].population, ww.e.nations[b].population);
    let share = ww.e.provinces[q].people;
    let levels: f64 = ww.e.nations[b].buildings.iter().filter(|x| x.province == q).map(|x| x.levels).sum();
    ww.wars.set_control(&mut ww.e, q, Some(a));
    let mut t = super::peace::PeaceTerms::white(80.0);
    t.winner = Some(0);
    t.provinces = vec![q];
    super::peace::make_peace(&mut ww.wars, &mut ww.e, &ed, k, t, "a test");
    assert_eq!(ww.wars.owner[q], Some(a));
    assert!(ww.e.nations[a].provinces.contains(&q) && !ww.e.nations[b].provinces.contains(&q));
    let moved = pop.1 * share;
    assert!((ww.e.nations[a].population - pop.0 - moved).abs() < 1.0, "its people moved");
    let now: f64 = ww.e.nations[a].buildings.iter().filter(|x| x.province == q).map(|x| x.levels).sum();
    assert!((now - levels).abs() < 1e-6, "its buildings moved");
    assert_eq!(ww.wars.prisoners[a][b], 0.0, "prisoners went home");
    assert!(!ww.wars.wars[k].active());
    assert!(ww.wars.at_war(a, b).is_none());
    assert_eq!(ww.e.provinces[q].occupied_by, None);
}

/// Over 50 years, on every map with six seeds, the brains go to war at a
/// believable rate, the world doesn't burn down, and wars end in peace.
#[test]
fn fifty_years_of_war_are_believable() {
    let jobs: Vec<(usize, u32)> = (1..=6).flat_map(|s| (0..3).map(move |m| (m, s))).collect();
    let results: Vec<super::report::WorldWars> = std::thread::scope(|scope| {
        let handles: Vec<_> = jobs.iter().map(|&(m, s)| scope.spawn(move || super::report::run_wars(m, s, 50.0, false).unwrap_or_else(|e| panic!("{e}")))).collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut wars = 0;
    let mut ended = 0;
    let mut nations = 0;
    for (k, r) in results.iter().enumerate() {
        let (m, s) = jobs[k];
        assert!(r.worst_share <= 1.0 / 3.0, "map {m} seed {s}: {:.0}% of nations at declared or total war at once", r.worst_share * 100.0);
        wars += r.wars;
        ended += r.ended;
        nations += r.nations;
    }
    let per_decade = wars as f64 / nations as f64 * 10.0 / 50.0;
    assert!((0.05..=1.0).contains(&per_decade), "{per_decade:.2} wars a nation per decade");
    assert!(ended as f64 >= 0.9 * wars as f64, "{ended} of {wars} wars ended in peace");
    assert!(results.iter().filter(|r| r.wars > 0).count() >= 15, "wars happen in nearly every world");
}

/// A whole war, from the first shot to the peace deal, as the spectator sees
/// it: the news says it began, battles were fought, and it ended in peace.
#[test]
fn a_whole_war_from_first_shot_to_peace() {
    let mut ww = WarWorld::new(0, 1, true).unwrap();
    let mut done = None;
    for _ in 0..(20 * 360) {
        ww.day();
        if let Some(k) = (0..ww.wars.wars.len()).find(|&k| ww.wars.wars[k].max_level >= 2 && !ww.wars.wars[k].active() && ww.wars.wars[k].battles > 0) {
            done = Some(k);
            break;
        }
    }
    let k = done.expect("a war started and ended within 20 years");
    let w = &ww.wars.wars[k];
    assert!(w.result.is_some() && w.peace.is_some(), "it ended with a deal: {:?}", w.result);
    let news: Vec<&String> = ww.wars.news.iter().map(|n| &n.text).collect();
    assert!(news.iter().any(|t| t.starts_with("Peace between")), "the news reported the peace");
}

/// 25 nations at the game's top speed (three months a second): a year of a
/// total war between the two biggest armies, with every brain thinking and
/// every other nation free to join, must leave the frame time to spare.
#[test]
fn the_war_is_fast_enough_for_25_nations_at_top_speed() {
    let mut ww = WarWorld::new(0, 1, true).unwrap();
    assert!(ww.e.nations.len() >= 25, "a full world");
    let (a, b) = rivals(&ww.wars).expect("two neighbours");
    let goal = Goal::Provinces(ww.wars.held(b).into_iter().filter(|q| !ww.wars.capital.contains(q)).take(3).collect());
    escalation::declare(&mut ww.wars, &mut ww.e, &ww.ed, a, b, 4, goal, "test".to_string());
    // CPU time of this thread, from /proc (utime + stime, in 1/100 s): a
    // busy machine running the other tests at once doesn't count against
    // the war. The war's share of it is its share of the wall clock.
    let cpu = || -> Option<f64> {
        let stat = std::fs::read_to_string("/proc/thread-self/stat").ok()?;
        let after = stat.rsplit_once(')')?.1;
        // Fields 14 and 15 of the line; "after" starts at field 3 (the state).
        let f: Vec<&str> = after.split_whitespace().collect();
        Some((f.get(11)?.parse::<f64>().ok()? + f.get(12)?.parse::<f64>().ok()?) / 100.0)
    };
    let cpu_start = cpu();
    let wall_start = std::time::Instant::now();
    let mut war = std::time::Duration::ZERO;
    let mut hours = 0;
    for _ in 0..ww.ed.days_per_year {
        ww.e.tick(&ww.ed);
        if let Some(br) = ww.brains.as_mut() {
            br.day(&mut ww.e, &ww.ed);
        }
        let t = std::time::Instant::now();
        ww.wars.day(&mut ww.e, &ww.ed, ww.brains.as_mut());
        war += t.elapsed();
        hours += ww.wars.battles.iter().filter(|x| !x.over).count();
    }
    let wall = wall_start.elapsed().as_secs_f64();
    let cpu_year = match (cpu_start, cpu()) {
        (Some(a), Some(b)) if b > a => b - a,
        _ => wall,
    };
    let per_year = cpu_year * war.as_secs_f64() / wall;
    println!("a year of war took {per_year:.3} s of CPU; the whole year (economy, brains and war) {cpu_year:.3} s ({hours} battle-days)");
    assert!(hours > 0, "there was fighting");
    // Top speed is a game year in 4 seconds: the whole simulation must
    // leave at least a fifth of that for drawing, in the heaviest war.
    let limit = if cfg!(debug_assertions) { 60.0 } else { 3.2 };
    assert!(cpu_year < limit, "a year of economy, brains and war for 25 nations took {cpu_year:.2} s");

}
