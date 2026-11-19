//! The economy's data files (`data/economy/*.toml`), read and checked.
//! Goods and buildings are turned into indexes so the daily tick never looks
//! a name up.

use std::collections::BTreeMap;
use std::path::Path;

use duckforge::prelude::Color;
use serde::Deserialize;

use crate::data::{parse_color, Data, DataError};

fn err<T>(text: impl Into<String>) -> Result<T, DataError> {
    Err(DataError(text.into()))
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// The files
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoodsFile {
    good: Vec<GoodEntry>,
}

#[derive(serde::Serialize, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoodEntry {
    id: String,
    name: String,
    tier: String,
    price: f64,
    weight: f64,
    #[serde(default)]
    spoil: f64,
    #[serde(default = "yes")]
    trade: bool,
    #[serde(default)]
    military: bool,
    color: String,
}

fn yes() -> bool {
    true
}

#[derive(serde::Serialize, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildingsFile {
    building: Vec<BuildingEntry>,
}

#[derive(serde::Serialize, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildingEntry {
    id: String,
    name: String,
    sector: String,
    tier: u8,
    output: String,
    amount: f64,
    #[serde(default)]
    inputs: BTreeMap<String, f64>,
    #[serde(default)]
    deposit: bool,
    #[serde(default)]
    cap_per_hex: f64,
    skill: f64,
    defect: f64,
    cost: f64,
    #[serde(default)]
    build_goods: BTreeMap<String, f64>,
    build_days: u32,
    upkeep: f64,
    #[serde(default)]
    coastal: bool,
}

#[derive(serde::Serialize, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EffectEntry {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    effects: BTreeMap<String, f64>,
}

#[derive(serde::Serialize, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EventsFile {
    event: Vec<EventEntry>,
}

#[derive(serde::Serialize, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EventEntry {
    id: String,
    name: String,
    text: String,
    chance: f64,
    days: u32,
    kind: String,
    #[serde(default)]
    effects: BTreeMap<String, f64>,
    #[serde(default)]
    good: bool,
    #[serde(default)]
    world: bool,
    #[serde(default)]
    price: BTreeMap<String, f64>,
    #[serde(default)]
    destroy: Option<[f64; 2]>,
}

#[derive(serde::Serialize, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrenciesFile {
    style: BTreeMap<String, CurrencyStyle>,
}

#[derive(serde::Serialize, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrencyStyle {
    names: Vec<String>,
}

#[derive(serde::Serialize, Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Basket {
    pub food: f64,
    pub goods: f64,
    pub goods_exp: f64,
    pub power: f64,
    pub fuel: f64,
    pub luxury: f64,
    pub luxury_exp: f64,
    pub food_share: f64,
    pub essentials_share: f64,
    pub goods_share: f64,
    /// Day 1: goods, power and fuel take this share of what their caps allow.
    pub day_one_share: f64,
}

#[derive(serde::Serialize, Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prices {
    pub price_speed: f64,
    pub price_cap: f64,
    pub planned_price_speed: f64,
    pub planned_ceiling: f64,
    pub world_price_speed: f64,
    /// Daily pull of a world price back to its anchor (in logs).
    pub world_price_pull: f64,
    /// World prices stay within these multiples of their anchor.
    pub world_price_band: [f64; 2],
    /// A traded good's home price stays within [a × export parity, b × import parity].
    pub home_band: [f64; 2],
    pub days_of_cover: f64,
    pub supply_damping: f64,
    pub cost_pull: f64,
    pub cpi_food: f64,
    pub cpi_goods: f64,
    pub cpi_fuel: f64,
    pub cpi_power: f64,
    pub cpi_services: f64,
}

#[derive(serde::Serialize, Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Money {
    pub credit_growth_base: f64,
    pub credit_slope: f64,
    pub pass_through: f64,
    pub r_neutral: f64,
    pub pi_target: f64,
    pub taylor_pi: f64,
    pub taylor_gap: f64,
    pub rate_step: f64,
    pub rate_max: f64,
    /// Rate cut per point of unemployment over its natural rate.
    pub taylor_u: f64,
    pub taylor_u_ceiling: f64,
    pub taylor_recent: f64,
}

