//! Planning, every week (docs/AI.md, sections 3 and 4): how the nation is
//! doing (its needs), every action it could take, a score for each (how
//! much it helps the goal now), and the best few taken, through the
//! player's own levers.

use super::data::{Difficulty, NeedVec, CATEGORIES, CHILL, DEBT, GROWTH, HEAT, JOBS, NEEDS, SECURITY, SUPPLY, SUPPORT, TRADE};
use super::{AiData, Brain, Considered, Decision};
use crate::econ::data::interpolate;
use crate::econ::player::Lever;
use crate::econ::{EconData, Economy, TradePolicy};

const EPS: f64 = 1e-9;

/// A move undone within this many days is a quick flip-flop (the tuning
/// tool's measure; `undo_days` in brain.toml is the rule against it).
pub const QUICK_DAYS: u32 = 90;

/// Something a brain can do: every one is a button the player has too.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Act {
    /// Presses on a lever (the interest rate's are fixed-rate presses).
    Lever(Lever, i32),
    /// Hands the rate back to the central bank.
    RateAuto,
    Build(usize),
    Close(usize),
    Road(usize),
    Port(usize),
    BanExports(usize),
    LiftBan(usize),
    Embargo(usize),
    LiftEmbargo(usize),
}

impl Act {
    /// Its name in brain.toml's `[action.*]`.
    pub fn effect_key(self) -> &'static str {
        match self {
            Act::Lever(l, s) => {
                let up = s > 0;
                match l {
                    Lever::IncomeTax => if up { "income_tax_up" } else { "income_tax_down" },
                    Lever::CorporateTax => if up { "corporate_tax_up" } else { "corporate_tax_down" },
                    Lever::SalesTax => if up { "sales_tax_up" } else { "sales_tax_down" },
                    Lever::Welfare => if up { "welfare_up" } else { "welfare_down" },
                    Lever::Tariff => if up { "tariff_up" } else { "tariff_down" },
                    Lever::InterestRate => if up { "rate_up" } else { "rate_down" },
                    Lever::Research => if up { "research_up" } else { "research_down" },
                    Lever::Arms => if up { "arms_up" } else { "arms_down" },
                }
            }
            Act::RateAuto => "rate_auto",
            Act::Build(_) => "build",
            Act::Close(_) => "close",
            Act::Road(_) => "road",
            Act::Port(_) => "port",
            Act::BanExports(_) => "ban_exports",
            Act::LiftBan(_) => "lift_ban",
            Act::Embargo(_) => "embargo",
            Act::LiftEmbargo(_) => "lift_embargo",
        }
    }

    /// Its category (index into `CATEGORIES`): a brain takes one of each a week.
    pub fn category(self) -> usize {
        match self {
            Act::Lever(Lever::InterestRate, _) | Act::RateAuto => 1,
            Act::Lever(Lever::Tariff, _) | Act::BanExports(_) | Act::LiftBan(_) | Act::Embargo(_) | Act::LiftEmbargo(_) => 4,
            Act::Lever(Lever::Research, _) => 6,
            Act::Lever(Lever::Arms, _) => 5,
            Act::Lever(..) => 0,
            Act::Build(_) | Act::Close(_) => 2,
            Act::Road(_) | Act::Port(_) => 3,
        }
    }

    /// Which lever it moves and which way, to notice it being moved back.
    fn moves(self) -> Option<(u64, i8)> {
        let lever_key = |l: Lever| Lever::ALL.iter().position(|&x| x == l).unwrap_or(0) as u64;
        match self {
            Act::Lever(l, s) => Some((lever_key(l), s.signum() as i8)),
            Act::RateAuto => Some((lever_key(Lever::InterestRate), 0)),
            Act::Build(k) => Some((100 + k as u64, 1)),
            Act::Close(k) => Some((100 + k as u64, -1)),
            Act::BanExports(g) => Some((1000 + g as u64, 1)),
            Act::LiftBan(g) => Some((1000 + g as u64, -1)),
            Act::Embargo(j) => Some((2000 + j as u64, 1)),
            Act::LiftEmbargo(j) => Some((2000 + j as u64, -1)),
            Act::Road(_) | Act::Port(_) => None,
        }
    }

    /// In words, for the spectator.
    pub fn label(self, e: &Economy, ed: &EconData) -> String {
        match self {
            Act::Lever(l, s) => {
                let what = match l {
                    Lever::IncomeTax => "income tax",
                    Lever::CorporateTax => "corporate tax",
                    Lever::SalesTax => "sales tax",
                    Lever::Welfare => "welfare",
                    Lever::Tariff => "tariffs",
                    Lever::InterestRate => "the interest rate",
                    Lever::Research => "research and schools",
                    Lever::Arms => "arms purchases",
                };
                format!("{} {what}", if s > 0 { "Raise" } else { "Cut" })
            }
            Act::RateAuto => "Let the central bank set the rate".to_string(),
            Act::Build(k) => format!("Build {}", ed.buildings[k].name),
            Act::Close(k) => format!("Close idle {}", ed.buildings[k].name),
            Act::Road(_) => "Lay roads where goods are stuck".to_string(),
            Act::Port(_) => "Build a bigger port".to_string(),
            Act::BanExports(g) => format!("Stop {} exports", ed.goods[g].name.to_lowercase()),
            Act::LiftBan(g) => format!("Trade {} freely again", ed.goods[g].name.to_lowercase()),
            Act::Embargo(j) => format!("Embargo {}", e.nations[j].name),
            Act::LiftEmbargo(j) => format!("Lift the embargo on {}", e.nations[j].name),
        }
    }
}

