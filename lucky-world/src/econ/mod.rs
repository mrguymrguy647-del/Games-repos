//! The economy (docs/ECONOMY.md): every nation's production, people, money
//! and state, and the world market between them, one day at a time.
//!
//! Nothing here draws. `Economy::tick` advances the whole world one day;
//! the screens read the state. The same world and seed always give the same
//! economy.

pub mod data;
mod init;
mod market;
pub mod player;
mod policy;
pub mod report;
pub mod tick;
#[cfg(test)]
mod tests;

use duckforge::prelude::Rng;

pub use data::EconData;
use data::Effects;

/// What a nation's government does with a good at the border.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradePolicy {
    Free,
    NoExports,
    NoImports,
}

/// One building kind in one province, with however many levels it has.
#[derive(Debug, Clone)]
pub struct Building {
    pub kind: usize,
    pub province: usize,
    pub levels: f64,
    pub open: bool,
    /// Workers hired today.
    pub hired: f64,
    /// Output today (after defects).
    pub output: f64,
    /// Inputs it got as a share of what it needed today (1: all).
    pub input_factor: f64,
    /// Inputs it has been getting, smoothed over a few weeks: it only keeps
    /// on the crews it can feed (the rest are let go to work elsewhere).
    pub fed: f64,
    /// Profit per level per day (local money), smoothed over about a month.
    pub profit: f64,
    /// Days in a row it has lost money.
    pub losing_days: u32,
    /// How much of its levels it runs (0.2 to 1): firms losing money cut back.
    pub utilization: f64,
    /// Built by the player (the autopilot leaves it alone).
    pub player_built: bool,
}

/// Levels being built.
#[derive(Debug, Clone)]
pub struct Construction {
    pub kind: usize,
    pub province: usize,
    pub levels: f64,
    pub days_left: u32,
    pub by_state: bool,
    pub by_player: bool,
}

/// A province's economy: deposits and infrastructure.
#[derive(Debug, Clone)]
pub struct ProvinceEcon {
    /// Richness of each good's deposit (0 for goods that aren't dug).
    pub deposits: Vec<f64>,
    pub hexes: f64,
    pub coastal: bool,
    pub road: f64,
    pub rail: f64,
    pub port: f64,
    /// Share of its nation's people living here.
    pub people: f64,
    /// What its buildings moved yesterday (weight units) and today's factor.
    pub tonnage: f64,
    pub transport: f64,
    /// Held by an enemy at war (docs/WAR.md, section 3), and the share of
    /// its output its owner still gets.
    pub occupied_by: Option<usize>,
    pub output_share: f64,
}

/// An event happening to a nation now.
#[derive(Debug, Clone)]
pub struct ActiveEvent {
    pub event: usize,
    pub days_left: u32,
}

/// A line in the news.
#[derive(Debug, Clone)]
pub struct News {
    pub day: u32,
    pub nation: Option<usize>,
    pub text: String,
    pub good: bool,
}

/// One budget, by line (local money per day, or summed over a period).
#[derive(Debug, Clone, Default)]
pub struct Budget {
    pub income_tax: f64,
    pub corporate_tax: f64,
    pub sales_tax: f64,
    pub tariffs: f64,
    pub tribute_in: f64,
    pub corruption: f64,
    pub army: f64,
    pub welfare: f64,
    pub government: f64,
    pub infrastructure: f64,
    pub construction: f64,
    pub procurement: f64,
    pub research: f64,
    pub interest: f64,
    pub tribute_out: f64,
}

impl Budget {
    pub fn revenue(&self) -> f64 {
        self.income_tax + self.corporate_tax + self.sales_tax + self.tariffs + self.tribute_in - self.corruption
    }

    pub fn spending(&self) -> f64 {
        self.army + self.welfare + self.government + self.infrastructure + self.construction + self.procurement + self.research + self.interest + self.tribute_out
    }

    pub fn add(&mut self, o: &Budget) {
        self.income_tax += o.income_tax;
        self.corporate_tax += o.corporate_tax;
        self.sales_tax += o.sales_tax;
        self.tariffs += o.tariffs;
        self.tribute_in += o.tribute_in;
        self.corruption += o.corruption;
        self.army += o.army;
        self.welfare += o.welfare;
        self.government += o.government;
        self.infrastructure += o.infrastructure;
        self.construction += o.construction;
        self.procurement += o.procurement;
        self.research += o.research;
        self.interest += o.interest;
        self.tribute_out += o.tribute_out;
    }
}

