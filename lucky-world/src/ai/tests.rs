//! The brains' tests (docs/AI.md, section 12): the data, the same decisions
//! from the same seed, no peeking at what a brain shouldn't know, the same
//! rules as the player, and speed.

use super::report::run_world;
use super::{planning, strategy, AiData, Brains};
use crate::econ::{EconData, Economy};

fn world(map: usize, seed: u32) -> (crate::data::Data, crate::mapdata::Atlas, EconData, crate::world::World, Economy) {
    crate::make_world(map, seed).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn the_brain_files_load_and_name_real_wheel_results() {
    let ai = AiData::load().unwrap_or_else(|e| panic!("{e}"));
    let data = crate::data::Data::load().unwrap_or_else(|e| panic!("{e}"));
    for key in ai.traits.keys() {
        let (wheel, option) = key.split_once('.').expect("wheel.option");
        let w = data.wheels.iter().find(|w| w.id == wheel).unwrap_or_else(|| panic!("data/ai/personality.toml: there is no wheel \"{wheel}\""));
        assert!(
            w.options.iter().any(|o| o.id == option),
            "data/ai/personality.toml: [{key}]: the {wheel} wheel has no option \"{option}\""
        );
    }
    for level in ["none", "amateur", "competent", "feared", "legendary"] {
        assert!(data.wheels.iter().any(|w| w.id == "secret_service" && w.options.iter().any(|o| o.id == level)), "no secret service level {level}");
    }
    for d in ["easy", "normal", "hard"] {
        assert!(ai.difficulties.contains_key(d), "no [difficulty.{d}]");
    }
}

#[test]
fn a_mistake_in_brain_toml_is_one_clear_sentence() {
    let brain: toml::Value = std::fs::read_to_string("data/ai/brain.toml").unwrap().parse().unwrap();
    let personality: toml::Value = std::fs::read_to_string("data/ai/personality.toml").unwrap().parse().unwrap();
    let mut bad = brain.clone();
    bad.get_mut("action").unwrap().get_mut("build").unwrap().as_table_mut().unwrap().insert("jobz".into(), toml::Value::Float(0.3));
    let err = AiData::from_values(&bad, &personality).unwrap_err().to_string();
    assert!(err.contains("action.build.jobz") && err.contains("data/ai/brain.toml"), "{err}");
    let mut bad = brain;
    bad.get_mut("action").unwrap().get_mut("build").unwrap().as_table_mut().unwrap().insert("jobs".into(), toml::Value::Float(3.0));
    let err = AiData::from_values(&bad, &personality).unwrap_err().to_string();
    assert!(err.contains("between -1 and 1"), "{err}");
}

#[test]
fn intel_is_sharper_with_a_better_secret_service() {
    let ai = AiData::load().unwrap();
    let levels = ["none", "amateur", "competent", "feared", "legendary"];
    for w in levels.windows(2) {
        assert!(ai.intel[w[0]] > ai.intel[w[1]], "{} should guess worse than {}", w[0], w[1]);
    }
    // And the errors are what the data says: a brain sees itself exactly,
    // others within its error.
    let (data, atlas, ed, world, mut e) = world(0, 3);
    let mut b = Brains::new(ai, &data, &atlas, &world, &mut e, 3, |_| true);
    e.run_days(&ed, 30);
    for i in 0..3 {
        let brain = b.brains[i].as_mut().unwrap();
        let me = strategy::observe(brain, &e, &ed, i);
        assert_eq!(me.gdp, e.nations[i].gdp, "a brain knows its own numbers");
        let mut sum = 0.0;
        let others: Vec<usize> = (0..e.nations.len()).filter(|&j| j != i).collect();
        for _ in 0..20 {
            for &j in &others {
                sum += (strategy::observe(brain, &e, &ed, j).gdp / e.nations[j].gdp).ln().abs();
            }
        }
        let mean = sum / (20 * others.len()) as f64;
        // The mean |error| of a normal error is about 0.8 sd.
        let sd = brain.intel_error;
        assert!(mean > 0.5 * sd && mean < 1.2 * sd, "{}: spies {} (error {sd}) were off by {mean:.3} on average", e.nations[i].name, brain.secret_service);
    }
}

#[test]
fn the_same_seed_gives_the_same_decisions() {
    let run = || {
        let (e, _, b) = run_world(1, 5, 2.0, true).unwrap_or_else(|e| panic!("{e}"));
        let b = b.unwrap();
        let logs: Vec<Vec<super::Decision>> = b.brains.iter().flatten().map(|x| x.log.clone()).collect();
        let goals: Vec<usize> = b.brains.iter().flatten().map(|x| x.goal).collect();
        (logs, goals, e.nations.iter().map(|n| n.gdp).collect::<Vec<f64>>())
    };
    let (a, b) = (run(), run());
    assert!(a.0.iter().map(|l| l.len()).sum::<usize>() > 100, "the brains decided something");
    assert_eq!(a.0, b.0, "same world, same seed: the same decisions on the same days");
    assert_eq!(a.1, b.1);
    assert_eq!(a.2, b.2);
}

/// What a brain may not know about other nations: anything its secret
/// service doesn't report (their buildings, stores, treasury, debt, taxes,
/// research). Changing all of that for everyone else must not change one
/// thing it thinks or does.
#[test]
fn npcs_never_use_information_they_shouldnt_have() {
    let (data, atlas, ed, world, mut e) = world(2, 4);
    let mut b = Brains::new(AiData::load().unwrap(), &data, &atlas, &world, &mut e, 4, |_| true);
    for _ in 0..400 {
        e.tick(&ed);
        b.day(&mut e, &ed);
    }
    let me = 3;
    let mut hidden = e.clone();
    for (j, n) in hidden.nations.iter_mut().enumerate() {
        if j == me {
            continue;
        }
        n.buildings.iter_mut().for_each(|x| x.levels *= 1.7);
        n.stock.iter_mut().for_each(|x| *x *= 3.0);
        n.treasury = n.treasury * 5.0 + 1e9;
        n.debt *= 2.5;
        n.income_tax = 0.6;
        n.corporate_tax = 0.05;
        n.welfare_rate = 0.7;
        n.research = 0.05;
        n.knowledge = 0.04;
        n.savings *= 4.0;
        n.rate = 0.2;
    }
    let think = |e: &mut Economy, b: &Brains| {
        let mut brain = b.brains[me].clone().unwrap();
        strategy::think(&mut brain, &b.data, &b.geo, e, &ed);
        planning::plan(&mut brain, &b.data, &b.difficulty, e, &ed);
        (brain.goal, brain.threat, brain.counter, brain.estimates.clone(), brain.needs, brain.thinking.clone())
    };
    let seen = think(&mut e.clone(), &b);
    let blind = think(&mut hidden, &b);
    assert_eq!(seen, blind, "a brain's thinking changed with things it can't see");

    // And it does notice what its spies report (the test can tell).
    let mut visible = e.clone();
    for (j, n) in visible.nations.iter_mut().enumerate() {
        if j != me {
            n.gdp *= 3.0;
        }
    }
    assert_ne!(think(&mut visible, &b).3, seen.3, "estimates follow what the spies see");
}

/// The brains play by the player's rules: the autopilot never builds for
/// them (no state budget, no roads), everything the state of a brain nation
/// builds is an order from the player's own button, and every lever stays
/// in the range the player's buttons allow.
#[test]
fn brains_use_only_the_players_levers() {
    use crate::econ::player::Lever;
    let (data, atlas, ed, world, mut e) = world(0, 8);
    let mut b = Brains::new(AiData::load().unwrap(), &data, &atlas, &world, &mut e, 8, |i| i != 0);
    let roads: Vec<f64> = e.provinces.iter().map(|p| p.road + p.rail + p.port).collect();
    let mut built = 0;
    for _ in 0..720 {
        e.tick(&ed);
        b.day(&mut e, &ed);
        for n in e.nations.iter().skip(1) {
            assert!(n.brain && n.manual_taxes);
            for c in n.construction.iter().filter(|c| c.by_state) {
                assert!(c.by_player, "{}: the autopilot built for a brain", n.name);
            }
            for l in Lever::ALL {
                let (lo, hi) = Economy::lever_range(l);
                let v = e.lever_with(e.nations.iter().position(|x| x.name == n.name).unwrap(), l, Some(&ed));
                assert!(v >= lo - 1e-9 && v <= hi + 1e-9, "{}: {} at {v}", n.name, l.name());
            }
        }
        built += e.nations.iter().skip(1).map(|n| n.construction.iter().filter(|c| c.by_player).count()).sum::<usize>();
    }
    assert!(built > 0, "the brains built something with the build button");
    // Roads, rail and ports in a brain's land grow only by its own orders
    // (storms may wreck them; the autopilot builds only the player's).
    let orders: usize = b.brains.iter().flatten().map(|x| x.stats.taken_by.get("road").copied().unwrap_or(0) + x.stats.taken_by.get("port").copied().unwrap_or(0)).sum::<u32>() as usize;
    let mine: Vec<usize> = world.provinces_of(0);
    let grew = (0..e.provinces.len()).filter(|q| !mine.contains(q)).filter(|&q| e.provinces[q].road + e.provinces[q].rail + e.provinces[q].port > roads[q] + 1e-9).count();
    assert!(grew <= orders, "{grew} brain provinces got roads or ports with {orders} orders");
}

/// 25 nations at the game's top speed (three months a second) need about
/// 90 days of thinking a second: a year of it must take well under a second.
#[test]
fn the_brains_are_fast_enough_for_25_nations_at_top_speed() {
    let (data, atlas, ed, world, mut e) = world(0, 1);
    assert!(e.nations.len() >= 25, "a full world");
    let mut b = Brains::new(AiData::load().unwrap(), &data, &atlas, &world, &mut e, 1, |_| true);
    let mut thinking = std::time::Duration::ZERO;
    for _ in 0..ed.days_per_year {
        e.tick(&ed);
        let t = std::time::Instant::now();
        b.day(&mut e, &ed);
        thinking += t.elapsed();
    }
    let per_year = thinking.as_secs_f64();
    // Release builds take a few hundredths of a second; debug builds about
    // twenty times that.
    let limit = if cfg!(debug_assertions) { 12.0 } else { 1.0 };
    assert!(per_year < limit, "a year of 25 brains took {per_year:.2} s");
}

#[test]
fn the_battle_hook_gives_no_orders_yet() {
    use super::battle::{BattlePlanner, BattleView, NoBattles};
    assert!(NoBattles.plan(&BattleView::default()).is_empty());
}
