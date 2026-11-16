//! The economy report: how each nation is doing, in a table.
//!
//! `cargo run --release -- --economy-report [YEARS] [--map grand|isles|five_shores] [--world-seed N]`

use super::init::base_name;
use super::{EconData, Economy};

fn money(w: f64) -> String {
    if w.abs() >= 1e12 {
        format!("${:.2}T", w / 1e12)
    } else if w.abs() >= 1e9 {
        format!("${:.1}B", w / 1e9)
    } else if w.abs() >= 1e6 {
        format!("${:.0}M", w / 1e6)
    } else {
        format!("${:.0}k", w / 1e3)
    }
}

fn people(p: f64) -> String {
    if p >= 1e9 {
        format!("{:.2}B", p / 1e9)
    } else if p >= 1e6 {
        format!("{:.1}M", p / 1e6)
    } else {
        format!("{:.0}k", p / 1e3)
    }
}

/// One line per nation, plus the world's prices.
pub fn report(e: &Economy, ed: &EconData) -> String {
    let years = e.day as f64 / ed.days_per_year as f64;
    let mut out = String::new();
    out.push_str(&format!("LUCKY WORLD economy report: {} ({:.1} years)\n\n", e.date(ed), years));
    out.push_str(&format!(
        "{:<30} {:<16} {:>8} {:>10} {:>10} {:>7} {:>8} {:>6} {:>6} {:>6} {:>5} {:>5} {:>9}  {}\n",
        "Nation", "Model", "People", "GDP start", "GDP now", "Growth", "Per head", "Infl.", "Unemp", "Debt", "Rate", "Stab", "Trade/yr", "Top export"
    ));
    out.push_str(&format!("{}\n", "-".repeat(160)));
    for n in &e.nations {
        let growth = if years > 0.1 { ((n.gdp / n.gdp_start).powf(1.0 / years) - 1.0) * 100.0 } else { 0.0 };
        let top = n
            .ledger
            .exported
            .iter()
            .enumerate()
            .filter(|(_, x)| **x > 0.0)
            .max_by(|a, b| (a.1 * e.world_price[a.0]).total_cmp(&(b.1 * e.world_price[b.0])))
            .map(|(g, _)| ed.goods[g].name.clone())
            .unwrap_or_else(|| "-".to_string());
        let name: String = n.name.chars().take(30).collect();
        out.push_str(&format!(
            "{:<30} {:<16} {:>8} {:>10} {:>10} {:>6.1}% {:>8} {:>5.1}% {:>5.1}% {:>5.0}% {:>5} {:>5.0} {:>9}  {}\n",
            name,
            ed.models[n.model].name,
            people(n.population),
            money(n.gdp_start),
            money(n.gdp),
            growth,
            format!("${:.0}", n.gdp_per_person()),
            n.inflation * 100.0,
            n.unemployment(ed) * 100.0,
            n.debt_ratio(ed) * 100.0,
            ed.ratings[n.rating].name,
            n.stability,
            money(n.balance_ema * ed.days_per_year as f64),
            top,
        ));
    }
    out.push_str("\nWorld prices (W$ per unit, start -> now):\n");
    for (g, good) in ed.goods.iter().enumerate() {
        out.push_str(&format!(
            "  {:<15} {:>10.0} -> {:>10.0} ({:+.0}%)\n",
            good.name,
            e.base_world_price[g],
            e.world_price[g],
            (e.world_price[g] / e.base_world_price[g] - 1.0) * 100.0
        ));
    }
    let total: f64 = e.nations.iter().map(|n| n.gdp).sum();
    let start: f64 = e.nations.iter().map(|n| n.gdp_start).sum();
    out.push_str(&format!(
        "\nWorld GDP {} -> {} ({:+.1}% a year), world inflation {:.1}%\n",
        money(start),
        money(total),
        if years > 0.1 { ((total / start).powf(1.0 / years) - 1.0) * 100.0 } else { 0.0 },
        e.world_inflation * 100.0
    ));
    let recent: Vec<&super::News> = e.news.iter().rev().take(8).collect();
    if !recent.is_empty() {
        out.push_str("\nLatest news:\n");
        for item in recent.iter().rev() {
            let who = item.nation.map_or("World".to_string(), |i| e.nations[i].name.clone());
            out.push_str(&format!("  day {:>5}  {:<30} {}\n", item.day, who, item.text));
        }
    }
    out
}

