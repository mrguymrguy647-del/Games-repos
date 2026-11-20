//! The player's settings, kept in `lucky_world.settings.toml` beside the
//! game: picture quality, window size, fullscreen, volumes, UI scale and
//! the map's keys.

use duckforge::prelude::*;
use std::collections::BTreeMap;

pub const FILE: &str = "lucky_world.settings.toml";

/// Picture quality. The world map is always drawn by the graphics card
/// (no CPU-only effects there, ISSUES.md item 15); quality decides how the
/// wheel screens are smoothed, and what the map shows.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    /// Everything on the graphics card, no smoothing, fewer map decorations.
    Low,
    /// Everything on the graphics card, no smoothing.
    Medium,
    /// The wheels smoothed (2x); the map on the graphics card.
    High,
    /// The wheels smoothed 3x with a glow; the map on the graphics card.
    Ultra,
}

impl Quality {
    pub const ALL: [Quality; 4] = [Quality::Low, Quality::Medium, Quality::High, Quality::Ultra];

    /// Supersampling and bloom on the wheel screens.
    pub fn wheel_effects(self) -> (u32, f32) {
        match self {
            Quality::Low | Quality::Medium => (1, 0.0),
            Quality::High => (2, 0.0),
            Quality::Ultra => (3, 1.0),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Quality::Low => "Low",
            Quality::Medium => "Medium",
            Quality::High => "High",
            Quality::Ultra => "Ultra",
        }
    }

    pub fn words(self) -> &'static str {
        match self {
            Quality::Low => "All on the graphics card, no smoothing, fewer trees and props",
            Quality::Medium => "All on the graphics card, no smoothing",
            Quality::High => "Smooth wheels (2x); the map stays on the graphics card",
            Quality::Ultra => "Smoother wheels (3x) with a glow; costly on slow PCs",
        }
    }
}

/// Things the player does on the map, each on a key they can change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Act {
    Pause,
    Economy,
    WarRoom,
    Diplomacy,
    Spies,
    Overlay,
    Battle,
    Card,
    Home,
    NextNation,
    MapMode,
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
    TurnLeft,
    TurnRight,
    ZoomIn,
    ZoomOut,
    TiltUp,
    TiltDown,
    QuickSave,
    QuickLoad,
    Help,
    Log,
}

