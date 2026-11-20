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
        assert!((1e9..=3e12).contains(&gdp), "GDP {gdp}");
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

/// Every wheel spun by hand, one by one: a wheel never spins until it is
/// told to, each lands, and the game goes on to the map.
#[test]
fn the_playthrough_spins_every_wheel_by_hand_and_goes_onto_the_map() {
    use duckforge::prelude::Key;
    let mut g = game();
    g.menu_started = false;
    let mut bot = crate::bot::Bot::new(g, 1);
    // Bot::new starts on the menu; this test starts from the New Game screen.
    bot.wait(5);
    assert!(bot.click_on("New Game"), "{:?}", bot.log);
    let wheels = bot.g().data.wheels.len();
    bot.tap(Key::Digit2);
    bot.tap(Key::Space);
    let mut cards = 0;
    while bot.g().landed.len() < wheels || matches!(bot.screen(), crate::game::Screen::Round(_)) {
        match bot.screen() {
            crate::game::Screen::Wheel => {
                let before = bot.g().landed.len();
                bot.wait(60);
                assert_eq!(bot.g().landed.len(), before, "wheel {} spun by itself", before + 1);
                assert!(!bot.g().view.as_ref().unwrap().spinning(), "wheel {} spins only when told", before + 1);
                bot.tap(Key::Space);
                assert!(bot.wait_until(1200, |g| g.landed.len() == before + 1), "wheel {} never landed", before + 1);
                bot.wait(30);
                bot.tap(Key::Space);
            }
            crate::game::Screen::Round(_) => {
                cards += 1;
                bot.wait(30);
                bot.tap(Key::Space);
            }
            other => panic!("on {other:?} with {} of {wheels} wheels landed", bot.g().landed.len()),
        }
        assert!(bot.sim.context().frame() < 60_000, "it took too long");
    }
    assert_eq!(cards, 4, "a card after each of the four rounds");
    assert!(bot.wait_until(60, |g| g.screen == crate::game::Screen::Final));
    bot.wait(40);
    bot.tap(Key::Space);
    bot.wait(10);
    bot.tap(Key::Space);
    assert!(bot.wait_until(120, |g| g.screen == crate::game::Screen::Rivals));
    bot.wait(30);
    bot.tap(Key::Space);
    assert!(bot.wait_until(120, |g| g.screen == crate::game::Screen::Map));
    bot.wait(10);
    let played = &bot.sim;
    assert_eq!(played.text("screen"), "Map");
    assert_eq!(played.number("wheels_landed"), wheels as f64);
    assert!(played.number("main_nations") >= 6.0);
    assert!(played.number("nations") > 6.0 && played.number("nations") <= 25.0);
    let g = played.game();
    let player = g.player.as_ref().unwrap();
    assert_eq!(player.landed, g.landed, "the country card shows what the wheels landed on");
    assert!(!g.skipped);
    // Every spin ticked and landed with a sound; the map opened with its own.
    assert!(played.sound_frames("tick").len() > 100);
    assert_eq!(played.sound_frames("land").len(), wheels + 1, "each wheel and the full country card");
    assert_eq!(played.sound_frames("open_map").len(), 1);
    assert_eq!(played.text("map_mode"), "Political");
    assert!(played.failed_checks().is_empty(), "{:?}", played.failed_checks());
}

/// Whether a reported text ever had this value during the run.
fn seen(played: &Simulation<crate::game::Lucky>, name: &str, value: &str) -> bool {
    played.history(name).iter().any(|(_, v)| matches!(v, duckforge::prelude::ReportValue::Text(t) if t == value))
}