#[derive(serde::Serialize, Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub deficit_target: f64,
    pub tax_step: f64,
    pub army_pay: f64,
    pub upkeep_tank: f64,
    pub upkeep_jet: f64,
    pub tribute: f64,
    pub procurement: BTreeMap<String, f64>,
    pub starting_stockpile_years: f64,
    /// In a slump: public works hire this share of the jobless over the
    /// natural rate, and retraining adds this much to the skilled share a
    /// year, up to `retraining_max`.
    pub public_works: f64,
    pub public_works_pay: f64,
    pub public_works_max: f64,
    /// With a public works scheme set by hand: it starts this far over the natural rate.
    pub public_works_from: f64,
    pub slump_inflation_limit: f64,
    pub retraining: f64,
    pub retraining_max: f64,
    pub infrastructure_upkeep: f64,
    pub road_cost: f64,
    pub rail_cost: f64,
    pub port_cost: f64,
}

#[derive(serde::Serialize, Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trade {
    pub sea_cost_share: f64,
    pub sea_cost_weight: f64,
    pub land_cost_share: f64,
    pub land_cost_weight: f64,
    pub transit_fee: f64,
    pub fx_trade: f64,
    pub fx_inflation: f64,
    pub fx_rate: f64,
    /// Daily pull of the exchange rate towards purchasing-power parity.
    pub fx_ppp: f64,
    /// In a slump the currency is let go: the tilt falls this much per
    /// point of unemployment over the slump line.
    pub fx_slump: f64,
    pub capital_flow: f64,
}

#[derive(serde::Serialize, Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rating {
    pub name: String,
    pub score: f64,
    pub ceiling: f64,
    pub premium: f64,
}

#[derive(serde::Serialize, Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelDef {
    pub name: String,
    pub gov_share: f64,
    pub income_tax: f64,
    pub corporate_tax: f64,
    pub sales_tax: f64,
    pub tariff: f64,
    pub corruption: f64,
    pub private_investment: f64,
    pub state_investment: f64,
    pub base_stability: f64,
    pub planned: bool,
    #[serde(skip)]
    pub id: String,
}

#[derive(serde::Serialize, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsFile {
    days_per_year: u32,
    workers_per_level: f64,
    participation: f64,
    tech_slope: f64,
    skill_by_hdi: Vec<[f64; 2]>,
    natural_unemployment_by_hdi: Vec<[f64; 2]>,
    birth_by_hdi: Vec<[f64; 2]>,
    birth_unemployment: f64,
    main_resource_bonus: f64,
    land_type_bonus: f64,
    deposit_spread: [f64; 2],
    transport_base: f64,
    transport_road: f64,
    transport_rail: f64,
    port_capacity: f64,
    land_capacity: f64,
    labour_share: f64,
    hdi_income_slope: f64,
    tech_growth: f64,
    research_effect: f64,
    research_ref: f64,
    research_lag: f64,
    frontier_per_head: f64,
    frontier_above: f64,
    services_markup: f64,
    services_premium: f64,
    invest_margin: f64,
    wage_anchor_speed: f64,
    informal_slope: f64,
    informal_floor: f64,
    port_headroom: f64,
    wage_tightness: f64,
    wage_shortage: f64,
    wage_follows_pay: f64,
    wage_sticky: f64,
    welfare_rate: f64,
    pension_share: f64,
    payout: f64,
    save_base: f64,
    save_rate_slope: f64,
    skill_pay: f64,
    military_split: BTreeMap<String, f64>,
    latent_defects: f64,
    import_defect: f64,
    rush_defect: f64,
    conversion_yield: f64,
    basket: Basket,
    prices: Prices,
    money: Money,
    state: State,
    trade: Trade,
    rating: Vec<Rating>,
    model: BTreeMap<String, ModelDef>,
    plan_priority: Vec<String>,
}