/// The goods ledger: where every unit came from and went (for the test
/// that goods are never made from nothing).
#[derive(Debug, Clone, Default)]
pub struct Ledger {
    pub produced: Vec<f64>,
    pub used: Vec<f64>,
    pub consumed: Vec<f64>,
    pub imported: Vec<f64>,
    pub exported: Vec<f64>,
    pub spoiled: Vec<f64>,
    pub destroyed: Vec<f64>,
    pub stockpiled: Vec<f64>,
}

impl Ledger {
    pub fn new(goods: usize) -> Ledger {
        let z = vec![0.0; goods];
        Ledger {
            produced: z.clone(),
            used: z.clone(),
            consumed: z.clone(),
            imported: z.clone(),
            exported: z.clone(),
            spoiled: z.clone(),
            destroyed: z.clone(),
            stockpiled: z,
        }
    }

    pub fn clear(&mut self) {
        for v in [
            &mut self.produced,
            &mut self.used,
            &mut self.consumed,
            &mut self.imported,
            &mut self.exported,
            &mut self.spoiled,
            &mut self.destroyed,
            &mut self.stockpiled,
        ] {
            v.iter_mut().for_each(|x| *x = 0.0);
        }
    }
}

/// Weekly samples for the charts.
#[derive(Debug, Clone, Default)]
pub struct History {
    pub day: Vec<u32>,
    /// GDP, W$ per year.
    pub gdp: Vec<f32>,
    pub inflation: Vec<f32>,
    pub unemployment: Vec<f32>,
    pub stability: Vec<f32>,
    pub debt: Vec<f32>,
    pub rate: Vec<f32>,
    /// Each good's local price.
    pub prices: Vec<Vec<f32>>,
    pub trade_balance: Vec<f32>,
}

/// One nation's economy.
#[derive(Debug, Clone)]
pub struct NationEcon {
    pub name: String,
    pub currency: String,
    pub symbol: String,
    /// Index into `EconData::models`.
    pub model: usize,
    /// Its wheels' and continent's modifiers.
    pub base_effects: Effects,
    /// Today's modifiers: base effects with active events on top.
    pub mods: Effects,
    /// Hot modifiers, read from `mods` whenever it changes (see `refresh_fast_mods`).
    pub mod_tech: f64,
    pub mod_defect: f64,
    pub mod_out: Vec<f64>,
    pub mod_event: Vec<f64>,
    pub mod_trade_cost: f64,
    /// Events began or ended today: `mods` is rebuilt.
    pub mods_dirty: bool,
    pub events: Vec<ActiveEvent>,
    pub overlord: Option<usize>,
    pub landlocked: bool,
    pub provinces: Vec<usize>,
    pub capital: usize,
    pub army_option: String,
    pub main_resource: Option<usize>,

    // People.
    pub population: f64,
    pub hdi: f64,
    /// Day 1's productivity fit: factories break even at the wheel's GDP.
    pub tech_cal: f64,
    /// Technical progress since day 1 (grows `tech_growth` a year).
    pub tech_progress: f64,
    /// People on public works (a slump programme), and the skilled share
    /// retraining has added.
    pub public_works: f64,
    pub retrained: f64,
    /// Day 1's appetite fit: the basket households can afford.
    pub need_cal: f64,
    pub duty: f64,
    pub skilled: f64,
    pub farming: f64,
    pub mining: f64,
    pub industry: f64,
    pub services: f64,
    pub government: f64,
    pub army: f64,
    pub unemployed: f64,
    pub food_sat: f64,
    pub goods_sat: f64,
    pub lux_sat: f64,
    pub power_sat: f64,
    pub happiness: f64,
    pub stability: f64,