#[test]
fn the_economy_playthrough_runs_the_clock_and_every_screen() {
    let played = PlayTest::new(crate::config(), game()).inputs("playthroughs/economy.json").run();
    assert_eq!(played.text("screen"), "Map");
    // Speed 5 for about 24 seconds: some 700 days went by, then the clock was paused.
    assert!(played.number("day") > 600.0, "only {} days passed", played.number("day"));
    assert!(played.flag("paused"));
    assert_eq!(played.number("speed"), 5.0);
    // Every page of the economy window was opened, then it was closed.
    for tab in ["BUDGET", "CHAINS", "MARKET", "TRADE", "STOCKPILE", "CHARTS"] {
        assert!(seen(&played, "econ_tab", tab), "the {tab} page never opened");
    }
    assert_eq!(played.text("econ_tab"), "none");
    // The overlays were shown, then turned off.
    assert!(seen(&played, "overlay", "Resources") && seen(&played, "overlay", "Industry") && seen(&played, "overlay", "War"));
    assert_eq!(played.text("overlay"), "Off");
    // Raising income tax costs a coin; ordering a building is answered:
    // built (a coin) or why not (the world decides: a store may be short).
    assert!(!played.sound_frames("coin").is_empty());
    let toasts: Vec<String> = played.history("toast").iter().filter_map(|(_, v)| if let duckforge::prelude::ReportValue::Text(t) = v { Some(t.clone()) } else { None }).collect();
    assert!(toasts.iter().any(|t| t.starts_with("Income tax")), "{toasts:?}");
    assert!(toasts.iter().any(|t| t.starts_with("Building") || t.contains("in store to build") || t.contains("has room")), "the build was answered: {toasts:?}");
    assert!(!played.sound_frames("month").is_empty(), "no month chimes");
    assert!(played.failed_checks().is_empty(), "{:?}", played.failed_checks());
}

/// The highest number a report reached during the run.
fn most(played: &Simulation<crate::game::Lucky>, name: &str) -> f64 {
    played.history(name).iter().filter_map(|(_, v)| if let duckforge::prelude::ReportValue::Number(x) = v { Some(*x) } else { None }).fold(f64::MIN, f64::max)
}

/// `--spectate-war`: a world of brains opens with a declared war, watched
/// from the first shot to the peace, with every war screen opened on the way.
#[test]
fn the_war_playthrough_watches_a_whole_war_from_first_shot_to_peace() {
    let mut g = game();
    g.spectate_at_start = true;
    g.start_war = true;
    let played = PlayTest::new(crate::config(), g).inputs("playthroughs/war.json").run();
    assert_eq!(played.text("screen"), "Map");
    assert!(played.flag("spectating"));
    // The war was declared on day one, fought and ended in a peace.
    assert!(most(&played, "war_level_max") >= 3.0, "it never became a declared war");
    assert!(most(&played, "battle_reports") >= 10.0, "only {} battles", most(&played, "battle_reports"));
    assert!(most(&played, "dead_total") > 1000.0);
    let ended = played.history("last_war_ended");
    assert!(!ended.is_empty(), "no war ended");
    // A battle was watched up close, and every page of the war room was opened.
    assert!(most(&played, "watching_battle") >= 0.0, "no battle was watched");
    for page in ["WARS", "ARMY", "SUPPLY", "REPORTS"] {
        assert!(seen(&played, "war_room", page), "the {page} page never opened");
    }
    assert_eq!(played.text("war_room"), "closed");
    assert!(played.history("report_open").iter().any(|(_, v)| matches!(v, duckforge::prelude::ReportValue::Bool(true))), "no report was read");
    assert!(seen(&played, "overlay", "War"));
    // Gunfire when battles start, explosions when they end, drums while it lasts.
    assert!(!played.sound_frames("gunfire").is_empty());
    assert!(!played.sound_frames("explosion").is_empty());
    assert!(played.failed_checks().is_empty(), "{:?}", played.failed_checks());
}