/// One nation's key numbers, for following a run day by day.
/// The CPI taken apart: each part's price against day 1 (local money), the
/// exchange rate against day 1, and the world prices of the CPI goods.
pub fn cpi_line(e: &Economy, ed: &EconData, i: usize) -> String {
    let n = &e.nations[i];
    let rel = |g: usize| n.price[g] * n.exchange_start / ed.goods[g].price;
    let world = |g: usize| e.world_price[g] / ed.goods[g].price;
    format!(
        "d{:>5} cpi {:.3} infl {:>6.1}% | food {:.3} goods {:.3} fuel {:.3} power {:.3} wage {:.3} | e/e0 {:.3} | world food {:.3} goods {:.3} fuel {:.3} | rate {:>4.1}% m {:.3} u {:>4.1}%",
        e.day,
        n.cpi,
        n.inflation * 100.0,
        rel(ed.food),
        rel(ed.consumer_goods),
        rel(ed.fuel),
        rel(ed.electricity),
        n.wage / n.wage0.max(1e-12),
        n.exchange / n.exchange_start.max(1e-12),
        world(ed.food),
        world(ed.consumer_goods),
        world(ed.fuel),
        n.rate * 100.0,
        n.money / n.money_start.max(1e-12),
        n.unemployment(ed) * 100.0,
    )
}

pub fn trace_line(e: &Economy, ed: &EconData, i: usize) -> String {
    let n = &e.nations[i];
    let x = n.exchange;
    format!(
        "d{:>4} pop {:>7.0}k hh {:>9} profit {:>9} sav {:>9} gdp {:>9} wage {:>7.2} cpi {:>6.3} infl {:>6.1}% unemp {:>5.1}% serv {:>7.0}k spend {:>9} food {:>4.2} goods {:>4.2} power {:>4.2} pfood {:>8.0} stockfood {:>8.0} treas {:>9} debt {:>9} print {:>8} r {:>4.1}% e {:.4} farms {:.0} mines {:.0} ind {:.0}",
        e.day,
        n.population / 1e3,
        money(n.household_spending * x * 360.0),
        money(n.profits * x * 360.0),
        money(n.savings * x),
        money(n.gdp_local * x * 360.0),
        n.wage * x,
        n.cpi,
        n.inflation * 100.0,
        n.unemployment(ed) * 100.0,
        n.services / 1e3,
        money(n.services_spending * x * 360.0),
        n.food_sat,
        n.goods_sat,
        n.power_sat,
        n.price[ed.food] * x,
        n.stock[ed.food],
        money(n.treasury * x),
        money(n.debt * x),
        money(n.printed * x * 360.0),
        n.rate * 100.0,
        x,
        n.farming / 1000.0,
        n.mining / 1000.0,
        n.industry / 1000.0,
    )
}

/// Every good's flows today and every building's state (for debugging a run).
pub fn detail(e: &Economy, ed: &EconData, i: usize) -> String {
    let n = &e.nations[i];
    let mut out = format!(
        "{} day {} tech {:.2} hdi {:.2} skilled {:.2} landlocked {} trade capacity {:.0} used {:.0} tariff {:.2}\n",
        n.name, e.day, super::tech(ed, n), n.hdi, n.skilled, n.landlocked, n.trade_capacity, n.trade_used, n.tariff
    );
    for (g, good) in ed.goods.iter().enumerate() {
        let l = &n.ledger;
        out.push_str(&format!(
            "  {:<15} stock {:>12.0} made {:>10.1} used {:>10.1} eaten {:>10.1} in {:>9.1} out {:>9.1} spoil {:>8.1} want {:>10.1} got {:>10.1} price {:>9.1} (world {:>8.0}) use {:>10.1}\n",
            good.name, n.stock[g], l.produced[g], l.used[g], l.consumed[g], l.imported[g], l.exported[g], l.spoiled[g], n.wanted[g], n.got[g],
            n.price[g] * n.exchange, e.world_price[g], n.use_rate[g]
        ));
    }
    for b in &n.buildings {
        let bd = &ed.buildings[b.kind];
        let p = &e.provinces[b.province];
        out.push_str(&format!(
            "  {:<24} prov {:>3} levels {:>9.1} hired {:>11.0} out {:>10.1} inputs {:>4.2} transport {:>4.2} rich {:>4.2} profit/lvl {:>10.2}\n",
            bd.name, b.province, b.levels, b.hired, b.output, b.input_factor, p.transport,
            if bd.deposit { p.deposits[bd.output] } else { 1.0 }, b.profit * n.exchange
        ));
    }
    out
}

