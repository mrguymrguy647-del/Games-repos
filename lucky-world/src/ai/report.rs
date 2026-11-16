//! The brains' tools (docs/AI.md, section 11): `--ai-compare` (brains
//! against the Phase 3 autopilot), `--ai-tune` (hundreds of NPC-only worlds,
//! looking for dumb behaviour) and `--ai-trace` (one brain's weeks).

use super::{goal_name, AiData, Brains};
use crate::econ::{EconData, Economy};

type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
/// A world's work for `on_threads`.
type Job<T> = Box<dyn FnOnce() -> T + Send>;

/// How a world ended up.
#[derive(Debug, Clone, Copy, Default)]
pub struct Outcome {
    pub gdp_start: f64,
    pub gdp_end: f64,
    /// Nations above 15% unemployment at the end.
    pub jobless: u32,
    /// Nations rated junk (BB or worse) at the end.
    pub junk: u32,
    pub nations: u32,
    pub years: f64,
}

impl Outcome {
    pub fn of(e: &Economy, ed: &EconData, years: f64) -> Outcome {
        let mut o = Outcome { years, ..Outcome::default() };
        for n in &e.nations {
            o.gdp_start += n.gdp_start;
            o.gdp_end += n.gdp;
            o.jobless += u32::from(n.unemployment(ed) > 0.15);
            o.junk += u32::from(n.rating >= 4);
            o.nations += 1;
        }
        o
    }

    pub fn add(&mut self, o: &Outcome) {
        self.gdp_start += o.gdp_start;
        self.gdp_end += o.gdp_end;
        self.jobless += o.jobless;
        self.junk += o.junk;
        self.nations += o.nations;
        self.years = o.years;
    }

    /// World growth a year (W$ GDP, start to end).
    pub fn growth(&self) -> f64 {
        if self.gdp_start <= 0.0 || self.years <= 0.0 {
            return 0.0;
        }
        (self.gdp_end / self.gdp_start).powf(1.0 / self.years) - 1.0
    }
}

/// A world on a map from a seed, every nation run by a brain (or none: the
/// Phase 3 autopilot), simulated for `years`.
pub fn run_world(map: usize, seed: u32, years: f64, brains: bool) -> Res<(Economy, EconData, Option<Brains>)> {
    let (data, atlas, ed, world, mut e) = crate::make_world(map, seed).map_err(|x| x.to_string())?;
    let mut b = if brains { Some(Brains::new(AiData::load()?, &data, &atlas, &world, &mut e, seed, |_| true)) } else { None };
    let days = (years * ed.days_per_year as f64).round() as u32;
    for _ in 0..days {
        e.tick(&ed);
        if let Some(b) = b.as_mut() {
            b.day(&mut e, &ed);
        }
    }
    Ok((e, ed, b))
}

fn parse_nums(args: &[String]) -> Vec<u32> {
    args.iter().take_while(|a| !a.starts_with("--")).filter_map(|a| a.parse().ok()).collect()
}

/// Runs jobs on all cores, keeping their order.
fn on_threads<T: Send + 'static>(jobs: Vec<Job<T>>) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let jobs = std::sync::Arc::new(std::sync::Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>()));
    let out = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let jobs = jobs.clone();
            let out = out.clone();
            std::thread::spawn(move || loop {
                let job = jobs.lock().ok().and_then(|mut j| j.pop());
                let Some((k, f)) = job else { break };
                let r = f();
                if let Ok(mut o) = out.lock() {
                    o.push((k, r));
                }
            })
        })
        .collect();
    for h in handles {
        let _ = h.join();
    }
    let mut out = std::mem::take(&mut *out.lock().unwrap_or_else(|p| p.into_inner()));
    out.sort_by_key(|x| x.0);
    out.into_iter().map(|x| x.1).collect()
}