/// The player declares war on a neighbour, gives orders, runs the supply
/// screen (Auto off, a request approved), reads a battle report and offers peace.
#[test]
fn the_player_war_playthrough_declares_orders_supplies_and_reads_reports() {
    use duckforge::prelude::Key;
    let mut bot = crate::bot::Bot::new(game(), 1);
    assert!(bot.new_game(0, "normal"), "{:?}", bot.log);
    // Choose a neighbour on the map, then declare war on it from the war room.
    // (The neighbour with the biggest army: a small one may give up before a battle.)
    let mut neighbours = bot.neighbours();
    {
        let w = bot.g().wars.as_ref().unwrap();
        let army = |n: usize| w.divisions.iter().filter(|d| d.alive && w.nation_of(d.faction) == n).count();
        neighbours.sort_by_key(|&n| std::cmp::Reverse(army(n)));
        neighbours.truncate(1);
    }
    let target = bot.visit_one_of(&neighbours).unwrap_or_else(|| panic!("no neighbour to choose: {neighbours:?}"));
    bot.tap(Key::K);
    assert!(bot.g().map_ui.war.open.is_some(), "the war room opened");
    bot.tap(Key::L); // a bigger war to begin with
    bot.tap(Key::D);
    assert!(bot.g().shell.confirm.as_ref().is_some_and(|c| c.danger), "declaring war asks first, in red");
    bot.tap(Key::Y);
    assert!(bot.g().wars.as_ref().unwrap().at_war(0, target).is_some(), "at war with the chosen neighbour");
    // The army: orders for the first divisions.
    bot.tap(Key::Tab);
    for _ in 0..5 {
        bot.tap(Key::A);
        bot.tap(Key::Down);
    }
    // Supply: the first front up to high priority, Auto off.
    bot.tap(Key::Tab);
    bot.tap(Key::Right);
    bot.tap(Key::T);
    bot.tap(Key::K);
    assert!(bot.g().map_ui.war.open.is_none());
    // Let the war run until a battle has been fought and a request waits.
    bot.tap(Key::Digit3);
    let fought = bot.wait_until(6000, |g| g.map_ui.war.card.is_some());
    {
        let w = bot.g().wars.as_ref().unwrap();
        assert!(fought, "no battle in time (day {}, at war {:?}, reports {}, battles {}, wars {:?}, divisions mine {})", bot.g().economy.as_ref().unwrap().day, w.at_war(0, target), w.reports.len(), w.battles.len(), w.wars.iter().map(|x| (x.sides.clone(), x.cause.clone())).collect::<Vec<_>>(), w.divisions.iter().filter(|d| d.alive && d.faction == 0).count());
    }
    // The battle's card: Enter opens its report.
    bot.tap(Key::Enter);
    assert!(bot.g().map_ui.war.full.is_some(), "the battle report opened");
    bot.tap(Key::Escape);
    assert!(bot.g().map_ui.war.full.is_none() && !bot.g().shell.pause);
    // Supply: approve a request when one comes in.
    assert!(bot.wait_until(8000, |g| g.wars.as_ref().unwrap().forces[0].requests.iter().any(|r| r.answered.is_none())), "no supply request in time");
    bot.tap(Key::Space); // the clock stops while we read
    bot.tap(Key::K);
    bot.taps(Key::Tab, 2);
    bot.tap(Key::Y);
    // Reports, then peace offered from the wars page.
    bot.tap(Key::Tab);
    bot.tap(Key::Enter);
    bot.tap(Key::Escape);
    bot.tap(Key::Tab);
    bot.tap(Key::P);
    bot.tap(Key::K);
    bot.tap(Key::Space);
    bot.wait(60);
    let played = &bot.sim;
    let most = |name: &str| most(played, name);
    assert_eq!(played.text("screen"), "Map");
    assert!(!played.flag("spectating"));
    assert!(most("player_war_level") >= 3.0, "the player declared war");
    assert!(most("divisions_order_not_auto") > 0.0, "no order was given");
    assert!(played.history("supply_auto").iter().any(|(_, v)| matches!(v, duckforge::prelude::ReportValue::Bool(false))), "Auto was never turned off");
    assert_eq!(most("priority_0"), 3.0);
    assert!(played.history("report_open").iter().any(|(_, v)| matches!(v, duckforge::prelude::ReportValue::Bool(true))), "no report was read");
    assert!(played.history("report_card").iter().any(|(_, v)| matches!(v, duckforge::prelude::ReportValue::Bool(true))), "no battle report card after a battle");
    let g = played.game();
    let w = g.wars.as_ref().unwrap();
    assert!(w.forces[0].requests.iter().any(|r| r.answered == Some(true)), "no request was approved");
    assert!(w.wars.iter().any(|x| x.sides[0].contains(&0) && x.cause == "the player's choice"));
    assert!(played.failed_checks().is_empty(), "{:?}", played.failed_checks());
}

