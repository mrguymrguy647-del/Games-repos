//! Everything round the game itself: the main menu, the New Game choices,
//! saving and loading (and the yearly autosave), the settings, the help
//! screen, the pause menu, the victory and defeat screens, notifications
//! that don't spam, music that follows war and peace, and tooltips.

use std::cell::RefCell;
use std::collections::VecDeque;

use duckforge::prelude::*;

use super::{Lucky, Screen};
use crate::save::{self, Saved, Summary};
use crate::settings::{Act, Quality};
use crate::ui::{self, GOLD, MUTED, SOFT};
use crate::victory::Outcome;

/// The big window the menus' pages use (the economy window's frame).
pub const WIN: Rect = Rect { x: 110.0, y: 64.0, w: 1060.0, h: 600.0 };
const MENU_AT: Vec2 = Vec2 { x: 442.0, y: 208.0 };
/// The menu panel picture is drawn at this scale (396 x 482).
const MENU_SCALE: f32 = 0.86;
const ROW_H: f32 = 56.0;

/// The menu's rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Continue,
    NewGame,
    Tutorial,
    Load,
    Settings,
    Help,
    Quit,
}

impl MenuItem {
    fn words(self) -> (&'static str, &'static str) {
        match self {
            MenuItem::Continue => ("CONTINUE", "your latest save"),
            MenuItem::NewGame => ("NEW GAME", "choose a world, spin your nation"),
            MenuItem::Tutorial => ("TUTORIAL", "a short guided game (10 minutes)"),
            MenuItem::Load => ("LOAD", "a saved game"),
            MenuItem::Settings => ("SETTINGS", "picture, sound, keys"),
            MenuItem::Help => ("HELP", "how to play"),
            MenuItem::Quit => ("QUIT", "to the desktop"),
        }
    }
}

/// The pause menu's rows (on the map, Esc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseItem {
    Resume,
    Save,
    Load,
    Settings,
    Help,
    MainMenu,
    Quit,
}

const PAUSE_ITEMS: [PauseItem; 7] = [PauseItem::Resume, PauseItem::Save, PauseItem::Load, PauseItem::Settings, PauseItem::Help, PauseItem::MainMenu, PauseItem::Quit];

impl PauseItem {
    fn words(self) -> &'static str {
        match self {
            PauseItem::Resume => "Resume",
            PauseItem::Save => "Save game",
            PauseItem::Load => "Load game",
            PauseItem::Settings => "Settings",
            PauseItem::Help => "Help",
            PauseItem::MainMenu => "Main menu (saves first)",
            PauseItem::Quit => "Quit to desktop (saves first)",
        }
    }
}

/// Manual save slots (plus the quick save and the autosaves).
pub const SLOTS: [&str; 6] = ["Quick save", "Slot 1", "Slot 2", "Slot 3", "Slot 4", "Slot 5"];

/// The Settings screen's rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetRow {
    Quality,
    Window,
    Fullscreen,
    Music,
    Effects,
    UiScale,
    Autosave,
    Notes,
    Tips,
    Keys,
    Back,
}

const SET_ROWS: [SetRow; 11] = [SetRow::Quality, SetRow::Window, SetRow::Fullscreen, SetRow::Music, SetRow::Effects, SetRow::UiScale, SetRow::Autosave, SetRow::Notes, SetRow::Tips, SetRow::Keys, SetRow::Back];
const UI_SCALES: [f32; 4] = [0.9, 1.0, 1.1, 1.15];

/// What the shell remembers between frames.
#[derive(Default)]
pub struct Shell {
    pub row: usize,
    /// The pause menu is open (on the map), and its save-slot list.
    pub pause: bool,
    pub pause_slots: bool,
    pub slots: Vec<save::Slot>,
    /// Where Esc goes back to from Load, Settings and Help.
    pub back: Option<Screen>,
    pub help_page: usize,
    /// The Settings screen's key page, and the action waiting for a key.
    pub keys_page: bool,
    pub binding: Option<Act>,
    /// A line shown on the menu screens for a while.
    pub message: Option<(String, f32)>,
    /// The clock was paused before the pause menu opened.
    was_paused: bool,
    /// The year last autosaved.
    pub autosaved_year: u32,
    /// The settings changed since the screen opened (saved on the way out).
    dirty: bool,
}

/// Notifications: one at a time, the rest queued; repeats and minor news
/// dropped; a sound at most every two seconds; all of it in a log (L).
#[derive(Default)]
pub struct Notes {
    queue: VecDeque<(String, f32, u8)>,
    /// What was said lately, and when (repeats within 30 s are dropped).
    recent: Vec<(String, f32)>,
    last_sound: f32,
    /// Everything said: (date, words, level).
    pub log: Vec<(String, String, u8)>,
    pub log_open: bool,
    pub shown: usize,
}

/// Level of a notification: 0 critical (war on us, a result), 1 important
/// (proposals, peace offers, battles of ours), 2 minor (news, the economy).
pub const CRITICAL: u8 = 0;
pub const IMPORTANT: u8 = 1;
pub const MINOR: u8 = 2;