/// The whole world's flows of each good today (made, used, traded, stock).
pub fn world_flows(e: &Economy, ed: &EconData) -> String {
    let mut out = String::from("World flows today:\n");
    for (g, good) in ed.goods.iter().enumerate() {
        let (mut made, mut used, mut traded, mut stock, mut want, mut got) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        for n in &e.nations {
            made += n.ledger.produced[g];
            used += n.ledger.used[g] + n.ledger.consumed[g] + n.ledger.stockpiled[g];
            traded += n.ledger.imported[g];
            stock += n.stock[g];
            want += n.wanted[g];
            got += n.got[g];
        }
        out.push_str(&format!(
            "  {:<15} made {:>11.0} used {:>11.0} traded {:>10.0} stock {:>13.0} want/got {:>5.2} world price {:>9.0}\n",
            good.name, made, used, traded, stock, if want > 0.0 { got / want } else { 1.0 }, e.world_price[g]
        ));
    }
    out
}

/// One good across every nation: stock (in days of use), price against the
/// world, export parity, and what was wanted and got (for debugging trade).
pub fn good_across(e: &Economy, ed: &EconData, g: usize) -> String {
    let mut out = format!("{} across nations (world price {:.0}):\n", ed.goods[g].name, e.world_price[g]);
    for n in &e.nations {
        let (ip, ep) = super::tick::parity(ed, n, g, e.world_price[g]);
        out.push_str(&format!(
            "  {:<30} stock {:>9.0} ({:>5.1} days) made {:>9.0} p/world {:>5.2} p/ep {:>5.2} p/ip {:>5.2} in {:>8.0} out {:>8.0} want {:>9.0} got {:>9.0} model {}\n",
            n.name.chars().take(30).collect::<String>(),
            n.stock[g],
            n.stock[g] / n.use_rate[g].max(1e-9),
            n.ledger.produced[g],
            n.price[g] * n.exchange / e.world_price[g],
            n.price[g] / ep,
            n.price[g] / ip,
            n.ledger.imported[g],
            n.ledger.exported[g],
            n.wanted[g],
            n.got[g],
            ed.models[n.model].name
        ));
    }
    out
}

