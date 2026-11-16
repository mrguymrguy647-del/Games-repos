//! `--diplomacy-report` (docs/DIPLOMACY.md, section 13): full NPC worlds
//! (economy, brains, war, diplomacy, spies) for decades, summed up; with
//! `--compare`, the same worlds without diplomacy beside them.

use std::collections::BTreeMap;

use super::{Kind, State};
use crate::war::report::WarWorld;

type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// What one world did.
#[derive(Debug, Clone, Default)]
pub struct WorldDiplo {
    pub text: String,
    pub growth: f64,
    pub wars: usize,
    /// By the highest level: (count, days); civil wars in slot 0.
    pub length: [(usize, f64); 5],
    pub killed: f64,
    pub stats: BTreeMap<String, f64>,
    pub nations: usize,
}

/// Runs one world for `years`, with diplomacy or without.
pub fn run(map: usize, seed: u32, years: f64, diplomacy: bool) -> Res<WorldDiplo> {
    let mut ww = WarWorld::new(map, seed, true)?;
    if diplomacy {
        ww = ww.with_diplomacy(seed)?;
    }
    let start: f64 = ww.e.nations.iter().map(|n| n.gdp).sum();
    let days = (years * ww.ed.days_per_year as f64) as u32;
    for _ in 0..days {
        ww.day();
    }
    let end: f64 = ww.e.nations.iter().map(|n| n.gdp).sum();
    let mut out = WorldDiplo { growth: (end / start.max(1.0)).powf(1.0 / years.max(1.0)) - 1.0, nations: ww.wars.nations, ..WorldDiplo::default() };
    let w = &ww.wars;
    // (A cold war that warmed up is a long arms race, not a skirmish: left out.)
    for x in w.wars.iter().filter(|x| x.max_level > 0 && (x.start_level > 0 || x.civil)) {
        out.wars += 1;
        let slot = if x.civil { 0 } else { x.max_level };
        out.length[slot].0 += 1;
        out.length[slot].1 += (x.end.unwrap_or(w.day) - x.start) as f64;
        out.killed += x.losses[0].killed + x.losses[1].killed;
    }
    for (k, v) in &w.stats {
        if k.starts_with("wars prevented") {
            out.stats.insert(k.to_string(), *v);
        }
    }
    let mut lines = vec![format!("== {} (seed {seed}), {years} years{} ==", ww.data.maps[map].name, if diplomacy { "" } else { ", no diplomacy" })];
    lines.push(format!("  world GDP growth {:.2}% a year; {} wars, {} killed", out.growth * 100.0, out.wars, crate::war::report::big(out.killed)));
    if let Some(d) = &ww.diplo {
        for (k, v) in &d.stats {
            out.stats.insert(k.clone(), *v);
        }
        // Treaties in force at the end, and the alliance blocs.
        let mut active: BTreeMap<&str, usize> = BTreeMap::new();
        for t in d.live() {
            *active.entry(t.kind.name()).or_insert(0) += 1;
        }
        lines.push(format!("  in force at the end: {}", active.iter().map(|(k, v)| format!("{v} {}", k.to_lowercase())).collect::<Vec<_>>().join(", ")));
        let broken: Vec<String> = d
            .treaties
            .iter()
            .filter_map(|t| match &t.state {
                State::Broken { by, day, how } => Some(format!("    Year {}: {} broke its {} ({})", day / 360 + 1, d.names[*by], t.kind.name().to_lowercase(), how)),
                _ => None,
            })
            .collect();
        lines.push(format!("  treaties broken: {}", broken.len()));
        lines.extend(broken.into_iter().take(8));
        let (lo, hi) = d.reputation.iter().enumerate().fold((0, 0), |(l, h), (i, &r)| (if r < d.reputation[l] { i } else { l }, if r > d.reputation[h] { i } else { h }));
        lines.push(format!("  reputation: best {} ({:.0}), worst {} ({:.0})", d.names[hi], d.reputation[hi], d.names[lo], d.reputation[lo]));
        let missions = d.spies.reports.len();
        let caught = d.spies.reports.iter().filter(|r| r.caught > 0).count();
        lines.push(format!("  spy missions finished: {missions} ({} succeeded, {caught} with agents caught)", d.spies.reports.iter().filter(|r| r.success).count()));
        for r in d.talks.rounds.iter().filter(|r| r.agreed).take(3) {
            lines.push(format!("    talks, Year {}: {} → {}: {} ({})", r.day / 360 + 1, d.names[r.from], d.names[r.to], r.offer, r.answer));
        }
        for n in d.news.iter().rev().take(4) {
            lines.push(format!("    news, Year {}: {}", n.day / 360 + 1, n.text));
        }
        let _ = Kind::ALL;
    }
    out.text = lines.join("\n");
    Ok(out)
}

