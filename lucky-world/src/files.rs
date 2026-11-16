//! Reading the game's own files so that one blocked file can't stop it.
//!
//! On Windows a file that was just unzipped can be held by antivirus for a
//! moment ("Access is denied"), or kept back for good. So each read is
//! tried a few times, and if it still fails the copy built into the game
//! (see `build.rs`) is used instead and a note is kept for
//! `lucky-world-notes.txt`.

use duckforge::prelude::*;
use duckforge::LoadError;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

include!(concat!(env!("OUT_DIR"), "/built_in.rs"));

static NOTES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The built-in copy of a file under `data/` or `assets/`, if there is one.
pub fn built_in(path: &Path) -> Option<&'static [u8]> {
    let key = path.to_string_lossy().replace('\\', "/");
    let key = key.trim_start_matches("./");
    BUILT_IN.binary_search_by(|(p, _)| (*p).cmp(key)).ok().map(|i| BUILT_IN[i].1)
}

/// What had to fall back to a built-in copy, and why.
pub fn notes() -> Vec<String> {
    NOTES.lock().map(|n| n.clone()).unwrap_or_default()
}

fn note(path: &Path, why: &dyn std::fmt::Display) {
    if let Ok(mut n) = NOTES.lock() {
        n.push(format!("{}: {why} - used the copy built into the game", path.display()));
    }
}

/// Runs `load` up to five times, waiting a little longer each time, while
/// the error isn't "not found" (a missing file won't appear by waiting).
fn patient<T, E>(mut load: impl FnMut() -> Result<T, E>, kind: impl Fn(&E) -> io::ErrorKind) -> Result<T, E> {
    let mut tries = 0;
    loop {
        match load() {
            Err(e) if tries < 4 && kind(&e) != io::ErrorKind::NotFound => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(100 * tries));
            }
            done => return done,
        }
    }
}

/// A text file (TOML, map grids).
pub fn read_text(path: impl AsRef<Path>) -> io::Result<String> {
    read_text_with(path.as_ref(), |p| std::fs::read_to_string(p))
}

fn read_text_with(path: &Path, read: impl Fn(&Path) -> io::Result<String>) -> io::Result<String> {
    match patient(|| read(path), io::Error::kind) {
        Ok(text) => Ok(text),
        Err(e) => match built_in(path) {
            Some(bytes) => {
                note(path, &e);
                Ok(String::from_utf8_lossy(bytes).into_owned())
            }
            None => Err(e),
        },
    }
}

/// A PNG picture.
pub fn sprite(path: impl AsRef<Path>) -> Result<Sprite, LoadError> {
    let path = path.as_ref();
    patient(|| Sprite::load_png(path), LoadError::kind).or_else(|e| match built_in(path).and_then(decode_png) {
        Some(s) => {
            note(path, &e);
            Ok(s)
        }
        None => Err(e),
    })
}

/// A font file.
pub fn font(path: impl AsRef<Path>) -> Result<Font, LoadError> {
    let path = path.as_ref();
    patient(|| Font::load(path), LoadError::kind).or_else(|e| match built_in(path) {
        Some(bytes) => {
            note(path, &e);
            Font::from_bytes(&path.to_string_lossy(), bytes.to_vec())
        }
        None => Err(e),
    })
}

/// Anything Duck Forge loads only from a file (sounds, models): if the file
/// beside the game can't be read, the built-in copy is written to a temp
/// folder and loaded from there.
pub fn load<T>(path: impl AsRef<Path>, from: impl Fn(&Path) -> Result<T, LoadError>) -> Result<T, LoadError> {
    let path = path.as_ref();
    patient(|| from(path), LoadError::kind).or_else(|e| {
        let Some(bytes) = built_in(path) else { return Err(e) };
        let spare: PathBuf = std::env::temp_dir().join(concat!("LuckyWorld-", env!("CARGO_PKG_VERSION"))).join(path);
        let written = spare.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::write(&spare, bytes));
        if written.is_err() {
            return Err(e);
        }
        note(path, &e);
        patient(|| from(&spare), LoadError::kind)
    })
}

/// Decodes a PNG's bytes the way `Sprite::load_png` does (Duck Forge has no
/// public "sprite from PNG bytes"; see ISSUES.md).
fn decode_png(bytes: &[u8]) -> Option<Sprite> {
    let mut decoder = png::Decoder::new(io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return None,
    };
    let mut pixels = Vec::with_capacity(w * h);
    for y in 0..h {
        for p in buf[y * info.line_size..][..w * channels].chunks_exact(channels) {
            pixels.push(match *p {
                [v] => Color::rgba(v, v, v, 255),
                [v, a] => Color::rgba(v, v, v, a),
                [r, g, b] => Color::rgba(r, g, b, 255),
                [r, g, b, a] => Color::rgba(r, g, b, a),
                _ => return None,
            });
        }
    }
    Some(Sprite::from_pixels(w, h, pixels))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_asset_and_data_file_is_built_in() {
        assert!(built_in(Path::new("data/wheels.toml")).is_some());
        assert!(built_in(Path::new("assets/art/icon_handshake.png")).is_some());
        assert!(built_in(Path::new(r"assets\art\icon_spy.png")).is_some(), "Windows paths too");
        assert!(built_in(Path::new("assets/art/no_such.png")).is_none());
    }

    #[test]
    fn a_built_in_picture_decodes_like_the_file() {
        let path = Path::new("assets/art/icon_handshake.png");
        let from_file = Sprite::load_png(path).unwrap();
        let built = decode_png(built_in(path).unwrap()).unwrap();
        assert_eq!((built.width(), built.height()), (from_file.width(), from_file.height()));
        for y in 0..built.height() {
            for x in 0..built.width() {
                assert_eq!(built.get_pixel(x, y), from_file.get_pixel(x, y));
            }
        }
    }

    #[test]
    fn a_blocked_file_is_retried_then_read_from_the_built_in_copy() {
        use std::cell::Cell;
        let tries = Cell::new(0);
        let denied = |_: &Path| {
            tries.set(tries.get() + 1);
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "Access is denied. (os error 5)"))
        };
        let text = read_text_with(Path::new("data/wheels.toml"), denied).unwrap();
        assert_eq!(tries.get(), 5, "tried five times before giving up on the file");
        assert_eq!(text, std::fs::read_to_string("data/wheels.toml").unwrap());
        assert!(notes().iter().any(|n| n.contains("wheels.toml") && n.contains("Access is denied")));
    }

    #[test]
    fn a_missing_file_is_not_waited_for() {
        use std::cell::Cell;
        let tries = Cell::new(0);
        let gone = |_: &Path| {
            tries.set(tries.get() + 1);
            Err(io::Error::new(io::ErrorKind::NotFound, "gone"))
        };
        assert!(read_text_with(Path::new("data/war/war.toml"), gone).is_ok());
        assert_eq!(tries.get(), 1);
        assert!(read_text_with(Path::new("elsewhere/war.toml"), |_| Err(io::Error::new(io::ErrorKind::NotFound, "gone"))).is_err(), "no built-in copy: still an error");
    }

    #[test]
    fn a_blocked_sound_or_model_loads_from_a_spare_copy() {
        // Pretend the file beside the game can't be opened: only the spare
        // copy written to the temp folder loads.
        let real = Path::new("assets/art/icon_spy.png");
        let sprite = load(real, |p| if p == real { Sprite::load_png("assets/art/not_there.png") } else { Sprite::load_png(p) }).unwrap();
        assert_eq!(sprite.width(), Sprite::load_png(real).unwrap().width());
    }
}