/// The music: a track and a crossfade.
#[derive(Default)]
pub struct Music {
    pub track: Option<Track>,
    id: Option<LoopId>,
    volume: f32,
    /// The track fading out, and its volume.
    fading: Option<(LoopId, f32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Track {
    Menu,
    Peace,
    War,
}

/// Tooltips: the areas with a tip, as drawn last frame, and the one under the mouse.
#[derive(Default)]
pub struct Tips {
    pub areas: RefCell<Vec<(Rect, String)>>,
    /// Drawing at the UI scale now (tip areas are turned into screen pixels).
    pub scaled: std::cell::Cell<bool>,
    pub hover: Option<(Rect, String)>,
    pub hover_time: f32,
}

impl Lucky {
    // ---------------------------------------------------------------
    // Opening and leaving screens
    // ---------------------------------------------------------------

    /// The main menu (where a real start begins).
    pub fn open_menu(&mut self) {
        self.shell.pause = false;
        self.shell.slots = save::list();
        // The first row: Continue when there's a save, else New Game.
        self.shell.row = 0;
        self.go(Screen::Menu);
    }

    fn menu_items(&self) -> Vec<MenuItem> {
        let mut v = Vec::new();
        if self.latest_save().is_some() {
            v.push(MenuItem::Continue);
        }
        v.extend([MenuItem::NewGame, MenuItem::Tutorial, MenuItem::Load, MenuItem::Settings, MenuItem::Help, MenuItem::Quit]);
        v
    }

    fn latest_save(&self) -> Option<&save::Slot> {
        self.shell.slots.iter().find(|s| s.summary.is_ok())
    }

    fn say(&mut self, words: impl Into<String>) {
        self.shell.message = Some((words.into(), 4.0));
    }

    fn open_sub(&mut self, screen: Screen) {
        self.shell.back = Some(self.screen);
        self.shell.row = 0;
        self.shell.keys_page = false;
        self.shell.binding = None;
        self.shell.dirty = false;
        if screen == Screen::Load {
            self.shell.slots = save::list();
        }
        let keep = self.screen_time;
        self.go(screen);
        if self.shell.back == Some(Screen::Map) {
            self.screen_time = keep;
        }
    }

    fn close_sub(&mut self, ctx: &mut Context) {
        if self.shell.dirty {
            self.apply_settings(ctx);
            if let Err(e) = self.settings.save() {
                self.say(format!("Couldn't save the settings: {e}"));
            }
            self.shell.dirty = false;
        }
        let back = self.shell.back.take().unwrap_or(Screen::Menu);
        self.shell.row = 0;
        crate::ui::sfx(ctx, &self.assets.click, 0.6, 0.0);
        if back == Screen::Menu {
            self.open_menu();
        } else {
            self.go(back);
            if back == Screen::Map {
                self.shell.pause = true;
            }
        }
    }

    /// Puts the settings into effect: volumes, music, quality.
    pub fn apply_settings(&mut self, ctx: &mut Context) {
        ui::set_effects_volume(self.settings.effects);
        if let Some(id) = self.map_ui.ambient {
            ctx.set_loop_volume(id, 0.55 * self.settings.effects);
        }
        if let Some(id) = self.music.id {
            self.music.volume = 0.0;
            ctx.set_loop_volume(id, 0.0);
        }
        crate::fullscreen::request(self.settings.fullscreen);
    }

    // ---------------------------------------------------------------
    // The main menu
    // ---------------------------------------------------------------

    fn menu_rect(i: usize) -> Rect {
        Rect::new(MENU_AT.x + 24.0, MENU_AT.y + 32.0 + i as f32 * ROW_H, 348.0, ROW_H - 6.0)
    }

    pub(super) fn update_menu(&mut self, ctx: &mut Context) {
        ctx.idle_ok();
        self.want_music(ctx, Track::Menu);
        if let Some(w) = &mut self.title_wheel {
            w.angle += 0.1 * ctx.dt();
        }
        let items = self.menu_items();
        let mouse = self.ui_mouse();
        let before = self.shell.row;
        if ctx.input.was_pressed(Key::Down) || ctx.input.was_pressed(Key::S) {
            self.shell.row = (self.shell.row + 1) % items.len();
        }
        if ctx.input.was_pressed(Key::Up) || ctx.input.was_pressed(Key::W) {
            self.shell.row = (self.shell.row + items.len() - 1) % items.len();
        }
        let mut chosen = None;
        for i in 0..items.len() {
            if mouse.is_some_and(|m| Self::menu_rect(i).contains(m)) {
                if self.mouse_moved || ctx.input.was_mouse_pressed(MouseButton::Left) {
                    self.shell.row = i;
                }
                if ctx.input.was_mouse_pressed(MouseButton::Left) {
                    chosen = Some(items[i]);
                }
            }
        }
        if self.shell.row != before {
            crate::ui::sfx(ctx, &self.assets.menu_move, 0.6, 0.0);
        }
        self.shell.row = self.shell.row.min(items.len() - 1);
        if ctx.input.was_pressed(Key::Enter) || ctx.input.was_pressed(Key::Space) {
            chosen = Some(items[self.shell.row]);
        }
        if ctx.input.was_pressed(Key::N) {
            chosen = Some(MenuItem::NewGame);
        }
        if ctx.input.was_pressed(Key::Escape) {
            chosen = Some(MenuItem::Quit);
        }
        let Some(item) = chosen else { return };
        crate::ui::sfx(ctx, &self.assets.click, 0.8, 0.0);
        match item {
            MenuItem::Continue => {
                if let Some(path) = self.latest_save().map(|s| s.path.clone()) {
                    self.load_from(ctx, &path);
                }
            }
            MenuItem::NewGame => {
                self.setup.tutorial = false;
                self.restart();
                self.setup.tutorial = false;
                self.tutorial = None;
            }
            MenuItem::Tutorial => self.start_tutorial(ctx),
            MenuItem::Load => self.open_sub(Screen::Load),
            MenuItem::Settings => self.open_sub(Screen::Settings),
            MenuItem::Help => self.open_sub(Screen::Help),
            MenuItem::Quit => ctx.quit(),
        }
    }

    pub(super) fn draw_menu(&self, canvas: &mut Canvas) {
        let a = &self.assets;
        if let Some(w) = &self.title_wheel {
            self.light_3d(canvas);
            let cam = Camera3D::looking_at(vec3(0.0, 1.55, -2.6), crate::wheel::CENTER);
            w.draw(canvas, &cam, a, self.time);
            ui::dim(canvas, 0.74);
        }
        let drop = (1.0 - ui::ease_out_back(self.screen_time / 0.8)) * -120.0;
        canvas.blit_sprite(&a.logo, vec2(140.0, 14.0 + drop), Blit::default());
        canvas.label("logo", Rect::new(162.0, 39.0, 960.0, 150.0));
        self.push_ui(canvas);
        canvas.blit_sprite(&a.menu_panel, MENU_AT, Blit { scale: MENU_SCALE, ..Blit::default() });
        canvas.label("menu", Rect::new(MENU_AT.x, MENU_AT.y, 460.0 * MENU_SCALE, 560.0 * MENU_SCALE));
        let items = self.menu_items();
        for (i, item) in items.iter().enumerate() {
            let r = Self::menu_rect(i);
            let on = i == self.shell.row;
            if on {
                canvas.fill_rect(r, Color::rgba(245, 197, 66, if on { 60 } else { 30 }));
                canvas.draw_rect(r, ui::alpha(GOLD, 0.7));
            }
            let icon = match item {
                MenuItem::Continue | MenuItem::NewGame => &a.icon_play,
                MenuItem::Tutorial => &a.icon_book,
                MenuItem::Load => &a.icon_load,
                MenuItem::Settings => &a.icon_gear,
                MenuItem::Help => &a.icon_help,
                MenuItem::Quit => &a.icon_quit,
            };
            canvas.blit_sprite(icon, vec2(r.x + 14.0, r.y + 13.0), Blit::default());
            let (big, small) = item.words();
            ui::plain(canvas, &a.font, big, vec2(r.x + 52.0, r.y + 5.0), 21.0, if on { GOLD } else { Color::WHITE }, Align::Left);
            let small = if *item == MenuItem::Continue {
                self.latest_save().and_then(|s| s.summary.as_ref().ok()).map_or(small.to_string(), |s| format!("{} · {} · {}", s.nation, s.date, save::ago(s.saved_at)))
            } else {
                small.to_string()
            };
            ui::text(canvas, &a.font, &small, vec2(r.x + 52.0, r.y + 29.0), 13.0, MUTED, Align::Left, 285.0);
            canvas.label(&format!("menu {big}"), r);
        }
        self.pop_ui(canvas);
        self.draw_message(canvas);
        ui::plain(canvas, &a.font, "Up / Down: choose  ·  Enter: go  ·  N: new game  ·  Esc: quit", vec2(640.0, 697.0), 13.0, SOFT, Align::Center);
        ui::plain(canvas, &a.font, &format!("v{}", env!("CARGO_PKG_VERSION")), vec2(1266.0, 698.0), 12.0, SOFT, Align::Right);
    }

    fn draw_message(&self, canvas: &mut Canvas) {
        if let Some((words, t)) = &self.shell.message {
            let a = t.min(1.0);
            canvas.fill_rect(Rect::new(240.0, 672.0, 800.0, 36.0), Color::rgba(10, 5, 30, (230.0 * a) as u8));
            ui::text(canvas, &self.assets.font, words, vec2(640.0, 680.0), 17.0, ui::alpha(GOLD, a), Align::Center, 780.0);
        }
    }

    // ---------------------------------------------------------------
    // The pause menu (on the map)
    // ---------------------------------------------------------------

    pub(super) fn open_pause(&mut self, ctx: &mut Context) {
        self.shell.pause = true;
        self.shell.pause_slots = false;
        self.shell.row = 0;
        self.shell.was_paused = self.clock.paused;
        self.clock.paused = true;
        crate::ui::sfx(ctx, &self.assets.card_sound, 0.6, 0.0);
    }

    /// F2 on the map: the help, and back to the paused map after.
    pub(super) fn open_help_from_map(&mut self, ctx: &mut Context) {
        self.open_pause(ctx);
        self.open_sub(Screen::Help);
    }

    fn close_pause(&mut self, ctx: &mut Context) {
        self.shell.pause = false;
        self.shell.pause_slots = false;
        self.clock.paused = self.shell.was_paused;
        crate::ui::sfx(ctx, &self.assets.click, 0.6, 0.0);
    }

    fn pause_rect(i: usize) -> Rect {
        Rect::new(470.0, 196.0 + i as f32 * 48.0, 340.0, 42.0)
    }

    fn slot_rect(i: usize) -> Rect {
        Rect::new(330.0, 196.0 + i as f32 * 58.0, 620.0, 52.0)
    }

    /// The pause menu's keys and clicks (the map waits).
    pub(super) fn update_pause(&mut self, ctx: &mut Context) {
        let mouse = self.ui_mouse();
        let count = if self.shell.pause_slots { SLOTS.len() } else { PAUSE_ITEMS.len() };
        let before = self.shell.row;
        if ctx.input.was_pressed(Key::Down) {
            self.shell.row = (self.shell.row + 1) % count;
        }
        if ctx.input.was_pressed(Key::Up) {
            self.shell.row = (self.shell.row + count - 1) % count;
        }
        let mut go = ctx.input.was_pressed(Key::Enter);
        for i in 0..count {
            let r = if self.shell.pause_slots { Self::slot_rect(i) } else { Self::pause_rect(i) };
            if mouse.is_some_and(|m| r.contains(m)) {
                if self.mouse_moved || ctx.input.was_mouse_pressed(MouseButton::Left) {
                    self.shell.row = i;
                }
                go |= ctx.input.was_mouse_pressed(MouseButton::Left);
            }
        }
        if self.shell.row != before {
            crate::ui::sfx(ctx, &self.assets.menu_move, 0.6, 0.0);
        }
        if ctx.input.was_pressed(Key::Escape) {
            if self.shell.pause_slots {
                self.shell.pause_slots = false;
                self.shell.row = 1;
            } else {
                self.close_pause(ctx);
            }
            return;
        }
        if !go {
            return;
        }
        if self.shell.pause_slots {
            let label = SLOTS[self.shell.row];
            let path = save::path_for(label);
            self.save_game(ctx, label, &path);
            self.shell.pause_slots = false;
            self.shell.row = 0;
            self.shell.slots = save::list();
            return;
        }
        crate::ui::sfx(ctx, &self.assets.click, 0.8, 0.0);
        match PAUSE_ITEMS[self.shell.row] {
            PauseItem::Resume => self.close_pause(ctx),
            PauseItem::Save => {
                self.shell.slots = save::list();
                self.shell.pause_slots = true;
                self.shell.row = 0;
            }
            PauseItem::Load => self.open_sub(Screen::Load),
            PauseItem::Settings => self.open_sub(Screen::Settings),
            PauseItem::Help => self.open_sub(Screen::Help),
            PauseItem::MainMenu => {
                self.leave_game(ctx);
                self.open_menu();
            }
            PauseItem::Quit => {
                self.leave_game(ctx);
                ctx.quit();
            }
        }
    }

    /// Leaving a game: saved first (spectating and finished tutorials aren't).
    fn leave_game(&mut self, ctx: &mut Context) {
        if !self.spectating && self.economy.is_some() && !self.fate.outcome.is_some_and(|o| !o.can_play_on()) {
            let path = save::path_for("Autosave");
            self.save_game(ctx, "Autosave", &path);
        }
        self.stop_game_sounds(ctx);
        self.shell.pause = false;
    }

    pub(super) fn stop_game_sounds(&mut self, ctx: &mut Context) {
        if let Some(id) = self.map_ui.ambient.take() {
            ctx.stop_loop(id);
        }
        if let Some(id) = self.map_ui.war.drums.take() {
            ctx.stop_loop(id);
        }
    }

    pub(super) fn draw_pause(&self, canvas: &mut Canvas) {
        let a = &self.assets;
        ui::dim(canvas, 0.7);
        self.push_ui(canvas);
        if self.shell.pause_slots {
            let frame = Rect::new(300.0, 120.0, 680.0, 480.0);
            panel(canvas, frame);
            ui::plain(canvas, &a.font, "SAVE GAME", vec2(640.0, 140.0), 24.0, GOLD, Align::Center);
            ui::plain(canvas, &a.font, "Choose a slot (it's overwritten). Saves are in the \"saves\" folder beside the game.", vec2(640.0, 172.0), 13.0, MUTED, Align::Center);
            for (i, label) in SLOTS.iter().enumerate() {
                let r = Self::slot_rect(i);
                let on = i == self.shell.row;
                canvas.fill_rect(r, if on { Color::rgba(245, 197, 66, 50) } else { Color::rgba(255, 255, 255, 12) });
                if on {
                    canvas.draw_rect(r, ui::alpha(GOLD, 0.7));
                }
                canvas.blit_sprite(&a.icon_save, vec2(r.x + 12.0, r.y + 14.0), Blit::default());
                ui::plain(canvas, &a.font, label, vec2(r.x + 46.0, r.y + 6.0), 18.0, if on { GOLD } else { Color::WHITE }, Align::Left);
                let path = save::path_for(label);
                let what = self.shell.slots.iter().find(|s| s.path == path).map_or("empty".to_string(), |s| match &s.summary {
                    Ok(x) => format!("{} · {} · {} · {}", x.nation, x.date, x.difficulty, save::ago(x.saved_at)),
                    Err(e) => format!("can't be loaded: {e}"),
                });
                ui::text(canvas, &a.font, &what, vec2(r.x + 46.0, r.y + 29.0), 13.0, MUTED, Align::Left, 560.0);
                canvas.label(&format!("slot {label}"), r);
            }
            ui::plain(canvas, &a.font, "Up / Down: choose  ·  Enter: save here  ·  Esc: back", vec2(640.0, 562.0), 13.0, SOFT, Align::Center);
        } else {
            let frame = Rect::new(440.0, 110.0, 400.0, 470.0);
            panel(canvas, frame);
            ui::plain(canvas, &a.font, "PAUSED", vec2(640.0, 132.0), 26.0, GOLD, Align::Center);
            if let Some(e) = self.economy.as_ref() {
                ui::plain(canvas, &a.font, &e.date(&self.econ_data), vec2(640.0, 166.0), 14.0, MUTED, Align::Center);
            }
            for (i, item) in PAUSE_ITEMS.iter().enumerate() {
                let r = Self::pause_rect(i);
                let on = i == self.shell.row;
                if on {
                    canvas.fill_rect(r, Color::rgba(245, 197, 66, 55));
                    canvas.draw_rect(r, ui::alpha(GOLD, 0.7));
                }
                ui::plain(canvas, &a.font, item.words(), vec2(r.center().x, r.y + 10.0), 19.0, if on { GOLD } else { Color::WHITE }, Align::Center);
                canvas.label(&format!("pause {}", item.words()), r);
            }
            ui::plain(canvas, &a.font, "Esc: resume", vec2(640.0, 548.0), 13.0, SOFT, Align::Center);
        }
        self.pop_ui(canvas);
    }

    // ---------------------------------------------------------------
    // Saving and loading
    // ---------------------------------------------------------------

    fn summary(&self, label: &str) -> Summary {
        let (date, day) = self.economy.as_ref().map_or((String::new(), 0), |e| (e.date(&self.econ_data), e.day));
        Summary {
            label: label.to_string(),
            nation: self.player.as_ref().map_or(String::new(), |p| p.name.clone()),
            map: self.data.maps[self.map].name.clone(),
            date,
            day,
            difficulty: self.setup.difficulty.clone(),
            saved_at: save::now(),
        }
    }

    /// The game as bytes (None before the map).
    pub fn save_bytes(&self, label: &str) -> Option<Result<Vec<u8>, String>> {
        let game = self.save_ref()?;
        Some(save::to_bytes(&self.summary(label), &game))
    }

    /// The game's parts, borrowed, in the save's shape.
    fn save_ref(&self) -> Option<SavedRef<'_>> {
        let (Some(world), Some(e), Some(b), Some(w), Some(d)) = (self.world.as_ref(), self.economy.as_ref(), self.brains.as_ref(), self.wars.as_ref(), self.diplo.as_ref()) else { return None };
        Some(SavedRef {
            map: self.map,
            landed: &self.landed,
            player: &self.player,
            rivals: &self.rivals,
            world,
            sim: SimRef { economy: e, brains: b, wars: w, diplo: d },
            spectating: self.spectating,
            setup: &self.setup,
            fate: &self.fate,
            speed: self.clock.speed,
            paused: self.shell.pause && self.shell.was_paused || !self.shell.pause && self.clock.paused,
            tutorial_step: self.tutorial.as_ref().map(|t| t.step),
        })
    }

    /// Saves to `path`, saying how it went.
    pub fn save_game(&mut self, ctx: &mut Context, label: &str, path: &std::path::Path) -> bool {
        let result = match self.save_bytes(label) {
            None => Err("there's no game to save yet".to_string()),
            Some(r) => r.and_then(|bytes| save::write(path, &bytes).map(|_| bytes.len())),
        };
        match result {
            Ok(bytes) => {
                crate::ui::sfx(ctx, &self.assets.save_sound, 0.7, 0.0);
                let words = format!("Saved: {label} ({} KB)", bytes / 1024);
                self.notify(ctx, words.clone(), MINOR);
                self.say(words);
                ctx.report("saved", label);
                true
            }
            Err(e) => {
                crate::ui::sfx(ctx, &self.assets.denied, 0.7, 0.0);
                let words = format!("Couldn't save: {e}");
                self.notify(ctx, words.clone(), CRITICAL);
                self.say(words);
                false
            }
        }
    }

    /// Loads a save and goes onto its map; on failure says why and stays.
    pub fn load_from(&mut self, ctx: &mut Context, path: &std::path::Path) -> bool {
        let loaded = std::fs::read(path).map_err(|e| format!("couldn't read it ({e})")).and_then(|b| save::from_bytes(&b));
        match loaded {
            Ok((_, game)) => {
                self.install(ctx, game);
                ctx.report("loaded", path.to_string_lossy().to_string());
                true
            }
            Err(e) => {
                crate::ui::sfx(ctx, &self.assets.denied, 0.8, 0.0);
                self.say(format!("Can't load {}: {e}", path.file_name().map_or(String::new(), |f| f.to_string_lossy().to_string())));
                false
            }
        }
    }

    /// Puts a loaded game in place of whatever was there.
    pub fn install(&mut self, ctx: &mut Context, game: Saved) {
        self.stop_game_sounds(ctx);
        self.restart();
        self.map = game.map;
        self.landed = game.landed;
        self.player = game.player;
        self.rivals = game.rivals;
        self.map_view = Some(crate::map_view::MapView::new(&self.atlas, &game.world, &self.models));
        self.world = Some(game.world);
        let s = game.sim;
        self.economy = Some(s.economy);
        self.brains = Some(s.brains);
        self.wars = Some(s.wars);
        self.diplo = Some(s.diplo);
        self.spectating = game.spectating;
        self.setup = game.setup;
        self.fate = game.fate;
        self.clock = super::econ_screen::Clock::default();
        self.clock.speed = game.speed.min(super::econ_screen::SPECTATOR_SPEED);
        self.clock.paused = game.paused;
        self.tutorial = game.tutorial_step.map(super::tutorial::Tutorial::at);
        // Nothing old pops up as new.
        self.after_load_seen();
        let year = self.economy.as_ref().map_or(0, |e| e.year(&self.econ_data));
        self.shell.autosaved_year = year;
        self.shell.pause = false;
        self.enter_map(ctx);
        if let Some(view) = self.map_view.as_mut() {
            let world = self.world.as_ref().expect("installed");
            let (x, y) = self.atlas.map(world.map).provinces[world.capital[0]].middle;
            let p = view.world(x, y, 0.0);
            view.orbit.target = vec2(p.x, p.z);
        }
        let date = self.economy.as_ref().map_or(String::new(), |e| e.date(&self.econ_data));
        self.notify(ctx, format!("Loaded: {date}"), MINOR);
    }

    /// Once a game year (on the map, playing): the autosave, keeping the one before.
    pub(super) fn maybe_autosave(&mut self, ctx: &mut Context) {
        if self.spectating || !self.settings.autosave || self.fate.outcome.is_some_and(|o| !o.can_play_on()) {
            return;
        }
        let Some(year) = self.economy.as_ref().map(|e| e.year(&self.econ_data)) else { return };
        if year <= self.shell.autosaved_year {
            return;
        }
        self.shell.autosaved_year = year;
        let now = save::path_for("Autosave");
        let before = save::path_for("Autosave previous");
        if now.exists() {
            let _ = std::fs::rename(&now, &before);
        }
        // Compressed and written on another thread (a 100-year save is ~13 MB).
        let summary = self.summary("Autosave");
        let result = self.save_ref().map(|g| save::write_in_background(now, &summary, &g));
        match result {
            Some(Ok(())) => {
                crate::ui::sfx(ctx, &self.assets.save_sound, 0.4, 0.0);
                self.notify(ctx, format!("Autosaved ({})", summary.date), MINOR);
                ctx.report("saved", "Autosave");
            }
            Some(Err(e)) => self.notify(ctx, format!("Couldn't autosave: {e}"), CRITICAL),
            None => {}
        }
    }

    // ---------------------------------------------------------------
    // The Load screen
    // ---------------------------------------------------------------

    fn load_rect(i: usize) -> Rect {
        Rect::new(WIN.x + 40.0, WIN.y + 96.0 + i as f32 * 54.0, WIN.w - 80.0, 48.0)
    }

    pub(super) fn update_load(&mut self, ctx: &mut Context) {
        ctx.idle_ok();
        let n = self.shell.slots.len();
        let mouse = self.ui_mouse();
        if ctx.input.was_pressed(Key::Escape) {
            self.close_sub(ctx);
            return;
        }
        if n == 0 {
            return;
        }
        let before = self.shell.row;
        if ctx.input.was_pressed(Key::Down) {
            self.shell.row = (self.shell.row + 1) % n;
        }
        if ctx.input.was_pressed(Key::Up) {
            self.shell.row = (self.shell.row + n - 1) % n;
        }
        let mut go = ctx.input.was_pressed(Key::Enter);
        let first = self.shell.row.saturating_sub(8);
        for i in first..n.min(first + 9) {
            if mouse.is_some_and(|m| Self::load_rect(i - first).contains(m)) {
                if self.mouse_moved || ctx.input.was_mouse_pressed(MouseButton::Left) {
                    self.shell.row = i;
                }
                go |= ctx.input.was_mouse_pressed(MouseButton::Left);
            }
        }
        if self.shell.row != before {
            crate::ui::sfx(ctx, &self.assets.menu_move, 0.6, 0.0);
        }
        if ctx.input.was_pressed(Key::X) {
            let slot = self.shell.slots[self.shell.row].clone();
            match std::fs::remove_file(&slot.path) {
                Ok(()) => self.say(format!("Deleted {}", slot.path.display())),
                Err(e) => self.say(format!("Couldn't delete it: {e}")),
            }
            self.shell.slots = save::list();
            self.shell.row = self.shell.row.min(self.shell.slots.len().saturating_sub(1));
            return;
        }
        if go {
            let slot = self.shell.slots[self.shell.row].clone();
            if let Err(e) = &slot.summary {
                crate::ui::sfx(ctx, &self.assets.denied, 0.8, 0.0);
                self.say(format!("That save can't be loaded: {e}"));
                return;
            }
            crate::ui::sfx(ctx, &self.assets.click, 0.8, 0.0);
            self.shell.back = None;
            self.load_from(ctx, &slot.path);
        }
    }

    pub(super) fn draw_load(&self, canvas: &mut Canvas) {
        let a = &self.assets;
        self.push_ui(canvas);
        canvas.blit_sprite(&a.econ_window, vec2(WIN.x, WIN.y), Blit::default());
        canvas.blit_sprite(&a.icon_load, vec2(WIN.x + 40.0, WIN.y + 30.0), Blit::default());
        ui::plain(canvas, &a.font, "LOAD A GAME", vec2(WIN.x + 74.0, WIN.y + 28.0), 24.0, GOLD, Align::Left);
        ui::plain(canvas, &a.font, "Newest first. Autosaves happen every game year (Settings can turn them off).", vec2(WIN.right() - 30.0, WIN.y + 36.0), 12.0, MUTED, Align::Right);
        let n = self.shell.slots.len();
        if n == 0 {
            ui::plain(canvas, &a.font, "No saves yet. On the map, Esc then Save game, or F6 for a quick save.", vec2(640.0, 300.0), 18.0, MUTED, Align::Center);
        }
        let first = self.shell.row.saturating_sub(8);
        for i in first..n.min(first + 9) {
            let slot = &self.shell.slots[i];
            let r = Self::load_rect(i - first);
            let on = i == self.shell.row;
            canvas.fill_rect(r, if on { Color::rgba(245, 197, 66, 50) } else { Color::rgba(255, 255, 255, 10) });
            if on {
                canvas.draw_rect(r, ui::alpha(GOLD, 0.7));
            }
            let file = slot.path.file_stem().map_or(String::new(), |f| f.to_string_lossy().to_string());
            match &slot.summary {
                Ok(s) => {
                    canvas.blit_sprite(&a.icon_save, vec2(r.x + 12.0, r.y + 12.0), Blit::default());
                    ui::text(canvas, &a.serif, &s.nation, vec2(r.x + 46.0, r.y + 4.0), 18.0, if on { GOLD } else { Color::WHITE }, Align::Left, 360.0);
                    ui::plain(canvas, &a.font, &format!("{} · {}", s.label, s.map), vec2(r.x + 46.0, r.y + 28.0), 13.0, MUTED, Align::Left);
                    ui::plain(canvas, &a.font, &s.date, vec2(r.x + 620.0, r.y + 6.0), 16.0, Color::WHITE, Align::Right);
                    ui::plain(canvas, &a.font, &format!("{} · {}", s.difficulty, save::ago(s.saved_at)), vec2(r.x + 620.0, r.y + 28.0), 13.0, SOFT, Align::Right);
                    ui::plain(canvas, &a.font, &format!("{file}.lws"), vec2(r.right() - 14.0, r.y + 16.0), 12.0, SOFT, Align::Right);
                }
                Err(e) => {
                    ui::plain(canvas, &a.font, &format!("{file}.lws"), vec2(r.x + 46.0, r.y + 4.0), 17.0, SOFT, Align::Left);
                    ui::text(canvas, &a.font, &format!("Can't be loaded: {e}"), vec2(r.x + 46.0, r.y + 27.0), 13.0, ui::RED, Align::Left, r.w - 60.0);
                }
            }
            canvas.label(&format!("save {file}"), r);
        }
        ui::plain(canvas, &a.font, "Up / Down: choose  ·  Enter or click: load  ·  X: delete  ·  Esc: back", vec2(640.0, WIN.bottom() - 34.0), 13.0, SOFT, Align::Center);
        self.pop_ui(canvas);
        self.draw_message(canvas);
    }

    // ---------------------------------------------------------------
    // Settings
    // ---------------------------------------------------------------

    fn set_rect(i: usize) -> Rect {
        Rect::new(WIN.x + 40.0, WIN.y + 92.0 + i as f32 * 42.0, WIN.w - 80.0, 38.0)
    }

    fn set_value(&self, r: SetRow) -> String {
        let s = &self.settings;
        let pct = |v: f32| format!("{:.0}%", v * 100.0);
        match r {
            SetRow::Quality => format!("{}: {}", s.quality.name(), s.quality.words()),
            SetRow::Window => match s.window_scale {
                1 => "1280 x 720 (drag the window's edge for any size; applies next start)".to_string(),
                _ => "2560 x 1440 (applies next start)".to_string(),
            },
            SetRow::Fullscreen => if s.fullscreen { "On (borderless, the whole screen)" } else { "Off (a window)" }.to_string(),
            SetRow::Music => pct(s.music),
            SetRow::Effects => pct(s.effects),
            SetRow::UiScale => format!("{} (windows and menus)", pct(s.ui_scale)),
            SetRow::Autosave => if s.autosave { "Every game year" } else { "Off" }.to_string(),
            SetRow::Notes => ["Only what matters most", "Important news", "Everything"][s.notifications as usize].to_string(),
            SetRow::Tips => if s.tooltips { "On: hover a number to read what it is" } else { "Off" }.to_string(),
            SetRow::Keys => format!("{} actions · Enter to change", Act::ALL.len()),
            SetRow::Back => "Esc".to_string(),
        }
    }

    fn set_name(r: SetRow) -> &'static str {
        match r {
            SetRow::Quality => "Graphics quality",
            SetRow::Window => "Window size",
            SetRow::Fullscreen => "Fullscreen",
            SetRow::Music => "Music volume",
            SetRow::Effects => "Effects volume",
            SetRow::UiScale => "UI scale",
            SetRow::Autosave => "Autosave",
            SetRow::Notes => "Notifications",
            SetRow::Tips => "Tooltips",
            SetRow::Keys => "Keys",
            SetRow::Back => "Back",
        }
    }

