//! Phase 7's balance and endurance tools:
//!
//! - `--balance WORLDS YEARS`: thousands of starts. Every nation in
//!   `WORLDS` worlds per map, all run by brains for `YEARS`, twice (with and
//!   without the underdog rule); how each start's luck turned out.
//! - `--wheel-effects`: every option of every wheel, landed on one nation
//!   with everything else the same; what it changes on day one in the
//!   economy, the war and diplomacy (and the options that change nothing).
//! - `--long-game YEARS [SEED]`: a full game on each map, year by year: speed,
//!   memory, the save's size and how big the history has grown.

use std::collections::BTreeMap;

use crate::sim::{Bundle, Sim};
use crate::victory;

/// Each nation's start, as the underdog rule measures it (share of the
/// median nation's GDP and soldiers).
fn starts(sim: &Sim) -> Vec<f64> {
    crate::econ::underdog::Underdog::start(&sim.economy, Default::default()).start
}

/// What became of one nation.
#[derive(Debug, Clone)]
struct Fate {
    start: f64,
    underdog: bool,
    alive: bool,
    vassal: bool,
    rank: usize,
    rank_start: usize,
    /// The first win it would have had, if any (conquest, economic, diplomatic).
    won: Option<&'static str>,
}

fn ranks(scores: &[f64]) -> Vec<usize> {
    (0..scores.len()).map(|i| 1 + (0..scores.len()).filter(|&j| scores[j] > scores[i]).count()).collect()
}

/// One world, all brains, `years` long: what became of every nation.
fn run_world(b: &Bundle, map: usize, seed: u32, years: u32, underdog: bool) -> Vec<Fate> {
    let world = b.world(map, seed);
    let mut sim = Sim::new(&b.rules(), &world, seed, true, "normal");
    let start = starts(&sim);
    if !underdog {
        sim.economy.underdog.on = false;
    }
    let v = &b.victory.victory;
    let n = sim.economy.nations.len();
    let is_underdog: Vec<bool> = (0..n).map(|i| sim.economy.underdog.strength[i] > 0.0).collect();
    let rank_start = ranks(&victory::scores(v, &sim.economy, &b.econ, &sim.wars));
    let mut lead = vec![0u32; n];
    let mut bloc = vec![0u32; n];
    let mut won: Vec<Option<&'static str>> = vec![None; n];
    let dpy = b.econ.days_per_year;
    for day in 0..years * dpy {
        sim.day(&b.econ);
        let e = &sim.economy;
        let best = (0..n).max_by(|&x, &y| e.nations[x].gdp.total_cmp(&e.nations[y].gdp)).unwrap_or(0);
        let next = (0..n).filter(|&j| j != best).map(|j| e.nations[j].gdp).fold(0.0, f64::max);
        for (i, l) in lead.iter_mut().enumerate() {
            *l = if i == best && e.nations[i].gdp >= v.economic_lead * next { *l + 1 } else { 0 };
        }
        if day % 30 == 29 {
            for i in 0..n {
                if won[i].is_some() || e.nations[i].overlord.is_some() || e.nations[i].population < 1.0 {
                    continue;
                }
                let (mine, held, allies, leads) = victory::shares(e, &sim.wars, &sim.diplo, i);
                bloc[i] = if held >= v.diplomatic_share && allies >= v.diplomatic_allies && leads { bloc[i] + 30 } else { 0 };
                won[i] = if mine >= v.conquest_share {
                    Some("conquest")
                } else if lead[i] as f64 >= v.economic_years * dpy as f64 {
                    Some("economic")
                } else if bloc[i] as f64 >= v.diplomatic_years * dpy as f64 {
                    Some("diplomatic")
                } else {
                    None
                };
            }
        }
    }
    let r = ranks(&victory::scores(v, &sim.economy, &b.econ, &sim.wars));
    (0..n)
        .map(|i| Fate {
            start: start[i],
            underdog: is_underdog[i],
            alive: sim.economy.nations[i].population >= 1.0 && sim.wars.owner.iter().any(|o| *o == Some(i)),
            vassal: sim.economy.nations[i].overlord.is_some(),
            rank: r[i],
            rank_start: rank_start[i],
            won: won[i],
        })
        .collect()
}

