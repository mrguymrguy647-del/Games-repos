//! LUCKY WORLD: spin 16 wheels for your country, meet your rivals.
//!
//! Run it from this folder (it reads `data/` and `assets/`):
//!   cargo run --release
//! On a computer with no screen:
//!   cargo run --release --no-default-features -- --headless --inputs playthroughs/lucky_world.json --sheet-events --out out/look

mod country;
mod data;
mod fate;
mod game;
mod hex;
mod mapdata;
mod ui;
mod wheel;
mod world;
#[cfg(test)]
mod tests;

use duckforge::prelude::*;

/// Wheel ids the code refers to by name (tests check wheels.toml has them).
pub const WHEELS_THE_CODE_NAMES: [&str; 11] = [
    "continent", "size", "coast", "population", "ideology", "gdp", "army", "duty", "leader", "fate", "impact",
];

pub fn config() -> Config {
    Config {
        title: String::from("LUCKY WORLD"),
        width: ui::W as usize,
        height: ui::H as usize,
        scale: 1,
        tune_file: Some("lucky_world.tune.json".into()),
        ..Config::default()
    }
}

pub fn new_game() -> Result<game::Lucky, Box<dyn std::error::Error>> {
    let data = data::Data::load()?;
    let assets = ui::Assets::load()?;
    Ok(game::Lucky::new(data, assets))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    duckforge::run(config(), new_game()?)
}
