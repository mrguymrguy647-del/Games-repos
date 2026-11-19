//! A bot that plays LUCKY WORLD as a person would: it presses keys, waits
//! for the game, and reads what's on screen (the game's state) to decide
//! the next key. The player test plays a whole game with it, and `--tour`
//! uses it to take the screenshots in docs/.

use duckforge::prelude::*;

use crate::game::{Lucky, Screen};

pub struct Bot {
    pub sim: Simulation<Lucky>,
    /// What it did, one line per step of its plan.
    pub log: Vec<String>,
}

#[cfg_attr(not(test), allow(dead_code))] // some helpers only the player test uses
impl Bot {
    /// The game from the main menu (as a real start), with this seed.
    pub fn new(mut game: Lucky, seed: u32) -> Bot {
        game.menu_started = true;
        game.open_menu();
        game.start_settings();
        let mut config = crate::config();
        config.seed = Some(seed);
        Bot { sim: Simulation::new(config, game), log: Vec::new() }
    }

    pub fn g(&self) -> &Lucky {
        self.sim.game()
    }

    pub fn note(&mut self, words: impl Into<String>) {
        let day = self.g().economy.as_ref().map_or(0, |e| e.day);
        self.log.push(format!("[frame {} day {day}] {}", self.sim.context().frame(), words.into()));
    }

    /// Presses and lets go of a key, then lets the game see it settle.
    pub fn tap(&mut self, key: Key) {
        self.sim.press(key);
        self.sim.step();
        self.sim.release(key);
        self.sim.step();
        self.wait(4);
    }

    pub fn taps(&mut self, key: Key, times: usize) {
        for _ in 0..times {
            self.tap(key);
        }
    }

    pub fn wait(&mut self, frames: u64) {
        self.sim.run(frames);
    }

    /// Waits until `done` (or `most` frames); says whether it happened.
    pub fn wait_until(&mut self, most: u64, done: impl Fn(&Lucky) -> bool) -> bool {
        for _ in 0..most {
            if done(self.g()) {
                return true;
            }
            if self.sim.quit_requested() {
                return false;
            }
            self.sim.step();
        }
        done(self.g())
    }

    pub fn screen(&self) -> Screen {
        self.g().screen
    }

    pub fn click(&mut self, at: Vec2) {
        self.sim.move_mouse(at);
        self.sim.step();
        self.sim.press_mouse(MouseButton::Left);
        self.sim.step();
        self.sim.release_mouse(MouseButton::Left);
        self.sim.step();
        self.wait(4);
    }

    /// Saves what's on screen now.
    pub fn shot(&mut self, path: &str) {
        if let Some(dir) = std::path::Path::new(path).parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = self.sim.picture().save_png(path);
    }

    /// The toast showing now.
    pub fn toast(&self) -> String {
        self.sim.text("toast").to_string()
    }

    /// Esc until the pause menu is open (closing whatever is open first).
    pub fn pause(&mut self) -> bool {
        for _ in 0..6 {
            if self.g().shell.pause {
                return true;
            }
            self.tap(Key::Escape);
        }
        self.g().shell.pause
    }

    /// From the menu: a new game on the chosen world, skip-spun, onto the map.
    pub fn new_game(&mut self, map: usize, difficulty: &str) -> bool {
        self.wait_until(60, |g| g.screen == Screen::Menu);
        // New Game is the first row without saves, the second with them.
        let row = if self.g().shell.slots.iter().any(|s| s.summary.is_ok()) { 1 } else { 0 };
        self.taps(Key::Down, row);
        self.tap(Key::Enter);
        if !self.wait_until(30, |g| g.screen == Screen::Title) {
            return false;
        }
        self.tap([Key::Digit1, Key::Digit2, Key::Digit3][map.min(2)]);
        for _ in 0..4 {
            if self.g().setup.difficulty == difficulty {
                break;
            }
            self.tap(Key::D);
        }
        self.note(format!("new game: {}, {}", self.g().data.maps[map].name, self.g().setup.difficulty));
        self.tap(Key::S);
        self.wait_until(60, |g| g.screen == Screen::Final);
        self.wait(30);
        self.tap(Key::Space);
        self.wait(30);
        self.tap(Key::Space);
        self.wait_until(60, |g| g.screen == Screen::Rivals);
        self.wait(30);
        self.tap(Key::Space);
        self.wait_until(60, |g| g.screen == Screen::Map)
    }