/// `--balance WORLDS YEARS`
pub fn balance(args: &[String]) -> Result<(), String> {
    let nums: Vec<u32> = args.iter().filter_map(|a| a.parse().ok()).collect();
    let worlds = nums.first().copied().unwrap_or(20);
    let years = nums.get(1).copied().unwrap_or(20);
    let b = Bundle::load().map_err(|e| e.to_string())?;
    let maps = b.data.maps.len();
    let jobs: Vec<(usize, u32, bool)> = (0..maps).flat_map(|m| (1..=worlds).flat_map(move |s| [(m, s, true), (m, s, false)])).collect();
    let started = std::time::Instant::now();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let out = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|sc| {
        for _ in 0..threads {
            sc.spawn(|| {
                let b = Bundle::load().expect("data loads");
                loop {
                    let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(&(m, s, u)) = jobs.get(k) else { break };
                    let fates = run_world(&b, m, s, years, u);
                    out.lock().unwrap().push((m, s, u, fates));
                }
            });
        }
    });
    let mut out = out.into_inner().unwrap();
    out.sort_by_key(|x| (x.0, x.1, !x.2));
    let all = |u: bool| -> Vec<Fate> { out.iter().filter(|x| x.2 == u).flat_map(|x| x.3.clone()).collect() };
    let (with, without) = (all(true), all(false));
    println!("LUCKY WORLD balance: {} worlds x {} maps x {} years, every nation run by a brain ({} starts each way, {:.0} s)", worlds, maps, years, with.len(), started.elapsed().as_secs_f64());
    println!();
    println!("Starts by luck (share of the median nation's GDP and soldiers on day 1), underdog rule ON:");
    table(&with);
    println!();
    println!("The same worlds with the underdog rule OFF:");
    table(&without);
    println!();
    let terrible = |f: &&Fate| f.start < 0.25;
    let (tw, tn): (Vec<&Fate>, Vec<&Fate>) = (with.iter().filter(terrible).collect(), without.iter().filter(terrible).collect());
    let alive = |v: &[&Fate]| pct(v.iter().filter(|f| f.alive && !f.vassal).count(), v.len());
    let up = |v: &[&Fate]| pct(v.iter().filter(|f| f.rank < f.rank_start).count(), v.len());
    println!("Terrible spins (under a quarter of the median): {} starts", tw.len());
    println!("  free and alive after {years} years: {} with the underdog rule, {} without", alive(&tw), alive(&tn));
    println!("  climbed the ranks:                 {} with, {} without", up(&tw), up(&tn));
    let perfect: Vec<&Fate> = with.iter().filter(|f| f.rank_start == 1).collect();
    let still = perfect.iter().filter(|f| f.rank == 1).count();
    let wins = perfect.iter().filter(|f| f.won.is_some()).count();
    println!("Perfect spins (the strongest nation of its world on day 1): {} starts", perfect.len());
    println!("  still first after {years} years: {}; won (conquest, economic or diplomatic) within them: {}", pct(still, perfect.len()), pct(wins, perfect.len()));
    let mut by: BTreeMap<&str, usize> = BTreeMap::new();
    for f in with.iter().filter(|f| f.won.is_some()) {
        *by.entry(f.won.unwrap()).or_default() += 1;
    }
    println!("  every win in those worlds (any nation): {by:?}");
    Ok(())
}

fn pct(a: usize, b: usize) -> String {
    if b == 0 { "-".to_string() } else { format!("{:.0}%", 100.0 * a as f64 / b as f64) }
}

fn table(fs: &[Fate]) {
    let bands = [(0.0, 0.25, "terrible (<0.25)"), (0.25, 0.5, "poor (0.25-0.5)"), (0.5, 1.0, "below median"), (1.0, 2.0, "above median"), (2.0, 5.0, "strong (2-5x)"), (5.0, f64::MAX, "giant (5x+)")];
    println!("  {:<18} {:>6} {:>9} {:>8} {:>8} {:>10} {:>10} {:>6}", "start", "starts", "underdog", "alive", "vassal", "rank then", "rank now", "won");
    for (lo, hi, name) in bands {
        let v: Vec<&Fate> = fs.iter().filter(|f| f.start >= lo && f.start < hi).collect();
        if v.is_empty() {
            continue;
        }
        let mean = |g: &dyn Fn(&Fate) -> f64| v.iter().map(|f| g(f)).sum::<f64>() / v.len() as f64;
        println!(
            "  {:<18} {:>6} {:>9} {:>8} {:>8} {:>10.1} {:>10.1} {:>6}",
            name,
            v.len(),
            pct(v.iter().filter(|f| f.underdog).count(), v.len()),
            pct(v.iter().filter(|f| f.alive).count(), v.len()),
            pct(v.iter().filter(|f| f.vassal).count(), v.len()),
            mean(&|f| f.rank_start as f64),
            mean(&|f| f.rank as f64),
            pct(v.iter().filter(|f| f.won.is_some()).count(), v.len()),
        );
    }
}