/// `--ai-compare [YEARS] [SEEDS...]`: every map with each seed, once with the
/// autopilot and once with brains; prints the before/after table.
pub fn compare(args: &[String]) -> Res<()> {
    let mut nums = parse_nums(args);
    let years = if nums.is_empty() { 50 } else { nums.remove(0) } as f64;
    let seeds = if nums.is_empty() { vec![1, 2, 3, 4, 5, 6] } else { nums };
    let why = args.iter().any(|a| a == "--why");
    let maps = crate::data::Data::load().map_err(|x| x.to_string())?.maps.iter().map(|m| m.id.clone()).collect::<Vec<_>>();
    let mut jobs: Vec<Job<Res<(Outcome, Outcome)>>> = Vec::new();
    for &seed in &seeds {
        for map in 0..maps.len() {
            jobs.push(Box::new(move || {
                let (e, ed, _) = run_world(map, seed, years, false)?;
                let before = Outcome::of(&e, &ed, years);
                let (e, ed, _) = run_world(map, seed, years, true)?;
                if why {
                    for (i, n) in e.nations.iter().enumerate() {
                        if n.rating >= 4 || n.unemployment(&ed) > 0.15 {
                            println!(
                                "  map {map} seed {seed} #{i} {}: {} u {:.0}% debt {:.0}% deficit {:.1}% infl {:.1}% stab {:.0} taxes {:.0}/{:.0}/{:.0} welfare {:.0}% research {:.1}% arms {:.1}% works {:.0}% (on {:.1}%)",
                                n.name, ed.ratings[n.rating].name, n.unemployment(&ed) * 100.0, n.debt_ratio(&ed) * 100.0, n.deficit_ema * 100.0,
                                n.inflation * 100.0, n.stability, n.income_tax * 100.0, n.corporate_tax * 100.0, n.sales_tax * 100.0,
                                n.welfare_rate * 100.0, n.research * 100.0, n.arms_share(&ed) * 100.0,
                                e.lever_with(i, crate::econ::player::Lever::PublicWorks, Some(&ed)) * 100.0, n.public_works / (n.population * ed.participation - n.army).max(1.0) * 100.0
                            );
                            let b = &n.budget_last_month;
                            let g = n.gdp_local * 30.0 / 100.0;
                            println!(
                                "      % of GDP: in {:.1} (inc {:.1} corp {:.1} sales {:.1} tariff {:.1} corrupt -{:.1}) out {:.1} (army {:.1} welfare {:.1} gov {:.1} infra {:.1} build {:.1} arms {:.1} research {:.1} interest {:.1} tribute {:.1})",
                                b.revenue() / g, b.income_tax / g, b.corporate_tax / g, b.sales_tax / g, b.tariffs / g, b.corruption / g,
                                b.spending() / g, b.army / g, b.welfare / g, b.government / g, b.infrastructure / g, b.construction / g, b.procurement / g, b.research / g, b.interest / g, b.tribute_out / g
                            );
                        }
                    }
                }
                Ok((before, Outcome::of(&e, &ed, years)))
            }));
        }
    }
    let start = std::time::Instant::now();
    let results = on_threads(jobs);
    println!("{:<12} {:>4}   {:>22}   {:>22}", "", "", "AUTOPILOT (Phase 3)", "NPC BRAINS (Phase 4)");
    println!("{:<12} {:>4}   {:>7} {:>7} {:>6}   {:>7} {:>7} {:>6}", "map", "seed", "growth", ">15%", "junk", "growth", ">15%", "junk");
    let (mut a, mut b) = (Outcome::default(), Outcome::default());
    let mut k = 0;
    for &seed in &seeds {
        for map in &maps {
            let (x, y) = match &results[k] {
                Ok(r) => *r,
                Err(err) => return Err(err.to_string().into()),
            };
            k += 1;
            println!(
                "{:<12} {:>4}   {:>6.2}% {:>7} {:>6}   {:>6.2}% {:>7} {:>6}",
                map, seed, x.growth() * 100.0, x.jobless, x.junk, y.growth() * 100.0, y.jobless, y.junk
            );
            a.add(&x);
            b.add(&y);
        }
    }
    println!("{}", "-".repeat(66));
    println!(
        "{:<17}   {:>6.2}% {:>7} {:>6}   {:>6.2}% {:>7} {:>6}",
        format!("ALL ({} nations)", a.nations), a.growth() * 100.0, a.jobless, a.junk, b.growth() * 100.0, b.jobless, b.junk
    );
    let verdict = |better: bool| if better { "better" } else { "NOT better" };
    println!("\nWorld growth a year:            {:.2}% -> {:.2}%  ({})", a.growth() * 100.0, b.growth() * 100.0, verdict(b.growth() > a.growth()));
    println!("Nations above 15% unemployment: {} -> {}  ({})", a.jobless, b.jobless, verdict(b.jobless < a.jobless));
    println!("Junk credit ratings:            {} -> {}  ({})", a.junk, b.junk, verdict(b.junk < a.junk));
    println!("({} worlds, {} years each, twice, in {:.0} s)", results.len(), years, start.elapsed().as_secs_f64());
    Ok(())
}

