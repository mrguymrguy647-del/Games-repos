//! Reports (docs/WAR.md, sections 12 and 13): battle reports in words,
//! and `--war-report`, which runs NPC worlds and sums up every war.

use super::{BattleReport, SideReport, Wars, LEVELS};

/// A number in words: 1,234 → "1,234", 2.5M → "2.5M".
pub fn big(x: f64) -> String {
    let x = x.round();
    if x.abs() >= 10_000_000.0 {
        return format!("{:.1}M", x / 1e6);
    }
    let s = format!("{}", x.abs() as u64);
    let mut out = String::new();
    for (k, ch) in s.chars().enumerate() {
        if k > 0 && (s.len() - k) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if x < 0.0 { format!("-{out}") } else { out }
}

/// A long list cut short: "a, b, c, d, e and 12 more".
pub fn some(items: &[String], n: usize) -> String {
    if items.len() <= n + 1 {
        return items.join(", ");
    }
    format!("{} and {} more", items[..n].join(", "), items.len() - n)
}

fn side_lines(w: &Wars, name: &str, s: &SideReport) -> Vec<String> {
    let who = s.factions.iter().map(|&f| w.names[f].clone()).collect::<Vec<_>>().join(", ");
    vec![
        format!("{name}: {who}"),
        format!("  divisions       {} ({})", s.divisions.len(), some(&s.divisions, 4)),
        format!("  generals        {}", some(&s.generals, 3)),
        format!("  soldiers        {} at the start, {} tanks", big(s.soldiers_start), big(s.tanks_start)),
        format!("  rounds fired    {} ({} misfired; {} rifle jams)", big(s.rounds_fired), big(s.misfires), big(s.jams)),
        format!("  shells fired    {} ({} duds)", big(s.shells_fired), big(s.duds)),
        format!("  killed          {}", big(s.killed)),
        format!("  wounded         {}", big(s.wounded)),
        format!("  captured        {}", big(s.captured)),
        format!("  surrendered     {}", big(s.surrendered)),
        format!("  deserted        {}", big(s.deserted)),
        format!("  tanks           {} destroyed, {} broke down, {} abandoned, {} captured from the enemy", big(s.tanks_lost), big(s.tanks_broke_down), big(s.tanks_abandoned), big(s.tanks_captured)),
        format!("  ammo used       {} units ({} rounds' worth)", big(s.ammo_used), big(s.ammo_used * w.data.combat.rounds_per_ammo)),
        format!("  fuel used       {} t", big(s.fuel_used)),
    ]
}

/// A battle report in words.
pub fn battle_text(w: &Wars, r: &BattleReport) -> String {
    let mut out = vec![
        format!("BATTLE OF {}", r.province_name.to_uppercase()),
        format!(
            "{} hours, day {} to {} · {} · weather: {} · {}",
            r.hours,
            r.day_start,
            r.day_end,
            r.terrain,
            r.weather.join(", "),
            if r.winner == 0 { "the attackers took the province" } else { "the defenders held" }
        ),
    ];
    out.extend(side_lines(w, "ATTACKERS", &r.attacker));
    out.extend(side_lines(w, "DEFENDERS", &r.defender));
    out.join("\n")
}

/// A war's summary line block.
pub fn war_text(w: &Wars, k: usize, days_per_year: u32, atlas: &crate::mapdata::Atlas, world: &crate::world::World) -> String {
    let x = &w.wars[k];
    let names = |s: usize| x.sides[s].iter().map(|&f| w.names[f].clone()).collect::<Vec<_>>().join(" + ");
    let year = |d: u32| d as f64 / days_per_year as f64 + 1.0;
    let end = x.end.map_or("still on".to_string(), |d| format!("year {:.1}", year(d)));
    let lines = [
        format!("{}{} vs {}{}", names(0), if x.proxies[0].is_empty() { String::new() } else { format!(" (armed by {})", x.proxies[0].iter().map(|&f| w.names[f].clone()).collect::<Vec<_>>().join(", ")) }, names(1), if x.proxies[1].is_empty() { String::new() } else { format!(" (armed by {})", x.proxies[1].iter().map(|&f| w.names[f].clone()).collect::<Vec<_>>().join(", ")) }),
        format!("  why: {}; goal: {}", x.cause, x.goal.words(world, atlas)),
        format!("  year {:.1} to {end}; started as {}, rose to {}; {} battles", year(x.start), LEVELS[x.start_level], LEVELS[x.max_level], x.battles),
        format!(
            "  losses: {} dead, {} wounded, {} captured, {} deserted, {} tanks, {} jets, {} warships  |  {} dead, {} wounded, {} captured, {} deserted, {} tanks, {} jets, {} warships",
            big(x.losses[0].killed), big(x.losses[0].wounded), big(x.losses[0].captured), big(x.losses[0].deserted), big(x.losses[0].tanks), big(x.losses[0].jets), big(x.losses[0].warships),
            big(x.losses[1].killed), big(x.losses[1].wounded), big(x.losses[1].captured), big(x.losses[1].deserted), big(x.losses[1].tanks), big(x.losses[1].jets), big(x.losses[1].warships)
        ),
        format!("  result: {}", x.result.clone().unwrap_or_else(|| "still fighting".to_string())),
    ];
    lines.join("\n")
}

type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// A world with brains and war, made from a map and seed.
pub struct WarWorld {
    pub data: crate::data::Data,
    pub atlas: crate::mapdata::Atlas,
    pub ed: crate::econ::EconData,
    pub world: crate::world::World,
    pub e: crate::econ::Economy,
    pub brains: Option<crate::ai::Brains>,
    pub wars: Wars,
}

impl WarWorld {
    pub fn new(map: usize, seed: u32, brains: bool) -> Res<WarWorld> {
        let (data, atlas, ed, world, mut e) = crate::make_world(map, seed).map_err(|x| x.to_string())?;
        let ai = crate::ai::AiData::load()?;
        let brains = if brains { Some(crate::ai::Brains::new(ai.clone(), &data, &atlas, &world, &mut e, seed, |_| true)) } else { None };
        let mut wars = Wars::new(super::WarData::load()?, &data, &atlas, &world, &mut e, &ed, seed);
        wars.set_intel(&ai);
        Ok(WarWorld { data, atlas, ed, world, e, brains, wars })
    }

    pub fn day(&mut self) {
        self.e.tick(&self.ed);
        if let Some(b) = self.brains.as_mut() {
            b.day(&mut self.e, &self.ed);
        }
        self.wars.day(&mut self.e, &self.ed, self.brains.as_mut());
    }
}

/// The two neighbours with the biggest armies (for a forced battle).
pub fn rivals(w: &Wars) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize, f64)> = None;
    for a in 0..w.nations {
        for b in (a + 1)..w.nations {
            let touch = w.held(a).iter().any(|&q| w.geo.neighbours[q].iter().any(|&r| w.control[r] == Some(b)));
            if !touch {
                continue;
            }
            let s = w.strength(a).0.min(w.strength(b).0);
            if best.is_none_or(|x| s > x.2) {
                best = Some((a, b, s));
            }
        }
    }
    best.map(|x| (x.0, x.1))
}