/// What the brain sees of its own nation this week (all of it is its own).
struct Look {
    /// Each good's shortage (0 to 1) and how much that good matters.
    short: Vec<f64>,
    importance: Vec<f64>,
    capacity: Vec<f64>,
    /// Debt over the current rating's ceiling.
    debt_use: f64,
    gdp_year: f64,
    /// The most choked province and its transport.
    choked: Option<(usize, f64)>,
}

fn look(ai: &AiData, e: &Economy, ed: &EconData, i: usize) -> Look {
    let n = &e.nations[i];
    let goods = ed.goods.len();
    let input_goods: Vec<bool> = (0..goods).map(|g| ed.buildings.iter().any(|b| b.inputs.iter().any(|&(x, _)| x == g))).collect();
    let mut short = vec![0.0; goods];
    let mut importance = vec![0.0; goods];
    for g in 0..goods {
        let im = &ai.importance;
        importance[g] = if g == ed.food || g == ed.grain {
            im.food
        } else if g == ed.electricity {
            im.electricity
        } else if g == ed.consumer_goods {
            im.consumer_goods
        } else if g == ed.fuel {
            im.fuel
        } else if ed.goods[g].military {
            im.military
        } else if input_goods[g] {
            im.inputs
        } else {
            0.3 * im.inputs
        };
        let got = if n.wanted[g] > EPS { 1.0 - n.got[g] / n.wanted[g] } else { 0.0 };
        let cover = ed.prices.days_of_cover * n.use_rate[g];
        let stock = if cover > EPS && ed.goods[g].spoil < 1.0 { 1.0 - n.stock[g] / (0.5 * cover) } else { 0.0 };
        short[g] = got.max(0.5 * stock).clamp(0.0, 1.0);
    }
    let ceiling = ed.ratings[n.rating].ceiling.max(EPS);
    // The same aim as the player's road and port keys.
    let choked = e.most_choked(i).filter(|&q| e.provinces[q].transport < ai.needs.choked).map(|q| (q, e.provinces[q].transport));
    Look {
        short,
        importance,
        capacity: e.capacity(ed, i),
        debt_use: n.debt_ratio(ed) / ceiling,
        gdp_year: (n.gdp_local * ed.days_per_year as f64).max(EPS),
        choked,
    }
}