    /// Left (-1) or right (+1) on a settings row.
    fn nudge(&mut self, r: SetRow, d: i32) {
        let s = &mut self.settings;
        let step = |v: f32| ((v + d as f32 * 0.1) * 10.0).round() / 10.0;
        match r {
            SetRow::Quality => {
                let i = Quality::ALL.iter().position(|q| *q == s.quality).unwrap_or(2) as i32;
                s.quality = Quality::ALL[(i + d).rem_euclid(4) as usize];
            }
            SetRow::Window => s.window_scale = if s.window_scale == 1 { 2 } else { 1 },
            SetRow::Fullscreen => s.fullscreen = !s.fullscreen,
            SetRow::Music => s.music = step(s.music).clamp(0.0, 1.0),
            SetRow::Effects => s.effects = step(s.effects).clamp(0.0, 1.0),
            SetRow::UiScale => {
                let i = UI_SCALES.iter().position(|x| (*x - s.ui_scale).abs() < 0.01).unwrap_or(1) as i32;
                s.ui_scale = UI_SCALES[(i + d).clamp(0, UI_SCALES.len() as i32 - 1) as usize];
            }
            SetRow::Autosave => s.autosave = !s.autosave,
            SetRow::Notes => s.notifications = (s.notifications as i32 + d).clamp(0, 2) as u8,
            SetRow::Tips => s.tooltips = !s.tooltips,
            SetRow::Keys | SetRow::Back => {}
        }
        self.shell.dirty = true;
    }