/// The two strongest coastal nations with no land border (a war by sea).
pub fn rivals_by_sea(w: &Wars, e: &crate::econ::Economy, ed: &crate::econ::EconData) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize, f64)> = None;
    for a in 0..w.nations {
        for b in 0..w.nations {
            if a == b || e.nations[a].landlocked || e.nations[b].landlocked {
                continue;
            }
            let touch = w.held(a).iter().any(|&q| w.geo.neighbours[q].iter().any(|&r| w.control[r] == Some(b)));
            if touch || super::sea::transports(w, e, ed, a) < 20.0 {
                continue;
            }
            let s = w.strength(a).0.min(2.0 * w.strength(b).0);
            if best.is_none_or(|x| s > x.2) {
                best = Some((a, b, s));
            }
        }
    }
    best.map(|x| (x.0, x.1))
}

/// `--war-battle MAP SEED [sea]`: two neighbours go to war (no brains); prints
/// the first battle's report and the books.
pub fn battle_demo(args: &[String]) -> Res<()> {
    let data = crate::data::Data::load().map_err(|x| x.to_string())?;
    let id = args.first().cloned().unwrap_or_else(|| "grand".into());
    let map = data.maps.iter().position(|m| m.id == id).ok_or(format!("there's no map \"{id}\""))?;
    let seed: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut ww = WarWorld::new(map, seed, false)?;
    ww.wars.ai = false;
    let sea = args.get(2).is_some_and(|a| a == "sea");
    let (a, b) = if sea { rivals_by_sea(&ww.wars, &ww.e, &ww.ed).ok_or("no two coastal nations apart")? } else { rivals(&ww.wars).ok_or("no two nations share a border")? };
    let mut border: Vec<usize> = ww.wars.held(b).into_iter().filter(|&q| ww.wars.geo.neighbours[q].iter().any(|&r| ww.wars.control[r] == Some(a))).collect();
    if sea {
        border = ww.wars.held(b).into_iter().filter(|&q| ww.wars.geo.coastal[q]).take(3).collect();
        println!("{} ({} transports, {} warships) against {} ({} warships), by sea", ww.wars.names[a], ww.e.nations[a].stockpile[ww.ed.transports], ww.e.nations[a].stockpile[ww.ed.warships], ww.wars.names[b], ww.e.nations[b].stockpile[ww.ed.warships]);
    }
    let k = super::escalation::declare(&mut ww.wars, &mut ww.e, &ww.ed, a, b, 3, super::Goal::Provinces(border), "a forced test".into());
    for _ in 0..180 {
        ww.day();
        if !ww.wars.reports.is_empty() {
            break;
        }
    }
    let Some(r) = ww.wars.reports.first().cloned() else { return Err("no battle in 180 days".into()) };
    println!("{}", battle_text(&ww.wars, &r));
    for day in 0..360 {
        ww.day();
        if sea && day % 30 == 0 {
            let fleets = |f: usize| ww.wars.forces[f].fleets.iter().map(|x| x.1).sum::<f64>();
            let held = |f: usize| (0..ww.wars.zone_fleets.len()).filter(|&z| super::sea::hostile(&ww.wars, if f == a { b } else { a }, z)).count();
            let starving: f64 = ww.wars.divisions.iter().filter(|d| d.alive && d.faction == a && d.food <= 0.0).map(|d| d.soldiers).sum();
            let unsupplied = { let l = super::supply::lines(&ww.wars, &ww.e, a); ww.wars.divisions.iter().filter(|d| d.alive && d.faction == a && l[d.province].is_none()).count() };
            let coast: Vec<usize> = { let mut z: Vec<usize> = ww.wars.held(b).iter().flat_map(|&q| ww.wars.geo.zones_of[q].clone()).collect(); z.sort(); z.dedup(); z };
            println!("  day {day}: fleets {:?}; enemy coast zones {:?}; control {:?}", ww.wars.forces[a].fleets, coast, coast.iter().map(|&z| ww.wars.zone_control[z]).collect::<Vec<_>>());
            println!("  day {day}: {} starving soldiers, {unsupplied} divisions cut off, dead {}", big(starving), big(ww.wars.forces[a].dead));
            println!("  day {day}: warships {:.0} vs {:.0}; sea zones held {} vs {}; {} blockaded {:.0}%; {} occupies {} of {}'s provinces", fleets(a), fleets(b), held(a), held(b), ww.wars.names[b], ww.e.nations[b].blockade * 100.0, ww.wars.names[a], ww.wars.held(a).iter().filter(|&&q| ww.wars.owner[q] == Some(b)).count(), ww.wars.names[b]);
        }
    }
    println!("\n{}", war_text(&ww.wars, k, ww.ed.days_per_year, &ww.atlas, &ww.world));
    println!("battles fought: {}", ww.wars.reports.len());
    for f in [a, b] {
        let (mut cap, mut sur, mut hits, mut killed) = (0.0, 0.0, 0.0, 0.0);
        for r in &ww.wars.reports {
            for side in [&r.attacker, &r.defender] {
                if side.factions.contains(&f) {
                    cap += side.captured;
                    sur += side.surrendered;
                    hits += side.hits;
                    killed += side.killed;
                }
            }
        }
        println!("{}: in battle reports: hits {} killed {} captured {} surrendered {}; prisoners held by enemy {}", ww.wars.names[f], big(hits), big(killed), big(cap), big(sur), big(ww.wars.prisoners[if f == a { b } else { a }][f]));
    }
    // The books.
    let mut worst = 0.0f64;
    for d in &ww.wars.divisions {
        worst = worst.max((d.tally.expected() - d.soldiers).abs());
    }
    println!("soldier books: largest mismatch in any division {worst}");
    Ok(())
}