// ---------------------------------------------------------------------------
// As the simulation uses them
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Raw,
    Refined,
    Final,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Good {
    pub id: String,
    pub name: String,
    pub tier: Tier,
    pub price: f64,
    pub weight: f64,
    pub spoil: f64,
    pub trade: bool,
    pub military: bool,
    #[serde(with = "crate::save_codec::color")]
    pub color: Color,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sector {
    Farming,
    Mining,
    Industry,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct BuildingDef {
    pub id: String,
    pub name: String,
    pub sector: Sector,
    pub tier: u8,
    pub output: usize,
    pub amount: f64,
    /// (good, units per unit of output)
    pub inputs: Vec<(usize, f64)>,
    pub deposit: bool,
    pub cap_per_hex: f64,
    pub skill: f64,
    pub defect: f64,
    pub cost: f64,
    pub build_goods: Vec<(usize, f64)>,
    pub build_days: u32,
    pub upkeep: f64,
    /// Only in a coastal province (shipyards).
    pub coastal: bool,
}

/// Every modifier a wheel result or an event can carry.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Effects(pub BTreeMap<String, f64>);

/// Modifiers multiplied together (1 = no change); all others are added.
pub const MULTIPLIED: [&str; 13] = [
    "output.raw",
    "output.refined",
    "output.final",
    "output.all",
    "defect",
    "build_cost",
    "trade_cost",
    "transport",
    "port",
    "army_upkeep",
    "procurement",
    "demand",
    "investment",
];
pub const ADDED: [&str; 9] = [
    "corruption",
    "black_market",
    "stability",
    "skill",
    "rating",
    "growth",
    "infrastructure",
    "debt",
    "treasury",
];

impl Effects {
    /// Whether a modifier name is multiplied (else added).
    pub fn multiplied(key: &str) -> bool {
        MULTIPLIED.contains(&key) || key == "tech" || key.starts_with("output.") || key.starts_with("event.")
    }

    pub fn get(&self, key: &str) -> f64 {
        self.0.get(key).copied().unwrap_or(if Self::multiplied(key) { 1.0 } else { 0.0 })
    }

    /// Adds another set on top: multiplied ones multiply, added ones add.
    pub fn stack(&mut self, other: &Effects) {
        for (k, v) in &other.0 {
            let e = self.0.entry(k.clone()).or_insert(if Self::multiplied(k) { 1.0 } else { 0.0 });
            if Self::multiplied(k) {
                *e *= v;
            } else {
                *e += v;
            }
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Modifier,
    Discovery,
    Fire,
    Damage,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct EventDef {
    pub id: String,
    pub name: String,
    pub text: String,
    pub chance: f64,
    pub days: u32,
    pub kind: EventKind,
    pub effects: Effects,
    pub good: bool,
    pub world: bool,
    pub price: Vec<(usize, f64)>,
    pub destroy: [f64; 2],
}

/// The economy's data.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct EconData {
    pub goods: Vec<Good>,
    pub buildings: Vec<BuildingDef>,
    /// (wheel id, option id) -> (model id if any, effects)
    pub wheel_effects: BTreeMap<(String, String), (Option<String>, Effects)>,
    pub continent_effects: BTreeMap<String, Effects>,
    pub events: Vec<EventDef>,
    pub currencies: BTreeMap<String, Vec<String>>,
    pub models: Vec<ModelDef>,
    pub days_per_year: u32,
    pub workers_per_level: f64,
    pub participation: f64,
    pub tech_slope: f64,
    pub skill_by_hdi: Vec<[f64; 2]>,
    pub natural_unemployment_by_hdi: Vec<[f64; 2]>,
    pub birth_by_hdi: Vec<[f64; 2]>,
    pub birth_unemployment: f64,
    pub main_resource_bonus: f64,
    pub land_type_bonus: f64,
    pub deposit_spread: [f64; 2],
    pub transport_base: f64,
    pub transport_road: f64,
    pub transport_rail: f64,
    pub port_capacity: f64,
    pub land_capacity: f64,
    pub labour_share: f64,
    pub hdi_income_slope: f64,
    pub tech_growth: f64,
    pub research_effect: f64,
    pub research_ref: f64,
    pub research_lag: f64,
    pub frontier_per_head: f64,
    pub frontier_above: f64,
    pub services_markup: f64,
    pub services_premium: f64,
    pub invest_margin: f64,
    pub wage_anchor_speed: f64,
    pub informal_slope: f64,
    pub informal_floor: f64,
    pub port_headroom: f64,
    pub wage_tightness: f64,
    pub wage_shortage: f64,
    /// The wage's weight on what all buildings could pay (the rest: day 1's
    /// wage grown with tech).
    pub wage_follows_pay: f64,
    /// The share of a currency move W$ wages take at once (local wages are
    /// sticky, so a devaluation makes workers cheaper for a while).
    pub wage_sticky: f64,
    pub welfare_rate: f64,
    pub pension_share: f64,
    pub payout: f64,
    pub save_base: f64,
    pub save_rate_slope: f64,
    pub skill_pay: f64,
    pub basket: Basket,
    pub prices: Prices,
    pub money: Money,
    pub state: State,
    pub trade: Trade,
    pub ratings: Vec<Rating>,
    /// Goods in plan priority order (planned economies share inputs this way).
    pub plan_priority: Vec<usize>,
    // The goods the code names.
    pub food: usize,
    pub consumer_goods: usize,
    pub electricity: usize,
    pub fuel: usize,
    pub grain: usize,
    pub ammo: usize,
    pub tanks: usize,
    pub jets: usize,
    pub rifles: usize,
    pub warships: usize,
    pub transports: usize,
    /// How the arms budget is split between weapons: (good, share).
    pub military_split: Vec<(usize, f64)>,
    /// The share of a weapons factory's defects that pass quality control
    /// (into the stockpile, to fail in battle), and imports' faulty share.
    pub latent_defects: f64,
    pub import_defect: f64,
    /// Total war: converted factories' defects ×, and the weapons' worth
    /// they make for a unit of consumer goods' worth.
    pub rush_defect: f64,
    pub conversion_yield: f64,
    pub steel: usize,
    pub timber: usize,
}

/// Interpolates `x` along points `[[x, y], ...]` (sorted by x).
pub fn interpolate(points: &[[f64; 2]], x: f64) -> f64 {
    if points.is_empty() {
        return 0.0;
    }
    if x <= points[0][0] {
        return points[0][1];
    }
    for w in points.windows(2) {
        let ([x0, y0], [x1, y1]) = (w[0], w[1]);
        if x <= x1 {
            return y0 + (y1 - y0) * (x - x0) / (x1 - x0);
        }
    }
    points[points.len() - 1][1]
}

impl EconData {
    pub fn load(data: &Data) -> Result<EconData, DataError> {
        Self::load_from(&Path::new(crate::data::DATA_DIR).join("economy"), data)
    }

    pub fn load_from(dir: &Path, data: &Data) -> Result<EconData, DataError> {
        let read = |name: &str| -> Result<String, DataError> {
            let path = dir.join(name);
            crate::files::read_text(&path).map_err(|e| DataError(format!("couldn't read {} ({e})", path.display())))
        };
        let texts = [
            read("goods.toml")?,
            read("buildings.toml")?,
            read("effects.toml")?,
            read("events.toml")?,
            read("settings.toml")?,
            read("currencies.toml")?,
        ];
        Self::parse(data, &texts)
    }

    /// The six files' text: goods, buildings, effects, events, settings, currencies.
    pub fn parse(data: &Data, t: &[String; 6]) -> Result<EconData, DataError> {
        let parse = |name: &str, text: &str| -> Result<toml::Value, DataError> {
            toml::from_str(text).map_err(|e| DataError(format!("data/economy/{name}: {}", one_line(&e.to_string()))))
        };
        let goods_file: GoodsFile = from(&parse("goods.toml", &t[0])?, "goods.toml")?;
        let buildings_file: BuildingsFile = from(&parse("buildings.toml", &t[1])?, "buildings.toml")?;
        let effects_value = parse("effects.toml", &t[2])?;
        let events_file: EventsFile = from(&parse("events.toml", &t[3])?, "events.toml")?;
        let settings: SettingsFile = from(&parse("settings.toml", &t[4])?, "settings.toml")?;
        let currencies: CurrenciesFile = from(&parse("currencies.toml", &t[5])?, "currencies.toml")?;

        // Goods.
        let gf = "data/economy/goods.toml";
        let mut goods = Vec::new();
        for g in goods_file.good {
            if goods.iter().any(|x: &Good| x.id == g.id) {
                return err(format!("{gf}: two goods are called \"{}\"", g.id));
            }
            let tier = match g.tier.as_str() {
                "raw" => Tier::Raw,
                "refined" => Tier::Refined,
                "final" => Tier::Final,
                other => return err(format!("{gf}, good \"{}\": tier \"{other}\" must be raw, refined or final", g.id)),
            };
            if g.price <= 0.0 || g.weight < 0.0 || !(0.0..=1.0).contains(&g.spoil) {
                return err(format!("{gf}, good \"{}\": price must be above 0, weight 0 or more, spoil between 0 and 1", g.id));
            }
            let color = parse_color(&g.color).ok_or_else(|| DataError(format!("{gf}, good \"{}\": bad colour", g.id)))?;
            goods.push(Good {
                id: g.id,
                name: g.name,
                tier,
                price: g.price,
                weight: g.weight,
                spoil: g.spoil,
                trade: g.trade,
                military: g.military,
                color,
            });
        }
        let good = |id: &str, here: &str| -> Result<usize, DataError> {
            goods.iter().position(|g| g.id == id).ok_or_else(|| DataError(format!("{here}: \"{id}\" isn't a good in {gf}")))
        };
        let named = |id: &str| good(id, "the game's code");

        // Buildings.
        let bf = "data/economy/buildings.toml";
        let mut buildings = Vec::new();
        for b in buildings_file.building {
            let here = format!("{bf}, building \"{}\"", b.id);
            if buildings.iter().any(|x: &BuildingDef| x.id == b.id) {
                return err(format!("{bf}: two buildings are called \"{}\"", b.id));
            }
            let sector = match b.sector.as_str() {
                "farming" => Sector::Farming,
                "mining" => Sector::Mining,
                "industry" => Sector::Industry,
                other => return err(format!("{here}: sector \"{other}\" must be farming, mining or industry")),
            };
            if !(1..=4).contains(&b.tier) {
                return err(format!("{here}: tier must be 1 (raw), 2 (power), 3 (refined) or 4 (final)"));
            }
            let output = good(&b.output, &here)?;
            let mut inputs = Vec::new();
            for (k, v) in &b.inputs {
                let g = good(k, &here)?;
                if g == output {
                    return err(format!("{here}: it can't use its own output as an input"));
                }
                inputs.push((g, *v));
            }
            if b.deposit != b.inputs.is_empty() {
                return err(format!("{here}: a building either digs a deposit (deposit = true, no inputs) or has inputs"));
            }
            if b.deposit && goods[output].tier != Tier::Raw {
                return err(format!("{here}: only raw goods come from deposits"));
            }
            let mut build_goods = Vec::new();
            for (k, v) in &b.build_goods {
                build_goods.push((good(k, &here)?, *v));
            }
            if b.amount <= 0.0 || !(0.0..1.0).contains(&b.defect) || !(0.0..=1.0).contains(&b.skill) {
                return err(format!("{here}: amount must be above 0, defect and skill between 0 and 1"));
            }
            buildings.push(BuildingDef {
                id: b.id,
                name: b.name,
                sector,
                tier: b.tier,
                output,
                amount: b.amount,
                inputs,
                deposit: b.deposit,
                cap_per_hex: b.cap_per_hex,
                skill: b.skill,
                defect: b.defect,
                cost: b.cost,
                build_goods,
                build_days: b.build_days.max(1),
                upkeep: b.upkeep,
                coastal: b.coastal,
            });
        }
        // Every good can be made, and inputs are made by an earlier tier.
        for (g, gd) in goods.iter().enumerate() {
            let makers: Vec<&BuildingDef> = buildings.iter().filter(|b| b.output == g).collect();
            if makers.is_empty() {
                return err(format!("{bf}: no building makes \"{}\"", gd.id));
            }
        }
        for b in &buildings {
            for &(g, _) in &b.inputs {
                let earliest = buildings.iter().filter(|m| m.output == g).map(|m| m.tier).min().unwrap_or(9);
                if earliest >= b.tier {
                    return err(format!(
                        "{bf}, building \"{}\": its input \"{}\" is made in tier {earliest}, not before tier {}",
                        b.id, goods[g].id, b.tier
                    ));
                }
            }
        }

        // Effects.
        let ef = "data/economy/effects.toml";
        let event_ids: Vec<String> = events_file.event.iter().map(|e| e.id.clone()).collect();
        let check_effects = |here: &str, map: &BTreeMap<String, f64>| -> Result<Effects, DataError> {
            for (k, v) in map {
                let known = MULTIPLIED.contains(&k.as_str())
                    || ADDED.contains(&k.as_str())
                    || k == "tech"
                    || k.strip_prefix("output.").is_some_and(|g| goods.iter().any(|x| x.id == g))
                    || k.strip_prefix("event.").is_some_and(|e| event_ids.iter().any(|x| x == e));
                if !known {
                    return err(format!("{here}: \"{k}\" isn't a modifier (see the list at the top of {ef})"));
                }
                if Effects::multiplied(k) && *v < 0.0 {
                    return err(format!("{here}: \"{k}\" multiplies, so it can't be below 0"));
                }
            }
            Ok(Effects(map.clone()))
        };
        let mut models: Vec<ModelDef> = Vec::new();
        for (id, mut m) in settings.model {
            m.id = id;
            models.push(m);
        }
        for need in ["market", "planned", "mixed", "colony"] {
            if !models.iter().any(|m| m.id == need) {
                return err(format!("data/economy/settings.toml: [model.{need}] is missing"));
            }
        }
        let mut wheel_effects = BTreeMap::new();
        let mut continent_effects = BTreeMap::new();
        let table = effects_value.as_table().ok_or_else(|| DataError(format!("{ef}: expected tables")))?;
        for (wheel_id, options) in table {
            let options = options.as_table().ok_or_else(|| DataError(format!("{ef}: [{wheel_id}] should hold [{wheel_id}.<option>] tables")))?;
            for (option_id, entry) in options {
                let here = format!("{ef}, [{wheel_id}.{option_id}]");
                let entry: EffectEntry = entry
                    .clone()
                    .try_into()
                    .map_err(|e: toml::de::Error| DataError(format!("{here}: {}", one_line(&e.to_string()))))?;
                let effects = check_effects(&here, &entry.effects)?;
                if let Some(m) = &entry.model {
                    if !models.iter().any(|x| &x.id == m) {
                        return err(format!("{here}: model \"{m}\" isn't one of settings.toml's [model.*]"));
                    }
                }
                if wheel_id == "continent_tag" {
                    continent_effects.insert(option_id.clone(), effects);
                    continue;
                }
                let Some(w) = data.wheel_index(wheel_id) else {
                    return err(format!("{ef}: \"{wheel_id}\" isn't a wheel in data/wheels.toml"));
                };
                if !data.wheels[w].from_map && !data.wheels[w].options.iter().any(|o| &o.id == option_id) {
                    return err(format!("{ef}: wheel \"{wheel_id}\" has no option \"{option_id}\""));
                }
                wheel_effects.insert((wheel_id.clone(), option_id.clone()), (entry.model, effects));
            }
        }
        // Every ideology picks a model.
        let ideology = &data.wheels[data.wheel("ideology")];
        for o in &ideology.options {
            match wheel_effects.get(&("ideology".to_string(), o.id.clone())) {
                Some((Some(_), _)) => {}
                _ => return err(format!("{ef}: [ideology.{}] needs model = \"market\" (or planned, mixed, colony)", o.id)),
            }
        }

        // Events.
        let vf = "data/economy/events.toml";
        let mut events = Vec::new();
        for e in events_file.event {
            let here = format!("{vf}, event \"{}\"", e.id);
            let kind = match e.kind.as_str() {
                "modifier" => EventKind::Modifier,
                "discovery" => EventKind::Discovery,
                "fire" => EventKind::Fire,
                "damage" => EventKind::Damage,
                other => return err(format!("{here}: kind \"{other}\" must be modifier, discovery, fire or damage")),
            };
            let effects = check_effects(&here, &e.effects)?;
            let mut price = Vec::new();
            for (k, v) in &e.price {
                price.push((good(k, &here)?, *v));
            }
            if !(0.0..=12.0).contains(&e.chance) {
                return err(format!("{here}: chance (per year) must be between 0 and 12"));
            }
            events.push(EventDef {
                id: e.id,
                name: e.name,
                text: e.text,
                chance: e.chance,
                days: e.days,
                kind,
                effects,
                good: e.good,
                world: e.world,
                price,
                destroy: e.destroy.unwrap_or([0.2, 0.5]),
            });
        }

        let mut plan_priority = Vec::new();
        for id in &settings.plan_priority {
            plan_priority.push(good(id, "data/economy/settings.toml, plan_priority")?);
        }
        if settings.rating.is_empty() {
            return err("data/economy/settings.toml: there must be at least one [[rating]]");
        }
        for id in settings.state.procurement.keys() {
            let army = &data.wheels[data.wheel("army")];
            if !army.options.iter().any(|o| &o.id == id) {
                return err(format!("data/economy/settings.toml: procurement names \"{id}\", which isn't an Army Strength option"));
            }
        }
        let mut military_split = Vec::new();
        for (id, share) in &settings.military_split {
            let g = good(id, "data/economy/settings.toml, military_split")?;
            if !goods[g].military {
                return err(format!("data/economy/settings.toml: military_split names \"{id}\", which isn't a military good"));
            }
            military_split.push((g, *share));
        }
        let mut ratings = settings.rating;
        ratings.sort_by(|a, b| b.score.total_cmp(&a.score));

        Ok(EconData {
            food: named("food")?,
            consumer_goods: named("consumer_goods")?,
            electricity: named("electricity")?,
            fuel: named("fuel")?,
            grain: named("grain")?,
            ammo: named("ammo")?,
            tanks: named("tanks")?,
            jets: named("jets")?,
            rifles: named("rifles")?,
            warships: named("warships")?,
            transports: named("transports")?,
            military_split,
            latent_defects: settings.latent_defects,
            import_defect: settings.import_defect,
            rush_defect: settings.rush_defect,
            conversion_yield: settings.conversion_yield,
            steel: named("steel")?,
            timber: named("timber")?,
            goods,
            buildings,
            wheel_effects,
            continent_effects,
            events,
            currencies: currencies.style.into_iter().map(|(k, v)| (k, v.names)).collect(),
            models,
            days_per_year: settings.days_per_year,
            workers_per_level: settings.workers_per_level,
            participation: settings.participation,
            tech_slope: settings.tech_slope,
            skill_by_hdi: settings.skill_by_hdi,
            natural_unemployment_by_hdi: settings.natural_unemployment_by_hdi,
            birth_by_hdi: settings.birth_by_hdi,
            birth_unemployment: settings.birth_unemployment,
            main_resource_bonus: settings.main_resource_bonus,
            land_type_bonus: settings.land_type_bonus,
            deposit_spread: settings.deposit_spread,
            transport_base: settings.transport_base,
            transport_road: settings.transport_road,
            transport_rail: settings.transport_rail,
            port_capacity: settings.port_capacity,
            land_capacity: settings.land_capacity,
            labour_share: settings.labour_share,
            hdi_income_slope: settings.hdi_income_slope,
            tech_growth: settings.tech_growth,
            research_effect: settings.research_effect,
            research_ref: settings.research_ref,
            research_lag: settings.research_lag,
            frontier_per_head: settings.frontier_per_head,
            frontier_above: settings.frontier_above,
            services_markup: settings.services_markup,
            services_premium: settings.services_premium,
            invest_margin: settings.invest_margin,
            wage_anchor_speed: settings.wage_anchor_speed,
            informal_slope: settings.informal_slope,
            informal_floor: settings.informal_floor,
            port_headroom: settings.port_headroom,
            wage_tightness: settings.wage_tightness,
            wage_shortage: settings.wage_shortage,
            wage_follows_pay: settings.wage_follows_pay,
            wage_sticky: settings.wage_sticky,
            welfare_rate: settings.welfare_rate,
            pension_share: settings.pension_share,
            payout: settings.payout,
            save_base: settings.save_base,
            save_rate_slope: settings.save_rate_slope,
            skill_pay: settings.skill_pay,
            basket: settings.basket,
            prices: settings.prices,
            money: settings.money,
            state: settings.state,
            trade: settings.trade,
            ratings,
            plan_priority,
        })
    }

    pub fn model(&self, id: &str) -> &ModelDef {
        self.models.iter().find(|m| m.id == id).unwrap_or(&self.models[0])
    }

    pub fn good_index(&self, id: &str) -> Option<usize> {
        self.goods.iter().position(|g| g.id == id)
    }

    pub fn building_index(&self, id: &str) -> Option<usize> {
        self.buildings.iter().position(|b| b.id == id)
    }

    pub fn rating_for(&self, score: f64) -> usize {
        self.ratings.iter().position(|r| score >= r.score).unwrap_or(self.ratings.len() - 1)
    }
}

fn from<T: serde::de::DeserializeOwned>(value: &toml::Value, name: &str) -> Result<T, DataError> {
    value
        .clone()
        .try_into()
        .map_err(|e: toml::de::Error| DataError(format!("data/economy/{name}: {}", one_line(&e.to_string()))))
}
