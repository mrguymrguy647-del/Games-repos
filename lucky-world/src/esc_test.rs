//! The Esc test: Esc never quits the game and never leaves a game. It
//! closes what is open on top; with nothing open it opens the pause menu,
//! on every in-game screen (the wheels included). Leaving to the main menu
//! or the desktop always asks first, and saves first.

use duckforge::prelude::*;

use crate::bot::Bot;
use crate::game::Screen;
use crate::victory::{Loss, Outcome, Win};

fn game() -> crate::game::Lucky {
    crate::new_game().unwrap_or_else(|e| panic!("{e}"))
}

fn saves(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("lucky-world-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    crate::save::set_folder(&dir);
    dir
}

fn save_files(dir: &std::path::Path) -> usize {
    crate::save::list_in(dir).iter().filter(|s| s.summary.is_ok()).count()
}

/// Esc, again and again: the game is still there, nothing quits, nothing
/// asks to leave, and once everything is closed the pause menu comes up.
fn hammer(bot: &mut Bot, what: &str) {
    let mut paused = false;
    for k in 1..=9 {
        bot.tap(Key::Escape);
        assert!(!bot.sim.quit_requested(), "{what}: Esc number {k} quit the game");
        assert!(bot.g().in_game(), "{what}: Esc number {k} left the game (now on {:?})", bot.screen());
        assert!(!matches!(bot.screen(), Screen::Menu | Screen::Title), "{what}: Esc number {k} went to {:?}", bot.screen());
        assert!(bot.g().shell.confirm.is_none(), "{what}: Esc alone never asks to leave");
        paused |= bot.g().shell.pause;
    }
    assert!(paused, "{what}: with everything closed, Esc opens the pause menu");
    if bot.g().shell.pause {
        bot.tap(Key::Escape);
    }
    assert!(!bot.g().shell.pause, "{what}: Esc closes the pause menu again");
}

/// Something is open: the first Esc closes it and nothing else.
fn first_esc_closes(bot: &mut Bot, what: &str, open: impl Fn(&crate::game::Lucky) -> bool) {
    assert!(open(bot.g()), "{what} is open ({:?})", bot.log.last());
    bot.tap(Key::Escape);
    assert!(!open(bot.g()), "Esc closes {what}");
    assert!(!bot.g().shell.pause, "Esc on {what} closes it, and does not also open the pause menu");
    assert_eq!(bot.screen(), Screen::Map, "Esc on {what} stays on the map");
    hammer(bot, what);
}

#[test]
fn esc_never_quits_and_never_leaves_a_game_without_asking() {
    let dir = saves("esc-test");
    let mut bot = Bot::new(game(), 11);
    bot.wait(5);
    assert!(bot.wait_until(30, |g| g.screen == Screen::Menu));

    // The main menu: Esc asks; it does not quit. Esc again: never mind.
    bot.tap(Key::Escape);
    assert!(!bot.sim.quit_requested(), "Esc on the main menu does not quit");
    assert!(bot.g().shell.confirm.as_ref().is_some_and(|c| c.title.contains("Quit")), "Esc on the main menu asks first");
    bot.tap(Key::Escape);
    assert!(bot.g().shell.confirm.is_none() && bot.screen() == Screen::Menu && !bot.sim.quit_requested());
    // The Quit button asks too.
    assert!(bot.click_on("Quit"), "{:?}", bot.log);
    assert!(bot.g().shell.confirm.is_some() && !bot.sim.quit_requested(), "Quit asks first");
    assert!(bot.click_on("Cancel"));
    assert!(bot.g().shell.confirm.is_none() && !bot.sim.quit_requested());

    // Choosing a new game: nothing is under way, so Esc is "back", not "quit".
    assert!(bot.click_on("New Game"), "{:?}", bot.log);
    assert_eq!(bot.screen(), Screen::Title);
    bot.tap(Key::Escape);
    assert_eq!(bot.screen(), Screen::Menu);
    assert!(!bot.sim.quit_requested());
    assert!(bot.click_on("New Game"), "{:?}", bot.log);
    assert_eq!(bot.screen(), Screen::Title);

    // The wheels: before the spin, in the spin, after it.
    bot.tap(Key::Space);
    assert_eq!(bot.screen(), Screen::Wheel);
    bot.wait(40);
    assert!(bot.g().landed.is_empty(), "the wheel waits for Spin: it never spins by itself");
    hammer(&mut bot, "a wheel before its spin");
    bot.tap(Key::Space);
    hammer(&mut bot, "a spinning wheel");
    assert!(bot.wait_until(900, |g| g.landed.len() == 1), "the wheel lands");
    hammer(&mut bot, "a landed wheel");

    // Leaving from the wheels asks, saves, and Continue brings the wheels back.
    assert!(bot.pause());
    assert!(bot.click_on("Main menu"), "{:?}", bot.log);
    assert!(bot.g().shell.confirm.is_some(), "Main menu asks first");
    assert!(bot.g().in_game(), "and the game is still there while it asks");
    bot.tap(Key::Escape);
    assert!(bot.g().shell.confirm.is_none() && bot.g().in_game() && bot.g().shell.pause, "Esc on the question: never mind, back in the pause menu");
    assert!(bot.click_on("Main menu"));
    assert!(bot.click_on("Save and leave"), "{:?}", bot.log);
    assert!(bot.wait_until(30, |g| g.screen == Screen::Menu));
    assert!(save_files(&dir) > 0, "leaving saved the game first");
    assert!(bot.click_on("Continue"), "{:?}", bot.log);
    assert!(bot.wait_until(60, |g| g.screen == Screen::Wheel), "Continue goes back to the wheels (now on {:?})", bot.screen());
    assert_eq!(bot.g().landed.len(), 1, "with the wheel already spun");

    // On to the first round card.
    for _ in 0..400 {
        if matches!(bot.screen(), Screen::Round(_)) {
            break;
        }
        bot.tap(Key::Space);
        bot.wait(10);
    }
    assert!(matches!(bot.screen(), Screen::Round(_)), "a round card comes up (on {:?})", bot.screen());
    hammer(&mut bot, "a round card");

    // The country card, the rivals, a rival's card.
    bot.tap(Key::S);
    assert!(bot.wait_until(60, |g| g.screen == Screen::Final));
    hammer(&mut bot, "the country card");
    bot.wait(30);
    bot.tap(Key::Space);
    bot.wait(30);
    bot.tap(Key::Space);
    assert!(bot.wait_until(60, |g| g.screen == Screen::Rivals));
    hammer(&mut bot, "the rivals");
    bot.wait(30);
    assert!(bot.click_on("rival 1"));
    assert!(bot.g().inspect.is_some(), "a click opens a rival's card");
    bot.tap(Key::Escape);
    assert!(bot.g().inspect.is_none() && !bot.g().shell.pause && bot.screen() == Screen::Rivals, "Esc closes the rival's card and nothing else");
    hammer(&mut bot, "the rivals again");
    bot.tap(Key::Space);
    assert!(bot.wait_until(60, |g| g.screen == Screen::Map));
    bot.wait(20);

    // The map, bare, then with each thing open.
    hammer(&mut bot, "the map");
    for (button, what) in [("Economy", "the economy window"), ("War room", "the war room"), ("Battles", "the battle reports"), ("Diplomacy", "the diplomacy window"), ("Spies", "the spies window")] {
        assert!(bot.click_on(button), "{:?}", bot.log);
        first_esc_closes(&mut bot, what, |g| g.map_ui.window_open());
    }
    assert!(bot.click_on("News log"), "{:?}", bot.log);
    first_esc_closes(&mut bot, "the news log", |g| g.notes.log_open);
    assert!(bot.click_on("Country card"), "{:?}", bot.log);
    first_esc_closes(&mut bot, "the country card on the map", |g| g.map_ui.card.is_some());
    // The right-click menu (the camera starts on my nation: the middle of the map is mine).
    let (_, at) = bot.g().clickable_province(0).expect("one of my provinces is on screen");
    bot.right_click(at);
    first_esc_closes(&mut bot, "the right-click menu", |g| g.map_ui.menu.is_some());
    // A province chosen and its details open: Esc closes the details, then lets go of the province.
    let (p, at) = bot.g().clickable_province(0).expect("one of my provinces is on screen");
    bot.click(at);
    assert_eq!(bot.g().map_ui.selected, Some(p), "a click chooses a province");
    assert!(bot.click_on("Details"), "{:?}", bot.log);
    first_esc_closes(&mut bot, "the province details", |g| g.map_ui.details);

    // The pause menu's own pages: Esc is one step back, never out.
    for (button, screen) in [("Settings", Screen::Settings), ("How to play", Screen::Help), ("Load game", Screen::Load)] {
        assert!(bot.pause());
        assert!(bot.click_on(button), "{:?}", bot.log);
        assert_eq!(bot.screen(), screen);
        bot.tap(Key::Escape);
        assert_eq!(bot.screen(), Screen::Map, "Esc on {button} goes back to the game");
        assert!(bot.g().shell.pause, "back in the pause menu");
        assert!(!bot.sim.quit_requested());
        bot.tap(Key::Escape);
        assert!(!bot.g().shell.pause);
    }
    assert!(bot.pause());
    assert!(bot.click_on("Save game"));
    assert!(bot.g().shell.pause_slots);
    bot.tap(Key::Escape);
    assert!(!bot.g().shell.pause_slots && bot.g().shell.pause && bot.screen() == Screen::Map, "Esc on the save slots: back to the pause menu");

    // Quit to desktop asks; Esc on the question is "never mind".
    assert!(bot.click_on("Quit to desktop"), "{:?}", bot.log);
    assert!(bot.g().shell.confirm.is_some() && !bot.sim.quit_requested(), "Quit to desktop asks first");
    bot.tap(Key::Escape);
    assert!(bot.g().shell.confirm.is_none() && !bot.sim.quit_requested() && bot.g().in_game());
    // A click beside the question is "never mind" too.
    assert!(bot.click_on("Quit to desktop"));
    bot.click(vec2(60.0, 60.0));
    assert!(bot.g().shell.confirm.is_none() && !bot.sim.quit_requested() && bot.g().in_game());

    // Main menu: asks, saves, and Continue brings the map back.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let day = bot.g().economy.as_ref().unwrap().day;
    assert!(bot.click_on("Main menu"));
    assert!(bot.g().shell.confirm.is_some() && bot.screen() == Screen::Map);
    assert!(bot.click_on("Save and leave"));
    assert!(bot.wait_until(30, |g| g.screen == Screen::Menu));
    assert!(save_files(&dir) > 0, "leaving the map saved the game first");
    assert!(bot.click_on("Continue"), "{:?}", bot.log);
    assert!(bot.wait_until(120, |g| g.screen == Screen::Map));
    assert_eq!(bot.g().economy.as_ref().unwrap().day, day, "Continue is the day I left on");

    // A war declared from the keyboard asks first, and Esc says no.
    bot.tap(Key::Escape);
    assert!(bot.g().shell.pause);
    bot.tap(Key::Escape);

    // The end screens: Esc is the pause menu there too, and leaving asks.
    for o in [Outcome::Won(Win::Score), Outcome::Lost(Loss::Overthrown), Outcome::Lost(Loss::Conquered)] {
        bot.sim.with_context(|g, ctx| {
            g.fate.outcome = Some(o);
            g.show_outcome(ctx, o);
        });
        bot.wait(60);
        assert_eq!(bot.screen(), Screen::Outcome);
        hammer(&mut bot, o.title());
        bot.tap(Key::M);
        assert!(bot.g().shell.confirm.is_some() && bot.screen() == Screen::Outcome, "{}: Main menu asks first", o.title());
        bot.tap(Key::Escape);
        assert!(bot.g().shell.confirm.is_none() && bot.screen() == Screen::Outcome);
    }

    // And the real way out: Quit to desktop, yes. Only now does the game quit.
    assert!(bot.pause());
    assert!(bot.click_on("Quit to desktop"));
    assert!(!bot.sim.quit_requested());
    bot.tap(Key::Enter);
    bot.wait(3);
    assert!(bot.sim.quit_requested(), "Quit to desktop, then yes, quits");
    let _ = std::fs::remove_dir_all(&dir);
}