/// Runs the worlds on threads; returns them in order.
fn run_all(jobs: &[(usize, u32)], years: f64, diplomacy: bool) -> Res<Vec<WorldDiplo>> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let queue = std::sync::Arc::new(std::sync::Mutex::new(jobs.iter().copied().enumerate().collect::<Vec<_>>()));
    let results = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let (queue, results) = (queue.clone(), results.clone());
            std::thread::spawn(move || loop {
                let job = queue.lock().ok().and_then(|mut q| q.pop());
                let Some((k, (m, s))) = job else { break };
                let r = run(m, s, years, diplomacy).map_err(|e| e.to_string());
                if let Ok(mut out) = results.lock() {
                    out.push((k, r));
                }
            })
        })
        .collect();
    for h in handles {
        let _ = h.join();
    }
    let mut results = std::mem::take(&mut *results.lock().unwrap_or_else(|p| p.into_inner()));
    results.sort_by_key(|x| x.0);
    results.into_iter().map(|(_, r)| r.map_err(|e| e.into())).collect()
}

/// Sums up many worlds.
pub fn summary(worlds: &[WorldDiplo]) -> Vec<String> {
    let mut stats: BTreeMap<String, f64> = BTreeMap::new();
    let mut length = [(0usize, 0.0f64); 5];
    let (mut wars, mut killed, mut nations) = (0, 0.0, 0);
    for w in worlds {
        for (k, v) in &w.stats {
            *stats.entry(k.clone()).or_insert(0.0) += v;
        }
        for k in 0..5 {
            length[k].0 += w.length[k].0;
            length[k].1 += w.length[k].1;
        }
        wars += w.wars;
        killed += w.killed;
        nations += w.nations;
    }
    let growth: Vec<f64> = worlds.iter().map(|w| w.growth).collect();
    let mean = growth.iter().sum::<f64>() / growth.len().max(1) as f64;
    let (lo, hi) = growth.iter().fold((f64::MAX, f64::MIN), |(a, b), &g| (a.min(g), b.max(g)));
    let mut out = vec![
        format!("  world GDP growth: {:.2}% a year on average (worlds from {:.2}% to {:.2}%)", mean * 100.0, lo * 100.0, hi * 100.0),
        format!("  wars: {wars} ({:.2} a nation per decade), {} soldiers killed", wars as f64 / nations.max(1) as f64 * 10.0 / 50.0, crate::war::report::big(killed)),
    ];
    for (k, name) in ["Civil war", "Border skirmish", "Military operation", "Declared war", "Total war"].iter().enumerate() {
        let (n, d) = length[k];
        if n > 0 {
            out.push(format!("  {name}: {n}, {:.2} years on average", d / n as f64 / 360.0));
        }
    }
    let groups = [("signed: ", "treaties signed"), ("broken: ", "treaties broken"), ("proposed: ", "proposed"), ("missions sent: ", "spy missions sent"), ("missions succeeded: ", "missions succeeded"), ("missions failed: ", "missions failed"), ("negotiated peace: ", "negotiated peace")];
    for (prefix, name) in groups {
        let parts: Vec<String> = stats.iter().filter(|(k, _)| k.starts_with(prefix)).map(|(k, v)| format!("{} {}", crate::war::report::big(*v), &k[prefix.len()..])).collect();
        if !parts.is_empty() {
            let total: f64 = stats.iter().filter(|(k, _)| k.starts_with(prefix)).map(|(_, v)| v).sum();
            out.push(format!("  {name}: {} ({})", crate::war::report::big(total), parts.join(", ")));
        }
    }
    for (k, v) in &stats {
        if !groups.iter().any(|(p, _)| k.starts_with(p)) {
            out.push(format!("  {k}: {}", if v.fract() == 0.0 { crate::war::report::big(*v) } else { format!("{v:.1}") }));
        }
    }
    out
}

/// `--diplomacy-report [YEARS] [SEEDS...] [--compare]`.
pub fn diplomacy_report(args: &[String]) -> Res<()> {
    let compare = args.iter().any(|a| a == "--compare");
    let mut nums: Vec<u32> = args.iter().take_while(|a| !a.starts_with("--")).filter_map(|a| a.parse().ok()).collect();
    let years = if nums.is_empty() { 50 } else { nums.remove(0) } as f64;
    let seeds = if nums.is_empty() { vec![1, 2, 3, 4, 5, 6] } else { nums };
    let maps = crate::data::Data::load().map_err(|x| x.to_string())?.maps.len();
    let jobs: Vec<(usize, u32)> = seeds.iter().flat_map(|&s| (0..maps).map(move |m| (m, s))).collect();
    let start = std::time::Instant::now();
    let with = run_all(&jobs, years, true)?;
    for w in &with {
        println!("{}\n", w.text);
    }
    println!("SUMMARY, with diplomacy: {} worlds x {years} years", with.len());
    for l in summary(&with) {
        println!("{l}");
    }
    if compare {
        let without = run_all(&jobs, years, false)?;
        println!("\nTHE SAME WORLDS WITHOUT DIPLOMACY:");
        for l in summary(&without) {
            println!("{l}");
        }
    }
    println!("({:.0} s)", start.elapsed().as_secs_f64());
    Ok(())
}