    // Production.
    pub buildings: Vec<Building>,
    pub construction: Vec<Construction>,
    /// The order buildings hire in (by profit or by plan), refreshed monthly.
    pub hire_order: Vec<usize>,
    pub stock: Vec<f64>,
    /// Weapons put away for the war phase (by good; only military goods).
    pub stockpile: Vec<f64>,
    pub price: Vec<f64>,
    pub plan_price: Vec<f64>,
    /// What was wanted today and what was got (for prices and shortages).
    pub wanted: Vec<f64>,
    pub got: Vec<f64>,
    /// Smoothed daily use of each good (inputs, households, state).
    pub use_rate: Vec<f64>,
    /// Smoothed daily exports of each good (over about ten days).
    pub export_rate: Vec<f64>,
    /// What a unit of each good costs to make at home (inputs, wages and
    /// upkeep over output), smoothed over about a month; 0 if nobody makes it.
    pub unit_cost: Vec<f64>,
    pub ledger: Ledger,
    pub trade_policy: Vec<TradePolicy>,
    /// Raw goods it couldn't dig at home on day 1 (used once, at the start).
    pub raw_short: Vec<f64>,
    /// Power homes bought yesterday (industry may use the rest).
    pub home_power_bought: f64,

    // Money.
    pub wage: f64,
    pub wage0: f64,
    /// What factories can pay per worker, in W$ (smoothed): wages are pulled there.
    pub wage_anchor: f64,
    /// Day 1's wage over what the buildings could pay then: what they can pay
    /// later is scaled by it, so day 1 is a steady state for wages.
    pub pay_scale: f64,
    /// Inflation over the last three months, at a yearly rate (smoothed).
    pub inflation_recent: f64,
    /// Set when the state defaulted this month (for the news).
    pub defaulted: bool,
    /// What service work pays, as a share of the wage: 1 in a normal labour
    /// market, less when work is scarce (informal work: street stalls, odd
    /// jobs), so the same spending on services employs more people.
    pub service_pay: f64,
    /// The wage in W$ (the local wage is this over the exchange rate).
    pub wage_w: f64,
    /// Day 1's tech level (real wages grow with tech).
    pub tech_start: f64,
    /// Day 1's HDI and people (HDI drifts with income from there).
    pub hdi_start: f64,
    pub population_start: f64,
    pub money: f64,
    /// Day 1's money and real output: the currency's anchor.
    pub money_start: f64,
    pub real_start: f64,
    /// Real output measured in work: people at work times their tech (smoothed).
    pub real_supply: f64,
    pub rate: f64,
    pub manual_rate: Option<f64>,
    pub cpi: f64,
    pub cpi_ring: Vec<f64>,
    /// Goods prices alone (no services): what wages are indexed to, so wages don't chase themselves.
    pub goods_cpi: f64,
    pub goods_ring: Vec<f64>,
    pub goods_inflation: f64,
    pub inflation: f64,
    pub expected_inflation: f64,
    pub exchange: f64,
    /// The exchange rate on day 1 (prices are measured against it, so the CPI starts at 1).
    pub exchange_start: f64,
    pub printed: f64,
    pub savings: f64,
    pub profits: f64,
    pub services_spending: f64,
    pub household_spending: f64,
    /// Money spent at home today that becomes demand for services and
    /// construction tomorrow (building upkeep, infrastructure, projects).
    pub injection: f64,
    /// Money paid to households tomorrow besides wages (bond interest, the
    /// corrupt spending their take).
    pub transfers: f64,
    /// Building work under way: (local money per day, days left).
    pub projects: Vec<(f64, u32)>,

    // The state.
    pub income_tax: f64,
    pub corporate_tax: f64,
    pub sales_tax: f64,
    pub tariff: f64,
    pub welfare_rate: f64,
    pub manual_taxes: bool,
    pub treasury: f64,
    pub debt: f64,
    pub rating: usize,
    pub bond_rate: f64,
    pub corruption: f64,
    pub black_market: f64,
    pub embargo: Vec<bool>,
    pub budget_today: Budget,
    pub budget_month: Budget,
    pub budget_last_month: Budget,
    pub deficit_ema: f64,