    fn key_rect(i: usize) -> Rect {
        let (col, row) = (i / 13, i % 13);
        Rect::new(WIN.x + 40.0 + col as f32 * 495.0, WIN.y + 92.0 + row as f32 * 32.0, 485.0, 29.0)
    }

    pub(super) fn update_settings(&mut self, ctx: &mut Context) {
        ctx.idle_ok();
        let mouse = self.ui_mouse();
        // Waiting for a key to bind.
        if let Some(act) = self.shell.binding {
            if ctx.input.was_pressed(Key::Escape) {
                self.shell.binding = None;
                return;
            }
            if let Some(k) = Key::ALL.into_iter().find(|&k| ctx.input.was_pressed(k)) {
                if crate::settings::reserved(k) {
                    crate::ui::sfx(ctx, &self.assets.denied, 0.7, 0.0);
                    self.say(format!("{} is kept for the game itself; choose another key", k.name()));
                } else {
                    self.settings.bind(act, k);
                    self.shell.binding = None;
                    self.shell.dirty = true;
                    crate::ui::sfx(ctx, &self.assets.click, 0.8, 0.0);
                    self.say(format!("{}: {}", act.words(), k.name()));
                }
            }
            return;
        }
        if self.shell.keys_page {
            let n = Act::ALL.len();
            if ctx.input.was_pressed(Key::Escape) {
                self.shell.keys_page = false;
                self.shell.row = SET_ROWS.iter().position(|r| *r == SetRow::Keys).unwrap_or(0);
                return;
            }
            let before = self.shell.row;
            if ctx.input.was_pressed(Key::Down) {
                self.shell.row = (self.shell.row + 1) % n;
            }
            if ctx.input.was_pressed(Key::Up) {
                self.shell.row = (self.shell.row + n - 1) % n;
            }
            if ctx.input.was_pressed(Key::Right) || ctx.input.was_pressed(Key::Left) {
                self.shell.row = (self.shell.row + 13) % 26 % n;
            }
            let mut go = ctx.input.was_pressed(Key::Enter);
            for i in 0..n {
                if mouse.is_some_and(|m| Self::key_rect(i).contains(m)) {
                    if self.mouse_moved || ctx.input.was_mouse_pressed(MouseButton::Left) {
                    self.shell.row = i;
                }
                    go |= ctx.input.was_mouse_pressed(MouseButton::Left);
                }
            }
            if self.shell.row != before {
                crate::ui::sfx(ctx, &self.assets.menu_move, 0.6, 0.0);
            }
            if ctx.input.was_pressed(Key::Backspace) {
                self.settings.reset_keys();
                self.shell.dirty = true;
                self.say("Every key is back to its default");
            }
            if go {
                self.shell.binding = Some(Act::ALL[self.shell.row]);
                crate::ui::sfx(ctx, &self.assets.click, 0.7, 0.0);
            }
            return;
        }
        if ctx.input.was_pressed(Key::Escape) {
            self.close_sub(ctx);
            return;
        }
        let n = SET_ROWS.len();
        let before = self.shell.row;
        if ctx.input.was_pressed(Key::Down) {
            self.shell.row = (self.shell.row + 1) % n;
        }
        if ctx.input.was_pressed(Key::Up) {
            self.shell.row = (self.shell.row + n - 1) % n;
        }
        let mut go = ctx.input.was_pressed(Key::Enter);
        let mut d = 0;
        if ctx.input.was_pressed(Key::Left) {
            d = -1;
        }
        if ctx.input.was_pressed(Key::Right) {
            d = 1;
        }
        for i in 0..n {
            let r = Self::set_rect(i);
            if mouse.is_some_and(|m| r.contains(m)) {
                if self.mouse_moved || ctx.input.was_mouse_pressed(MouseButton::Left) {
                    self.shell.row = i;
                }
                if ctx.input.was_mouse_pressed(MouseButton::Left) {
                    // Left of the middle lowers, right raises.
                    let m = mouse.unwrap_or_default();
                    d = if m.x < r.x + 300.0 { -1 } else { 1 };
                    go = true;
                }
            }
        }
        if self.shell.row != before {
            crate::ui::sfx(ctx, &self.assets.menu_move, 0.6, 0.0);
        }
        let row = SET_ROWS[self.shell.row];
        match row {
            SetRow::Keys if go => {
                self.shell.keys_page = true;
                self.shell.row = 0;
                crate::ui::sfx(ctx, &self.assets.click, 0.7, 0.0);
            }
            SetRow::Back if go => self.close_sub(ctx),
            SetRow::Keys | SetRow::Back => {}
            _ => {
                if d == 0 && go {
                    d = 1;
                }
                if d != 0 {
                    self.nudge(row, d);
                    self.apply_settings(ctx);
                    crate::ui::sfx(ctx, &self.assets.click, 0.7, 0.0);
                }
            }
        }
    }

