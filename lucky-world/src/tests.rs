//! The wheels' promises, checked over many thousands of rolled countries.

use duckforge::prelude::Rng;

use crate::country::{roll_country, roll_rivals, Country};
use crate::data::Data;
use crate::fate::{self, Nation};

fn data() -> Data {
    Data::load().unwrap_or_else(|e| panic!("{e}"))
}

/// Many countries on every map, each rolled alone.
fn many(data: &Data, per_map: usize) -> Vec<Country> {
    let mut rng = Rng::new(7);
    let mut out = Vec::new();
    for map in 0..data.maps.len() {
        for _ in 0..per_map {
            out.push(roll_country(data, map, Vec::new(), &[], true, &mut rng));
        }
    }
    out
}

#[test]
fn the_data_files_load() {
    let d = data();
    assert_eq!(d.wheels.len(), 16);
    for (round, count) in [(1, 4), (2, 4), (3, 5), (4, 3)] {
        assert_eq!(d.round_wheels(round).len(), count, "round {round}");
    }
    // Every wheel id the game's own code names.
    for id in crate::WHEELS_THE_CODE_NAMES {
        assert!(d.wheel_index(id).is_some(), "the code uses a wheel \"{id}\" that wheels.toml doesn't have");
    }
}

#[test]
fn a_typo_in_a_rule_is_one_clear_sentence() {
    let wheels = std::fs::read_to_string("data/wheels.toml").unwrap();
    let maps = std::fs::read_to_string("data/maps.toml").unwrap();
    let names = std::fs::read_to_string("data/names.toml").unwrap();
    let broken = wheels.replace("if = \"coast=landlocked\"\n  times = { steppe", "if = \"coast=landlockd\"\n  times = { steppe");
    let e = Data::parse(&broken, &maps, &names).unwrap_err();
    assert!(e.0.contains("landlockd") && e.0.contains("wheel \"land\""), "{e}");
    let later = wheels.replace("if = \"continent#island\"\n  times = { tiny", "if = \"gdp>5\"\n  times = { tiny");
    let e = Data::parse(&later, &maps, &names).unwrap_err();
    assert!(e.0.contains("spun after it"), "{e}");
}

#[test]
fn chances_add_up_and_thin_options_leave_the_wheel() {
    let d = data();
    for c in many(&d, 300) {
        for w in 0..16 {
            let o = fate::odds(&d, c.map, w, &c.landed[..w], &[]);
            let total: f64 = o.chances.iter().sum();
            assert!((total - 1.0).abs() < 1e-9, "wheel {w} adds up to {total}");
            assert!(o.chances.iter().all(|&p| p == 0.0 || p >= d.min_share * 0.999 || p == o.chances.iter().cloned().fold(0.0, f64::max)));
            assert!(o.chances[c.landed[w].option] > 0.0, "landed on an option that wasn't on the wheel");
        }
    }
}

#[test]
fn nobody_is_landlocked_on_an_island() {
    let d = data();
    let isles = d.maps.iter().position(|m| m.id == "isles").unwrap();
    let mut on_isles = 0;
    for c in many(&d, 3000) {
        let continent = &d.maps[c.map].continents[c.landed[0].option];
        if continent.tags.iter().any(|t| t == "island") {
            assert_ne!(c.option_id(&d, "coast"), "landlocked", "{} on {}", c.name, continent.label);
        }
        if c.map == isles {
            on_isles += 1;
            assert_ne!(c.option_id(&d, "coast"), "landlocked");
        }
    }
    assert!(on_isles > 0);
}

#[test]
fn gdp_follows_population_and_only_tiny_nations_are_tiny_but_rich() {
    let d = data();
    let mut rich = 0;
    for c in many(&d, 4000) {
        let gdp = c.value(&d, "gdp");
        let people = c.value(&d, "population");
        assert!((5e7..=3e12).contains(&gdp), "GDP {gdp}");
        let per_person = gdp / people;
        if c.option_id(&d, "gdp") == "tiny_rich" {
            rich += 1;
            assert!(people <= 3e6, "tiny but rich with {people} people");
        } else {
            // Real countries go from about $250 to $130,000 a person; allow the rolls some room.
            assert!(
                (40.0..=400_000.0).contains(&per_person),
                "{}: ${per_person:.0} a person ({} people, GDP {})",
                c.name,
                people,
                gdp
            );
        }
    }
    // Rare, but it happens.
    assert!(rich > 0, "nobody was tiny but rich in 12,000 countries");
    assert!(rich < 600, "tiny but rich {rich} times in 12,000: not rare");
}