/// How urgent each need is (0 to 1), and each in words.
fn needs(brain: &Brain, ai: &AiData, e: &Economy, ed: &EconData, l: &Look) -> (NeedVec, Vec<String>) {
    let nd = &ai.needs;
    let n = &e.nations[brain.nation];
    let mut v = [0.0; 9];
    let mut w = vec![String::new(); NEEDS.len()];
    let u = n.unemployment(ed);
    let natural = interpolate(&ed.natural_unemployment_by_hdi, n.hdi);
    v[JOBS] = ((u - natural) / nd.jobs_span).clamp(0.0, 1.0);
    w[JOBS] = format!("{:.0}% out of work (normal {:.0}%)", u * 100.0, natural * 100.0);
    v[HEAT] = ((n.inflation - nd.heat_from) / nd.heat_span).clamp(0.0, 1.0);
    v[CHILL] = ((nd.chill_from - n.inflation) / nd.chill_span).clamp(0.0, 1.0);
    w[HEAT] = format!("prices up {:.1}% a year", n.inflation * 100.0);
    w[CHILL] = format!("prices {} {:.1}% a year", if n.inflation < 0.0 { "down" } else { "up" }, n.inflation.abs() * 100.0);
    let deficit = ((n.deficit_ema - nd.deficit_ok) / nd.deficit_span).clamp(0.0, 1.0);
    let debt = ((l.debt_use - nd.debt_ok) / (1.0 - nd.debt_ok).max(EPS)).clamp(0.0, 1.0);
    // The rating agencies' own formula (docs/ECONOMY.md): how near the
    // next downgrade towards junk is.
    let score = 100.0 - 60.0 * n.debt_ratio(ed) - 300.0 * n.deficit_ema.max(0.0) - 80.0 * (n.inflation - 0.05).max(0.0) + 0.2 * (n.stability - 50.0) + n.mods.get("rating");
    let rating = ((nd.rating_safe - score) / nd.rating_span.max(EPS)).clamp(0.0, 1.0);
    let mut d = deficit.max(debt).max(rating);
    if n.rating >= 4 {
        d = d.max(nd.junk);
    }
    let printing = n.printed_last_month > 0.0 || n.printed_month > 0.0;
    if printing {
        d = d.max(0.8);
    }
    // An emergency: printing money to pay a deep deficit. It outweighs every
    // other need (the treasury's autopilot answers it with hard austerity).
    if printing && n.deficit_ema > nd.deficit_ok + nd.deficit_span {
        d = 1.0 + nd.emergency;
    }
    v[DEBT] = d;
    w[DEBT] = format!(
        "{} rating, debt {:.0}% of GDP, deficit {:.1}%{}",
        ed.ratings[n.rating].name,
        n.debt_ratio(ed) * 100.0,
        n.deficit_ema * 100.0,
        if n.printed_last_month > 0.0 { ", printing money" } else { "" }
    );
    let mut worst = 0;
    for g in 0..l.short.len() {
        if l.short[g] * l.importance[g] > l.short[worst] * l.importance[worst] {
            worst = g;
        }
    }
    v[SUPPLY] = (l.short[worst] * l.importance[worst]).clamp(0.0, 1.0);
    w[SUPPLY] = if v[SUPPLY] > 0.01 { format!("short of {} ({:.0}%)", ed.goods[worst].name.to_lowercase(), l.short[worst] * 100.0) } else { "nothing short".to_string() };
    let growth = n.real_growth * ed.days_per_year as f64;
    v[GROWTH] = nd.growth_floor + (1.0 - nd.growth_floor) * ((nd.growth_ok - growth) / nd.growth_ok.max(EPS)).clamp(0.0, 1.0);
    w[GROWTH] = format!("growing {:.1}% a year", growth * 100.0);
    v[SUPPORT] = ((nd.support_ok - n.stability) / nd.support_span).clamp(0.0, 1.0);
    w[SUPPORT] = format!("stability {:.0}", n.stability);
    let arms_gap = if brain.personality.t.arms_floor > 0.0 { (1.0 - n.arms_share(ed) / brain.personality.t.arms_floor).clamp(0.0, 1.0) } else { 0.0 };
    let mut sec = brain.threat_level.max(arms_gap);
    if brain.goal == 2 {
        sec = sec.max(nd.dominate_floor);
    }
    v[SECURITY] = sec;
    w[SECURITY] = match brain.threat {
        Some(t) if brain.threat_level > 0.05 => format!("{} looks threatening", e.nations[t].name),
        _ if arms_gap > 0.0 => "the army wants more arms".to_string(),
        _ => "no threat in sight".to_string(),
    };
    let used = if n.trade_capacity > EPS { n.trade_used / n.trade_capacity } else { 0.0 };
    let full = ((used - nd.trade_full) / (1.0 - nd.trade_full).max(EPS)).clamp(0.0, 1.0);
    let stuck = l.choked.map_or(0.0, |(_, t)| ((nd.choked - t) / nd.choked.max(EPS) * 4.0).clamp(0.0, 1.0));
    v[TRADE] = full.max(stuck);
    w[TRADE] = if stuck > full { "goods stuck on bad roads".to_string() } else { format!("{:.0}% of its trade room used", used * 100.0) };
    for (c, x) in v.iter_mut().enumerate() {
        let cap = if c == DEBT { 1.0 + nd.emergency } else { 1.0 };
        *x = (*x * brain.personality.t.need[c]).clamp(0.0, cap);
    }
    (v, w)
}