    pub(super) fn draw_settings(&self, canvas: &mut Canvas) {
        let a = &self.assets;
        self.push_ui(canvas);
        canvas.blit_sprite(&a.econ_window, vec2(WIN.x, WIN.y), Blit::default());
        canvas.blit_sprite(&a.icon_gear, vec2(WIN.x + 40.0, WIN.y + 30.0), Blit::default());
        if self.shell.keys_page {
            ui::plain(canvas, &a.font, "SETTINGS · KEYS ON THE MAP", vec2(WIN.x + 74.0, WIN.y + 28.0), 24.0, GOLD, Align::Left);
            ui::plain(canvas, &a.font, "Enter: change · Backspace: defaults · Esc: back", vec2(WIN.right() - 30.0, WIN.y + 36.0), 12.0, MUTED, Align::Right);
            for (i, act) in Act::ALL.iter().enumerate() {
                let r = Self::key_rect(i);
                let on = i == self.shell.row;
                canvas.fill_rect(r, if on { Color::rgba(245, 197, 66, 50) } else { Color::rgba(255, 255, 255, 10) });
                ui::plain(canvas, &a.font, act.words(), vec2(r.x + 12.0, r.y + 6.0), 15.0, if on { GOLD } else { Color::WHITE }, Align::Left);
                let key = if self.shell.binding == Some(*act) { "press a key...".to_string() } else { self.settings.key(*act).name() };
                let changed = self.settings.key(*act) != act.default_key();
                ui::plain(canvas, &a.font, &key, vec2(r.right() - 12.0, r.y + 6.0), 15.0, if changed { ui::BLUE } else { MUTED }, Align::Right);
                canvas.label(&format!("key {}", act.id()), r);
            }
        } else {
            ui::plain(canvas, &a.font, "SETTINGS", vec2(WIN.x + 74.0, WIN.y + 28.0), 24.0, GOLD, Align::Left);
            ui::plain(canvas, &a.font, "Up / Down: choose · Left / Right or click: change · Esc: back (saved in lucky_world.settings.toml)", vec2(WIN.right() - 30.0, WIN.y + 36.0), 12.0, MUTED, Align::Right);
            for (i, row) in SET_ROWS.iter().enumerate() {
                let r = Self::set_rect(i);
                let on = i == self.shell.row;
                canvas.fill_rect(r, if on { Color::rgba(245, 197, 66, 50) } else { Color::rgba(255, 255, 255, 10) });
                if on {
                    canvas.draw_rect(r, ui::alpha(GOLD, 0.7));
                }
                ui::plain(canvas, &a.font, Self::set_name(*row), vec2(r.x + 14.0, r.y + 9.0), 17.0, if on { GOLD } else { Color::WHITE }, Align::Left);
                let v = self.set_value(*row);
                let arrows = !matches!(row, SetRow::Keys | SetRow::Back);
                if arrows {
                    ui::plain(canvas, &a.font, "<", vec2(r.x + 250.0, r.y + 8.0), 17.0, SOFT, Align::Left);
                    ui::plain(canvas, &a.font, ">", vec2(r.right() - 20.0, r.y + 8.0), 17.0, SOFT, Align::Left);
                }
                ui::text(canvas, &a.font, &v, vec2(r.x + 276.0, r.y + 10.0), 15.0, MUTED, Align::Left, r.w - 310.0);
                if matches!(row, SetRow::Music | SetRow::Effects) {
                    let v = if *row == SetRow::Music { self.settings.music } else { self.settings.effects };
                    let bar = Rect::new(r.x + 340.0, r.y + 15.0, 300.0, 8.0);
                    canvas.fill_rect(bar, Color::rgba(255, 255, 255, 30));
                    canvas.fill_rect(Rect::new(bar.x, bar.y, bar.w * v, bar.h), GOLD);
                }
                canvas.label(&format!("setting {}", Self::set_name(*row)), r);
            }
            let note = "The world map is always drawn by your graphics card; quality only changes how the wheels are smoothed and how busy the map is.";
            ui::text(canvas, &a.font, note, vec2(640.0, WIN.bottom() - 36.0), 12.0, SOFT, Align::Center, WIN.w - 80.0);
        }
        self.pop_ui(canvas);
        self.draw_message(canvas);
    }

    // ---------------------------------------------------------------
    // Help
    // ---------------------------------------------------------------

    pub(super) fn update_help(&mut self, ctx: &mut Context) {
        ctx.idle_ok();
        let pages = self.help.len().max(1);
        let before = self.shell.help_page;
        if ctx.input.was_pressed(Key::Right) || ctx.input.was_pressed(Key::Tab) || ctx.input.was_pressed(Key::Down) {
            self.shell.help_page = (self.shell.help_page + 1) % pages;
        }
        if ctx.input.was_pressed(Key::Left) || ctx.input.was_pressed(Key::Up) {
            self.shell.help_page = (self.shell.help_page + pages - 1) % pages;
        }
        let mouse = self.ui_mouse();
        if ctx.input.was_mouse_pressed(MouseButton::Left) {
            for i in 0..pages {
                if mouse.is_some_and(|m| Self::help_tab(i).contains(m)) {
                    self.shell.help_page = i;
                }
            }
        }
        if self.shell.help_page != before {
            crate::ui::sfx(ctx, &self.assets.menu_move, 0.7, 0.0);
        }
        if ctx.input.was_pressed(Key::Escape) || ctx.input.was_pressed(Key::Enter) {
            self.close_sub(ctx);
        }
    }