/// What the tuning tool counts in a world.
#[derive(Debug, Clone, Default)]
pub struct Dumb {
    pub brains: u32,
    pub taken: u32,
    pub failed: u32,
    pub flips: u32,
    pub quick_flips: u32,
    pub flips_by: std::collections::BTreeMap<&'static str, u32>,
    pub taken_by: std::collections::BTreeMap<&'static str, u32>,
    pub builds: u32,
    pub unneeded: u32,
    pub unneeded_by: std::collections::BTreeMap<String, u32>,
    pub defaults: u32,
    /// Nations that ended junk-rated while printing money (bad policy).
    pub broke: u32,
    pub outcome: Outcome,
    /// The worst nations: (flips + unneeded + defaults x 5, who).
    pub worst: Vec<(u32, String)>,
    /// Nations that defaulted: (defaults, who).
    pub defaulted: Vec<(u32, String)>,
}

/// `--ai-tune [WORLDS] [YEARS]`: NPC-only worlds (seeds 1.., the maps in
/// turn), reporting dumb behaviour.
pub fn tune(args: &[String]) -> Res<()> {
    let nums = parse_nums(args);
    let worlds = nums.first().copied().unwrap_or(30);
    let years = nums.get(1).copied().unwrap_or(30) as f64;
    let maps = crate::data::Data::load().map_err(|x| x.to_string())?.maps.len();
    let jobs: Vec<Job<Res<Dumb>>> = (0..worlds)
        .map(|w| {
            let f: Job<Res<Dumb>> = Box::new(move || {
                let (map, seed) = (w as usize % maps, 100 + w);
                let (e, ed, b) = run_world(map, seed, years, true)?;
                let b = b.ok_or("no brains")?;
                let mut d = Dumb { outcome: Outcome::of(&e, &ed, years), ..Dumb::default() };
                for brain in b.brains.iter().flatten() {
                    let s = &brain.stats;
                    let n = &e.nations[brain.nation];
                    d.brains += 1;
                    d.taken += s.taken;
                    d.failed += s.failed;
                    d.flips += s.flips;
                    d.quick_flips += s.quick_flips;
                    for (k, v) in &s.flips_by {
                        *d.flips_by.entry(k).or_insert(0) += v;
                    }
                    for (k, v) in &s.taken_by {
                        *d.taken_by.entry(k).or_insert(0) += v;
                    }
                    d.builds += s.builds;
                    d.unneeded += s.unneeded;
                    for (k, v) in &s.unneeded_by {
                        *d.unneeded_by.entry(k.clone()).or_insert(0) += v;
                    }
                    d.defaults += s.defaults;
                    d.broke += u32::from(n.rating >= 6 && n.printed_last_month > 0.0);
                    if s.defaults > 0 {
                        d.defaulted.push((s.defaults, format!("map {map} seed {seed} {} ({}, {}): {} defaults; now {} rating, debt {:.0}%, deficit {:.1}%, jobless {:.0}%, works {:.0}%", n.name, brain.personality.labels.join(", "), ed.models[n.model].name, s.defaults, ed.ratings[n.rating].name, n.debt_ratio(&ed) * 100.0, n.deficit_ema * 100.0, n.unemployment(&ed) * 100.0, e.lever_with(brain.nation, crate::econ::player::Lever::PublicWorks, Some(&ed)) * 100.0)));
                    }
                    let bad = s.flips + s.unneeded + 5 * s.defaults;
                    d.worst.push((bad, format!("map {map} seed {seed} {} ({}): {} flips, {}/{} builds idle, {} defaults", n.name, brain.personality.labels.join(", "), s.flips, s.unneeded, s.builds, s.defaults)));
                }
                Ok(d)
            });
            f
        })
        .collect();
    let start = std::time::Instant::now();
    let results = on_threads(jobs);
    let mut t = Dumb::default();
    for r in results {
        let d = r.map_err(|x| x.to_string())?;
        t.brains += d.brains;
        t.taken += d.taken;
        t.failed += d.failed;
        t.flips += d.flips;
        t.quick_flips += d.quick_flips;
        for (k, v) in &d.flips_by {
            *t.flips_by.entry(k).or_insert(0) += v;
        }
        for (k, v) in &d.taken_by {
            *t.taken_by.entry(k).or_insert(0) += v;
        }
        t.builds += d.builds;
        t.unneeded += d.unneeded;
        for (k, v) in &d.unneeded_by {
            *t.unneeded_by.entry(k.clone()).or_insert(0) += v;
        }
        t.defaults += d.defaults;
        t.broke += d.broke;
        t.outcome.add(&d.outcome);
        t.worst.extend(d.worst);
        t.defaulted.extend(d.defaulted);
    }
    let pct = |a: u32, b: u32| if b == 0 { 0.0 } else { 100.0 * a as f64 / b as f64 };
    println!("{} NPC-only worlds, {} years, {} brains ({:.0} s)", worlds, years, t.brains, start.elapsed().as_secs_f64());
    println!("  world growth {:.2}% a year, {} nations over 15% unemployment, {} junk", t.outcome.growth() * 100.0, t.outcome.jobless, t.outcome.junk);
    println!("  decisions:      {} ({} couldn't be done, {:.1}%)", t.taken, t.failed, pct(t.failed, t.taken + t.failed));
    println!("  bankruptcies:   {} defaults; {} nations ended near-default and printing money", t.defaults, t.broke);
    println!("  unneeded builds: {} of {} ({:.1}%) still mostly idle a year later", t.unneeded, t.builds, pct(t.unneeded, t.builds));
    let mut by: Vec<(&String, &u32)> = t.unneeded_by.iter().collect();
    by.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    println!("                  most: {}", by.iter().take(8).map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", "));
    println!(
        "  flip-flops:     {} of {} decisions ({:.2}%) undid a move within {} days; {} more ({:.1}%) adjusted one back within half a year",
        t.quick_flips, t.taken, pct(t.quick_flips, t.taken), super::planning::QUICK_DAYS, t.flips - t.quick_flips, pct(t.flips - t.quick_flips, t.taken)
    );
    let mut by: Vec<(&&str, &u32)> = t.flips_by.iter().collect();
    by.sort_by(|a, b| b.1.cmp(a.1));
    println!("                  most: {}", by.iter().take(6).map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", "));
    let mut by: Vec<(&&str, &u32)> = t.taken_by.iter().collect();
    by.sort_by(|a, b| b.1.cmp(a.1));
    println!("  decisions by action: {}", by.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", "));
    t.worst.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    t.defaulted.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    println!("  {} nations defaulted; the most:", t.defaulted.len());
    for (_, who) in t.defaulted.iter().take(6) {
        println!("    {who}");
    }
    println!("  the worst:");
    for (_, who) in t.worst.iter().take(8) {
        println!("    {who}");
    }
    Ok(())
}