/// The diplomacy and spy screens: relations and reasons, a proposal and its
/// answer, the inbox, the talks, a spy trained and sent, both new overlays.
#[test]
fn the_diplomacy_playthrough_proposes_signs_and_spies() {
    let played = PlayTest::new(crate::config(), game()).inputs("playthroughs/diplomacy.json").run();
    assert_eq!(played.text("screen"), "Map");
    for page in ["NATIONS", "TREATIES", "PROPOSE", "INBOX", "TALKS"] {
        assert!(seen(&played, "diplo_page", page), "the {page} page never opened");
    }
    for page in ["AGENTS", "MISSIONS", "REPORTS"] {
        assert!(seen(&played, "spy_page", page), "the {page} spy page never opened");
    }
    assert_eq!(played.text("diplo_page"), "closed");
    assert_eq!(played.text("spy_page"), "closed");
    // Every proposal was answered with a reason.
    let answers: Vec<String> = played.history("diplo_message").iter().filter_map(|(_, v)| if let duckforge::prelude::ReportValue::Text(t) = v { Some(t.clone()) } else { None }).collect();
    assert!(answers.iter().any(|a| a.contains(": No: ") || a.contains(": Yes: ")), "{answers:?}");
    assert!(answers.iter().any(|a| a.starts_with("Signed")), "a deal from the inbox was signed: {answers:?}");
    assert!(answers.iter().any(|a| a.contains("in training")), "an agent was trained");
    assert!(answers.iter().any(|a| a.contains("agents sent") || a.contains("agent sent")), "a mission was sent");
    assert!(most(&played, "treaties_ours") >= 1.0);
    assert!(seen(&played, "overlay", "Alliances") && seen(&played, "overlay", "Relations"));
    assert!(played.failed_checks().is_empty(), "{:?}", played.failed_checks());
}