/// Last month's budget, line by line (W$ per year), and taxes and debt.
pub fn budget(e: &Economy, ed: &EconData, i: usize) -> String {
    let n = &e.nations[i];
    let b = &n.budget_last_month;
    let y = |x: f64| money(x * n.exchange * 12.0);
    format!(
        "{} budget (W$/yr): income tax {} corporate {} sales {} tariffs {} tribute in {} corruption -{} | army {} welfare {} gov {} infra {} construction {} procurement {} interest {} tribute out {} | revenue {} spending {} | taxes {:.0}%/{:.0}%/{:.0}% welfare {:.0}% debt {:.0}% GDP {} rating {} printed this month {}\n",
        n.name, y(b.income_tax), y(b.corporate_tax), y(b.sales_tax), y(b.tariffs), y(b.tribute_in), y(b.corruption),
        y(b.army), y(b.welfare), y(b.government), y(b.infrastructure), y(b.construction), y(b.procurement), y(b.interest), y(b.tribute_out),
        y(b.revenue()), y(b.spending()),
        n.income_tax * 100.0, n.corporate_tax * 100.0, n.sales_tax * 100.0, n.welfare_rate * 100.0,
        n.debt_ratio(ed) * 100.0, money(n.gdp), ed.ratings[n.rating].name, money(n.printed_month * n.exchange)
    ) + &format!(
        "  today (W$/yr): income tax {} welfare {} gov {} army {} interest {} revenue {} spending {}\n",
        money(n.budget_today.income_tax * n.exchange * 360.0),
        money(n.budget_today.welfare * n.exchange * 360.0),
        money(n.budget_today.government * n.exchange * 360.0),
        money(n.budget_today.army * n.exchange * 360.0),
        money(n.budget_today.interest * n.exchange * 360.0),
        money(n.budget_today.revenue() * n.exchange * 360.0),
        money(n.budget_today.spending() * n.exchange * 360.0)
    ) + &format!(
        "  staff: government {:.0}k (public works {:.0}k), army {:.0}k, unemployed {:.0}k of {:.0}k; wage W${:.0}/yr; retrained +{:.1} pts\n",
        n.government / 1e3,
        n.public_works / 1e3,
        n.army / 1e3,
        n.unemployed / 1e3,
        n.population * ed.participation / 1e3,
        n.wage * n.exchange * 360.0,
        n.retrained * 100.0
    )
}

/// How healthy a run was, measured every month (for the balance rules and
/// their tests, and `--economy-health`).
#[derive(Debug, Clone, Default)]
pub struct Health {
    /// The highest GDP per head any nation reached (W$), and who.
    pub max_per_head: f64,
    pub max_per_head_nation: String,
    /// The longest run of years any nation spent above 25% unemployment.
    pub longest_jobless_years: f64,
    pub longest_jobless_nation: String,
    /// World inflation (GDP-weighted) at each year's end: lowest and highest.
    pub world_inflation_low: f64,
    pub world_inflation_high: f64,
    /// The most nations above 15% inflation in the same month, who they were
    /// (with their inflation) and when.
    pub most_high_inflation: usize,
    pub high_inflation_who: String,
    /// The year world inflation was furthest out, and the nations that
    /// weighed most in it (share of world GDP x inflation).
    pub world_inflation_who: String,
    /// Nations ending below a fifth of their starting GDP.
    pub collapsed: Vec<String>,
}

impl Health {
    /// The four balance rules: per head under 300k, no 5-year jobless spell,
    /// world inflation between -3% and 8% every year, at most 2 nations over 15%.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.max_per_head > 300_000.0 {
            out.push(format!("{}: {} per head", self.max_per_head_nation, money(self.max_per_head)));
        }
        if self.longest_jobless_years > 5.0 {
            out.push(format!("{}: {:.1} years over 25% unemployment", self.longest_jobless_nation, self.longest_jobless_years));
        }
        if self.world_inflation_low < -0.03 || self.world_inflation_high > 0.08 {
            out.push(format!("world inflation {:.1}% to {:.1}%", self.world_inflation_low * 100.0, self.world_inflation_high * 100.0));
        }
        if self.most_high_inflation > 2 {
            out.push(format!("{} nations over 15% inflation at once", self.most_high_inflation));
        }
        for n in &self.collapsed {
            out.push(format!("{n}: collapsed below a fifth of its GDP"));
        }
        out
    }
}

