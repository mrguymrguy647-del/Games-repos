//! Saved games: everything (the wheels, the map's owners, the economy, every
//! brain and its memory, the wars, treaties and spies, and every random
//! number generator's exact state), so a loaded game goes on exactly as it
//! would have.
//!
//! A save file is:
//!
//! | Bytes | |
//! |---|---|
//! | 12 | `LUCKYWORLD` and a newline, then `\0` |
//! | 4 | the save format's version (little-endian) |
//! | 4 + n | the summary the Load screen shows (bincode) |
//! | rest | the game, bincode, gzip-compressed |
//!
//! A save from another version is refused with a sentence saying why,
//! never a crash: the game reads the version before anything else, and
//! anything that fails to decode is an error, not a panic.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::country::Country;
use crate::fate::Landed;
use crate::sim::Sim;
use crate::world::World;

/// The save format. Raise it whenever a saved type changes shape; list the
/// versions this build can still read in `READS`.
pub const VERSION: u32 = 1;
/// Versions this build can load (older ones are refused politely).
pub const READS: &[u32] = &[1];
const MAGIC: &[u8; 12] = b"LUCKYWORLD\n\0";
/// The most a save may unpack to (a 100-year game is a few MB).
const LIMIT: u64 = 1 << 30;

/// The few words the Load screen shows for each save.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct Summary {
    /// "Autosave", "Quick save" or the player's slot name.
    pub label: String,
    pub nation: String,
    pub map: String,
    /// "12 Mar, Year 3".
    pub date: String,
    pub day: u32,
    pub difficulty: String,
    /// Seconds since 1970 when it was saved.
    pub saved_at: u64,
}

/// How a game is set up on the New Game screen.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct Setup {
    /// "easy", "normal", "hard" or "brutal".
    pub difficulty: String,
    /// Years until the game ends on score (0: no limit).
    pub years: u32,
    /// The short teaching game.
    pub tutorial: bool,
}

impl Default for Setup {
    fn default() -> Setup {
        Setup { difficulty: "normal".to_string(), years: 100, tutorial: false }
    }
}

/// Everything a game is, once the nations are on the map.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Saved {
    pub map: usize,
    pub landed: Vec<Landed>,
    pub player: Option<Country>,
    pub rivals: Vec<Country>,
    /// The placed world and its simulation; `None` in a save made among
    /// the wheels (before the nations are on the map).
    pub world: Option<World>,
    pub sim: Option<Sim>,
    pub spectating: bool,
    pub setup: Setup,
    /// How the game is going: victory, defeat, the story so far.
    pub fate: crate::victory::Fate,
    /// The clock's speed (0-5) and whether it was paused.
    pub speed: usize,
    pub paused: bool,
    /// The tutorial's step, if one is running.
    pub tutorial_step: Option<usize>,
}

/// Writes a game to bytes.
pub fn to_bytes(summary: &Summary, game: &impl serde::Serialize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(1 << 20);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    let head = bincode::serialize(summary).map_err(|e| e.to_string())?;
    out.extend_from_slice(&(head.len() as u32).to_le_bytes());
    out.extend_from_slice(&head);
    let body = bincode::serialize(game).map_err(|e| e.to_string())?;
    let mut z = flate2::write::GzEncoder::new(out, flate2::Compression::fast());
    z.write_all(&body).map_err(|e| e.to_string())?;
    z.finish().map_err(|e| e.to_string())
}

/// The slow half of a save (compressing and writing), done on another
/// thread so the yearly autosave doesn't stop the game: the game is turned
/// into bytes first (`bincode`), on the game's thread, so what's saved is
/// exactly that moment.
pub fn write_in_background(path: PathBuf, summary: &Summary, game: &impl serde::Serialize) -> Result<(), String> {
    let mut head = Vec::with_capacity(64);
    head.extend_from_slice(MAGIC);
    head.extend_from_slice(&VERSION.to_le_bytes());
    let s = bincode::serialize(summary).map_err(|e| e.to_string())?;
    head.extend_from_slice(&(s.len() as u32).to_le_bytes());
    head.extend_from_slice(&s);
    let body = bincode::serialize(game).map_err(|e| e.to_string())?;
    std::thread::spawn(move || {
        let mut z = flate2::write::GzEncoder::new(head, flate2::Compression::fast());
        let done = z.write_all(&body).map_err(|e| e.to_string()).and_then(|_| z.finish().map_err(|e| e.to_string())).and_then(|bytes| write(&path, &bytes));
        if let Err(e) = done {
            if let Ok(mut p) = PROBLEM.lock() {
                *p = Some(e);
            }
        }
    });
    Ok(())
}

static PROBLEM: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// A background save that went wrong since the last look.
pub fn background_problem() -> Option<String> {
    PROBLEM.lock().ok().and_then(|mut p| p.take())
}