#[test]
fn skip_spin_on_the_title_rolls_every_wheel_at_once() {
    let played = PlayTest::new(crate::config(), game())
        .script(r#"{"events": [{"frame": 10, "press": "S"}, {"frame": 12, "release": "S"}]}"#)
        .frames(30)
        .run();
    assert_eq!(played.text("screen"), "Final");
    assert_eq!(played.number("wheels_landed"), played.game().data.wheels.len() as f64);
    assert!(played.game().skipped);
}

#[test]
fn skip_spin_part_way_keeps_the_wheels_already_spun() {
    use duckforge::prelude::Key;
    let mut sim = Simulation::new(crate::config(), game());
    let tap = |sim: &mut Simulation<crate::game::Lucky>, k: Key| {
        sim.press(k);
        sim.step();
        sim.release(k);
        sim.step();
    };
    sim.run(5);
    tap(&mut sim, Key::Space); // the title: on to the first wheel
    // Spin the first wheel and the second, each landing.
    for n in 1..=2 {
        sim.run(20);
        tap(&mut sim, Key::Space); // spin
        sim.run(700);
        assert_eq!(sim.game().landed.len(), n);
        tap(&mut sim, Key::Space); // on to the next wheel
    }
    let before = sim.game().landed.clone();
    // The third wheel, spinning now: skip.
    sim.run(20);
    tap(&mut sim, Key::Space);
    sim.run(40);
    assert!(sim.game().view.as_ref().unwrap().spinning());
    sim.press(Key::S);
    sim.step();
    let g = sim.game();
    assert_eq!(g.screen, crate::game::Screen::Final);
    assert_eq!(&g.landed[..2], &before[..]);
    assert_eq!(g.landed.len(), g.data.wheels.len());
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

// ---------------------------------------------------------------------------
// Phase 2: the maps and who owns what
// ---------------------------------------------------------------------------

use crate::mapdata::Atlas;
use crate::world::{Coast, World};

fn atlas(d: &Data) -> Atlas {
    Atlas::load(d).unwrap_or_else(|e| panic!("{e}"))
}

/// A whole world on one map, from one seed.
fn world_on(d: &Data, a: &Atlas, map: usize, seed: u32) -> World {
    let mut rng = Rng::new(seed);
    let player = roll_country(d, map, Vec::new(), &[], true, &mut rng);
    let rivals = roll_rivals(d, &player, &mut rng);
    World::build(d, a, player, rivals, &mut rng)
}

#[test]
fn the_three_maps_load_and_every_land_hex_has_a_province() {
    let d = data();
    let a = atlas(&d);
    assert_eq!(a.maps.len(), 3);
    for m in &a.maps {
        for i in 0..m.cells.len() {
            let land = a.terrain_of(m, i).kind == crate::mapdata::TerrainKind::Land;
            assert_eq!(land, m.province_of[i].is_some(), "{} hex {:?}", m.id, m.cell(i));
        }
        for (p, prov) in m.provinces.iter().enumerate() {
            assert!(!prov.cells.is_empty(), "{} has an empty province {}", m.id, prov.id);
            for &q in &prov.neighbours {
                assert!(m.provinces[q].neighbours.contains(&p), "{}: {} and {} aren't neighbours both ways", m.id, prov.id, m.provinces[q].id);
            }
        }
        // Each map plays differently: the Grand Continent is mostly one landmass, the isles are many.
        let biggest = (0..m.landmass_count).map(|l| m.provinces_on(l).len()).max().unwrap();
        println!("{}: {} provinces, {} landmasses, biggest {}", m.id, m.provinces.len(), m.landmass_count, biggest);
        match m.id.as_str() {
            "grand" => assert!(biggest * 10 >= m.provinces.len() * 8),
            "isles" => assert!(m.landmass_count >= 15 && biggest * 4 <= m.provinces.len()),
            _ => assert!(m.landmass_count >= 5),
        }
    }
}

#[test]
fn a_mistake_in_a_map_is_one_clear_sentence() {
    let d = data();
    let a = atlas(&d);
    let terrains = a.terrains.clone();
    let grid = std::fs::read_to_string("data/maps/grand.txt").unwrap();
    let file = std::fs::read_to_string("data/maps/grand.toml").unwrap();
    let bad_letter = grid.replacen('P', "Q", 1);
    let e = crate::mapdata::parse_map(&d, 0, &terrains, &bad_letter, &file).unwrap_err();
    assert!(e.0.contains("\"Q\" isn't a terrain"), "{e}");
    let bad_continent = file.replacen("continent = \"heartland\"", "continent = \"atlantis\"", 1);
    let e = crate::mapdata::parse_map(&d, 0, &terrains, &grid, &bad_continent).unwrap_err();
    assert!(e.0.contains("atlantis"), "{e}");
}

#[test]
fn every_province_belongs_to_at_most_one_nation() {
    let d = data();
    let a = atlas(&d);
    for map in 0..3 {
        for seed in 1..=8 {
            let w = world_on(&d, &a, map, seed);
            let mut claimed = vec![0usize; w.owner.len()];
            for n in 0..w.nations.len() {
                let mine = w.provinces_of(n);
                assert!(!mine.is_empty(), "{} has no land", w.nations[n].name);
                assert!(mine.contains(&w.capital[n]), "{}'s capital isn't its own", w.nations[n].name);
                for p in mine {
                    claimed[p] += 1;
                }
            }
            assert!(claimed.iter().all(|&k| k <= 1));
            assert!(w.nations.len() <= a.settings.max_nations);
        }
    }
}

#[test]
fn landlocked_nations_never_touch_the_sea() {
    let d = data();
    let a = atlas(&d);
    let mut seen = 0;
    for map in 0..3 {
        for seed in 1..=15 {
            let w = world_on(&d, &a, map, seed);
            for n in 0..w.nations.len() {
                if Coast::of(&d, &w.nations[n]) == Coast::Landlocked {
                    seen += 1;
                    assert!(!w.touches_sea(a.map(map), n), "{} is landlocked but touches the sea", w.nations[n].name);
                }
            }
        }
    }
    assert!(seen > 10, "only {seen} landlocked nations: the test proves little");
}

#[test]
fn the_island_map_never_has_a_landlocked_nation() {
    let d = data();
    let a = atlas(&d);
    let isles = d.maps.iter().position(|m| m.id == "isles").unwrap();
    for seed in 1..=25 {
        let w = world_on(&d, &a, isles, seed);
        for n in 0..w.nations.len() {
            assert_ne!(Coast::of(&d, &w.nations[n]), Coast::Landlocked, "{}", w.nations[n].name);
            assert!(w.touches_sea(a.map(isles), n));
        }
    }
}

#[test]
fn island_nations_share_no_land_border() {
    let d = data();
    let a = atlas(&d);
    let mut seen = 0;
    for map in 0..3 {
        for seed in 1..=15 {
            let w = world_on(&d, &a, map, seed);
            for n in 0..w.nations.len() {
                if Coast::of(&d, &w.nations[n]) == Coast::Island {
                    seen += 1;
                    assert!(w.land_neighbours(a.map(map), n).is_empty(), "{} is an island nation with land neighbours", w.nations[n].name);
                }
            }
        }
    }
    assert!(seen > 5, "only {seen} island nations");
}

#[test]
fn the_same_seed_gives_the_same_world() {
    let d = data();
    let a = atlas(&d);
    for map in 0..3 {
        let one = world_on(&d, &a, map, 99);
        let two = world_on(&d, &a, map, 99);
        assert_eq!(one.owner, two.owner);
        assert_eq!(one.capital, two.capital);
        let names = |w: &World| w.nations.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&one), names(&two));
        let other = world_on(&d, &a, map, 100);
        assert_ne!(one.owner, other.owner, "a different seed should give a different world");
    }
}