// ---------------------------------------------------------------------------
// Every wheel result's effect
// ---------------------------------------------------------------------------

/// A serde value with its words taken out (labels and names are not effects).
fn numbers(v: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::String(_) => Value::Null,
        Value::Array(a) => Value::Array(a.into_iter().map(numbers).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, numbers(v))).collect()),
        other => other,
    }
}

fn print<T: serde::Serialize>(x: &T) -> String {
    numbers(serde_json::to_value(x).expect("serializable")).to_string()
}

/// The fields (by path) where two prints differ, a few of them.
fn differing(a: &str, b: &str, out: &mut std::collections::BTreeSet<String>) {
    let (Ok(x), Ok(y)) = (serde_json::from_str::<serde_json::Value>(a), serde_json::from_str::<serde_json::Value>(b)) else { return };
    fn walk(x: &serde_json::Value, y: &serde_json::Value, path: &str, out: &mut std::collections::BTreeSet<String>) {
        use serde_json::Value;
        match (x, y) {
            (Value::Object(p), Value::Object(q)) => {
                for (k, v) in p {
                    if let Some(w) = q.get(k) {
                        walk(v, w, if path.is_empty() { k } else { path }, out);
                    }
                }
            }
            (Value::Array(p), Value::Array(q)) if p.len() == q.len() => {
                for (v, w) in p.iter().zip(q) {
                    walk(v, w, path, out);
                }
            }
            _ if x != y => {
                out.insert(if path.is_empty() { "(value)".to_string() } else { path.to_string() });
            }
            _ => {}
        }
    }
    walk(&x, &y, "", out);
}