/// `--ai-trace MAP SEED NATION DAYS`: one brain's plans, week by week.
pub fn trace(args: &[String]) -> Res<()> {
    let data = crate::data::Data::load().map_err(|x| x.to_string())?;
    let map_id = args.first().cloned().unwrap_or_else(|| "grand".into());
    let map = data.maps.iter().position(|m| m.id == map_id).ok_or(format!("there's no map \"{map_id}\""))?;
    let nums = parse_nums(&args[1.min(args.len())..]);
    let seed = nums.first().copied().unwrap_or(1);
    let nation = nums.get(1).copied().unwrap_or(1) as usize;
    let days = nums.get(2).copied().unwrap_or(720);
    let (data, atlas, ed, world, mut e) = crate::make_world(map, seed).map_err(|x| x.to_string())?;
    let mut b = Brains::new(AiData::load()?, &data, &atlas, &world, &mut e, seed, |_| true);
    let mut last = u32::MAX;
    for _ in 0..days {
        e.tick(&ed);
        b.day(&mut e, &ed);
        let Some(brain) = b.brain(nation) else { return Err("no such nation".into()) };
        if brain.thinking.day != last && brain.thinking.day == e.day {
            last = e.day;
            let n = &e.nations[nation];
            println!(
                "{}  {} [{}] goal {} — {}  | u {:.1}% infl {:.1}% def {:.1}% debt {:.0}% {} res {:.2}%",
                e.date(&ed), n.name, brain.personality.labels.join(", "), goal_name(brain.goal), brain.goal_why,
                n.unemployment(&ed) * 100.0, n.inflation * 100.0, n.deficit_ema * 100.0, n.debt_ratio(&ed) * 100.0,
                ed.ratings[n.rating].name, n.research * 100.0
            );
            let needs: Vec<String> = super::NEEDS.iter().zip(brain.needs.iter()).map(|(k, v)| format!("{k} {v:.2}")).collect();
            println!("    needs: {}", needs.join(", "));
            for o in brain.thinking.options.iter().take(3) {
                println!("    {:+.3} {}  [{}]", o.score, o.label, o.reasons.join("; "));
            }
            for t in &brain.thinking.taken {
                println!("    -> {t}");
            }
        }
    }
    Ok(())
}