/// The version and summary at the front of a save, without the game.
pub fn read_summary(bytes: &[u8]) -> Result<(u32, Summary), String> {
    if bytes.len() < 20 || &bytes[..12] != MAGIC {
        return Err("this isn't a LUCKY WORLD save".to_string());
    }
    let version = u32::from_le_bytes(bytes[12..16].try_into().expect("4 bytes"));
    let len = u32::from_le_bytes(bytes[16..20].try_into().expect("4 bytes")) as usize;
    if bytes.len() < 20 + len {
        return Err("the save is cut short (was the disk full?)".to_string());
    }
    if !READS.contains(&version) {
        return Ok((version, Summary { label: String::new(), nation: String::new(), map: String::new(), date: String::new(), day: 0, difficulty: String::new(), saved_at: 0 }));
    }
    let s: Summary = bincode::deserialize(&bytes[20..20 + len]).map_err(|e| format!("the save's summary is damaged ({e})"))?;
    Ok((version, s))
}

/// Reads a game back. Another version's save is refused with a reason.
pub fn from_bytes(bytes: &[u8]) -> Result<(Summary, Saved), String> {
    let (version, summary) = read_summary(bytes)?;
    if version > VERSION {
        return Err(format!("this save was made by a newer LUCKY WORLD (save format {version}; this game reads {VERSION}). Update the game to load it"));
    }
    if !READS.contains(&version) {
        return Err(format!("this save is from an older LUCKY WORLD (save format {version}) that this version can't read any more"));
    }
    let len = u32::from_le_bytes(bytes[16..20].try_into().expect("4 bytes")) as usize;
    let mut body = Vec::new();
    flate2::read::GzDecoder::new(&bytes[20 + len..]).take(LIMIT).read_to_end(&mut body).map_err(|e| format!("the save is damaged ({e})"))?;
    use bincode::Options;
    let game: Saved = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .allow_trailing_bytes()
        .with_limit(LIMIT)
        .deserialize(&body)
        .map_err(|e| format!("the save is damaged or from a different build ({e})"))?;
    Ok((summary, game))
}

/// Where saves live: a `saves` folder beside the game (`--saves DIR` or a
/// test can point elsewhere).
pub fn folder() -> PathBuf {
    FOLDER.lock().ok().and_then(|f| f.clone()).unwrap_or_else(|| PathBuf::from("saves"))
}

static FOLDER: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

pub fn set_folder(dir: impl Into<PathBuf>) {
    if let Ok(mut f) = FOLDER.lock() {
        *f = Some(dir.into());
    }
}

/// A safe file name for a slot ("Autosave 3", "My empire").
pub fn path_for(slot: &str) -> PathBuf {
    let clean: String = slot.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c.to_ascii_lowercase() } else { '_' }).collect();
    folder().join(format!("{clean}.lws"))
}

/// Writes to a temporary file first, so a crash mid-save never ruins the old one.
pub fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("couldn't make the saves folder ({e})"))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("couldn't write the save ({e})"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("couldn't finish the save ({e})"))
}

/// One save on disk, for the Load screen.
#[derive(Debug, Clone)]
pub struct Slot {
    pub path: PathBuf,
    /// `Err` with the reason when it can't be loaded by this build.
    pub summary: Result<Summary, String>,
}

/// Every save in the folder, newest first.
pub fn list() -> Vec<Slot> {
    list_in(&folder())
}