/// Debug: a nation's army, month by month, in a forced war.
pub fn army_trace(args: &[String]) -> Res<()> {
    let data = crate::data::Data::load().map_err(|x| x.to_string())?;
    let id = args.first().cloned().unwrap_or_else(|| "grand".into());
    let map = data.maps.iter().position(|m| m.id == id).ok_or("no map")?;
    let seed: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut ww = WarWorld::new(map, seed, false)?;
    ww.wars.ai = false;
    let (a, b) = rivals(&ww.wars).ok_or("none")?;
    let border: Vec<usize> = ww.wars.held(b).into_iter().filter(|&q| ww.wars.geo.neighbours[q].iter().any(|&r| ww.wars.control[r] == Some(a))).collect();
    if std::env::var("NO_WAR").is_err() {
        super::escalation::declare(&mut ww.wars, &mut ww.e, &ww.ed, a, b, 3, super::Goal::Provinces(border), "test".into());
    }
    for day in 0..360 {
        ww.day();
        if day % 15 == 0 {
            for f in [a, b] {
                let ds: Vec<&super::Division> = ww.wars.divisions.iter().filter(|d| d.alive && d.faction == f).collect();
                let s: f64 = ds.iter().map(|d| d.soldiers).sum();
                let m: f64 = ds.iter().map(|d| d.morale * d.soldiers).sum::<f64>() / s.max(1.0);
                let eq: f64 = ds.iter().map(|d| super::land::equipped(&ww.wars, d) * d.soldiers).sum::<f64>() / s.max(1.0);
                let t = ds.iter().fold(super::Tally::default(), |mut t, d| {
                    t.killed += d.tally.killed;
                    t.deserted += d.tally.deserted;
                    t.captured += d.tally.captured;
                    t.joined += d.tally.joined;
                    t
                });
                let low = ds.iter().filter(|d| d.morale < 0.4).count();
                let battle = ds.iter().filter(|d| d.battle.is_some()).count();
                let n = &ww.e.nations[f];
                let occ = ww.wars.owner.iter().enumerate().filter(|(q, o)| **o == Some(f) && ww.wars.control[*q] != Some(f)).count();
                let own = ww.wars.owner.iter().filter(|o| **o == Some(f)).count();
                eprintln!("   {}: occupied {occ}/{own}, food made {:.0} wanted {:.0} got {:.0} grain made {:.0} stock {:.0}, famine pop {:.0}", ww.wars.names[f], n.ledger.produced[ww.ed.food], n.wanted[ww.ed.food], n.got[ww.ed.food], n.ledger.produced[ww.ed.grain], n.stock[ww.ed.grain], n.population);
                println!(
                    "d{day} {}: {} divs ({} low morale, {} in battle) soldiers {} morale {:.2} equipped {:.2} | killed {} deserted {} captured {} joined {} | hospital {} | ammo {} rifles {} food {} | pop {}",
                    ww.wars.names[f], ds.len(), low, battle, big(s), m, eq, big(t.killed), big(t.deserted), big(t.captured), big(t.joined), big(ww.wars.forces[f].hospital),
                    big(n.stockpile[ww.ed.ammo]), big(n.stockpile[ww.ed.rifles]), big(n.stock[ww.ed.food]), big(n.population)
                );
            }
        }
    }
    Ok(())
}