    // Trade.
    pub trade_capacity: f64,
    /// Trade (weight a day) carried by old routes (rivers, caravans,
    /// pipelines) on top of ports and border crossings: whatever day 1 needed
    /// beyond what they could ever carry.
    pub trade_routes: f64,
    /// Day 1's trade capacity (border crossings worn below it are rebuilt).
    pub trade_capacity_start: f64,
    pub trade_used: f64,
    pub exports_w: f64,
    pub imports_w: f64,
    pub balance_ema: f64,
    /// W$ of goods sent to each nation this month (for the trade screen).
    pub exports_to: Vec<f64>,
    pub imports_from: Vec<f64>,
    pub exports_to_last: Vec<f64>,
    pub imports_from_last: Vec<f64>,
    /// W$ per W$ of goods traded, paid for transport (last month).
    pub transport_paid: f64,
    pub traded_value: f64,
    pub transport_share: f64,
    /// Traders' margin today (local money): part of GDP.
    pub trade_margin: f64,
    /// Provinces with a land border to another nation (for overland trade).
    pub border_provinces: Vec<usize>,

    // Output.
    pub gdp_local: f64,
    /// GDP in W$ per year, smoothed over a month.
    pub gdp: f64,
    pub gdp_start: f64,
    pub real_gdp: f64,
    pub real_trend: f64,
    pub real_growth: f64,
    /// Money supply growth per day, smoothed over about three months.
    pub money_growth: f64,
    /// Printed money this month (local), for the fiscal autopilot.
    pub printed_month: f64,
    /// What was printed last month.
    pub printed_last_month: f64,
    pub history: History,
    /// The player decides: the autopilot leaves taxes, the rate, state
    /// building and trade policy alone once the player has changed them.
    pub player: bool,
    pub state_autopilot: bool,
    /// Run by an NPC brain (src/ai): the state's building and infrastructure
    /// are the brain's to decide, through the player's own levers.
    pub brain: bool,
    /// Research and schools, as a share of GDP a year (a lever, 0 by default),
    /// and the knowledge it has built up (follows it over `research_lag` years).
    pub research: f64,
    pub knowledge: f64,
    /// Arms purchases as a share of GDP a year, if set by hand (else the Army
    /// Strength wheel's).
    pub arms: Option<f64>,
    /// A public works scheme set by hand: its most, as a share of the
    /// workforce (else the autopilot's, in a slump only).
    pub works: Option<f64>,

    // War (docs/WAR.md): set by `src/war`, left alone without it.
    /// The share of each good's stock, and of the stockpile, that is faulty
    /// (weapons remember their factory's defects).
    pub stock_faulty: Vec<f64>,
    pub stockpile_faulty: Vec<f64>,
    /// Soldiers in the army (divisions and hospitals), when war is simulated;
    /// else the Active Duty wheel's share of the people.
    pub soldiers: Option<f64>,
    /// War's pull on stability (added to its target) and on output (×).
    pub war_stability: f64,
    pub war_output: f64,
    /// The share of its sea trade cut by enemy navies.
    pub blockade: f64,
    /// Total war: the share of consumer goods factories making weapons.
    pub conversion: f64,
}

/// The whole world's economy.
#[derive(Debug, Clone)]
pub struct Economy {
    pub day: u32,
    pub nations: Vec<NationEcon>,
    pub provinces: Vec<ProvinceEcon>,
    pub world_price: Vec<f64>,
    pub base_world_price: Vec<f64>,
    /// World events: (good, multiplier, days left).
    pub world_shocks: Vec<(usize, f64, u32)>,
    /// Trade agreements (diplomacy): no tariffs between the two, and each
    /// prefers the other's goods. Empty without diplomacy.
    pub trade_pact: Vec<Vec<bool>>,
    /// How much more a buyer weighs a trade partner's offers.
    pub trade_pref: f64,
    pub news: Vec<News>,
    pub world_history: Vec<Vec<f32>>,
    pub rng: Rng,
    pub world_inflation: f64,
    pub world_rate: f64,
}

impl Economy {
    pub fn year(&self, data: &EconData) -> u32 {
        self.day / data.days_per_year + 1
    }

    /// "12 Mar, Year 3".
    pub fn date(&self, data: &EconData) -> String {
        const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        let y = self.day / data.days_per_year + 1;
        let in_year = self.day % data.days_per_year;
        let month = (in_year / 30) as usize % 12;
        format!("{} {}, Year {}", in_year % 30 + 1, MONTHS[month], y)
    }