    fn help_tab(i: usize) -> Rect {
        Rect::new(WIN.x + 30.0, WIN.y + 90.0 + i as f32 * 42.0, 230.0, 36.0)
    }

    pub(super) fn draw_help(&self, canvas: &mut Canvas) {
        let a = &self.assets;
        self.push_ui(canvas);
        canvas.blit_sprite(&a.econ_window, vec2(WIN.x, WIN.y), Blit::default());
        canvas.blit_sprite(&a.icon_help, vec2(WIN.x + 40.0, WIN.y + 30.0), Blit::default());
        ui::plain(canvas, &a.font, "HOW TO PLAY", vec2(WIN.x + 74.0, WIN.y + 28.0), 24.0, GOLD, Align::Left);
        ui::plain(canvas, &a.font, "Up / Down or click: a page  ·  Esc: back", vec2(WIN.right() - 30.0, WIN.y + 36.0), 12.0, MUTED, Align::Right);
        let page = self.shell.help_page.min(self.help.len().saturating_sub(1));
        for (i, (title, _)) in self.help.iter().enumerate() {
            let r = Self::help_tab(i);
            let on = i == page;
            canvas.fill_rect(r, if on { Color::rgba(245, 197, 66, 60) } else { Color::rgba(255, 255, 255, 10) });
            ui::plain(canvas, &a.font, title, vec2(r.x + 12.0, r.y + 9.0), 15.0, if on { GOLD } else { Color::WHITE }, Align::Left);
            canvas.label(&format!("help {title}"), r);
        }
        if let Some((title, body)) = self.help.get(page) {
            let x = WIN.x + 290.0;
            ui::plain(canvas, &a.serif, title, vec2(x, WIN.y + 90.0), 26.0, Color::WHITE, Align::Left);
            let mut style = ui::style(15.0, Color::WHITE);
            style.outline = 0.0;
            style.shadow = None;
            style.wrap = Some(WIN.w - 330.0);
            let mut y = WIN.y + 130.0;
            for para in body.split("\n\n") {
                let para = para.replace('\n', " ");
                let bullet = para.starts_with("- ");
                let words = if bullet { format!("•  {}", &para[2..]) } else { para };
                let r = canvas.draw_font_text(&a.font, &words, vec2(x + if bullet { 12.0 } else { 0.0 }, y), &style);
                y += r.h + 10.0;
            }
        }
        self.pop_ui(canvas);
    }

    // ---------------------------------------------------------------
    // Victory and defeat
    // ---------------------------------------------------------------

    /// A result came in: its screen, its sound, the clock stops.
    pub(super) fn show_outcome(&mut self, ctx: &mut Context, o: Outcome) {
        self.clock.paused = true;
        self.fate.shown = true;
        crate::ui::sfx(ctx, if o.won() { &self.assets.victory } else { &self.assets.defeat }, 0.9, 0.0);
        if o.won() {
            ui::confetti(&mut self.particles, &mut self.fx, 160);
        }
        self.shell.row = 0;
        ctx.report("outcome", o.title());
        self.go(Screen::Outcome);
    }

    pub(super) fn update_outcome(&mut self, ctx: &mut Context) {
        ctx.idle_ok();
        let Some(o) = self.fate.outcome else {
            self.go(Screen::Map);
            return;
        };
        if self.screen_time < 0.6 {
            return;
        }
        let mouse = self.ui_mouse();
        let on_play = o.can_play_on() && (ctx.input.was_pressed(Key::Enter) || ctx.input.was_mouse_pressed(MouseButton::Left) && mouse.is_some_and(|m| OUTCOME_PLAY.contains(m)));
        let to_menu = ctx.input.was_pressed(Key::Escape) || ctx.input.was_pressed(Key::M) || ctx.input.was_mouse_pressed(MouseButton::Left) && mouse.is_some_and(|m| OUTCOME_MENU.contains(m));
        if on_play {
            self.fate.playing_on = true;
            self.clock.paused = false;
            crate::ui::sfx(ctx, &self.assets.click, 0.8, 0.0);
            self.go(Screen::Map);
            let words = match o {
                Outcome::Lost(crate::victory::Loss::Vassal) => "Playing on as a vassal: win back your independence (K: the war room, P: diplomacy)",
                Outcome::Lost(_) => "Playing on: rebuild, and the world may still be yours",
                Outcome::Won(_) => "Playing on: the world goes on (other victories still count)",
            };
            self.notify(ctx, words.to_string(), CRITICAL);
        } else if to_menu {
            self.leave_game(ctx);
            self.open_menu();
        }
    }

    pub(super) fn draw_outcome(&self, canvas: &mut Canvas) {
        let a = &self.assets;
        let Some(o) = self.fate.outcome else { return };
        let (Some(world), Some(e)) = (self.world.as_ref(), self.economy.as_ref()) else { return };
        let ed = &self.econ_data;
        let me = &world.nations[0];
        self.push_ui(canvas);
        let banner = if o.won() { &a.banner_victory } else { &a.banner_defeat };
        let rise = (1.0 - ui::ease_out_back(self.screen_time / 0.7)) * -60.0;
        if o.won() {
            ui::glow(canvas, a, vec2(640.0, 70.0), 1000.0, 0.25 + 0.06 * (self.time * 2.0).sin());
        }
        canvas.blit_sprite(banner, vec2(190.0, 6.0 + rise), Blit::default());
        ui::text(canvas, &a.font, o.title(), vec2(640.0, 36.0 + rise), 40.0, if o.won() { ui::GOLD_HI } else { Color::from_hex(0xffd0d0) }, Align::Center, 640.0);
        canvas.label("outcome banner", Rect::new(190.0, 6.0, 900.0, 130.0));
        let why = self.outcome_words(o);
        ui::text(canvas, &a.font, &why, vec2(640.0, 142.0), 17.0, Color::WHITE, Align::Center, 1180.0);

        // Left: the nation and its wheels.
        let left = Rect::new(30.0, 176.0, 380.0, 470.0);
        panel(canvas, left);
        crate::country::draw_flag(canvas, Rect::new(left.x + 18.0, left.y + 16.0, 60.0, 40.0), &me.flag);
        ui::text(canvas, &a.serif, &me.name, vec2(left.x + 90.0, left.y + 14.0), 22.0, Color::WHITE, Align::Left, 270.0);
        ui::plain(canvas, &a.font, &format!("{} · {}", self.data.maps[world.map].name, e.date(ed)), vec2(left.x + 90.0, left.y + 42.0), 13.0, MUTED, Align::Left);
        ui::plain(canvas, &a.font, "YOUR WHEELS", vec2(left.x + 18.0, left.y + 70.0), 12.0, GOLD, Align::Left);
        for (w, l) in me.landed.iter().enumerate() {
            let y = left.y + 90.0 + w as f32 * 23.0;
            ui::plain(canvas, &a.font, &self.data.wheels[w].title, vec2(left.x + 18.0, y), 13.0, SOFT, Align::Left);
            let words = crate::country::result_text(&self.data, world.map, w, l);
            ui::text(canvas, &a.font, &words, vec2(left.right() - 16.0, y), 13.0, Color::WHITE, Align::Right, 200.0);
        }

        // Middle: the wars, and the numbers over the years.
        let mid = Rect::new(424.0, 176.0, 420.0, 470.0);
        panel(canvas, mid);
        ui::plain(canvas, &a.font, &format!("YOUR WARS ({})", self.fate.wars.len()), vec2(mid.x + 18.0, mid.y + 14.0), 12.0, GOLD, Align::Left);
        if self.fate.wars.is_empty() {
            ui::plain(canvas, &a.font, "None: a peaceful history", vec2(mid.x + 18.0, mid.y + 36.0), 14.0, MUTED, Align::Left);
        }
        let dpy = ed.days_per_year;
        for (k, s) in self.fate.wars.iter().rev().take(7).enumerate() {
            let y = mid.y + 36.0 + k as f32 * 34.0;
            let years = format!("Year {}{}", s.start / dpy + 1, s.end.map_or(" to now".to_string(), |x| if x / dpy == s.start / dpy { String::new() } else { format!("-{}", x / dpy + 1) }));
            let color = match s.won {
                Some(true) => ui::GREEN,
                Some(false) => ui::RED,
                None => MUTED,
            };
            let what = if s.civil { "Civil war".to_string() } else if s.we_started { format!("We attacked {}", s.enemy) } else { format!("{} attacked us", s.enemy) };
            ui::text(canvas, &a.font, &what, vec2(mid.x + 18.0, y), 14.0, Color::WHITE, Align::Left, 270.0);
            ui::plain(canvas, &a.font, &years, vec2(mid.right() - 16.0, y), 12.0, SOFT, Align::Right);
            let res = s.result.clone().unwrap_or_else(|| "still going".to_string());
            ui::text(canvas, &a.font, &res, vec2(mid.x + 18.0, y + 16.0), 12.0, color, Align::Left, 384.0);
        }
        // The score over the years.
        let chart = Rect::new(mid.x + 18.0, mid.y + 300.0, mid.w - 36.0, 130.0);
        ui::plain(canvas, &a.font, "SCORE AND RANK, YEAR BY YEAR", vec2(chart.x, chart.y - 22.0), 12.0, GOLD, Align::Left);
        canvas.fill_rect(chart, Color::rgba(255, 255, 255, 10));
        let ys = &self.fate.years;
        if ys.len() >= 2 {
            let top = ys.iter().map(|y| y.score).fold(1.0, f64::max);
            let pt = |i: usize, v: f64| vec2(chart.x + chart.w * i as f32 / (ys.len() - 1) as f32, chart.bottom() - chart.h * (v / top) as f32);
            for i in 1..ys.len() {
                canvas.draw_line(pt(i - 1, ys[i - 1].score), pt(i, ys[i].score), GOLD);
                canvas.draw_line(pt(i - 1, ys[i - 1].score) + vec2(0.0, 1.0), pt(i, ys[i].score) + vec2(0.0, 1.0), GOLD);
            }
            let last = ys[ys.len() - 1];
            ui::plain(canvas, &a.font, &format!("{:.0} points · rank {}", last.score, last.rank), vec2(chart.right(), chart.bottom() + 6.0), 12.0, MUTED, Align::Right);
            let best = ys.iter().map(|y| y.rank).min().unwrap_or(0);
            ui::plain(canvas, &a.font, &format!("best rank {best}"), vec2(chart.x, chart.bottom() + 6.0), 12.0, MUTED, Align::Left);
        } else {
            ui::plain(canvas, &a.font, "(after the first year)", vec2(chart.center().x, chart.center().y - 8.0), 13.0, SOFT, Align::Center);
        }

        // Right: the best and worst moments.
        let right = Rect::new(858.0, 176.0, 392.0, 470.0);
        panel(canvas, right);
        let mut y = right.y + 14.0;
        for (good, title) in [(true, "BEST MOMENTS"), (false, "WORST MOMENTS")] {
            ui::plain(canvas, &a.font, title, vec2(right.x + 18.0, y), 12.0, if good { ui::GREEN } else { ui::RED }, Align::Left);
            y += 20.0;
            let top = self.fate.top(good, 4);
            if top.is_empty() {
                ui::plain(canvas, &a.font, "none yet", vec2(right.x + 18.0, y), 13.0, SOFT, Align::Left);
                y += 22.0;
            }
            for m in top {
                ui::plain(canvas, &a.font, &format!("Year {}", m.day / dpy + 1), vec2(right.x + 18.0, y), 12.0, SOFT, Align::Left);
                let mut st = ui::style(14.0, Color::WHITE);
                st.outline = 0.0;
                st.shadow = None;
                st.wrap = Some(right.w - 100.0);
                let r = canvas.draw_font_text(&a.font, &m.text, vec2(right.x + 82.0, y), &st);
                y += r.h.max(16.0) + 9.0;
            }
            y += 14.0;
        }

        // The buttons.
        let mouse = self.ui_mouse();
        for (r, words, show) in [(OUTCOME_PLAY, if o.won() { "PLAY ON (Enter)" } else { "PLAY ON (Enter)" }, o.can_play_on()), (OUTCOME_MENU, "MAIN MENU (Esc)", true)] {
            if !show {
                continue;
            }
            let hot = mouse.is_some_and(|m| r.contains(m));
            canvas.fill_rect(r, if hot { Color::rgba(245, 197, 66, 120) } else { Color::rgba(245, 197, 66, 50) });
            canvas.draw_rect(r, GOLD);
            ui::plain(canvas, &a.font, words, vec2(r.center().x, r.y + 10.0), 17.0, Color::WHITE, Align::Center);
            canvas.label(&format!("button {words}"), r);
        }
        self.pop_ui(canvas);
    }

