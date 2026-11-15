//! LUCKY WORLD: spin 16 wheels for your country, meet your rivals.
//!
//! Run it from this folder (it reads `data/` and `assets/`):
//!   cargo run --release
//! On a computer with no screen:
//!   cargo run --release --no-default-features -- --headless --inputs playthroughs/lucky_world.json --sheet-events --out out/look

mod country;
mod data;
mod econ;
mod fate;
mod game;
mod hex;
mod map_view;
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
    let atlas = mapdata::Atlas::load(&data)?;
    let assets = ui::Assets::load()?;
    let models = map_view::Models::load()?;
    let econ_data = econ::EconData::load(&data)?;
    Ok(game::Lucky::new(data, atlas, assets, models, econ_data))
}

/// Everything a world is made of: its data, maps, economy rules, owners and economy.
pub type MadeWorld = (data::Data, mapdata::Atlas, econ::EconData, world::World, econ::Economy);

/// Builds a whole world (player, rivals, fill nations) on a map from a seed,
/// as the game does, and its economy.
pub fn make_world(map: usize, seed: u32) -> Result<MadeWorld, Box<dyn std::error::Error>> {
    let data = data::Data::load()?;
    let atlas = mapdata::Atlas::load(&data)?;
    let ed = econ::EconData::load(&data)?;
    let mut rng = Rng::new(seed);
    let player = country::roll_country(&data, map, Vec::new(), &[], true, &mut rng);
    let rivals = country::roll_rivals(&data, &player, &mut rng);
    let world = world::World::build(&data, &atlas, player, rivals, &mut rng);
    let economy = econ::Economy::new(&data, &ed, &atlas, &world, seed);
    Ok((data, atlas, ed, world, economy))
}

/// `--economy-report [YEARS] [--map ID] [--world-seed N]`: simulate and print.
/// (`--seed` belongs to the engine, so the world's seed is `--world-seed`.)
fn economy_report(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut years = 50u32;
    let mut maps: Vec<String> = vec!["grand".into(), "isles".into(), "five_shores".into()];
    let mut seed = 1u32;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--map" if i + 1 < args.len() => {
                maps = vec![args[i + 1].clone()];
                i += 1;
            }
            "--world-seed" | "--seed" if i + 1 < args.len() => {
                seed = args[i + 1].parse().unwrap_or(1);
                i += 1;
            }
            other => {
                if let Ok(y) = other.parse() {
                    years = y;
                }
            }
        }
        i += 1;
    }
    let data = data::Data::load()?;
    for id in maps {
        let Some(map) = data.maps.iter().position(|m| m.id == id) else {
            return Err(format!("there's no map \"{id}\" (try grand, isles or five_shores)").into());
        };
        let (_, _, ed, _, mut economy) = make_world(map, seed)?;
        let start = std::time::Instant::now();
        economy.run_days(&ed, years * ed.days_per_year);
        println!("== {} (seed {seed}) ==", data.maps[map].name);
        println!("{}", econ::report::report(&economy, &ed));
        println!("({} years of {} nations simulated in {:.2} s)\n", years, economy.nations.len(), start.elapsed().as_secs_f64());
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = duckforge::game_args();
    if let Some(at) = args.iter().position(|a| a == "--economy-trace") {
        // --economy-trace MAP NATION DAYS EVERY [--world-seed N] [--detail] [--world] [--good ID] [--budget]
        let get = |k: usize, d: &str| args.get(at + k).cloned().unwrap_or_else(|| d.to_string());
        let data = data::Data::load()?;
        let map = data.maps.iter().position(|m| m.id == get(1, "grand")).unwrap_or(0);
        let (nation, days, every): (usize, u32, u32) = (get(2, "0").parse()?, get(3, "60").parse()?, get(4, "1").parse()?);
        let seed: u32 = args.iter().position(|a| a == "--world-seed").and_then(|k| args.get(k + 1)).and_then(|v| v.parse().ok()).unwrap_or(1);
        let (_, _, ed, _, mut economy) = make_world(map, seed)?;
        println!("{}", economy.nations[nation].name);
        for _ in 0..days {
            if economy.day % every == 0 {
                println!("{}", econ::report::trace_line(&economy, &ed, nation));
            }
            economy.tick(&ed);
        }
        if let Some(at) = args.iter().position(|a| a == "--good") {
            if let Some(g) = args.get(at + 1).and_then(|id| ed.good_index(id)) {
                println!("{}", econ::report::good_across(&economy, &ed, g));
            }
        }
        if args.iter().any(|a| a == "--budget") {
            println!("{}", econ::report::budget(&economy, &ed, nation));
        }
        if args.iter().any(|a| a == "--world") {
            println!("{}", econ::report::world_flows(&economy, &ed));
        }
        if args.iter().any(|a| a == "--detail") {
            println!("{}", econ::report::detail(&economy, &ed, nation));
        }
        return Ok(());
    }
    if let Some(at) = args.iter().position(|a| a == "--economy-report") {
        return economy_report(&args[at + 1..]);
    }
    duckforge::run(config(), new_game()?)
}