#[test]
fn population_fits_the_land() {
    let d = data();
    for c in many(&d, 3000) {
        let density = c.value(&d, "population") / c.value(&d, "size");
        // Monaco has about 19,000 a km2, Mongolia 2.
        assert!((0.05..=60_000.0).contains(&density), "{}: {density:.2} people per km2", c.name);
    }
}

#[test]
fn armies_fit_their_money_and_their_soldiers() {
    let d = data();
    let army = d.wheel("army");
    for c in many(&d, 4000) {
        let level = c.landed[army].level;
        let gdp = c.value(&d, "gdp");
        let duty = c.value(&d, "duty");
        if c.option_id(&d, "army") == "strongest" {
            assert!(gdp >= 1e12, "world's strongest army on ${gdp:.0}");
        }
        // A major army needs real money, or (a junta's way) a great many soldiers.
        if level >= 6.0 {
            assert!(gdp >= 1e10, "{}: a major army on {}", c.name, crate::country::money(gdp));
            assert!(gdp >= 5e10 || c.troops(&d) >= 100_000.0, "{}: a major army on {} with {:.0} soldiers", c.name, crate::country::money(gdp), c.troops(&d));
        }
        // No army means nobody on duty, and an army means somebody is.
        assert_eq!(c.option_id(&d, "army") == "none", duty == 0.0, "{}", c.name);
        // Plenty of soldiers for a strong army.
        if level >= 5.0 {
            assert!(c.troops(&d) >= 1_000.0, "{}: a strong army of {:.0}", c.name, c.troops(&d));
        }
    }
}

#[test]
fn colonies_have_a_history_that_makes_sense() {
    let d = data();
    for c in many(&d, 3000) {
        if c.option_id(&d, "ideology") == "colony" {
            let h = c.option_id(&d, "history");
            assert!(!["ex_colony", "former_empire", "breakaway", "revolution"].contains(&h.as_str()), "a colony that is {h}");
        }
        if c.option_id(&d, "coast") == "landlocked" {
            assert_ne!(c.option_id(&d, "fate"), "great_harbor");
        }
    }
}

#[test]
fn only_one_worlds_strongest_army_and_one_regional_power_per_continent() {
    let d = data();
    let mut rng = Rng::new(11);
    let army = d.wheel("army");
    let impact = d.wheel("impact");
    let strongest = d.wheels[army].options.iter().position(|o| o.id == "strongest").unwrap();
    let power = d.wheels[impact].options.iter().position(|o| o.id == "regional_power").unwrap();
    for round in 0..300 {
        let map = round % 3;
        let player = roll_country(&d, map, Vec::new(), &[], true, &mut rng);
        let mut world = roll_rivals(&d, &player, &mut rng);
        assert_eq!(world.len(), d.maps[map].rivals);
        world.push(player);
        let n = world.iter().filter(|c| c.landed[army].option == strongest).count();
        assert!(n <= 1, "{n} world's strongest armies");
        for continent in 0..d.maps[map].continents.len() {
            let n = world
                .iter()
                .filter(|c| c.landed[0].option == continent && c.landed[impact].option == power)
                .count();
            assert!(n <= 1, "{n} regional powers on one continent");
        }
        let mut names: Vec<_> = world.iter().map(|c| c.name.clone()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), world.len(), "two nations share a name");
    }
}

#[test]
fn the_same_seed_rolls_the_same_world() {
    let d = data();
    let roll = || {
        let mut rng = Rng::new(42);
        let player = roll_country(&d, 1, Vec::new(), &[], true, &mut rng);
        let rivals = roll_rivals(&d, &player, &mut rng);
        (player.name.clone(), player.landed.clone(), rivals.iter().map(|r| r.name.clone()).collect::<Vec<_>>())
    };
    assert_eq!(roll(), roll());
}

#[test]
fn skip_spin_keeps_what_was_already_spun() {
    let d = data();
    let mut rng = Rng::new(3);
    let full = roll_country(&d, 2, Vec::new(), &[], true, &mut rng);
    let kept = full.landed[..5].to_vec();
    let skipped = roll_country(&d, 2, kept.clone(), &[], true, &mut rng);
    assert_eq!(&skipped.landed[..5], &kept[..]);
    assert_eq!(skipped.landed.len(), 16);
}