    /// One sentence on why the game ended this way.
    fn outcome_words(&self, o: Outcome) -> String {
        use crate::victory::{Loss, Win};
        let v = &self.victory.victory;
        let (Some(e), Some(w), Some(d)) = (self.economy.as_ref(), self.wars.as_ref(), self.diplo.as_ref()) else { return String::new() };
        let (mine, held, allies, _) = crate::victory::shares(e, w, d, 0);
        match o {
            Outcome::Won(Win::Conquest) => format!("You hold {:.0}% of the world's provinces (you needed {:.0}%).", mine * 100.0, v.conquest_share * 100.0),
            Outcome::Won(Win::Economic) => format!("Your economy was {:.1} times the next biggest for {} years.", v.economic_lead, v.economic_years),
            Outcome::Won(Win::Diplomatic) => format!("Your alliance of {} nations has held {:.0}% of the world for {} years, and you lead it.", allies + 1, held * 100.0, v.diplomatic_years),
            Outcome::Won(Win::Score) => format!("After {} years, yours is the best score in the world.", self.setup.years),
            Outcome::Lost(Loss::Conquered) => "Your last province fell. Your nation is gone.".to_string(),
            Outcome::Lost(Loss::Overthrown) => "Rebels took the capital: a new government rules your nation.".to_string(),
            Outcome::Lost(Loss::Vassal) => "You lost a war and became a vassal. Play on to fight for your independence.".to_string(),
            Outcome::Lost(Loss::OutScored) => {
                let rank = self.fate.years.last().map_or(0, |y| y.rank);
                format!("After {} years another nation's score is higher: you are number {rank}.", self.setup.years)
            }
        }
    }

    // ---------------------------------------------------------------
    // Notifications
    // ---------------------------------------------------------------

    /// Says something on the map: shown now if nothing is, else queued.
    /// Repeats within 30 s are dropped; the settings decide which levels show.
    pub fn notify(&mut self, ctx: &mut Context, words: String, level: u8) {
        if words.is_empty() {
            return;
        }
        let now = self.time;
        let date = self.economy.as_ref().map_or(String::new(), |e| e.date(&self.econ_data));
        self.notes.log.push((date, words.clone(), level));
        if self.notes.log.len() > 300 {
            self.notes.log.remove(0);
        }
        if level > self.settings.notifications {
            return;
        }
        self.notes.recent.retain(|(_, t)| now - *t < 30.0);
        if self.notes.recent.iter().any(|(w, _)| *w == words) {
            return;
        }
        self.notes.recent.push((words.clone(), now));
        let secs = match level {
            CRITICAL => 6.0,
            IMPORTANT => 4.5,
            _ => 3.0,
        };
        let busy = self.toast.as_ref().is_some_and(|(_, t)| *t > 0.6);
        if !busy || level == CRITICAL {
            if busy {
                // The one showing goes back in the queue (critical news cuts in).
                if let Some((w, t)) = self.toast.take() {
                    self.notes.queue.push_front((w, t.max(1.5), IMPORTANT));
                }
            }
            self.show_note(ctx, words, secs, level);
        } else {
            self.notes.queue.push_back((words, secs, level));
            // Too much at once: the least important (oldest first) goes.
            while self.notes.queue.len() > 3 {
                let drop = (0..self.notes.queue.len()).max_by_key(|&i| (self.notes.queue[i].2, std::cmp::Reverse(i))).unwrap_or(0);
                self.notes.queue.remove(drop);
            }
        }
    }

    fn show_note(&mut self, ctx: &mut Context, words: String, secs: f32, level: u8) {
        self.toast = Some((words, secs));
        self.notes.shown += 1;
        if level <= IMPORTANT && self.time - self.notes.last_sound > 2.0 {
            self.notes.last_sound = self.time;
            crate::ui::sfx(ctx, &self.assets.news, if level == CRITICAL { 0.75 } else { 0.55 }, 0.0);
        }
    }

    /// Each frame: the next note when the last one is done.
    pub(super) fn update_notes(&mut self, ctx: &mut Context) {
        if let Some(e) = save::background_problem() {
            self.notify(ctx, format!("Couldn't autosave: {e}"), CRITICAL);
        }
        if self.toast.is_none() {
            if let Some((w, s, l)) = self.notes.queue.pop_front() {
                // Queued news shows a little shorter.
                self.show_note(ctx, w, s * 0.8, l);
            }
        }
        if let Some(m) = &mut self.shell.message {
            m.1 -= ctx.dt();
            if m.1 <= 0.0 {
                self.shell.message = None;
            }
        }
    }

    pub(super) fn draw_log(&self, canvas: &mut Canvas) {
        if !self.notes.log_open {
            return;
        }
        let a = &self.assets;
        ui::dim(canvas, 0.6);
        self.push_ui(canvas);
        canvas.blit_sprite(&a.econ_window, vec2(WIN.x, WIN.y), Blit::default());
        canvas.blit_sprite(&a.icon_bell, vec2(WIN.x + 40.0, WIN.y + 30.0), Blit::default());
        ui::plain(canvas, &a.font, "NOTIFICATIONS", vec2(WIN.x + 74.0, WIN.y + 28.0), 24.0, GOLD, Align::Left);
        let shown = ["only what matters most", "important news", "everything"][self.settings.notifications as usize];
        ui::plain(canvas, &a.font, &format!("Newest first. Pop-ups show {shown} (Settings). L or Esc: close"), vec2(WIN.right() - 30.0, WIN.y + 36.0), 12.0, MUTED, Align::Right);
        for (k, (date, words, level)) in self.notes.log.iter().rev().take(19).enumerate() {
            let y = WIN.y + 96.0 + k as f32 * 24.0;
            let color = match *level {
                CRITICAL => GOLD,
                IMPORTANT => Color::WHITE,
                _ => MUTED,
            };
            ui::plain(canvas, &a.font, date, vec2(WIN.x + 40.0, y), 13.0, SOFT, Align::Left);
            ui::text(canvas, &a.font, words, vec2(WIN.x + 190.0, y), 14.0, color, Align::Left, WIN.w - 230.0);
        }
        self.pop_ui(canvas);
    }

    /// A sound by name (for code that decides which before it can borrow the assets).
    pub fn sound_named(&self, ctx: &mut Context, name: &str) {
        let a = &self.assets;
        let (wav, v) = match name {
            "treaty" => (&a.treaty, 0.8),
            "denied" => (&a.denied, 0.7),
            "spy" => (&a.spy, 0.8),
            "declare" => (&a.declare, 0.7),
            "peace" => (&a.peace, 0.7),
            "build" => (&a.build, 0.7),
            "click" => (&a.click, 0.6),
            _ => (&a.select, 0.7),
        };
        crate::ui::sfx(ctx, wav, v, 0.0);
    }