/// What a 50-year world's wars came to.
#[derive(Debug, Clone, Default)]
pub struct WorldWars {
    pub text: String,
    pub wars: usize,
    pub cold: usize,
    pub civil: usize,
    pub by_level: [usize; 5],
    pub ended: usize,
    pub still_on: usize,
    pub proxies: usize,
    pub dead: f64,
    /// The most nations at declared or total war on any day, as a share.
    pub worst_share: f64,
    pub years_at_war: f64,
    pub nations: usize,
    pub battles: usize,
    /// By the highest level a war reached: (count, days, longest days).
    pub length: [(usize, f64, f64); 6],
    pub killed: f64,
    pub captured: f64,
    pub deserted: f64,
    /// Why wars started (the first words of their cause).
    pub causes: std::collections::BTreeMap<String, usize>,
    /// Peace deals that made a vassal, vassals that broke free, vassals at the end.
    pub vassalised: usize,
    pub freed: usize,
    pub vassals_end: usize,
    /// The biggest overlord at the end: its share of the world's provinces (with its vassals').
    pub biggest_bloc: f64,
    /// Nations whose stability ever fell under 20.
    pub unstable: usize,
    pub stats: std::collections::BTreeMap<String, f64>,
}

/// A war's cause, in a few words (for the report's tally).
pub fn cause_kind(cause: &str) -> String {
    let c = cause.split(':').next().unwrap_or(cause).trim();
    let words: Vec<&str> = c.split_whitespace().take(4).collect();
    words.join(" ")
}