/// Not a check: prints how often each option comes up on each map, to tune
/// the weights by. `cargo test --release odds_report -- --ignored --nocapture`
#[test]
#[ignore]
fn odds_report() {
    let d = data();
    let countries = many(&d, 20_000);
    for (m, map) in d.maps.iter().enumerate() {
        println!("\n=== {} ===", map.name);
        let here: Vec<&Country> = countries.iter().filter(|c| c.map == m).collect();
        for (w, wheel) in d.wheels.iter().enumerate() {
            let options = d.options_on(w, m);
            let mut counts = vec![0usize; options.len()];
            for c in &here {
                counts[c.landed()[w].option] += 1;
            }
            let line: Vec<String> = options
                .iter()
                .zip(&counts)
                .map(|(o, n)| format!("{} {:.1}%", o.label, 100.0 * *n as f64 / here.len() as f64))
                .collect();
            println!("{:<20} {}", wheel.title, line.join(" | "));
        }
    }
}


// ---------------------------------------------------------------------------
// The screens, played with no window
// ---------------------------------------------------------------------------

use duckforge::prelude::{PlayTest, Simulation};

fn game() -> crate::game::Lucky {
    crate::new_game().unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn the_playthrough_spins_all_sixteen_and_meets_the_rivals() {
    let played = PlayTest::new(crate::config(), game()).inputs("playthroughs/lucky_world.json").run();
    assert_eq!(played.text("screen"), "Rivals");
    assert_eq!(played.number("wheels_landed"), 16.0);
    assert_eq!(played.number("rivals"), 9.0);
    let g = played.game();
    let player = g.player.as_ref().unwrap();
    assert_eq!(player.landed, g.landed, "the country card shows what the wheels landed on");
    assert!(!g.skipped);
    // Every spin ticked and landed with a sound.
    assert!(played.sound_frames("tick").len() > 100);
    assert_eq!(played.sound_frames("land").len(), 17); // 16 wheels and the full country card
}

#[test]
fn skip_spin_on_the_title_rolls_every_wheel_at_once() {
    let played = PlayTest::new(crate::config(), game())
        .script(r#"{"events": [{"frame": 10, "press": "S"}, {"frame": 12, "release": "S"}]}"#)
        .frames(30)
        .run();
    assert_eq!(played.text("screen"), "Final");
    assert_eq!(played.number("wheels_landed"), 16.0);
    assert!(played.game().skipped);
}

#[test]
fn skip_spin_part_way_keeps_the_wheels_already_spun() {
    let mut sim = Simulation::new(crate::config(), game());
    sim.run(5);
    sim.press(duckforge::prelude::Key::Space);
    sim.step();
    sim.release(duckforge::prelude::Key::Space);
    // Land the first wheel, go on to the second and let it land too.
    for _ in 0..2 {
        sim.run(420);
        sim.press(duckforge::prelude::Key::Space);
        sim.step();
        sim.release(duckforge::prelude::Key::Space);
    }
    let before = sim.game().landed.clone();
    assert_eq!(before.len(), 2);
    // The third wheel is spinning now: skip.
    sim.run(150);
    assert!(sim.game().view.as_ref().unwrap().spinning());
    sim.press(duckforge::prelude::Key::S);
    sim.step();
    let g = sim.game();
    assert_eq!(g.screen, crate::game::Screen::Final);
    assert_eq!(&g.landed[..2], &before[..]);
    assert_eq!(g.landed.len(), 16);
    assert!(sim.failed_checks().is_empty(), "{:?}", sim.failed_checks());
}

#[test]
fn the_island_map_never_lands_on_landlocked() {
    // Choose The Thousand Isles, then spin the Land round many times over.
    for seed in 1..=12 {
        let played = PlayTest::new(crate::config(), game())
            .seed(seed)
            .script(r#"{"events": [{"frame": 5, "press": "Digit2"}, {"frame": 6, "release": "Digit2"}, {"frame": 10, "press": "S"}, {"frame": 11, "release": "S"}]}"#)
            .frames(20)
            .run();
        assert_eq!(played.text("map"), "isles");
        let g = played.game();
        let p = g.player.as_ref().unwrap();
        assert_ne!(p.option_id(&g.data, "coast"), "landlocked");
    }
}