    /// The player's neighbours (nations that share a border).
    pub fn neighbours(&self) -> Vec<usize> {
        let Some(w) = self.g().wars.as_ref() else { return Vec::new() };
        let mine = w.held(0);
        let mut out: Vec<usize> = mine.iter().flat_map(|&q| w.geo.neighbours[q].iter().filter_map(|&r| w.owner[r])).filter(|&o| o != 0).collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Tab round the nations until one of `wanted` is selected.
    pub fn visit_one_of(&mut self, wanted: &[usize]) -> Option<usize> {
        let n = self.g().world.as_ref().map_or(0, |w| w.nations.len());
        for _ in 0..n + 1 {
            self.tap(Key::Tab);
            let owner = self.g().map_ui.selected.and_then(|p| self.g().world.as_ref().and_then(|w| w.owner[p]));
            if let Some(o) = owner.filter(|o| wanted.contains(o)) {
                return Some(o);
            }
        }
        None
    }
}

/// `--tour DIR`: the bot plays through every new screen and saves a picture
/// of each (the README's screenshots), including a real game's victory and
/// a real game's defeat.
pub fn tour(args: &[String]) -> Result<(), String> {
    let dir = args.first().cloned().unwrap_or_else(|| "out/tour".to_string());
    let saves = format!("{dir}/saves");
    let _ = std::fs::remove_dir_all(&saves);
    crate::save::set_folder(&saves);
    let shot = |bot: &mut Bot, name: &str| {
        let path = format!("{dir}/{name}.png");
        bot.shot(&path);
        println!("saved {path} (screen {:?}, pause {}, slots {}, row {})", bot.g().screen, bot.g().shell.pause, bot.g().shell.pause_slots, bot.g().shell.row);
    };
    let fresh = || -> Result<Lucky, String> { crate::new_game().map_err(|e| e.to_string()) };

    // The menu, the New Game screen.
    let mut bot = Bot::new(fresh()?, 5);
    bot.wait(70);
    shot(&mut bot, "menu");
    bot.tap(Key::Enter);
    bot.wait(50);
    bot.tap(Key::D);
    bot.sim.move_mouse(vec2(640.0, 528.0));
    for _ in 0..30 {
        let _ = bot.sim.picture();
        bot.sim.step();
    }
    shot(&mut bot, "new_game");

    // The tutorial: the title, a wheel, the map.
    let mut bot = Bot::new(fresh()?, 3);
    bot.wait(10);
    bot.tap(Key::Down);
    bot.tap(Key::Enter);
    bot.wait(40);
    shot(&mut bot, "tutorial_start");
    bot.tap(Key::Space);
    bot.wait_until(600, |g| g.view.as_ref().is_some_and(|v| v.landed));
    bot.wait(20);
    shot(&mut bot, "tutorial_wheel");
    bot.tap(Key::S);
    bot.wait(40);
    bot.taps(Key::Space, 2);
    bot.wait(40);
    bot.tap(Key::Space);
    bot.wait_until(60, |g| g.screen == Screen::Map);
    bot.tap(Key::H);
    bot.tap(Key::Digit3);
    bot.wait(60);
    bot.tap(Key::O);
    bot.wait(60);
    shot(&mut bot, "tutorial");

    // A game on the map: a tooltip, the pause menu, settings, help, the log, the save slots.
    let mut bot = Bot::new(fresh()?, 11);
    bot.wait(10);
    bot.new_game(2, "normal");
    bot.tap(Key::Digit5);
    bot.wait(900);
    bot.tap(Key::Space);
    let _ = bot.sim.picture();
    let gdp = bot.g().tips.areas.borrow().iter().find(|(_, w)| w.starts_with("GDP:")).map(|(r, _)| r.center());
    if let Some(at) = gdp {
        bot.sim.move_mouse(at);
        for _ in 0..40 {
            let _ = bot.sim.picture();
            bot.sim.step();
        }
        shot(&mut bot, "tooltip");
    }
    bot.sim.move_mouse(vec2(640.0, 400.0));
    bot.tap(Key::O);
    bot.wait(20);
    shot(&mut bot, "econ_budget");
    bot.tap(Key::O);
    bot.tap(Key::P);
    bot.wait(20);
    shot(&mut bot, "diplomacy");
    bot.tap(Key::Escape);
    bot.pause();
    bot.wait(20);
    shot(&mut bot, "pause");
    bot.tap(Key::Down);
    bot.tap(Key::Enter);
    bot.tap(Key::Enter);
    bot.tap(Key::Down);
    bot.tap(Key::Enter);
    bot.wait(20);
    shot(&mut bot, "save_slots");
    bot.tap(Key::Escape);
    // Back on the pause menu, on its Save row.
    bot.taps(Key::Down, 2);
    bot.tap(Key::Enter);
    bot.wait(20);
    shot(&mut bot, "settings");
    bot.tap(Key::Escape);
    bot.taps(Key::Down, 4);
    bot.tap(Key::Enter);
    bot.taps(Key::Down, 1);
    bot.wait(20);
    shot(&mut bot, "help");
    bot.tap(Key::Escape);
    bot.tap(Key::Escape);
    bot.tap(Key::L);
    bot.wait(10);
    shot(&mut bot, "log");
    bot.tap(Key::L);
    bot.pause();
    bot.taps(Key::Down, 2);
    bot.tap(Key::Enter);
    bot.wait(20);
    shot(&mut bot, "load");

    // Real games to their end: the first won and the first lost.
    let years: u32 = args.get(1).and_then(|y| y.parse().ok()).unwrap_or(30);
    let (mut won, mut lost) = (false, false);
    for seed in 1..40u32 {
        if won && lost {
            break;
        }
        let mut g = fresh()?;
        g.setup.years = years;
        let mut bot = Bot::new(g, 100 + seed);
        bot.wait(10);
        bot.new_game((seed % 3) as usize, "normal");
        // A strong start for the victory picture, a weak one for the defeat.
        let rank = {
            let g = bot.g();
            let (e, w) = (g.economy.as_ref().unwrap(), g.wars.as_ref().unwrap());
            let s = crate::victory::scores(&g.victory.victory, e, &g.econ_data, w);
            1 + (1..s.len()).filter(|&j| s[j] > s[0]).count()
        };
        if (rank == 1 && won) || (rank > 1 && lost) || (rank > 1 && rank < 6) {
            continue;
        }
        bot.tap(Key::Digit5);
        if !bot.wait_until(years as u64 * 360 * 2 + 600, |g| g.screen == Screen::Outcome) {
            continue;
        }
        bot.wait(90);
        let o = bot.g().fate.outcome.expect("an outcome");
        println!("seed {seed}: start rank {rank}, {}", o.title());
        if o.won() && !won {
            won = true;
            shot(&mut bot, "victory");
        } else if !o.won() && !lost {
            lost = true;
            shot(&mut bot, "defeat");
        }
    }
    Ok(())
}
