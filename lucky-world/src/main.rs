//! LUCKY WORLD: spin 16 wheels for your country, meet your rivals.
//!
//! Run it from this folder (it reads `data/` and `assets/`):
//!   cargo run --release
//! On a computer with no screen:
//!   cargo run --release --no-default-features -- --headless --inputs playthroughs/lucky_world.json --sheet-events --out out/look

mod country;
mod data;
mod ai;
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
    let ai_data = ai::AiData::load()?;
    Ok(game::Lucky::new(data, atlas, assets, models, econ_data, ai_data))
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

/// `--economy-health [YEARS] [SEEDS...] [--why]`: every map with each seed, watched
/// monthly against the balance rules (see `econ::report::Health`).
fn economy_health(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    // --why: under each world that broke a rule, who did it.
    let why = args.iter().any(|a| a == "--why");
    let mut nums: Vec<u32> = args.iter().filter_map(|a| a.parse().ok()).collect();
    let years = if nums.is_empty() { 50 } else { nums.remove(0) };
    let seeds = if nums.is_empty() { vec![1, 2, 3, 4, 5, 6] } else { nums };
    let data = data::Data::load()?;
    let mut failures = 0;
    println!("{:<14} {:>5} {:>12} {:>9} {:>17} {:>9} {:>9}  problems", "map", "seed", "max/head", "jobless", "world infl.", ">15% max", "collapse");
    for seed in &seeds {
        for (map, m) in data.maps.iter().enumerate() {
            let (_, _, ed, _, mut e) = make_world(map, *seed)?;
            let h = econ::report::run_and_watch(&mut e, &ed, years);
            let p = h.problems();
            failures += usize::from(!p.is_empty());
            println!(
                "{:<14} {:>5} {:>12.0} {:>8.1}y {:>7.1}% .. {:>5.1}% {:>9} {:>9}  {}",
                m.id, seed, h.max_per_head, h.longest_jobless_years, h.world_inflation_low * 100.0, h.world_inflation_high * 100.0,
                h.most_high_inflation, h.collapsed.len(), p.join("; ")
            );
            if why && !p.is_empty() {
                println!("    most over 15% at once, {}", h.high_inflation_who);
                println!("    world inflation furthest out, {}", h.world_inflation_who);
            }
        }
    }
    println!("{} of {} worlds broke a rule", failures, seeds.len() * data.maps.len());
    Ok(())
}

/// `--economy-report [YEARS] [--map ID] [--world-seed N]`: simulate and print.
/// (`--seed` belongs to the engine, so the world's seed is `--world-seed`.)
fn economy_report(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut years = 50.0f64;
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
        economy.run_days(&ed, (years * ed.days_per_year as f64).round() as u32);
        println!("== {} (seed {seed}) ==", data.maps[map].name);
        println!("{}", econ::report::report(&economy, &ed));
        println!("({} years of {} nations simulated in {:.2} s)\n", years, economy.nations.len(), start.elapsed().as_secs_f64());
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = duckforge::game_args();
    if let Some(at) = args.iter().position(|a| a == "--economy-trace") {
        // --economy-trace MAP NATION DAYS EVERY [--world-seed N] [--cpi] [--detail] [--world] [--good ID] [--budget]
        let get = |k: usize, d: &str| args.get(at + k).cloned().unwrap_or_else(|| d.to_string());
        let data = data::Data::load()?;
        let map = data.maps.iter().position(|m| m.id == get(1, "grand")).unwrap_or(0);
        let (nation, days, every): (usize, u32, u32) = (get(2, "0").parse()?, get(3, "60").parse()?, get(4, "1").parse()?);
        let seed: u32 = args.iter().position(|a| a == "--world-seed").and_then(|k| args.get(k + 1)).and_then(|v| v.parse().ok()).unwrap_or(1);
        let (_, _, ed, _, mut economy) = make_world(map, seed)?;
        println!("{}", economy.nations[nation].name);
        for _ in 0..days {
            if economy.day % every == 0 {
                if args.iter().any(|a| a == "--cpi") {
                    println!("{}", econ::report::cpi_line(&economy, &ed, nation));
                } else {
                    println!("{}", econ::report::trace_line(&economy, &ed, nation));
                }
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
    if let Some(at) = args.iter().position(|a| a == "--economy-world") {
        // --economy-world MAP SEED MONTHS: the world month by month (for balancing).
        let get = |k: usize, d: &str| args.get(at + k).cloned().unwrap_or_else(|| d.to_string());
        let data = data::Data::load()?;
        let map = data.maps.iter().position(|m| m.id == get(1, "grand")).unwrap_or(0);
        let (_, _, ed, _, mut e) = make_world(map, get(2, "1").parse()?)?;
        let months: u32 = get(3, "120").parse()?;
        for _ in 0..months {
            e.run_days(&ed, 30);
            println!("{}", econ::report::world_line(&e, &ed));
        }
        return Ok(());
    }
    if let Some(at) = args.iter().position(|a| a == "--economy-health") {
        return economy_health(&args[at + 1..]);
    }
    if let Some(at) = args.iter().position(|a| a == "--ai-compare") {
        return ai::report::compare(&args[at + 1..]).map_err(|e| e.to_string().into());
    }
    if let Some(at) = args.iter().position(|a| a == "--ai-tune") {
        return ai::report::tune(&args[at + 1..]).map_err(|e| e.to_string().into());
    }
    if let Some(at) = args.iter().position(|a| a == "--ai-trace") {
        return ai::report::trace(&args[at + 1..]).map_err(|e| e.to_string().into());
    }
    if let Some(at) = args.iter().position(|a| a == "--economy-report") {
        return economy_report(&args[at + 1..]);
    }
    // --spectate [MAP]: straight into a world of NPC brains (the engine's
    // --seed picks the world).
    let mut game = new_game()?;
    if let Some(at) = args.iter().position(|a| a == "--spectate") {
        if let Some(id) = args.get(at + 1).filter(|a| !a.starts_with("--")) {
            game.map = game.data.maps.iter().position(|m| &m.id == id).ok_or(format!("there's no map \"{id}\" (try grand, isles or five_shores)"))?;
        }
        game.spectate_at_start = true;
    }
    duckforge::run(config(), game)
}
