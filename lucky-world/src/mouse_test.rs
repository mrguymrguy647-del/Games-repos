//! The mouse-only playthrough: a whole game played with clicks alone, not
//! one key press. The menu, every wheel spun, the rivals, the map; a
//! building, trade, a treaty, a war and its peace, a save and a load.
//! Every button it presses it finds by name, where a player would see it.

use duckforge::prelude::*;

use crate::bot::Bot;
use crate::game::Screen;

fn game() -> crate::game::Lucky {
    crate::new_game().unwrap_or_else(|e| panic!("{e}"))
}

/// Clicks a button by name, or fails with what could be clicked instead.
fn must(bot: &mut Bot, name: &str) {
    assert!(bot.click_on(name), "{}", bot.log.last().cloned().unwrap_or_default());
}

/// The nation on this map with the most soldiers next to the player.
fn strongest_neighbour(bot: &Bot) -> usize {
    let w = bot.g().wars.as_ref().unwrap();
    let army = |n: usize| w.divisions.iter().filter(|d| d.alive && w.nation_of(d.faction) == n).count();
    bot.neighbours().into_iter().max_by_key(|&n| army(n)).expect("a neighbour")
}

/// Right-clicks one of this nation's provinces (zooming out until one shows).
fn right_click_nation(bot: &mut Bot, n: usize) {
    for _ in 0..6 {
        if let Some((_, at)) = bot.g().clickable_province(n) {
            bot.right_click(at);
            assert!(bot.g().map_ui.menu.as_ref().is_some_and(|m| m.nation == Some(n)), "the menu opened for nation {n}");
            return;
        }
        must(bot, "minus");
        bot.wait(30);
    }
    panic!("nation {n} never came on screen");
}

