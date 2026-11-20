//! The player test: a whole game played headless by the bot, as a person
//! plays it: the menu, a new game, spinning, the economy (a tax, a building,
//! trade policy), diplomacy (a trade agreement and an alliance proposed),
//! a war declared on a neighbour and peace made, the settings, the help,
//! the log, a save, a load, and on to the game's end (a win or a loss),
//! playing on, the main menu and Continue. Every step is checked.

use duckforge::prelude::*;

use crate::bot::Bot;
use crate::game::Screen;

fn game() -> crate::game::Lucky {
    crate::new_game().unwrap_or_else(|e| panic!("{e}"))
}

/// A folder of its own for this test's saves.
fn saves() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("lucky-world-player-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    crate::save::set_folder(&dir);
    dir
}

#[test]
fn the_player_test_plays_a_whole_game_from_the_menu_to_the_end() {
    let dir = saves();
    let mut g = game();
    // A short game, so the ending comes in a test (the screen says 4 years).
    g.setup.years = 4;
    g.settings.autosave = true;
    let mut bot = Bot::new(g, 7);

    // The menu.
    bot.wait(5);
    assert!(bot.wait_until(30, |g| g.screen == Screen::Menu));
    assert_eq!(bot.sim.text("music"), "Menu", "the menu has its music");
    // Settings from the menu: the music a step quieter, then back.
    bot.taps(Key::Down, 3);
    bot.tap(Key::Enter);
    assert_eq!(bot.screen(), Screen::Settings);
    bot.taps(Key::Down, 3);
    let before = bot.g().settings.music;
    bot.tap(Key::Left);
    assert!(bot.g().settings.music < before, "Left lowers the music");
    bot.tap(Key::Escape);
    assert_eq!(bot.screen(), Screen::Menu);

    // A new game, spun at once, onto the map.
    assert!(bot.new_game(0, "hard"), "{:?}", bot.log);
    assert_eq!(bot.g().setup.difficulty, "hard");
    assert_eq!(bot.g().brains.as_ref().unwrap().difficulty_name, "hard");
    assert_eq!(bot.g().landed.len(), 16, "all sixteen wheels landed");
    bot.tap(Key::Digit3);
    bot.wait(60);
    assert!(bot.g().economy.as_ref().unwrap().day > 0, "the clock runs");

    // The economy: a tax, a building, trade policy.
    bot.tap(Key::O);
    assert!(bot.g().map_ui.econ.open.is_some());
    bot.tap(Key::Right);
    assert!(bot.toast().starts_with("Income tax"), "raising a tax is answered: {}", bot.toast());
    bot.tap(Key::Tab);
    bot.tap(Key::B);
    let t = bot.toast();
    assert!(t.starts_with("Building") || t.contains("store") || t.contains("room") || t.contains("can't"), "the build was answered: {t}");
    bot.note(format!("build: {t}"));
    bot.tap(Key::Tab);
    bot.tap(Key::T);
    bot.note(format!("trade policy: {}", bot.toast()));
    bot.tap(Key::O);
    assert!(bot.g().map_ui.econ.open.is_none());

    // Diplomacy: a trade agreement, then an alliance, proposed to the friendliest.
    bot.tap(Key::P);
    assert!(bot.g().map_ui.diplo.open.is_some());
    bot.taps(Key::Tab, 2);
    bot.tap(Key::Enter);
    let trade = bot.g().map_ui.diplo.message.clone().unwrap_or_default();
    assert!(!trade.is_empty(), "the trade proposal was answered");
    bot.note(format!("trade agreement: {trade}"));
    bot.taps(Key::Right, 3);
    bot.tap(Key::Enter);
    let ally = bot.g().map_ui.diplo.message.clone().unwrap_or_default();
    bot.note(format!("alliance: {ally}"));
    assert_ne!(ally, trade, "the alliance proposal got its own answer");
    bot.tap(Key::Escape);
    assert!(!bot.g().map_ui.diplo.wants_escape());

    // War on a neighbour, from the war room.
    let neighbours = bot.neighbours();
    let mut target = None;
    for _ in 0..3 {
        let Some(t) = bot.visit_one_of(&neighbours) else { break };
        bot.tap(Key::K);
        bot.tap(Key::D);
        assert!(bot.g().shell.confirm.as_ref().is_some_and(|c| c.danger), "declaring war asks first");
        bot.tap(Key::Y);
        let at_war = bot.g().wars.as_ref().unwrap().at_war(0, t).is_some();
        bot.tap(Key::Escape);
        if at_war {
            target = Some(t);
            break;
        }
    }
    let target = target.unwrap_or_else(|| panic!("couldn't go to war with a neighbour ({neighbours:?}): {:?}", bot.log));
    let enemy = bot.g().wars.as_ref().unwrap().names[target].clone();
    bot.note(format!("war on {enemy}"));
    bot.tap(Key::Escape); // the selection
    bot.tap(Key::Digit3);
    bot.wait(240);
    // Peace: offer it every few seconds and take theirs, until it's made.
    let mut peace = false;
    for _ in 0..30 {
        let w = bot.g().wars.as_ref().unwrap();
        if w.at_war(0, target).is_none() {
            peace = true;
            break;
        }
        bot.tap(Key::K);
        bot.tap(Key::Y); // their offer, if one waits
        bot.tap(Key::P); // ours
        bot.tap(Key::Escape);
        bot.wait(120);
    }
    assert!(peace, "no peace with {enemy} in time: {:?}", bot.log);
    bot.note(format!("peace with {enemy}"));
    let story = &bot.g().fate.wars;
    assert!(story.iter().any(|s| s.enemy == enemy && s.end.is_some()), "the story has the war, ended");

    // Help and the notification log.
    bot.tap(Key::F2);
    assert_eq!(bot.screen(), Screen::Help);
    bot.tap(Key::Down);
    bot.tap(Key::Escape);
    assert_eq!(bot.screen(), Screen::Map);
    assert!(bot.g().shell.pause, "back to the paused map");
    bot.tap(Key::Escape);
    bot.tap(Key::L);
    assert!(bot.g().notes.log_open && !bot.g().notes.log.is_empty(), "the log has notifications");
    bot.tap(Key::L);

    // Save from the pause menu (the quick save slot).
    assert!(bot.pause());
    bot.tap(Key::Down);
    bot.tap(Key::Enter);
    bot.tap(Key::Enter);
    let quick = crate::save::path_for("Quick save");
    assert!(quick.exists(), "the quick save slot was written");
    let saved_day = bot.g().economy.as_ref().unwrap().day;
    bot.tap(Key::Escape);
    assert!(!bot.g().shell.pause);
    bot.tap(Key::Digit5);
    bot.wait(200);
    assert!(bot.g().economy.as_ref().unwrap().day > saved_day + 30);
    // ...and load it again (F9): the clock goes back to the save's day.
    bot.tap(Key::F9);
    assert_eq!(bot.screen(), Screen::Map);
    assert_eq!(bot.g().economy.as_ref().unwrap().day, saved_day, "the loaded game is the saved one");
    bot.note("saved and loaded");

    // On to the end of the game (4 years), at top speed.
    bot.tap(Key::Digit5);
    let ended = bot.wait_until(20_000, |g| g.screen == Screen::Outcome);
    assert!(ended, "the game never ended: day {}", bot.g().economy.as_ref().unwrap().day);
    let outcome = bot.g().fate.outcome.expect("an outcome");
    bot.note(format!("the end: {}", outcome.title()));
    assert!(bot.g().fate.years.len() >= 3, "a year-by-year story");
    assert!(!bot.g().fate.moments.is_empty(), "best and worst moments");
    // An autosave each year on the way.
    assert!(crate::save::path_for("Autosave").exists(), "the yearly autosave");
    bot.wait(60);
    if outcome.can_play_on() {
        bot.tap(Key::Enter);
        assert_eq!(bot.screen(), Screen::Map, "playing on");
        bot.wait(120);
    }
    // To the main menu (saving first), and Continue loads it.
    assert!(bot.pause());
    bot.taps(Key::Down, 5);
    bot.tap(Key::Enter);
    assert!(bot.g().shell.confirm.is_some(), "leaving asks first");
    bot.tap(Key::Enter);
    assert_eq!(bot.screen(), Screen::Menu);
    assert!(matches!(bot.g().shell.slots.first().map(|s| &s.summary), Some(Ok(_))), "the menu lists the save");
    bot.tap(Key::Enter);
    assert_eq!(bot.screen(), Screen::Map, "Continue loaded the newest save");

    // Nothing went wrong on the way, and the sounds were there.
    assert!(bot.sim.failed_checks().is_empty(), "{:?}", bot.sim.failed_checks());
    for s in ["save", "build", "coin", "news", "menu_move"] {
        if s == "build" && !bot.log.iter().any(|l| l.contains("build: Building")) {
            continue;
        }
        assert!(!bot.sim.sound_frames(s).is_empty(), "no {s} sound");
    }
    assert!(!bot.sim.sound_frames("treaty").is_empty() || !bot.sim.sound_frames("denied").is_empty(), "the proposals were answered with a sound");
    assert!(!bot.sim.sound_frames("declare").is_empty(), "a war horn for the war");
    assert!(!bot.sim.sound_frames(if outcome.won() { "victory" } else { "defeat" }).is_empty());
    let music: Vec<String> = bot.sim.history("music").iter().filter_map(|(_, v)| if let ReportValue::Text(t) = v { Some(t.clone()) } else { None }).collect();
    assert!(music.iter().any(|m| m == "War") && music.iter().any(|m| m == "Peace"), "war and peace music: {music:?}");
    for line in &bot.log {
        println!("{line}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_tutorial_teaches_every_step_and_ends() {
    let mut bot = Bot::new(game(), 3);
    assert!(bot.wait_until(30, |g| g.screen == Screen::Menu));
    bot.tap(Key::Down); // Tutorial (no saves: New Game, Tutorial...)
    let row = bot.g().shell.row;
    if bot.g().shell.slots.iter().any(|s| s.summary.is_ok()) {
        bot.tap(Key::Down);
    }
    let _ = row;
    bot.tap(Key::Enter);
    assert_eq!(bot.screen(), Screen::Title);
    assert_eq!(bot.g().tutorial.as_ref().map(|t| t.step), Some(0));
    assert_eq!(bot.g().setup.difficulty, "easy");
    let step = |b: &Bot| b.g().tutorial.as_ref().map_or(usize::MAX, |t| t.step);
    // 1: spin. 2: two wheels. 3: skip.
    bot.tap(Key::Space);
    assert!(bot.wait_until(60, |g| g.tutorial.as_ref().is_some_and(|t| t.step == 1)));
    for _ in 0..2 {
        bot.wait(20);
        bot.tap(Key::Space); // spin
        assert!(bot.wait_until(900, |g| g.view.as_ref().is_some_and(|v| v.landed)));
        bot.wait(30);
        bot.tap(Key::Space); // on
    }
    assert!(bot.wait_until(120, |g| g.tutorial.as_ref().is_some_and(|t| t.step == 2)), "step {}", step(&bot));
    bot.tap(Key::S);
    assert!(bot.wait_until(60, |g| g.tutorial.as_ref().is_some_and(|t| t.step == 3)));
    bot.wait(30);
    bot.taps(Key::Space, 2);
    assert!(bot.wait_until(60, |g| g.tutorial.as_ref().is_some_and(|t| t.step == 4)));
    bot.wait(30);
    bot.tap(Key::Space);
    assert!(bot.wait_until(60, |g| g.tutorial.as_ref().is_some_and(|t| t.step == 5)));
    // 6: click a province of ours.
    bot.wait(30);
    let (_, at) = bot.g().clickable_province(0).expect("one of my provinces is on screen");
    bot.click(at);
    assert!(bot.wait_until(60, |g| g.tutorial.as_ref().is_some_and(|t| t.step == 6)), "step {}", step(&bot));
    bot.tap(Key::Digit3);
    assert!(bot.wait_until(60, |g| g.tutorial.as_ref().is_some_and(|t| t.step == 7)));
    bot.tap(Key::O);
    assert!(bot.wait_until(60, |g| g.tutorial.as_ref().is_some_and(|t| t.step == 8)));
    bot.tap(Key::Right);
    assert!(bot.wait_until(60, |g| g.tutorial.as_ref().is_some_and(|t| t.step == 9)));
    bot.tap(Key::Tab);
    bot.tap(Key::O);
    assert!(bot.wait_until(60, |g| g.tutorial.as_ref().is_some_and(|t| t.step == 10)));
    bot.tap(Key::K);
    bot.taps(Key::Tab, 2);
    assert!(bot.wait_until(60, |g| g.tutorial.as_ref().is_some_and(|t| t.step == 11)), "step {}", step(&bot));
    bot.tap(Key::Escape);
    assert!(bot.wait_until(60, |g| g.tutorial.as_ref().is_some_and(|t| t.step == 12)));
    bot.tap(Key::P);
    bot.taps(Key::Tab, 2);
    assert!(bot.wait_until(60, |g| g.tutorial.as_ref().is_some_and(|t| t.step == 13)), "step {}", step(&bot));
    bot.tap(Key::Enter);
    bot.tap(Key::Escape);
    assert!(bot.wait_until(60, |g| g.tutorial.as_ref().is_some_and(|t| t.step == 14)), "step {}", step(&bot));
    bot.tap(Key::Enter);
    assert!(bot.wait_until(30, |g| g.tutorial.is_none()), "the tutorial ends");
    assert!(bot.sim.failed_checks().is_empty(), "{:?}", bot.sim.failed_checks());
}

#[test]
fn a_loaded_save_from_another_version_is_listed_with_its_reason_not_crashed_on() {
    let dir = std::env::temp_dir().join(format!("lucky-world-old-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut bytes = b"LUCKYWORLD\n\0".to_vec();
    bytes.extend_from_slice(&999u32.to_le_bytes());
    bytes.extend_from_slice(&[0u8; 40]);
    std::fs::write(dir.join("future.lws"), &bytes).unwrap();
    std::fs::write(dir.join("junk.lws"), b"nonsense").unwrap();
    // Read through the same code the Load screen uses.
    let slots = crate::save::list_in(&dir);
    assert_eq!(slots.len(), 2);
    for s in &slots {
        let why = s.summary.as_ref().err().unwrap_or_else(|| panic!("{:?} should be refused", s.path));
        assert!(why.contains("newer") || why.contains("isn't a LUCKY WORLD save"), "{why}");
    }
    let _ = std::fs::remove_dir_all(dir);
}