/// Every action worth weighing this week, with its effects for the situation.
fn candidates(brain: &Brain, ai: &AiData, dif: &Difficulty, e: &Economy, ed: &EconData, l: &Look) -> Vec<(Act, NeedVec)> {
    let i = brain.nation;
    let n = &e.nations[i];
    let p = &brain.personality.t;
    let mut out: Vec<(Act, NeedVec)> = Vec::new();
    let base = |a: Act| ai.effect(a.effect_key());
    let cost_harm = |cost: f64| (cost / l.gdp_year * ai.cost_harm).min(1.0);
    let room = l.debt_use < 1.0 - ai.min_room;

    // Taxes and welfare: one point at a time, between the personality's
    // floor and the data's ceilings. A tax's help with the debt is what a
    // point of it brought in last month (its own books), so the brain raises
    // the taxes that pay; the higher a tax already is, the more another
    // point hurts (and a cut helps): taxes don't ratchet to the ceiling.
    let bl = &n.budget_last_month;
    let month_gdp = (n.gdp_local * 30.0).max(EPS);
    for lever in [Lever::IncomeTax, Lever::CorporateTax, Lever::SalesTax, Lever::Welfare, Lever::Tariff] {
        let v = e.lever_with(i, lever, Some(ed));
        let (lo, hi) = Economy::lever_range(lever);
        let (floor, ceiling, line) = match lever {
            Lever::IncomeTax => (p.tax_floor, ai.income_tax_max, bl.income_tax),
            Lever::CorporateTax => (p.tax_floor, ai.corporate_tax_max, bl.corporate_tax),
            Lever::SalesTax => (0.0, ai.sales_tax_max, bl.sales_tax),
            Lever::Welfare => (ai.welfare_floor, ai.welfare_max, bl.welfare),
            _ => (0.0, ai.tariff_max, bl.tariffs),
        };
        let (floor, ceiling) = (floor.max(lo), ceiling.min(hi));
        // A point's worth, as a share of GDP, over the reference point.
        let point = if v > 0.005 { line / v * 0.01 / month_gdp } else { ai.yield_ref };
        let pays = (point / ai.yield_ref.max(EPS)).clamp(0.1, 2.5);
        let level = match lever {
            Lever::Tariff => 1.0,
            Lever::Welfare => 0.5 + v / ai.welfare_level.max(EPS),
            _ => 0.5 + v / ai.tax_level.max(EPS),
        };
        if v < ceiling - EPS {
            let mut eff = base(Act::Lever(lever, 1));
            eff.iter_mut().filter(|x| **x < 0.0).for_each(|x| *x = (*x * level).max(-1.0));
            eff[DEBT] = (eff[DEBT] * pays).clamp(-1.0, 1.0);
            out.push((Act::Lever(lever, 1), eff));
        }
        if v > floor + EPS {
            let mut eff = base(Act::Lever(lever, -1));
            eff.iter_mut().filter(|x| **x > 0.0).for_each(|x| *x = (*x * level).min(1.0));
            eff[DEBT] = (eff[DEBT] * pays).clamp(-1.0, 1.0);
            out.push((Act::Lever(lever, -1), eff));
        }
    }

    // The interest rate: the central bank's job. A brain fixes it only to
    // fight a slump the bank isn't fighting (no inflation, many out of work)
    // or runaway prices, and hands it back once that has passed.
    let rate = n.manual_rate.unwrap_or(n.rate);
    let nd = &brain.needs;
    if nd[JOBS] > ai.rate_from && nd[HEAT] <= EPS && rate > 0.005 {
        out.push((Act::Lever(Lever::InterestRate, -2), base(Act::Lever(Lever::InterestRate, -2))));
    }
    if nd[HEAT] > ai.rate_from && rate < 0.4 {
        out.push((Act::Lever(Lever::InterestRate, 2), base(Act::Lever(Lever::InterestRate, 2))));
    }
    if n.manual_rate.is_some() {
        let mut eff = base(Act::RateAuto);
        // Worth more the further the reason for fixing it has gone.
        let calm = 1.0 - nd[JOBS].max(nd[HEAT]) / ai.rate_from.max(EPS);
        eff[HEAT] *= calm.max(0.0) * 2.0;
        eff[CHILL] *= calm.max(0.0) * 2.0;
        out.push((Act::RateAuto, eff));
    }

    // Research: worth more the further the brain looks, less the more there is.
    let horizon = p.horizon_years * dif.horizon / ai.research_horizon_ref.max(EPS);
    let research_max = p.research_max;
    if n.research < research_max - EPS {
        let mut eff = base(Act::Lever(Lever::Research, 1));
        eff[GROWTH] = (eff[GROWTH] * horizon * p.research_belief * (1.0 - n.research / research_max.max(EPS))).clamp(-1.0, 1.0);
        out.push((Act::Lever(Lever::Research, 1), eff));
    }
    if n.research > EPS {
        let mut eff = base(Act::Lever(Lever::Research, -1));
        eff[GROWTH] = (eff[GROWTH] * horizon * p.research_belief).clamp(-1.0, 1.0);
        out.push((Act::Lever(Lever::Research, -1), eff));
    }

    // Arms.
    let arms = n.arms_share(ed);
    if arms < ai.arms_max - EPS && brain.needs[SECURITY] > EPS {
        let mut eff = base(Act::Lever(Lever::Arms, 2));
        eff[SECURITY] *= 1.0 - arms / ai.arms_max;
        out.push((Act::Lever(Lever::Arms, 2), eff));
    }
    if arms > p.arms_floor + EPS {
        out.push((Act::Lever(Lever::Arms, -2), base(Act::Lever(Lever::Arms, -2))));
    }

    // Buildings: what is short, or sells abroad for well over its cost.
    let dpy = ed.days_per_year as f64;
    let mut builds: Vec<(f64, Act, NeedVec)> = Vec::new();
    if room {
        for (k, bd) in ed.buildings.iter().enumerate() {
            let g = bd.output;
            if g == ed.jets && n.hdi < 0.6 {
                continue;
            }
            let demand = n.use_rate[g] + n.export_rate[g];
            let short = l.short[g];
            // Nothing to feed it with, or its kind already idle for want of inputs.
            if bd.inputs.iter().any(|&(x, _)| n.stock[x] <= EPS && n.ledger.produced[x] + n.ledger.imported[x] <= EPS) {
                continue;
            }
            let existing: Vec<&crate::econ::Building> = n.buildings.iter().filter(|b| b.kind == k && b.levels > 0.0).collect();
            let levels: f64 = existing.iter().map(|b| b.levels).sum();
            if levels > 0.5 && existing.iter().map(|b| b.fed * b.levels).sum::<f64>() / levels < 0.8 {
                continue;
            }
            let (_, cost) = e.build_block(ed, i, k);
            let block = e.build_block(ed, i, k).0;
            // What a level earns (its kind's average, or what one would make).
            let per_level = if levels > 0.5 {
                existing.iter().map(|b| b.profit * b.levels).sum::<f64>() / levels
            } else {
                0.0
            };
            let roi = per_level * dpy * block / cost.max(EPS);
            // Selling abroad: the world pays well over what it costs to make here.
            let (_, export_price) = crate::econ::tick::parity(ed, n, g, e.world_price[g]);
            let exportable = n.unit_cost[g] > EPS && export_price > ai.export_margin * n.unit_cost[g] && n.trade_policy[g] == TradePolicy::Free;
            // Enough already (or coming): a shortage with enough capacity is
            // a problem upstream (inputs, workers, roads), not more buildings.
            if l.capacity[g] > ai.build_margin * demand.max(EPS) {
                continue;
            }
            // Not short: only a proven earner that sells abroad.
            if short < 0.05 && !(exportable && roi > ai.roi_min) {
                continue;
            }
            // The building goods must be in store (the player's build button says no otherwise).
            if bd.build_goods.iter().any(|&(x, per)| per > 0.0 && n.stock[x] < per * 0.5) {
                continue;
            }
            // One order of a kind at a time: wait to see what the last one does.
            if n.construction.iter().any(|c| c.kind == k && c.by_player) {
                continue;
            }
            if e.best_place(ed, i, k).is_none() {
                continue;
            }
            let mut eff = base(Act::Build(k));
            eff[SUPPLY] = (short * l.importance[g] * 1.5).min(1.0);
            eff[GROWTH] = (roi / ai.roi_full.max(EPS) * dif.horizon).clamp(-0.5, 1.0);
            eff[DEBT] = (eff[DEBT] - cost_harm(cost)).max(-1.0);
            if ed.goods[g].military {
                eff[SECURITY] = 0.2;
            }
            builds.push((eff[SUPPLY] + eff[GROWTH], Act::Build(k), eff));
        }
    }
    builds.sort_by(|a, b| b.0.total_cmp(&a.0));
    out.extend(builds.into_iter().take(6).map(|(_, a, eff)| (a, eff)));

    // Closing: the brain's own buildings idle and losing money for a season.
    let mut seen = Vec::new();
    for b in &n.buildings {
        if !b.player_built || seen.contains(&b.kind) {
            continue;
        }
        let g = ed.buildings[b.kind].output;
        if b.losing_days >= ai.close_days && b.utilization <= ai.close_idle + EPS && l.short[g] < 0.02 {
            seen.push(b.kind);
            out.push((Act::Close(b.kind), base(Act::Close(b.kind))));
        }
    }

    // Roads where goods are stuck; a port where trade is full.
    if let Some((q, t)) = l.choked {
        let pr = &e.provinces[q];
        if pr.road < 6.0 || pr.rail < 8.0 {
            let mut eff = base(Act::Road(q));
            let how = ((ai.needs.choked - t) / ai.needs.choked.max(EPS) * 4.0).clamp(0.0, 1.0);
            eff[GROWTH] *= how;
            eff[TRADE] *= how;
            let cost = if pr.road < 6.0 { ed.state.road_cost } else { ed.state.rail_cost } * pr.hexes / n.exchange;
            eff[DEBT] = (eff[DEBT] - cost_harm(cost)).max(-1.0);
            out.push((Act::Road(q), eff));
        }
    }
    let used = if n.trade_capacity > EPS { n.trade_used / n.trade_capacity } else { 0.0 };
    if used > ai.needs.trade_full {
        if let Some(q) = e.smallest_port(i) {
            let mut eff = base(Act::Port(q));
            eff[DEBT] = (eff[DEBT] - cost_harm(ed.state.port_cost / n.exchange)).max(-1.0);
            out.push((Act::Port(q), eff));
        }
    }

    // Trade policy: keep scarce goods at home; free them once there's enough.
    for g in 0..ed.goods.len() {
        match n.trade_policy[g] {
            TradePolicy::Free => {
                if l.short[g] > 0.1 && n.export_rate[g] > EPS {
                    let mut eff = base(Act::BanExports(g));
                    eff[SUPPLY] *= (l.short[g] * l.importance[g] * 2.0).min(1.0);
                    out.push((Act::BanExports(g), eff));
                }
            }
            _ => {
                // Freed only once the store is full again (else the shortage
                // comes straight back and the ban with it).
                if l.short[g] < 0.03 && n.stock[g] >= n.stock_target(ed, g) {
                    out.push((Act::LiftBan(g), base(Act::LiftBan(g))));
                }
            }
        }
    }

    // Embargoes: on whoever it plans against, if it's the kind to; lifted
    // when that has passed.
    let monthly = l.gdp_year * n.exchange / 12.0;
    for j in 0..e.nations.len() {
        if j == i {
            continue;
        }
        let trade = (n.exports_to_last[j] + n.imports_from_last[j]) / monthly.max(EPS);
        let hostile = brain.relations[j].regard < -0.5;
        if !n.embargo[j] && p.embargo > 0.0 && brain.counter == Some(j) && brain.threat_level > ai.embargo_threat {
            let mut eff = base(Act::Embargo(j));
            eff[SECURITY] *= (brain.threat_level + p.embargo).min(1.0);
            eff[GROWTH] *= (trade * 5.0).min(1.0);
            eff[TRADE] *= (trade * 5.0).min(1.0);
            out.push((Act::Embargo(j), eff));
        }
        if n.embargo[j] && brain.counter != Some(j) && !hostile {
            out.push((Act::LiftEmbargo(j), base(Act::LiftEmbargo(j))));
        }
    }
    out
}