#[test]
fn a_whole_game_with_the_mouse_alone() {
    let dir = std::env::temp_dir().join(format!("lucky-world-mouse-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    crate::save::set_folder(&dir);
    let keys_before = crate::game::gui::real_keys();
    let mut bot = Bot::new(game(), 9);
    bot.mouse_only = true;
    bot.wait(10);

    // The menu, the New Game screen.
    must(&mut bot, "New Game");
    assert_eq!(bot.screen(), Screen::Title);
    let world = bot.g().data.maps[0].name.clone();
    must(&mut bot, &world);
    must(&mut bot, "Spin the wheels");
    assert_eq!(bot.screen(), Screen::Wheel);

    // Every wheel, spun by a click on it; every round card, by its button.
    let wheels = bot.g().data.wheels.len();
    let mut guard = 0;
    while bot.screen() != Screen::Final {
        guard += 1;
        assert!(guard < 400, "stuck on {:?}", bot.screen());
        match bot.screen() {
            Screen::Wheel => {
                let n = bot.g().landed.len();
                bot.wait(10);
                must(&mut bot, "wheel");
                assert!(bot.wait_until(1500, |g| g.landed.len() == n + 1), "wheel {} landed", n + 1);
                bot.wait(20);
                must(&mut bot, "Spin");
            }
            Screen::Round(_) => {
                bot.wait(30);
                must(&mut bot, "Next round");
            }
            other => panic!("on {other:?}"),
        }
    }
    assert_eq!(bot.g().landed.len(), wheels, "every wheel spun by hand");
    assert!(!bot.g().skipped);
    // The country card, the rivals, one rival's card, the map.
    bot.wait(20);
    while bot.screen() == Screen::Final {
        must(&mut bot, "Meet your rivals");
        bot.wait(20);
    }
    assert_eq!(bot.screen(), Screen::Rivals);
    bot.wait(30);
    must(&mut bot, "rival 1");
    must(&mut bot, "Back");
    must(&mut bot, "Begin");
    assert!(bot.wait_until(60, |g| g.screen == Screen::Map));
    bot.note("on the map");

    // The clock, from the top bar.
    must(&mut bot, "3");
    bot.wait(60);
    assert!(bot.g().economy.as_ref().unwrap().day > 0, "the clock runs");

    // A building: a province of mine, Build here, the Chains page's button.
    let (p, at) = bot.g().clickable_province(0).expect("a province of mine on screen");
    bot.click(at);
    assert_eq!(bot.g().map_ui.selected, Some(p));
    must(&mut bot, "Build here");
    assert_eq!(bot.g().map_ui.econ.open, Some(crate::game::econ_screen::Tab::Chains));
    let build = bot.g().click_targets().into_iter().map(|(n, _)| n).find(|n| n.starts_with("Build a ")).expect("a build button");
    must(&mut bot, &build);
    let toast = bot.toast();
    assert!(toast.starts_with("Building") || toast.contains("store") || toast.contains("room") || toast.contains("can't"), "the build was answered: {toast}");
    bot.note(format!("build: {toast}"));
    // A tax, with its slider.
    must(&mut bot, "Budget");
    let tax = bot.g().economy.as_ref().unwrap().nations[0].income_tax;
    let slider = bot.g().click_targets().into_iter().find(|(n, _)| n == "slider 0").map(|(_, p)| p).expect("the income tax slider");
    bot.click(slider + vec2(40.0, 0.0));
    let after = bot.g().economy.as_ref().unwrap().nations[0].income_tax;
    assert!(after > tax, "the slider raised income tax ({tax} -> {after})");
    must(&mut bot, "close");
    assert!(bot.g().map_ui.econ.open.is_none());

    // Trade: the right-click menu's Trade, then a trade policy on the market.
    let friend = strongest_neighbour(&bot);
    right_click_nation(&mut bot, friend);
    must(&mut bot, "Trade");
    assert_eq!(bot.g().map_ui.econ.open, Some(crate::game::econ_screen::Tab::Trade));
    must(&mut bot, "Market");
    let good = bot.g().econ_data.goods.iter().position(|g| g.trade && !g.military).expect("a good that trades");
    must(&mut bot, &format!("row {good}"));
    let policy = bot.g().click_targets().into_iter().map(|(n, _)| n).find(|n| n.starts_with("Trade policy for")).expect("a trade policy button");
    must(&mut bot, &policy);
    bot.note(format!("trade: {}", bot.toast()));
    bot.click(vec2(40.0, 600.0)); // outside the window: it closes
    assert!(bot.g().map_ui.econ.open.is_none(), "a click outside closes the window");

    // A treaty: the right-click menu's Propose, then a treaty button.
    let others: Vec<usize> = bot.neighbours().into_iter().filter(|&n| n != friend).collect();
    let partner = others.first().copied().unwrap_or(friend);
    right_click_nation(&mut bot, partner);
    must(&mut bot, "Propose a treaty");
    assert!(bot.g().map_ui.diplo.open.is_some());
    let kinds = crate::diplo::Kind::PROPOSE;
    must(&mut bot, kinds[0].name());
    let answer = bot.g().map_ui.diplo.message.clone().unwrap_or_default();
    assert!(!answer.is_empty(), "the proposal was answered");
    bot.note(format!("treaty: {answer}"));
    must(&mut bot, "close");

    // War: the right-click menu, and yes in the red box.
    let enemy = strongest_neighbour(&bot);
    right_click_nation(&mut bot, enemy);
    must(&mut bot, "Declare war");
    assert!(bot.g().shell.confirm.as_ref().is_some_and(|c| c.danger), "declaring war asks first");
    must(&mut bot, "Declare war");
    assert!(bot.g().wars.as_ref().unwrap().at_war(0, enemy).is_some(), "at war");
    bot.note(format!("war on {}", bot.g().wars.as_ref().unwrap().names[enemy]));
    must(&mut bot, "5");
    bot.wait(300);
    // Peace: the war room, our war chosen, their offer taken or ours made.
    let mut peace = false;
    for _ in 0..40 {
        if bot.g().wars.as_ref().unwrap().at_war(0, enemy).is_none() {
            peace = true;
            break;
        }
        must(&mut bot, "War room");
        must(&mut bot, "row 0");
        if bot.click_any(&["Accept their offer"]).is_none() || bot.g().wars.as_ref().unwrap().at_war(0, enemy).is_some() {
            let _ = bot.click_any(&["Offer peace"]);
        }
        let _ = bot.click_any(&["close"]);
        bot.wait(150);
    }
    assert!(peace, "peace made: {:?}", bot.log);
    bot.note("peace");

    // Save from the pause menu, run on, load it again.
    must(&mut bot, "pause");
    must(&mut bot, "Menu");
    assert!(bot.g().shell.pause);
    must(&mut bot, "Save game");
    let slot = crate::game::shell::SLOTS[0];
    must(&mut bot, slot);
    assert!(crate::save::path_for(slot).exists(), "the slot was written");
    let saved_day = bot.g().economy.as_ref().unwrap().day;
    must(&mut bot, "Resume");
    must(&mut bot, "5");
    bot.wait(200);
    assert!(bot.g().economy.as_ref().unwrap().day > saved_day);
    must(&mut bot, "Menu");
    must(&mut bot, "Load game");
    assert_eq!(bot.screen(), Screen::Load);
    let k = bot.g().shell.slots.iter().position(|s| s.path == crate::save::path_for(slot)).expect("the save is listed");
    must(&mut bot, &format!("save {k}"));
    must(&mut bot, "Load this save");
    if bot.g().shell.confirm.is_some() {
        must(&mut bot, "Load");
    }
    assert!(bot.wait_until(120, |g| g.screen == Screen::Map));
    assert_eq!(bot.g().economy.as_ref().unwrap().day, saved_day, "the loaded game is the saved one");
    bot.note("saved and loaded");

    // Not one key: the recording holds only the mouse.
    let keys: Vec<_> = bot.sim.recording().events.iter().filter(|e| matches!(e.action, duckforge::script::InputAction::Press(_) | duckforge::script::InputAction::Release(_))).map(|e| format!("{:?}", e.action)).collect();
    assert!(keys.is_empty(), "keys were pressed: {keys:?}");
    assert_eq!(crate::game::gui::real_keys(), keys_before, "the game saw a key press");
    let presses = bot.sim.recording().events.iter().filter(|e| matches!(e.action, duckforge::script::InputAction::MousePress(_))).count();
    assert!(presses > 60, "only {presses} clicks");
    println!("{presses} clicks, no keys");
    assert!(bot.sim.failed_checks().is_empty(), "{:?}", bot.sim.failed_checks());
    for line in &bot.log {
        println!("{line}");
    }
    let _ = std::fs::remove_dir_all(dir);
}