impl Act {
    pub const ALL: [Act; 25] = [
        Act::Pause,
        Act::Economy,
        Act::WarRoom,
        Act::Diplomacy,
        Act::Spies,
        Act::Overlay,
        Act::Battle,
        Act::Card,
        Act::Home,
        Act::NextNation,
        Act::MapMode,
        Act::PanUp,
        Act::PanDown,
        Act::PanLeft,
        Act::PanRight,
        Act::TurnLeft,
        Act::TurnRight,
        Act::ZoomIn,
        Act::ZoomOut,
        Act::TiltUp,
        Act::TiltDown,
        Act::QuickSave,
        Act::QuickLoad,
        Act::Help,
        Act::Log,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Act::Pause => "pause",
            Act::Economy => "economy",
            Act::WarRoom => "war_room",
            Act::Diplomacy => "diplomacy",
            Act::Spies => "spies",
            Act::Overlay => "overlay",
            Act::Battle => "battle",
            Act::Card => "card",
            Act::Home => "home",
            Act::NextNation => "next_nation",
            Act::MapMode => "map_mode",
            Act::PanLeft => "pan_left",
            Act::PanRight => "pan_right",
            Act::PanUp => "pan_up",
            Act::PanDown => "pan_down",
            Act::TurnLeft => "turn_left",
            Act::TurnRight => "turn_right",
            Act::ZoomIn => "zoom_in",
            Act::ZoomOut => "zoom_out",
            Act::TiltUp => "tilt_up",
            Act::TiltDown => "tilt_down",
            Act::QuickSave => "quick_save",
            Act::QuickLoad => "quick_load",
            Act::Help => "help",
            Act::Log => "log",
        }
    }

    pub fn words(self) -> &'static str {
        match self {
            Act::Pause => "Pause / go on",
            Act::Economy => "Economy window",
            Act::WarRoom => "War room",
            Act::Diplomacy => "Diplomacy window",
            Act::Spies => "Spy window",
            Act::Overlay => "Map overlays",
            Act::Battle => "Jump to the biggest battle",
            Act::Card => "Country card",
            Act::Home => "Back home",
            Act::NextNation => "Next nation",
            Act::MapMode => "Terrain / nations view",
            Act::PanLeft => "Pan left",
            Act::PanRight => "Pan right",
            Act::PanUp => "Pan forward",
            Act::PanDown => "Pan back",
            Act::TurnLeft => "Turn left",
            Act::TurnRight => "Turn right",
            Act::ZoomIn => "Zoom in",
            Act::ZoomOut => "Zoom out",
            Act::TiltUp => "Tilt up",
            Act::TiltDown => "Tilt down",
            Act::QuickSave => "Quick save",
            Act::QuickLoad => "Quick load",
            Act::Help => "Help",
            Act::Log => "Notification log",
        }
    }

    pub fn default_key(self) -> Key {
        match self {
            Act::Pause => Key::Space,
            Act::Economy => Key::O,
            Act::WarRoom => Key::K,
            Act::Diplomacy => Key::P,
            Act::Spies => Key::I,
            Act::Overlay => Key::V,
            Act::Battle => Key::J,
            Act::Card => Key::C,
            Act::Home => Key::H,
            Act::NextNation => Key::Tab,
            Act::MapMode => Key::M,
            Act::PanLeft => Key::A,
            Act::PanRight => Key::D,
            Act::PanUp => Key::W,
            Act::PanDown => Key::S,
            Act::TurnLeft => Key::Q,
            Act::TurnRight => Key::E,
            Act::ZoomIn => Key::R,
            Act::ZoomOut => Key::F,
            Act::TiltUp => Key::T,
            Act::TiltDown => Key::G,
            Act::QuickSave => Key::F6,
            Act::QuickLoad => Key::F9,
            Act::Help => Key::F2,
            Act::Log => Key::L,
        }
    }
}

/// Keys the engine keeps for itself (debug overlay, FPS, recording, screenshots)
/// or the game uses everywhere (Esc, Enter, the speeds).
pub fn reserved(k: Key) -> bool {
    matches!(
        k,
        Key::F1 | Key::F3 | Key::F10 | Key::F12 | Key::Escape | Key::Enter | Key::Digit1 | Key::Digit2 | Key::Digit3 | Key::Digit4 | Key::Digit5 | Key::Digit6 | Key::Left | Key::Right | Key::Up | Key::Down
    )
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Which game wrote the file (2: phase 8, full screen by default). A
    /// file without it is older: its windowed start is from before.
    #[serde(default = "old_file")]
    pub version: u32,
    pub quality: Quality,
    /// The window's size when the game starts: 1 is 1280 x 720, 2 is 2560 x 1440.
    pub window_scale: usize,
    pub fullscreen: bool,
    /// 0 to 1.
    pub music: f32,
    pub effects: f32,
    /// The windows and menus' size: 0.9 to 1.15.
    pub ui_scale: f32,
    /// Saves itself every game year.
    pub autosave: bool,
    /// Which news pops up: 0 only what matters most, 1 important, 2 all.
    pub notifications: u8,
    /// Hover a number to read what it is.
    pub tooltips: bool,
    /// In fullscreen, the screen's edges push the map along.
    pub edge_scroll: bool,
    /// Action id -> key name; missing actions use their default.
    pub keys: BTreeMap<String, String>,
    /// The tutorial was offered (the first New Game suggests it once).
    pub tutorial_offered: bool,
}

/// The settings files' version now.
const VERSION: u32 = 2;

fn old_file() -> u32 {
    1
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            version: VERSION,
            quality: Quality::High,
            window_scale: 1,
            fullscreen: true,
            music: 0.7,
            effects: 0.9,
            ui_scale: 1.0,
            autosave: true,
            notifications: 1,
            tooltips: true,
            edge_scroll: true,
            keys: BTreeMap::new(),
            tutorial_offered: false,
        }
    }
}