/// What one nation (0) is on day one, part by part.
fn fingerprint(sim: &Sim) -> [(&'static str, String); 4] {
    let e = &sim.economy.nations[0];
    let w = &sim.wars;
    let divisions: Vec<_> = w.divisions.iter().filter(|d| d.faction == 0).collect();
    let d = &sim.diplo;
    let brain = sim.brains.brain(0).map(|b| (&b.personality.t, b.intel_error));
    let column: Vec<f64> = (0..d.n).map(|j| d.relations[j][0]).collect();
    [
        ("economy", print(e)),
        ("war", print(&(&w.forces[0], &divisions, w.intel_error[0]))),
        ("diplomacy", print(&(&d.traits[0], d.reputation[0], &d.relations[0], &column, d.intel_error[0], &d.spies.services.get(0)))),
        ("brain", print(&brain)),
    ]
}

/// `--wheel-effects [SEEDS]`
pub fn wheel_effects(args: &[String]) -> Result<(), String> {
    let seeds: Vec<u32> = args.iter().filter_map(|a| a.parse().ok()).collect();
    let seeds = if seeds.is_empty() { vec![1, 2, 3] } else { seeds };
    let b = Bundle::load().map_err(|e| e.to_string())?;
    let data = &b.data;
    let mut silent_total = 0;
    println!("Every option of every wheel, landed on nation 0 with all its other results the same (seeds {seeds:?}, all maps):");
    println!("each option is compared with the wheel's other options on day one; a part counts if the option changes it.\n");
    for w in 0..data.wheels.len() {
        let def = &data.wheels[w];
        // option id -> parts it changed (on some map and seed), and whether it was ever landed.
        let mut parts: BTreeMap<String, (bool, std::collections::BTreeSet<&'static str>)> = BTreeMap::new();
        let mut fields: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
        let mut labels: BTreeMap<String, String> = BTreeMap::new();
        for map in 0..data.maps.len() {
            for &seed in &seeds {
                let mut rng = duckforge::prelude::Rng::new(seed);
                let base = crate::country::roll_country(data, map, Vec::new(), &[], true, &mut rng);
                let rivals = crate::country::roll_rivals(data, &base, &mut rng);
                let odds = crate::fate::odds(data, map, w, &base.landed[..w], &[]);
                let mut prints: Vec<(String, [(&'static str, String); 4])> = Vec::new();
                for o in 0..odds.options.len() {
                    if odds.chance(o) <= 0.0 {
                        continue;
                    }
                    let mut landed = base.landed.clone();
                    let mut vr = duckforge::prelude::Rng::new(seed ^ 0x77);
                    landed[w] = crate::fate::land(data, &odds, o, &mut vr);
                    let mut cr = duckforge::prelude::Rng::new(seed ^ 0x99);
                    let me = crate::country::Country::finish(data, map, landed, true, &[], &mut cr);
                    let mut wr = duckforge::prelude::Rng::new(seed ^ 0x55);
                    let world = crate::world::World::build(data, &b.atlas, me, rivals.clone(), &mut wr);
                    let sim = Sim::new(&b.rules(), &world, seed, true, "normal");
                    labels.insert(odds.options[o].id.clone(), odds.options[o].label.clone());
                    prints.push((odds.options[o].id.clone(), fingerprint(&sim)));
                }
                for (id, fp) in &prints {
                    let entry = parts.entry(id.clone()).or_default();
                    entry.0 = true;
                    for (k, (name, text)) in fp.iter().enumerate() {
                        if let Some((_, f)) = prints.iter().find(|(other, f)| other != id && f[k].1 != *text) {
                            entry.1.insert(name);
                            // Which fields, compared with a neighbouring option (economy and war only: their names say most).
                            if k < 2 {
                                let set = fields.entry(id.clone()).or_default();
                                let mut d = std::collections::BTreeSet::new();
                                differing(text, &f[k].1, &mut d);
                                set.extend(d.into_iter().map(|x| format!("{x}")));
                            }
                        }
                    }
                }
            }
        }
        // The Continent wheel's options come from each map.
        let options: Vec<(String, String)> = if def.options.is_empty() { labels.iter().map(|(k, v)| (k.clone(), v.clone())).collect() } else { def.options.iter().map(|o| (o.id.clone(), o.label.clone())).collect() };
        println!("{} ({} options)", def.title, options.len());
        for (id, label) in &options {
            match parts.get(id) {
                None => println!("    {:<28} never on the wheel with these seeds (see the notes)", label),
                Some((_, p)) if p.is_empty() => {
                    silent_total += 1;
                    println!("    {:<28} CHANGES NOTHING on day one", label);
                }
                Some((_, p)) => {
                    let f: Vec<String> = fields.get(id).map_or(Vec::new(), |s| s.iter().take(7).cloned().collect());
                    println!("    {:<28} {:<36} e.g. {}", label, p.iter().copied().collect::<Vec<_>>().join(", "), f.join(", "));
                }
            }
        }
    }
    println!("\n{silent_total} options change nothing on day one.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Long games
// ---------------------------------------------------------------------------

/// The process's resident memory, in MB (Linux; 0 elsewhere).
pub fn rss_mb() -> f64 {
    std::fs::read_to_string("/proc/self/statm").ok().and_then(|s| s.split_whitespace().nth(1).and_then(|p| p.parse::<f64>().ok())).map_or(0.0, |pages| pages * 4096.0 / 1_048_576.0)
}

/// `--long-game YEARS [SEED] [MAP...]`
pub fn long_game(args: &[String]) -> Result<(), String> {
    let nums: Vec<u32> = args.iter().filter_map(|a| a.parse().ok()).collect();
    let years = nums.first().copied().unwrap_or(100);
    let seed = nums.get(1).copied().unwrap_or(1);
    let b = Bundle::load().map_err(|e| e.to_string())?;
    let maps: Vec<usize> = {
        let named: Vec<usize> = args.iter().filter_map(|a| b.data.maps.iter().position(|m| &m.id == a)).collect();
        if named.is_empty() { (0..b.data.maps.len()).collect() } else { named }
    };
    let dpy = b.econ.days_per_year;
    // Top speed is 30 days a second: a day must take under 33 ms on average.
    let budget_ms = 1000.0 / 30.0;
    for map in maps {
        let (_, mut sim) = b.game(map, seed, "normal");
        println!("== {} (seed {seed}), a {years}-year game: the player on autopilot, 24 brains ==", b.data.maps[map].name);
        println!("{:>5} {:>9} {:>9} {:>10} {:>8} {:>9} {:>8} {:>9} {:>8} {:>8}", "year", "ms/year", "ms/day", "worst day", "x speed", "memory", "save", "divisions", "reports", "treaties");
        let mut first_ms = 0.0;
        let mut last_ms = 0.0;
        let mut worst_overall = 0.0f64;
        let total = std::time::Instant::now();
        for y in 1..=years {
            let t = std::time::Instant::now();
            let mut worst = 0.0f64;
            for _ in 0..dpy {
                let d = std::time::Instant::now();
                sim.day(&b.econ);
                worst = worst.max(d.elapsed().as_secs_f64() * 1000.0);
            }
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            worst_overall = worst_overall.max(worst);
            if y <= 10 {
                first_ms += ms / 10.0;
            }
            if y + 10 > years {
                last_ms += ms / 10.0;
            }
            if y == 1 || y % 10 == 0 || y == years {
                let bytes = bincode::serialize(&sim).map(|v| v.len()).unwrap_or(0);
                let news = sim.wars.divisions.iter().filter(|d| d.alive).count();
                println!(
                    "{:>5} {:>9.0} {:>9.2} {:>9.1}ms {:>7.1}x {:>7.0}MB {:>6.1}MB {:>9} {:>8} {:>8}",
                    y,
                    ms,
                    ms / dpy as f64,
                    worst,
                    budget_ms / (ms / dpy as f64),
                    rss_mb(),
                    bytes as f64 / 1_048_576.0,
                    news,
                    sim.wars.reports.len(),
                    sim.diplo.treaties.len()
                );
            }
        }
        // The save at the end: its size on disk, and how long writing and reading take.
        let summary = crate::save::Summary { label: "test".into(), nation: String::new(), map: String::new(), date: String::new(), day: 0, difficulty: String::new(), saved_at: 0 };
        let t = std::time::Instant::now();
        let bytes = crate::save::to_bytes(&summary, &sim).map_err(|e| e.to_string())?;
        let write_ms = t.elapsed().as_secs_f64() * 1000.0;
        let t = std::time::Instant::now();
        let raw = bincode::serialize(&sim).map_err(|e| e.to_string())?;
        let encode_ms = t.elapsed().as_secs_f64() * 1000.0;
        let t = std::time::Instant::now();
        let back: Sim = bincode::deserialize(&raw).map_err(|e| e.to_string())?;
        let read_ms = t.elapsed().as_secs_f64() * 1000.0;
        drop(back);
        println!(
            "The save after {years} years: {:.1} MB on disk ({:.1} MB before compression); made in {:.0} ms ({:.0} ms on the game's thread for the yearly autosave, the rest on another), read in {:.0} ms (plus unpacking).",
            bytes.len() as f64 / 1_048_576.0,
            raw.len() as f64 / 1_048_576.0,
            write_ms,
            encode_ms,
            read_ms
        );
        // Does the history slow it down? The same world from here, with and
        // without its old records (battle and spy reports, news), five more years.
        let mut lean = sim.clone();
        lean.wars.reports.clear();
        lean.wars.news.clear();
        lean.economy.news.clear();
        lean.diplo.news.clear();
        lean.diplo.spies.reports.clear();
        let time5 = |s: &mut Sim| {
            let t = std::time::Instant::now();
            for _ in 0..5 * dpy {
                s.day(&b.econ);
            }
            t.elapsed().as_secs_f64() * 1000.0 / 5.0
        };
        let (full_ms, lean_ms) = (time5(&mut sim), time5(&mut lean));
        println!("Five more years: {full_ms:.0} ms a year with all its history, {lean_ms:.0} ms with its old reports and news cleared ({:+.0}%).", (full_ms / lean_ms.max(1e-9) - 1.0) * 100.0);
        let active = sim.wars.wars.iter().filter(|x| x.active()).count();
        let divisions = sim.wars.divisions.iter().filter(|d| d.alive).count();
        println!("At the end: {active} wars on, {divisions} divisions.");
        println!(
            "{} years in {:.0} s. Years 1-10: {:.0} ms a year; the last 10: {:.0} ms ({:+.0}%). Worst single day {:.1} ms (top speed allows {:.1} ms a day on average).\n",
            years,
            total.elapsed().as_secs_f64(),
            first_ms,
            last_ms,
            (last_ms / first_ms.max(1e-9) - 1.0) * 100.0,
            worst_overall,
            budget_ms
        );
    }
    Ok(())
}