    /// Runs whole days, as fast as it can.
    pub fn run_days(&mut self, data: &EconData, days: u32) {
        for _ in 0..days {
            self.tick(data);
        }
    }
}

/// A nation's tech level: output per worker grows with HDI.
pub fn tech(data: &EconData, n: &NationEcon) -> f64 {
    (data.tech_slope * (n.hdi - 0.65)).exp() * n.mod_tech * n.tech_cal * n.tech_progress
}

/// How much households want of goods, power and fuel: grows with tech, from
/// a day-1 level fitted to what they can afford.
pub fn living(data: &EconData, n: &NationEcon) -> f64 {
    tech(data, n) * n.need_cal
}

/// How much of its output a building makes with the nation's skilled
/// workers: full speed with enough, less (by the square root) with too few.
pub fn skill_factor(n: &NationEcon, bd: &data::BuildingDef) -> f64 {
    if bd.skill > 0.0 { (n.skilled / bd.skill).min(1.0).sqrt() } else { 1.0 }
}

/// The share of a building's output scrapped: its base rate, worse with too
/// few skilled workers, times the nation's defect modifier.
pub fn defect_rate(n: &NationEcon, bd: &data::BuildingDef) -> f64 {
    (bd.defect * (1.5 - 0.5 * skill_factor(n, bd)) * n.mod_defect).clamp(0.0, 0.9)
}

/// A nation's output modifier for a good: all output, its tier, and the good
/// itself (from the wheels, events and the land).
pub fn output_mod(_data: &EconData, n: &NationEcon, g: usize) -> f64 {
    n.mod_out[g]
}

/// How a nation's arms budget is shared between weapons: the data's split,
/// without jets under an HDI of 0.6 or ships without a coast, rescaled.
pub fn military_shares(ed: &EconData, n: &NationEcon) -> Vec<(usize, f64)> {
    let mut out: Vec<(usize, f64)> = ed
        .military_split
        .iter()
        .copied()
        .filter(|&(g, _)| !(g == ed.jets && n.hdi < 0.6) && !((g == ed.warships || g == ed.transports) && n.landlocked))
        .collect();
    let total: f64 = out.iter().map(|x| x.1).sum::<f64>().max(1e-9);
    out.iter_mut().for_each(|x| x.1 /= total);
    out
}

/// Mixes a faulty share into a stock: (share, amount) + (share, amount).
pub fn mix_faulty(share: f64, amount: f64, in_share: f64, added: f64) -> f64 {
    let total = amount.max(0.0) + added.max(0.0);
    if total <= 1e-12 { in_share } else { ((share * amount.max(0.0) + in_share * added.max(0.0)) / total).clamp(0.0, 1.0) }
}

/// The share of its stock above home cover a nation offers abroad each day.
pub const OFFER_SHARE: f64 = 0.15;
/// The share of its stock gap below home cover a nation bids for each day.
pub const BID_SHARE: f64 = 0.3;

impl NationEcon {
    /// The stock a nation keeps of a good: a month (`days_of_cover`) of home
    /// use, plus what it holds for sale abroad (its exports over
    /// `OFFER_SHARE`, about a week of them).
    pub fn stock_target(&self, ed: &EconData, g: usize) -> f64 {
        ed.prices.days_of_cover * self.use_rate[g] + self.export_rate[g] / OFFER_SHARE
    }

    /// Reads the modifiers the daily loop needs most out of `mods`.
    pub fn refresh_fast_mods(&mut self, data: &EconData) {
        self.mod_tech = self.mods.get("tech");
        self.mod_defect = self.mods.get("defect");
        self.mod_trade_cost = self.mods.get("trade_cost");
        self.mod_out = (0..data.goods.len())
            .map(|g| {
                let tier = match data.goods[g].tier {
                    data::Tier::Raw => self.mods.get("output.raw"),
                    data::Tier::Refined => self.mods.get("output.refined"),
                    data::Tier::Final => self.mods.get("output.final"),
                };
                self.mods.get("output.all") * tier * self.mods.get(&format!("output.{}", data.goods[g].id))
            })
            .collect();
        self.mod_event = data.events.iter().map(|ev| self.mods.get(&format!("event.{}", ev.id))).collect();
    }
}