/// The score of an action's effects for this brain's goal and needs.
fn score(brain: &Brain, ai: &AiData, eff: &NeedVec) -> (f64, [f64; 9]) {
    let w = &ai.goal_weights[brain.goal];
    let mut parts = [0.0; 9];
    for c in 0..9 {
        let e = eff[c];
        if e == 0.0 {
            continue;
        }
        let weight = if e > 0.0 { brain.needs[c] } else { brain.needs[c].max(ai.harm_floor) };
        let mut part = w[c] * weight * e;
        if c == DEBT && e < 0.0 {
            part *= 1.0 - brain.personality.t.risk;
        }
        parts[c] = part;
    }
    (parts.iter().sum(), parts)
}

fn normal(brain: &mut Brain) -> f64 {
    let u1 = (brain.rng.next_f32() as f64).max(1e-6);
    let u2 = brain.rng.next_f32() as f64;
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// The week's planning.
pub fn plan(brain: &mut Brain, ai: &AiData, dif: &Difficulty, e: &mut Economy, ed: &EconData) {
    let i = brain.nation;
    let day = e.day;
    let l = look(ai, e, ed, i);
    let (now, words) = needs(brain, ai, e, ed, &l);
    // Needs are smoothed over a few weeks (a bad week isn't a trend), except
    // the first week and a debt emergency, which count at once.
    let first = brain.thinking.day == 0 && brain.log.is_empty();
    let mut needs = brain.needs;
    for c in 0..needs.len() {
        needs[c] = if first || (c == DEBT && now[c] > 1.0) { now[c] } else { needs[c] + (now[c] - needs[c]) * ai.need_smoothing };
    }
    brain.needs = needs;
    brain.need_words = words;
    check_builds(brain, e, ed);

    let mut considered: Vec<Considered> = Vec::new();
    for (act, eff) in candidates(brain, ai, dif, e, ed, &l) {
        let (mut s, parts) = score(brain, ai, &eff);
        s += brain.personality.t.like[act.category()];
        let mut flip = false;
        if let Some((key, dir)) = act.moves() {
            if let Some(&(_, when, last)) = brain.moves.iter().find(|m| m.0 == key) {
                let age = day.saturating_sub(when);
                // Patience: a lever just moved waits to show what it did.
                if age < ai.patience_days && key < 100 {
                    continue;
                }
                // Nor undone before its effect has had time to show.
                if last != dir && dir != 0 && age < ai.undo_days {
                    continue;
                }
                if last != dir && dir != 0 && age < ai.flipflop_days {
                    s -= ai.flipflop_penalty * (1.0 - age as f64 / ai.flipflop_days as f64);
                    flip = true;
                }
            }
        }
        let noise = dif.noise + brain.personality.t.noise;
        let rank = if noise > 0.0 { s + noise * normal(brain) } else { s };
        let mut idx: Vec<usize> = (0..9).collect();
        idx.sort_by(|&a, &b| parts[b].abs().total_cmp(&parts[a].abs()).then(a.cmp(&b)));
        let reasons = idx
            .iter()
            .take(2)
            .filter(|&&c| parts[c].abs() > 0.005)
            .map(|&c| {
                let words = match act {
                    Act::Build(k) if c == SUPPLY => {
                        let g = ed.buildings[k].output;
                        format!("short of {} ({:.0}%)", ed.goods[g].name.to_lowercase(), l.short[g] * 100.0)
                    }
                    _ => brain.need_words[c].clone(),
                };
                format!("{} {} ({words})", if parts[c] > 0.0 { "helps" } else { "hurts" }, NEEDS[c])
            })
            .collect();
        let _ = flip;
        considered.push(Considered { act, label: act.label(e, ed), score: s, rank, reasons });
    }
    considered.sort_by(|a, b| b.rank.total_cmp(&a.rank).then(a.label.cmp(&b.label)));

    // The best of each kind, if it beats doing nothing, up to the week's limit.
    let mut taken_cats = [false; CATEGORIES.len()];
    let mut chosen: Vec<Considered> = Vec::new();
    for c in &considered {
        if chosen.len() >= dif.max_actions {
            break;
        }
        if c.score <= ai.hold_score || taken_cats[c.act.category()] {
            continue;
        }
        taken_cats[c.act.category()] = true;
        chosen.push(c.clone());
    }
    let mut taken = Vec::new();
    for c in chosen {
        let result = apply(e, ed, i, c.act);
        let mut flip = false;
        let mut quick = false;
        if let Some((key, dir)) = c.act.moves() {
            if let Some(m) = brain.moves.iter_mut().find(|m| m.0 == key) {
                flip = m.2 != dir && dir != 0 && day.saturating_sub(m.1) < ai.flipflop_days;
                quick = flip && day.saturating_sub(m.1) < QUICK_DAYS;
                *m = (key, day, dir);
            } else {
                brain.moves.push((key, day, dir));
            }
        }
        if result.is_ok() {
            brain.stats.taken += 1;
            *brain.stats.taken_by.entry(c.act.effect_key()).or_insert(0) += 1;
            if quick {
                brain.stats.quick_flips += 1;
            }
            if flip {
                brain.stats.flips += 1;
                *brain.stats.flips_by.entry(c.act.effect_key()).or_insert(0) += 1;
            }
            if let Act::Build(k) = c.act {
                brain.stats.builds += 1;
                brain.stats.checks.push((day + ed.days_per_year, k));
            }
        } else {
            brain.stats.failed += 1;
        }
        taken.push(match &result {
            Ok(s) => format!("{}: {s}", c.label),
            Err(s) => format!("{}: couldn't ({s})", c.label),
        });
        brain.log.push(Decision { day, act: c.act, label: c.label.clone(), score: c.score, flip, result });
        if brain.log.len() > 200 {
            brain.log.drain(..50);
        }
    }
    // For the spectator: the options by how much they help.
    considered.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.label.cmp(&b.label)));
    considered.truncate(6);
    brain.thinking = super::Thinking { day, options: considered, taken };
}