#[test]
fn nations_sit_where_their_wheels_say_and_the_land_gets_covered() {
    let d = data();
    let a = atlas(&d);
    for map in 0..3 {
        let mut placed = 0;
        let mut on_wheel_continent = 0;
        let mut coverage = 0.0;
        for seed in 1..=12 {
            let w = world_on(&d, &a, map, seed);
            coverage += w.coverage() / 12.0;
            assert!(w.nations.len() > w.main_count, "no nations were made to fill the land");
            let m = a.map(map);
            for n in 0..w.nations.len() {
                placed += 1;
                if m.provinces[w.capital[n]].continent == w.nations[n].landed[0].option {
                    on_wheel_continent += 1;
                } else {
                    assert!(w.notes[n].is_some(), "{} isn't on its continent and nobody said why", w.nations[n].name);
                }
            }
            // Bigger wheels, bigger countries: the biggest nation by provinces is at least medium.
            let biggest = (0..w.nations.len()).max_by_key(|&n| w.provinces_of(n).len()).unwrap();
            assert!(w.provinces_of(biggest).len() >= 4);
        }
        println!("map {map}: {:.0}% of nations on their wheel's continent, {:.1}% of land owned", 100.0 * on_wheel_continent as f64 / placed as f64, coverage * 100.0);
        assert!(on_wheel_continent * 10 >= placed * 8);
        assert!(coverage > 0.9, "only {:.0}% of the land owned", coverage * 100.0);
    }
}