    // ---------------------------------------------------------------
    // Music
    // ---------------------------------------------------------------

    /// Asks for a track; it fades in over a second or two as the last fades out.
    pub fn want_music(&mut self, ctx: &mut Context, track: Track) {
        if self.music.track != Some(track) {
            if let Some(id) = self.music.id.take() {
                if let Some((old, _)) = self.music.fading.take() {
                    ctx.stop_loop(old);
                }
                self.music.fading = Some((id, self.music.volume));
            }
            let wav = match track {
                Track::Menu => &self.assets.menu_theme,
                Track::Peace => &self.assets.peace_theme,
                Track::War => &self.assets.war_drums,
            };
            self.music.id = Some(ctx.play_loop(wav, 0.0));
            self.music.volume = 0.0;
            self.music.track = Some(track);
        }
    }

    pub fn stop_music(&mut self, ctx: &mut Context) {
        if let Some(id) = self.music.id.take() {
            ctx.stop_loop(id);
        }
        if let Some((id, _)) = self.music.fading.take() {
            ctx.stop_loop(id);
        }
        self.music.track = None;
    }

    pub(super) fn update_music(&mut self, ctx: &mut Context) {
        let dt = ctx.dt();
        let full = match self.music.track {
            Some(Track::Menu) => 0.55,
            Some(Track::Peace) => 0.35,
            Some(Track::War) => 0.4,
            None => 0.0,
        } * self.settings.music;
        if let Some(id) = self.music.id {
            if (self.music.volume - full).abs() > 0.001 {
                self.music.volume = if self.music.volume < full { (self.music.volume + dt * 0.5).min(full) } else { (self.music.volume - dt * 0.8).max(full) };
                ctx.set_loop_volume(id, self.music.volume);
            }
        }
        if let Some((id, v)) = self.music.fading {
            let v = v - dt * 0.6;
            if v <= 0.0 {
                ctx.stop_loop(id);
                self.music.fading = None;
            } else {
                ctx.set_loop_volume(id, v);
                self.music.fading = Some((id, v));
            }
        }
        ctx.report("music", self.music.track.map_or("none".to_string(), |t| format!("{t:?}")));
    }

    // ---------------------------------------------------------------
    // Tooltips and UI scale
    // ---------------------------------------------------------------

    /// Notes a number's area and its tip (drawn this frame, read next frame).
    pub fn tip(&self, rect: Rect, key: &str) {
        if !self.settings.tooltips {
            return;
        }
        let words = self.tip_text.get(key).cloned().unwrap_or_else(|| key.to_string());
        self.tips.areas.borrow_mut().push((self.screen_rect(rect), words));
    }

    /// A rectangle drawn at the UI scale, in screen pixels.
    fn screen_rect(&self, r: Rect) -> Rect {
        if !self.tips.scaled.get() {
            return r;
        }
        let s = self.settings.ui_scale;
        let c = vec2(ui::W / 2.0, ui::H / 2.0);
        let p = (vec2(r.x, r.y) - c) * s + c;
        Rect::new(p.x, p.y, r.w * s, r.h * s)
    }

    /// How close the player is to each victory, in words (the score's tip).
    pub fn victory_progress(&self) -> String {
        let v = &self.victory.victory;
        let (Some(e), Some(w), Some(d)) = (self.economy.as_ref(), self.wars.as_ref(), self.diplo.as_ref()) else { return String::new() };
        let (mine, held, allies, leads) = crate::victory::shares(e, w, d, 0);
        let next = (1..e.nations.len()).map(|j| e.nations[j].gdp).fold(0.0, f64::max).max(1.0);
        let lead_years = self.fate.lead_days as f64 / self.econ_data.days_per_year as f64;
        let ends = if self.setup.years == 0 { "no time limit".to_string() } else { format!("the game ends on score after Year {}", self.setup.years) };
        format!(
            "{} Conquest: you hold {:.0}% of the map (need {:.0}%). Economic: your GDP is {:.1}x the next (need {:.1}x for {} years; {:.1} so far). Diplomatic: your alliance holds {:.0}% with {} allies{} (need {:.0}% and {} allies for {} years; {:.1} so far). Rank is by last year's score; {ends}.",
            self.tip_text.get("score").cloned().unwrap_or_default(),
            mine * 100.0,
            v.conquest_share * 100.0,
            e.nations[0].gdp / next,
            v.economic_lead,
            v.economic_years,
            lead_years,
            held * 100.0,
            allies,
            if leads { ", led by you" } else { ", not led by you" },
            v.diplomatic_share * 100.0,
            v.diplomatic_allies,
            v.diplomatic_years,
            self.fate.bloc_days as f64 / self.econ_data.days_per_year as f64
        )
    }

    /// Like `tip`, with words made on the spot.
    pub fn tip_words(&self, rect: Rect, words: String) {
        if self.settings.tooltips {
            self.tips.areas.borrow_mut().push((self.screen_rect(rect), words));
        }
    }

    pub(super) fn update_tips(&mut self, ctx: &mut Context) {
        let mouse = self.mouse;
        let found = mouse.and_then(|m| self.tips.areas.borrow().iter().rev().find(|(r, _)| r.contains(m)).cloned());
        match (&self.tips.hover, found) {
            (Some((r, _)), Some((r2, w))) if *r == r2 => {
                self.tips.hover_time += ctx.dt();
                self.tips.hover = Some((r2, w));
            }
            (_, Some(f)) => {
                self.tips.hover = Some(f);
                self.tips.hover_time = 0.0;
            }
            (_, None) => {
                self.tips.hover = None;
                self.tips.hover_time = 0.0;
            }
        }
        ctx.report("tooltip", self.tips.hover.as_ref().filter(|_| self.tips.hover_time > 0.35).map_or(String::new(), |(_, w)| w.clone()));
    }

    pub(super) fn draw_tip(&self, canvas: &mut Canvas) {
        let Some((r, words)) = self.tips.hover.as_ref().filter(|_| self.tips.hover_time > 0.35) else { return };
        let a = &self.assets;
        let mut st = ui::style(14.0, Color::WHITE);
        st.outline = 0.0;
        st.shadow = None;
        st.wrap = Some(330.0);
        // Its size: the words' width, wrapped at 330 px (lines counted from the unwrapped width).
        let mut one = st.clone();
        one.wrap = None;
        let full = a.font.measure(words, &one);
        let lines = (full.x / 300.0).ceil().max(1.0);
        let w = full.x.min(330.0) + 24.0;
        let h = lines * full.y * 1.25 + 16.0;
        let mut x = r.x;
        let mut y = r.bottom() + 6.0;
        if x + w > ui::W - 6.0 {
            x = ui::W - 6.0 - w;
        }
        if y + h > ui::H - 6.0 {
            y = r.y - h - 6.0;
        }
        let box_ = Rect::new(x.max(6.0), y.max(6.0), w, h);
        canvas.fill_rect(box_, Color::rgba(12, 6, 32, 238));
        canvas.draw_rect(box_, ui::alpha(MUTED, 0.8));
        canvas.draw_font_text(&a.font, words, vec2(box_.x + 12.0, box_.y + 9.0), &st);
        canvas.label("tooltip", box_);
    }

    /// Windows and menus drawn at the UI scale, round the screen's middle.
    pub fn push_ui(&self, canvas: &mut Canvas) {
        let mut cam = Camera2D::new(ui::W, ui::H);
        cam.zoom = self.settings.ui_scale;
        cam.position = vec2(ui::W / 2.0, ui::H / 2.0);
        canvas.push_camera(&cam);
        self.tips.scaled.set(true);
    }

    pub fn pop_ui(&self, canvas: &mut Canvas) {
        canvas.pop_camera();
        self.tips.scaled.set(false);
    }

    /// The mouse in the scaled windows' coordinates.
    pub fn ui_mouse(&self) -> Option<Vec2> {
        let s = self.settings.ui_scale;
        let c = vec2(ui::W / 2.0, ui::H / 2.0);
        self.mouse.map(|m| (m - c) * (1.0 / s) + c)
    }
}

/// The outcome screen's buttons.
const OUTCOME_PLAY: Rect = Rect { x: 420.0, y: 660.0, w: 210.0, h: 44.0 };
const OUTCOME_MENU: Rect = Rect { x: 650.0, y: 660.0, w: 210.0, h: 44.0 };

/// A dark rounded panel with a soft outline.
pub fn panel(canvas: &mut Canvas, r: Rect) {
    canvas.fill_rect(r, Color::rgba(18, 10, 44, 236));
    canvas.draw_rect(r, Color::rgba(127, 168, 255, 170));
    canvas.draw_rect(Rect::new(r.x + 3.0, r.y + 3.0, r.w - 6.0, r.h - 6.0), Color::rgba(255, 255, 255, 30));
}

/// The save file's shape, borrowing the game's parts (no copies).
#[derive(serde::Serialize)]
struct SavedRef<'a> {
    map: usize,
    landed: &'a Vec<crate::fate::Landed>,
    player: &'a Option<crate::country::Country>,
    rivals: &'a Vec<crate::country::Country>,
    world: &'a crate::world::World,
    sim: SimRef<'a>,
    spectating: bool,
    setup: &'a crate::save::Setup,
    fate: &'a crate::victory::Fate,
    speed: usize,
    paused: bool,
    tutorial_step: Option<usize>,
}

#[derive(serde::Serialize)]
struct SimRef<'a> {
    economy: &'a crate::econ::Economy,
    brains: &'a crate::ai::Brains,
    wars: &'a crate::war::Wars,
    diplo: &'a crate::diplo::Diplomacy,
}

impl Notes {
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }
}