/// Runs one NPC world with war and sums up its wars.
pub fn run_wars(map: usize, seed: u32, years: f64, battles: bool) -> Res<WorldWars> {
    let mut ww = WarWorld::new(map, seed, true)?;
    let days = (years * ww.ed.days_per_year as f64) as u32;
    let mut out = WorldWars { nations: ww.wars.nations, ..WorldWars::default() };
    let mut nation_days_at_war = 0.0;
    let mut low: Vec<usize> = Vec::new();
    let mut worst_day = 0;
    let mut worst_who = String::new();
    let mut overlords: Vec<Option<usize>> = ww.e.nations.iter().map(|n| n.overlord).collect();
    for _ in 0..days {
        ww.day();
        let hot = (0..ww.wars.nations).filter(|&i| ww.wars.level_of(i) >= 3).count();
        nation_days_at_war += (0..ww.wars.nations).filter(|&i| ww.wars.level_of(i) >= 1).count() as f64;
        if hot as f64 / ww.wars.nations as f64 > out.worst_share {
            out.worst_share = hot as f64 / ww.wars.nations as f64;
            worst_day = ww.wars.day;
            let w = &ww.wars;
            worst_who = w.wars.iter().filter(|x| x.active() && x.level >= 3).map(|x| format!("{} [{}] {}v{}", x.motive, x.level, x.sides[0].len(), x.sides[1].len())).collect::<Vec<_>>().join("; ");
        }
        for i in 0..ww.wars.nations {
            if ww.e.nations[i].stability < 20.0 && !low.contains(&i) {
                low.push(i);
            }
            let now = ww.e.nations[i].overlord;
            if now != overlords[i] {
                if now.is_some() {
                    out.vassalised += 1;
                } else {
                    out.freed += 1;
                }
                overlords[i] = now;
            }
        }
    }
    out.unstable = low.len();
    out.vassals_end = overlords.iter().filter(|o| o.is_some()).count();
    let top = |mut i: usize| {
        let mut n = 0;
        while let Some(o) = ww.e.nations[i].overlord {
            i = o;
            n += 1;
            if n > 30 {
                break;
            }
        }
        i
    };
    let mut bloc = vec![0usize; ww.wars.nations];
    for q in 0..ww.wars.owner.len() {
        if let Some(o) = ww.wars.owner[q] {
            bloc[top(o)] += 1;
        }
    }
    out.biggest_bloc = *bloc.iter().max().unwrap_or(&0) as f64 / ww.wars.owner.iter().filter(|o| o.is_some()).count().max(1) as f64;
    out.text = format!("(at worst {:.0}% of nations at declared or total war at once, on day {}: {}) ", out.worst_share * 100.0, worst_day, worst_who);
    out.text = format!("{}({} nations' stability ever fell under 20; {} vassals made, {} broke free, {} at the end; the biggest bloc holds {:.0}% of the land)\n", out.text, low.len(), out.vassalised, out.freed, out.vassals_end, out.biggest_bloc * 100.0);
    let w = &ww.wars;
    let mut lines = vec![format!("== {} (seed {seed}), {years} years: {} wars ==", ww.data.maps[map].name, w.wars.iter().filter(|x| x.max_level > 0).count())];
    for (k, x) in w.wars.iter().enumerate() {
        if x.max_level == 0 {
            out.cold += 1;
            lines.push(format!("Cold war: {} vs {}, year {:.0} to {}", w.names[x.sides[0][0]], w.names[x.sides[1][0]], x.start as f64 / 360.0 + 1.0, x.end.map_or("now".to_string(), |d| format!("{:.0}", d as f64 / 360.0 + 1.0))));
            continue;
        }
        out.wars += 1;
        out.by_level[x.max_level] += 1;
        let days = (x.end.unwrap_or(w.day) - x.start) as f64;
        // (Civil wars are counted on their own, in slot 0.)
        let l = &mut out.length[if x.civil { 0 } else if x.goal == super::Goal::Independence { 5 } else { x.max_level }];
        l.0 += 1;
        l.1 += days;
        l.2 = l.2.max(days);
        for s in 0..2 {
            out.killed += x.losses[s].killed;
            out.captured += x.losses[s].captured;
            out.deserted += x.losses[s].deserted;
        }
        let kind = if !x.motive.is_empty() { x.motive.clone() } else if x.civil { "civil war".to_string() } else { cause_kind(&x.cause) };
        *out.causes.entry(kind).or_insert(0) += 1;
        if x.civil {
            out.civil += 1;
        }
        if x.end.is_some() {
            out.ended += 1;
        } else {
            out.still_on += 1;
        }
        if !x.proxies[0].is_empty() || !x.proxies[1].is_empty() {
            out.proxies += 1;
        }
        out.dead += x.losses[0].killed + x.losses[1].killed;
        out.battles += x.battles as usize;
        lines.push(war_text(w, k, ww.ed.days_per_year, &ww.atlas, &ww.world));
    }
    for (k, v) in &w.stats {
        out.stats.insert(k.to_string(), *v);
    }
    for x in w.wars.iter().filter(|x| x.max_level > 0) {
        if let Some(r) = &x.result {
            let how = r.split(':').next().unwrap_or("").to_string();
            let how = if how.ends_with("backs down") { "one side backs down".to_string() } else { how };
            *out.stats.entry(format!("ended: {how}")).or_insert(0.0) += 1.0;
        }
    }
    out.years_at_war = nation_days_at_war / ww.ed.days_per_year as f64 / w.nations as f64;
    if battles {
        if let Some(r) = w.reports.iter().max_by(|a, b| (a.attacker.killed + a.defender.killed).total_cmp(&(b.attacker.killed + b.defender.killed))) {
            lines.push(format!("\nThe bloodiest battle:\n{}", battle_text(w, r)));
        }
    }
    out.text = format!("{}{}", out.text, lines.join("\n"));
    Ok(out)
}

