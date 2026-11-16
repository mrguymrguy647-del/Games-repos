//! Builds every file in `data/` and `assets/` into the game, so it still
//! starts if Windows (or antivirus) won't let it open a file beside it.
//! The game reads the files on disk first; these copies are the fallback.

use std::{env, fs, path::Path};

fn walk(dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn main() {
    let root = env::var("CARGO_MANIFEST_DIR").unwrap();
    env::set_current_dir(&root).unwrap();
    let mut files = Vec::new();
    for dir in ["data", "assets"] {
        println!("cargo:rerun-if-changed={dir}");
        walk(Path::new(dir), &mut files);
    }
    files.sort();
    let mut code = String::from("/// Every file in data/ and assets/, by path, sorted.\npub static BUILT_IN: &[(&str, &[u8])] = &[\n");
    for f in &files {
        let full = Path::new(&root).join(f);
        code.push_str(&format!("    ({f:?}, include_bytes!({:?})),\n", full.to_string_lossy()));
    }
    code.push_str("];\n");
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("built_in.rs"), code).unwrap();
}