pub fn list_in(folder: &Path) -> Vec<Slot> {
    let Ok(dir) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut slots: Vec<(u64, Slot)> = dir
        .flatten()
        .map(|f| f.path())
        .filter(|p| p.extension().is_some_and(|x| x == "lws"))
        .map(|path| {
            let summary = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| {
                let (v, s) = read_summary(&b)?;
                if v != VERSION && !READS.contains(&v) {
                    Err(if v > VERSION { format!("made by a newer version (save format {v})") } else { format!("too old for this version (save format {v})") })
                } else {
                    Ok(s)
                }
            });
            let when = summary.as_ref().map_or(0, |s| s.saved_at);
            (when, Slot { path, summary })
        })
        .collect();
    slots.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.path.cmp(&b.1.path)));
    slots.into_iter().map(|(_, s)| s).collect()
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// "3 min ago", "2 h ago", "4 days ago".
pub fn ago(saved_at: u64) -> String {
    let s = now().saturating_sub(saved_at);
    match s {
        0..60 => "just now".to_string(),
        60..3600 => format!("{} min ago", s / 60),
        3600..86_400 => format!("{} h ago", s / 3600),
        _ => format!("{} days ago", s / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{Bundle, Sim};

    fn summary() -> Summary {
        Summary { label: "test".into(), nation: "Testland".into(), map: "grand".into(), date: "1 Jan, Year 1".into(), day: 0, difficulty: "normal".into(), saved_at: 0 }
    }

    fn saved(world: &crate::world::World, sim: &Sim) -> Saved {
        Saved {
            map: world.map,
            landed: world.nations[0].landed.clone(),
            player: Some(world.nations[0].clone()),
            rivals: world.nations[1..].to_vec(),
            world: Some(world.clone()),
            sim: Some(sim.clone()),
            spectating: false,
            setup: Setup::default(),
            fate: Default::default(),
            speed: 4,
            paused: false,
            tutorial_step: None,
        }
    }

    /// Where two runs differ ("" if they don't): the part, and the first
    /// differing byte of it.
    fn differ(a: &Sim, b: &Sim) -> String {
        let parts: [(&str, Vec<u8>, Vec<u8>); 4] = [
            ("economy", bincode::serialize(&a.economy).unwrap(), bincode::serialize(&b.economy).unwrap()),
            ("brains", bincode::serialize(&a.brains).unwrap(), bincode::serialize(&b.brains).unwrap()),
            ("wars", bincode::serialize(&a.wars).unwrap(), bincode::serialize(&b.wars).unwrap()),
            ("diplomacy", bincode::serialize(&a.diplo).unwrap(), bincode::serialize(&b.diplo).unwrap()),
        ];
        for (name, x, y) in parts {
            if x != y {
                let at = x.iter().zip(&y).position(|(p, q)| p != q).unwrap_or(x.len().min(y.len()));
                return format!("{name} differs at byte {at} of {} ({} long the other way)", x.len(), y.len());
            }
        }
        String::new()
    }

    #[test]
    fn the_rng_state_round_trips() {
        let mut r = duckforge::prelude::Rng::new(77);
        for _ in 0..1000 {
            r.next_u32();
            let s = crate::save_codec::rng::state(&r);
            let mut copy = crate::save_codec::rng::from_state(s);
            let mut orig = r.clone();
            assert_eq!(copy.next_u32(), orig.next_u32());
        }
    }

    /// Save -> load -> continue gives exactly the same future as never
    /// saving, on all three maps, through wars, treaties and spies.
    #[test]
    fn save_load_continue_is_the_same_future_as_never_saving() {
        let b = Bundle::load().unwrap();
        for map in 0..b.data.maps.len() {
            let seed = 11 + map as u32;
            // One world that never saves...
            let (_, mut never) = b.game(map, seed, "normal");
            // ...and the same world, saved and loaded part way.
            let (world, mut sim) = b.game(map, seed, "normal");
            for _ in 0..400 {
                never.day(&b.econ);
                sim.day(&b.econ);
            }
            let bytes = to_bytes(&summary(), &saved(&world, &sim)).unwrap();
            drop(sim);
            let (s, loaded) = from_bytes(&bytes).unwrap();
            assert_eq!(s, summary());
            let mut sim = loaded.sim.expect("saved on the map");
            assert_eq!(differ(&sim, &never), "", "{}: loading changed the state", b.data.maps[map].id);
            for day in 0..500 {
                never.day(&b.econ);
                sim.day(&b.econ);
                if day % 50 == 49 {
                    assert_eq!(differ(&sim, &never), "", "{}: the loaded game went its own way by day {}", b.data.maps[map].id, never.economy.day);
                }
            }
            // It had a past worth saving: news, treaties and spies' work.
            assert!(!never.economy.news.is_empty());
            assert!(!never.diplo.treaties.is_empty(), "no treaties in 900 days");
        }
    }

    #[test]
    fn another_versions_save_is_refused_with_a_reason_not_a_crash() {
        let b = Bundle::load().unwrap();
        let (world, sim) = b.game(0, 3, "normal");
        let mut bytes = to_bytes(&summary(), &saved(&world, &sim)).unwrap();
        // A newer version.
        bytes[12..16].copy_from_slice(&(VERSION + 1).to_le_bytes());
        let e = from_bytes(&bytes).err().unwrap();
        assert!(e.contains("newer"), "{e}");
        // An old one this build can't read.
        bytes[12..16].copy_from_slice(&0u32.to_le_bytes());
        let e = from_bytes(&bytes).err().unwrap();
        assert!(e.contains("older"), "{e}");
        // Damaged: cut short, or garbage.
        bytes[12..16].copy_from_slice(&VERSION.to_le_bytes());
        let e = from_bytes(&bytes[..bytes.len() / 2]).err().unwrap();
        assert!(e.contains("damaged"), "{e}");
        assert!(from_bytes(b"not a save at all, just words").is_err());
        let mut garbage = bytes.clone();
        let len = garbage.len();
        for x in &mut garbage[40..len - 8] {
            *x = x.wrapping_mul(31).wrapping_add(7);
        }
        assert!(from_bytes(&garbage).is_err());
        // And the real one loads.
        assert!(from_bytes(&bytes).is_ok());
    }
}