/// `--war-report [YEARS] [SEEDS...] [--battles]`: every map with each seed, NPC-only, with war.
pub fn war_report(args: &[String]) -> Res<()> {
    let battles = args.iter().any(|a| a == "--battles");
    let mut nums: Vec<u32> = args.iter().take_while(|a| !a.starts_with("--")).filter_map(|a| a.parse().ok()).collect();
    let years = if nums.is_empty() { 50 } else { nums.remove(0) } as f64;
    let seeds = if nums.is_empty() { vec![1, 2, 3, 4, 5, 6] } else { nums };
    let maps = crate::data::Data::load().map_err(|x| x.to_string())?.maps.len();
    let jobs: Vec<(usize, u32)> = seeds.iter().flat_map(|&s| (0..maps).map(move |m| (m, s))).collect();
    let start = std::time::Instant::now();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let queue = std::sync::Arc::new(std::sync::Mutex::new(jobs.clone().into_iter().enumerate().collect::<Vec<_>>()));
    let results = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let (queue, results) = (queue.clone(), results.clone());
            std::thread::spawn(move || loop {
                let job = queue.lock().ok().and_then(|mut q| q.pop());
                let Some((k, (m, s))) = job else { break };
                let r = run_wars(m, s, years, battles).map_err(|e| e.to_string());
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
    let mut t = WorldWars::default();
    for (_, r) in &results {
        let r = r.as_ref().map_err(|e| e.clone())?;
        println!("{}\n", r.text);
        t.wars += r.wars;
        t.cold += r.cold;
        t.civil += r.civil;
        t.ended += r.ended;
        t.still_on += r.still_on;
        t.proxies += r.proxies;
        t.dead += r.dead;
        t.battles += r.battles;
        t.nations += r.nations;
        t.years_at_war += r.years_at_war;
        t.worst_share = t.worst_share.max(r.worst_share);
        for k in 0..5 {
            t.by_level[k] += r.by_level[k];
        }
        for k in 0..6 {
            t.length[k].0 += r.length[k].0;
            t.length[k].1 += r.length[k].1;
            t.length[k].2 = t.length[k].2.max(r.length[k].2);
        }
        t.killed += r.killed;
        t.captured += r.captured;
        t.deserted += r.deserted;
        for (k, v) in &r.causes {
            *t.causes.entry(k.clone()).or_insert(0) += v;
        }
        t.vassalised += r.vassalised;
        t.freed += r.freed;
        t.vassals_end += r.vassals_end;
        t.biggest_bloc = t.biggest_bloc.max(r.biggest_bloc);
        t.unstable += r.unstable;
        for (k, v) in &r.stats {
            *t.stats.entry(k.clone()).or_insert(0.0) += v;
        }
    }
    let worlds = results.len() as f64;
    println!("SUMMARY: {} worlds x {years} years ({} nations)", results.len(), t.nations);
    println!("  wars: {} ({:.1} a world, {:.2} a nation per decade), {} civil wars, {} with proxy supporters, {} cold wars", t.wars, t.wars as f64 / worlds, t.wars as f64 / t.nations as f64 * 10.0 / years, t.civil, t.proxies, t.cold);
    println!("  highest level reached: {} border skirmishes, {} military operations, {} declared wars, {} total wars", t.by_level[1], t.by_level[2], t.by_level[3], t.by_level[4]);
    println!("  ended in peace: {} of {} ({} still going at the end)", t.ended, t.wars, t.still_on);
    println!("  a nation spends {:.1} of {years} years at war, on average; at worst {:.0}% of a world's nations were at declared or total war at once", t.years_at_war / worlds, t.worst_share * 100.0);
    println!("  {} battles, {} soldiers killed", t.battles, big(t.dead));
    println!("  killed {}, captured {} ({:.1}x killed), deserted {}", big(t.killed), big(t.captured), t.captured / t.killed.max(1.0), big(t.deserted));
    let dpy = 360.0;
    for k in 0..6 {
        let name = match k {
            0 => "Civil war",
            5 => "War of independence",
            _ => super::LEVELS[k],
        };
        let (n, d, m) = t.length[k];
        if n > 0 {
            println!("  {name}: {n} wars, {:.2} years on average, longest {:.2}", d / n as f64 / dpy, m / dpy);
        }
    }
    let mut causes: Vec<(&String, &usize)> = t.causes.iter().collect();
    causes.sort_by(|a, b| b.1.cmp(a.1));
    println!("  causes: {}", causes.iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(", "));
    println!("  vassals: {} made, {} broke free, {} at the end; the biggest bloc held {:.0}% of a world's land", t.vassalised, t.freed, t.vassals_end, t.biggest_bloc * 100.0);
    println!("  {} nations' stability ever fell under 20", t.unstable);
    for (k, v) in &t.stats {
        println!("  {k}: {}", big(*v));
    }
    println!("({:.0} s)", start.elapsed().as_secs_f64());
    Ok(())
}

/// `--war-world MAP SEED YEARS`: one world, in this thread (for debugging).
pub fn war_world(args: &[String]) -> Res<()> {
    let data = crate::data::Data::load().map_err(|x| x.to_string())?;
    let id = args.first().cloned().unwrap_or_else(|| "grand".into());
    let map = data.maps.iter().position(|m| m.id == id).ok_or("no map")?;
    let seed: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let years: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10.0);
    let r = run_wars(map, seed, years, true)?;
    println!("{}", r.text);
    Ok(())
}