/// A year after building, is anybody using it? (For the tuning tool.)
fn check_builds(brain: &mut Brain, e: &Economy, ed: &EconData) {
    let n = &e.nations[brain.nation];
    let day = e.day;
    let mut k = 0;
    while k < brain.stats.checks.len() {
        let (when, kind) = brain.stats.checks[k];
        if when > day {
            k += 1;
            continue;
        }
        brain.stats.checks.swap_remove(k);
        let mine: Vec<&crate::econ::Building> = n.buildings.iter().filter(|b| b.kind == kind && b.player_built && b.levels > 0.0).collect();
        let levels: f64 = mine.iter().map(|b| b.levels).sum();
        let busy = if levels > EPS { mine.iter().map(|b| b.utilization * b.fed.min(1.0) * b.levels).sum::<f64>() / levels } else { 0.0 };
        let _ = ed;
        if busy < 0.5 {
            brain.stats.unneeded += 1;
        }
    }
}

/// Does it, through the same functions the player's buttons call.
fn apply(e: &mut Economy, ed: &EconData, i: usize, act: Act) -> Result<String, String> {
    match act {
        Act::Lever(l, s) => Ok(e.pull_with(Some(ed), i, l, s)),
        Act::RateAuto => Ok(e.free_rate(i)),
        Act::Build(k) => e.player_build(ed, i, k),
        Act::Close(k) => e.player_close(ed, i, k),
        Act::Road(q) => e.player_road(ed, i, q),
        Act::Port(q) => e.player_port(ed, i, q),
        Act::BanExports(g) => Ok(e.set_trade_policy(ed, i, g, TradePolicy::NoExports)),
        Act::LiftBan(g) => Ok(e.set_trade_policy(ed, i, g, TradePolicy::Free)),
        Act::Embargo(j) | Act::LiftEmbargo(j) => Ok(e.toggle_embargo(i, j)),
    }
}