impl Settings {
    /// The settings file, or the defaults (and why, if the file was bad).
    pub fn load() -> (Settings, Option<String>) {
        match std::fs::read_to_string(FILE) {
            Err(_) => (Settings::default(), None),
            Ok(text) => match toml::from_str::<Settings>(&text) {
                Ok(s) => (s.clean(), None),
                Err(e) => (Settings::default(), Some(format!("{FILE} couldn't be read ({}); using the defaults", e.message()))),
            },
        }
    }

    pub fn save(&self) -> Result<(), String> {
        // Tests never touch the player's own settings file.
        if cfg!(test) {
            return Ok(());
        }
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(FILE, format!("# LUCKY WORLD's settings (the Settings screen writes this file).\n{text}")).map_err(|e| e.to_string())
    }

    /// Numbers in range; unknown key names dropped.
    pub fn clean(mut self) -> Settings {
        // Older games started in a window, so their files say so: full
        // screen is the default now (Settings or Alt+Enter turn it off).
        if self.version < VERSION {
            self.fullscreen = true;
            self.edge_scroll = true;
            self.version = VERSION;
        }
        self.window_scale = self.window_scale.clamp(1, 2);
        self.music = self.music.clamp(0.0, 1.0);
        self.effects = self.effects.clamp(0.0, 1.0);
        self.ui_scale = self.ui_scale.clamp(0.9, 1.15);
        self.notifications = self.notifications.min(2);
        self.keys.retain(|act, key| Act::ALL.iter().any(|a| a.id() == act) && Key::from_name(key).is_some_and(|k| !reserved(k)));
        self
    }

    pub fn key(&self, a: Act) -> Key {
        self.keys.get(a.id()).and_then(|k| Key::from_name(k)).unwrap_or(a.default_key())
    }

    /// Binds `a` to `k`; an action that had `k` swaps to `a`'s old key.
    pub fn bind(&mut self, a: Act, k: Key) {
        let old = self.key(a);
        for other in Act::ALL {
            if other != a && self.key(other) == k {
                self.keys.insert(other.id().to_string(), old.name());
            }
        }
        self.keys.insert(a.id().to_string(), k.name());
    }

    pub fn reset_keys(&mut self) {
        self.keys.clear();
    }
}

/// Every action has a different key.
#[cfg(test)]
pub fn clashes(s: &Settings) -> Vec<(Act, Act)> {
    let mut out = Vec::new();
    for (i, a) in Act::ALL.iter().enumerate() {
        for b in &Act::ALL[i + 1..] {
            if s.key(*a) == s.key(*b) {
                out.push((*a, *b));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_keys_are_the_ones_the_game_always_had_and_never_clash() {
        let s = Settings::default();
        assert!(clashes(&s).is_empty(), "{:?}", clashes(&s));
        assert_eq!(s.key(Act::Economy), Key::O);
        assert_eq!(s.key(Act::WarRoom), Key::K);
        for a in Act::ALL {
            assert!(!reserved(s.key(a)), "{a:?} is on a reserved key");
        }
    }

    #[test]
    fn rebinding_swaps_instead_of_clashing_and_survives_a_round_trip() {
        let mut s = Settings::default();
        s.bind(Act::Economy, Key::K);
        assert_eq!(s.key(Act::Economy), Key::K);
        assert_eq!(s.key(Act::WarRoom), Key::O);
        assert!(clashes(&s).is_empty());
        s.music = 0.25;
        s.quality = Quality::Ultra;
        let text = toml::to_string_pretty(&s).unwrap();
        let back: Settings = toml::from_str(&text).unwrap();
        assert_eq!(back.clean(), s);
        // A hand-edited file with nonsense in it is cleaned, not refused.
        let bad: Settings = toml::from_str("music = 7.0\nui_scale = 0.1\n[keys]\neconomy = \"Banana\"\nwar_room = \"F1\"\n").unwrap();
        let bad = bad.clean();
        assert_eq!(bad.music, 1.0);
        assert_eq!(bad.ui_scale, 0.9);
        assert_eq!(bad.key(Act::Economy), Key::O);
        assert_eq!(bad.key(Act::WarRoom), Key::K);
    }
}