/// Runs an economy for `years`, watching it every month.
pub fn run_and_watch(e: &mut Economy, ed: &EconData, years: u32) -> Health {
    let mut h = Health { world_inflation_low: f64::MAX, world_inflation_high: f64::MIN, ..Health::default() };
    let mut jobless_since: Vec<Option<u32>> = vec![None; e.nations.len()];
    let month = ed.days_per_year / 12;
    for m in 1..=years * 12 {
        e.run_days(ed, month);
        let mut high = 0;
        for (i, n) in e.nations.iter().enumerate() {
            let per_head = n.gdp / n.population.max(1.0);
            if per_head > h.max_per_head {
                h.max_per_head = per_head;
                h.max_per_head_nation = n.name.clone();
            }
            if n.inflation > 0.15 {
                high += 1;
            }
            if n.unemployment(ed) > 0.25 {
                let since = *jobless_since[i].get_or_insert(m);
                let years = (m - since + 1) as f64 / 12.0;
                if years > h.longest_jobless_years {
                    h.longest_jobless_years = years;
                    h.longest_jobless_nation = n.name.clone();
                }
            } else {
                jobless_since[i] = None;
            }
        }
        if high > h.most_high_inflation {
            h.most_high_inflation = high;
            let who: Vec<String> = e.nations.iter().filter(|n| n.inflation > 0.15).map(|n| format!("{} {:.0}%", base_name(&n.name), n.inflation * 100.0)).collect();
            h.high_inflation_who = format!("year {:.1}: {}", m as f64 / 12.0, who.join(", "));
        }
        // World inflation is judged once a year has gone by (day 1 has no past).
        if m % 12 == 0 {
            let w = e.world_inflation;
            let worst = (w - 0.025).abs() > (h.world_inflation_high.max(-1.0) - 0.025).abs().max((h.world_inflation_low.min(1.0) - 0.025).abs()) || m == 12;
            h.world_inflation_low = h.world_inflation_low.min(w);
            h.world_inflation_high = h.world_inflation_high.max(w);
            if worst {
                let total: f64 = e.nations.iter().map(|n| n.gdp).sum::<f64>().max(1.0);
                let mut parts: Vec<(f64, String)> =
                    e.nations.iter().map(|n| (n.gdp / total * n.inflation, format!("{} {:.0}% x {:.0}%", base_name(&n.name), n.gdp / total * 100.0, n.inflation * 100.0))).collect();
                parts.sort_by(|a, b| b.0.abs().total_cmp(&a.0.abs()));
                h.world_inflation_who = format!("year {}: {:.1}%: {}", m / 12, w * 100.0, parts.iter().take(3).map(|p| p.1.clone()).collect::<Vec<_>>().join(", "));
            }
        }
    }
    h.collapsed = e.nations.iter().filter(|n| n.gdp < n.gdp_start / 5.0).map(|n| n.name.clone()).collect();
    h
}

/// The world in one line: world inflation, how many nations are over 15%,
/// world prices against their start, and the median nation's numbers.
pub fn world_line(e: &Economy, ed: &EconData) -> String {
    let mut infl: Vec<f64> = e.nations.iter().map(|n| n.inflation).collect();
    infl.sort_by(|a, b| a.total_cmp(b));
    let mut un: Vec<f64> = e.nations.iter().map(|n| n.unemployment(ed)).collect();
    un.sort_by(|a, b| a.total_cmp(b));
    let mut rate: Vec<f64> = e.nations.iter().map(|n| n.rate).collect();
    rate.sort_by(|a, b| a.total_cmp(b));
    let fx: Vec<f64> = e.nations.iter().map(|n| (n.exchange / n.exchange_start).ln()).collect();
    let fx_mean = fx.iter().sum::<f64>() / fx.len() as f64;
    let mg: f64 = e.nations.iter().map(|n| n.money_growth * 360.0).sum::<f64>() / e.nations.len() as f64;
    let rel = |g: usize| e.world_price[g] / e.base_world_price[g];
    let gdp: f64 = e.nations.iter().map(|n| n.gdp).sum();
    format!(
        "d{:>5} world infl {:>6.1}% | >15%: {:>2} | median infl {:>6.1}% unemp {:>5.1}% rate {:>5.1}% | fx mean {:>+6.3} money growth {:>5.1}% | food {:.2} goods {:.2} grain {:.2} coal {:.2} oil {:.2} steel {:.2} | GDP {}",
        e.day,
        e.world_inflation * 100.0,
        infl.iter().filter(|x| **x > 0.15).count(),
        infl[infl.len() / 2] * 100.0,
        un[un.len() / 2] * 100.0,
        rate[rate.len() / 2] * 100.0,
        fx_mean,
        mg * 100.0,
        rel(ed.food),
        rel(ed.consumer_goods),
        rel(ed.grain),
        rel(ed.good_index("coal").unwrap_or(0)),
        rel(ed.good_index("oil").unwrap_or(0)),
        rel(ed.steel),
        money(gdp)
    )
}
