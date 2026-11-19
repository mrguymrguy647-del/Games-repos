//! Reads `data/ai/brain.toml` and `data/ai/personality.toml` (docs/AI.md).
//! A mistake is one sentence naming the file and the key.

use std::collections::BTreeMap;

use toml::Value;

/// The needs a brain weighs, in this order everywhere.
pub const NEEDS: [&str; 9] = ["jobs", "heat", "chill", "debt", "supply", "growth", "support", "security", "trade"];
pub const JOBS: usize = 0;
pub const HEAT: usize = 1;
pub const CHILL: usize = 2;
pub const DEBT: usize = 3;
pub const SUPPLY: usize = 4;
pub const GROWTH: usize = 5;
pub const SUPPORT: usize = 6;
pub const SECURITY: usize = 7;
pub const TRADE: usize = 8;

/// One number per need.
pub type NeedVec = [f64; 9];

/// What kind of thing an action is (a brain takes at most one of each a week).
pub const CATEGORIES: [&str; 8] = ["fiscal", "money", "building", "infrastructure", "trade", "military", "research", "jobs"];

/// The goals the strategy layer chooses between.
pub const GOALS: [&str; 4] = ["grow", "get_rich", "dominate", "survive"];

/// Where a key's value went wrong.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct AiDataError(pub String);

impl std::fmt::Display for AiDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AiDataError {}

type R<T> = Result<T, AiDataError>;

fn err<T>(words: String) -> R<T> {
    Err(AiDataError(words))
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Needs {
    pub jobs_span: f64,
    pub heat_from: f64,
    pub heat_span: f64,
    pub chill_from: f64,
    pub chill_span: f64,
    pub deficit_ok: f64,
    pub deficit_span: f64,
    pub debt_ok: f64,
    pub junk: f64,
    pub growth_ok: f64,
    pub growth_floor: f64,
    pub support_ok: f64,
    pub support_span: f64,
    pub trade_full: f64,
    pub choked: f64,
    pub dominate_floor: f64,
    pub rating_safe: f64,
    pub rating_span: f64,
    pub emergency: f64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Importance {
    pub food: f64,
    pub electricity: f64,
    pub consumer_goods: f64,
    pub fuel: f64,
    pub inputs: f64,
    pub military: f64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Difficulty {
    /// Noise in its planning scores.
    pub noise: f64,
    /// How far ahead it looks (a factor on its personality's years).
    pub horizon: f64,
    /// Most moves in one week's plan.
    pub max_actions: usize,
    /// A factor on how often it rethinks its strategy and plans (below 1 is
    /// quicker to react).
    pub react: f64,
    /// A factor on the share of world power that makes a nation "too strong"
    /// (below 1: brains notice a rising power, perhaps the player, sooner).
    pub alert: f64,
    /// How badly it misjudges the odds of a war it is weighing (a spread
    /// on the power ratio its spies gave it; 0 judges them as given).
    pub judgement: f64,
    /// Days between its looks for a war to start.
    pub war_every: u32,
}

impl Default for Difficulty {
    /// Normal: the brains as they were before difficulty touched war.
    fn default() -> Difficulty {
        Difficulty { noise: 0.05, horizon: 1.0, max_actions: 3, react: 1.0, alert: 1.0, judgement: 0.0, war_every: 30 }
    }
}

/// A personality, or a wheel result's change to one.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct Traits {
    pub need: NeedVec,
    pub like: [f64; 8],
    pub goal: [f64; 4],
    pub risk: f64,
    pub horizon_years: f64,
    pub noise: f64,
    pub research_max: f64,
    pub arms_floor: f64,
    pub tax_floor: f64,
    pub embargo: f64,
    pub overlord: f64,
    pub hegemon: f64,
    pub reach: f64,
    pub research_belief: f64,
}

/// Everything in `data/ai/`.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct AiData {
    pub strategy_days: u32,
    pub planning_days: u32,
    pub hold_score: f64,
    pub harm_floor: f64,
    pub goal_stickiness: f64,
    pub flipflop_days: u32,
    pub flipflop_penalty: f64,
    pub patience_days: u32,
    pub undo_days: u32,
    pub need_smoothing: f64,
    pub embargo_threat: f64,
    pub difficulty_name: String,
    pub needs: Needs,
    pub importance: Importance,
    /// Each goal's weight on each need (by `GOALS`).
    pub goal_weights: [NeedVec; 4],
    /// Each action's effects, by name (`income_tax_up`, `build`...).
    pub actions: BTreeMap<String, NeedVec>,
    pub build_margin: f64,
    pub roi_full: f64,
    pub cost_harm: f64,
    pub export_margin: f64,
    pub min_room: f64,
    pub roi_min: f64,
    pub lasting_short: f64,
    pub lasting_short_military: f64,
    pub short_smoothing: f64,
    pub free_workers: f64,
    pub input_short: f64,
    pub busy_min: f64,
    pub tax_level: f64,
    pub change_cost: f64,
    pub tighten_only: f64,
    pub budget_days: u32,
    pub budget_spending: bool,
    pub emergency_budget_days: u32,
    pub debt_budget_days: u32,
    pub debt_budget_need: f64,
    pub tax_max_points: i32,
    pub yield_ref: f64,
    pub income_tax_max: f64,
    pub corporate_tax_max: f64,
    pub sales_tax_max: f64,
    pub tariff_max: f64,
    pub welfare_floor: f64,
    pub welfare_max: f64,
    pub welfare_level: f64,
    pub rate_from: f64,
    pub rate_release: f64,
    pub close_idle: f64,
    pub close_days: u32,
    pub research_horizon_ref: f64,
    pub research_max: f64,
    pub arms_max: f64,
    pub works_step: i32,
    pub works_max: f64,
    pub works_debt: f64,
    pub arms_commit_days: u32,
    /// Intel error by secret service option id.
    pub intel: BTreeMap<String, f64>,
    pub army_value: f64,
    pub threat_border: f64,
    pub threat_continent: f64,
    pub threat_far: f64,
    pub threat_calm: f64,
    pub threat_ok: f64,
    pub threat_span: f64,
    pub hegemon_share: f64,
    pub hegemon_lead: f64,
    pub hegemon_pressure: f64,
    pub opp_reach: f64,
    pub opp_unemployment: f64,
    pub opp_stability: f64,
    pub opp_gdp_drop: f64,
    pub mem_decay: f64,
    pub mem_embargo: f64,
    pub mem_lift: f64,
    pub mem_kept: f64,
    pub mem_broken: f64,
    pub mem_help: f64,
    pub mem_partner: f64,
    pub mem_log: usize,
    pub difficulties: BTreeMap<String, Difficulty>,
    /// The default personality.
    pub default_traits: Traits,
    /// Each wheel result's change, by "wheel.option" (leader.warlord...).
    pub traits: BTreeMap<String, Traits>,
}

/// The actions every brain.toml must give effects for.
pub const ACTIONS: [&str; 27] = [
    "income_tax_down",
    "income_tax_up",
    "corporate_tax_down",
    "corporate_tax_up",
    "sales_tax_down",
    "sales_tax_up",
    "welfare_up",
    "welfare_down",
    "tariff_up",
    "tariff_down",
    "rate_down",
    "rate_up",
    "rate_auto",
    "research_up",
    "research_down",
    "arms_up",
    "arms_down",
    "build",
    "close",
    "road",
    "port",
    "ban_exports",
    "lift_ban",
    "embargo",
    "lift_embargo",
    "works_up",
    "works_down",
];

/// Reads a table's numbers, refusing keys it doesn't know.
struct Table<'a> {
    file: &'static str,
    at: String,
    t: &'a toml::map::Map<String, Value>,
}

impl<'a> Table<'a> {
    fn of(file: &'static str, at: &str, v: &'a Value) -> R<Table<'a>> {
        match v.as_table() {
            Some(t) => Ok(Table { file, at: at.to_string(), t }),
            None => err(format!("{file}: [{at}] should be a table")),
        }
    }

    fn sub(&self, key: &str) -> R<Table<'a>> {
        match self.t.get(key) {
            Some(v) => Table::of(self.file, &self.path(key), v),
            None => err(format!("{}: [{}] is missing", self.file, self.path(key))),
        }
    }

    fn path(&self, key: &str) -> String {
        if self.at.is_empty() { key.to_string() } else { format!("{}.{key}", self.at) }
    }

    fn num(&self, key: &str) -> R<f64> {
        match self.t.get(key) {
            Some(Value::Float(f)) => Ok(*f),
            Some(Value::Integer(i)) => Ok(*i as f64),
            Some(_) => err(format!("{}: {} should be a number", self.file, self.path(key))),
            None => err(format!("{}: {} is missing", self.file, self.path(key))),
        }
    }

    fn num_or(&self, key: &str, default: f64) -> R<f64> {
        if self.t.contains_key(key) { self.num(key) } else { Ok(default) }
    }

    fn only(&self, keys: &[&str]) -> R<()> {
        for k in self.t.keys() {
            if !keys.contains(&k.as_str()) {
                return err(format!("{}: {} is not a key the brains know (they know {})", self.file, self.path(k), keys.join(", ")));
            }
        }
        Ok(())
    }

    /// A table of numbers keyed by names from `names`, into a fixed array (unset: `default`).
    fn named<const N: usize>(&self, key: &str, names: &[&str; N], default: f64) -> R<[f64; N]> {
        let mut out = [default; N];
        let Some(v) = self.t.get(key) else { return Ok(out) };
        let t = Table::of(self.file, &self.path(key), v)?;
        t.only(names)?;
        for (k, name) in names.iter().enumerate() {
            out[k] = t.num_or(name, default)?;
        }
        Ok(out)
    }
}

fn read(path: &str) -> R<Value> {
    let text = match crate::files::read_text(path) {
        Ok(t) => t,
        Err(e) => return err(format!("{path}: can't be read ({e}); run the game from the lucky-world folder")),
    };
    match text.parse::<Value>() {
        Ok(v) => Ok(v),
        Err(e) => err(format!("{path}: {}", e.to_string().lines().next().unwrap_or("isn't valid TOML"))),
    }
}

impl AiData {
    pub fn load() -> R<AiData> {
        Self::from_values(&read("data/ai/brain.toml")?, &read("data/ai/personality.toml")?)
    }

    pub fn from_values(brain: &Value, personality: &Value) -> R<AiData> {
        const B: &str = "data/ai/brain.toml";
        const P: &str = "data/ai/personality.toml";
        let b = Table::of(B, "", brain)?;
        b.only(&[
            "strategy_days",
            "planning_days",
            "hold_score",
            "harm_floor",
            "goal_stickiness",
            "flipflop_days",
            "flipflop_penalty",
            "patience_days",
            "undo_days",
            "need_smoothing",
            "embargo_threat",
            "default_difficulty",
            "needs",
            "importance",
            "goal",
            "action",
            "build",
            "close",
            "research",
            "arms",
            "fiscal",
            "money",
            "works",
            "intel",
            "power",
            "threat",
            "hegemon",
            "opportunity",
            "memory",
            "difficulty",
        ])?;
        let n = b.sub("needs")?;
        let needs = Needs {
            jobs_span: n.num("jobs_span")?,
            heat_from: n.num("heat_from")?,
            heat_span: n.num("heat_span")?,
            chill_from: n.num("chill_from")?,
            chill_span: n.num("chill_span")?,
            deficit_ok: n.num("deficit_ok")?,
            deficit_span: n.num("deficit_span")?,
            debt_ok: n.num("debt_ok")?,
            junk: n.num("junk")?,
            growth_ok: n.num("growth_ok")?,
            growth_floor: n.num("growth_floor")?,
            support_ok: n.num("support_ok")?,
            support_span: n.num("support_span")?,
            trade_full: n.num("trade_full")?,
            choked: n.num("choked")?,
            dominate_floor: n.num("dominate_floor")?,
            rating_safe: n.num("rating_safe")?,
            rating_span: n.num("rating_span")?,
            emergency: n.num("emergency")?,
        };
        let im = b.sub("importance")?;
        im.only(&["food", "electricity", "consumer_goods", "fuel", "inputs", "military"])?;
        let importance = Importance {
            food: im.num("food")?,
            electricity: im.num("electricity")?,
            consumer_goods: im.num("consumer_goods")?,
            fuel: im.num("fuel")?,
            inputs: im.num("inputs")?,
            military: im.num("military")?,
        };
        let g = b.sub("goal")?;
        g.only(&GOALS)?;
        let mut goal_weights = [[0.0; 9]; 4];
        for (k, name) in GOALS.iter().enumerate() {
            let t = g.sub(name)?;
            t.only(&NEEDS)?;
            for (c, need) in NEEDS.iter().enumerate() {
                goal_weights[k][c] = t.num(need)?;
            }
        }
        let a = b.sub("action")?;
        let mut actions = BTreeMap::new();
        for name in ACTIONS.iter() {
            let t = a.sub(name)?;
            t.only(&NEEDS)?;
            let mut eff = [0.0; 9];
            for (c, need) in NEEDS.iter().enumerate() {
                eff[c] = t.num_or(need, 0.0)?;
                if eff[c].abs() > 1.0 {
                    return err(format!("{B}: action.{name}.{need} is {}, but an effect is between -1 and 1", eff[c]));
                }
            }
            actions.insert(name.to_string(), eff);
        }
        a.only(&ACTIONS)?;
        let bu = b.sub("build")?;
        let cl = b.sub("close")?;
        let re = b.sub("research")?;
        let ar = b.sub("arms")?;
        let it = b.sub("intel")?;
        let levels = ["none", "amateur", "competent", "feared", "legendary"];
        it.only(&levels)?;
        let mut intel = BTreeMap::new();
        for l in levels {
            intel.insert(l.to_string(), it.num(l)?);
        }
        let pw = b.sub("power")?;
        let th = b.sub("threat")?;
        let he = b.sub("hegemon")?;
        let op = b.sub("opportunity")?;
        let me = b.sub("memory")?;
        let difficulty_name = match b.t.get("default_difficulty") {
            Some(Value::String(s)) => s.clone(),
            _ => "normal".to_string(),
        };
        let mut difficulties = BTreeMap::new();
        let dt = b.sub("difficulty")?;
        for name in dt.t.keys() {
            let t = dt.sub(name)?;
            t.only(&["noise", "horizon", "max_actions", "react", "alert", "judgement", "war_every"])?;
            let or = |k: &str, d: f64| if t.t.contains_key(k) { t.num(k) } else { Ok(d) };
            difficulties.insert(
                name.clone(),
                Difficulty {
                    noise: t.num("noise")?,
                    horizon: t.num("horizon")?,
                    max_actions: t.num("max_actions")?.max(1.0) as usize,
                    react: or("react", 1.0)?.clamp(0.25, 4.0),
                    alert: or("alert", 1.0)?.clamp(0.5, 2.0),
                    judgement: or("judgement", 0.0)?.clamp(0.0, 1.0),
                    war_every: or("war_every", 30.0)?.clamp(5.0, 120.0) as u32,
                },
            );
        }
        if !difficulties.contains_key(&difficulty_name) {
            return err(format!("{B}: default_difficulty is \"{difficulty_name}\", but there is no [difficulty.{difficulty_name}]"));
        }
        AiData {
            strategy_days: b.num("strategy_days")?.max(1.0) as u32,
            planning_days: b.num("planning_days")?.max(1.0) as u32,
            hold_score: b.num("hold_score")?,
            harm_floor: b.num("harm_floor")?,
            goal_stickiness: b.num("goal_stickiness")?,
            flipflop_days: b.num("flipflop_days")?.max(1.0) as u32,
            flipflop_penalty: b.num("flipflop_penalty")?,
            patience_days: b.num("patience_days")?.max(0.0) as u32,
            undo_days: b.num("undo_days")?.max(0.0) as u32,
            need_smoothing: b.num("need_smoothing")?.clamp(0.01, 1.0),
            embargo_threat: b.num("embargo_threat")?,
            difficulty_name,
            needs,
            importance,
            goal_weights,
            actions,
            build_margin: bu.num("margin")?,
            roi_full: bu.num("roi_full")?,
            cost_harm: bu.num("cost_harm")?,
            export_margin: bu.num("export_margin")?,
            min_room: bu.num("min_room")?,
            roi_min: bu.num("roi_min")?,
            lasting_short: bu.num("lasting_short")?,
            lasting_short_military: bu.num("lasting_short_military")?,
            short_smoothing: bu.num("short_smoothing")?.clamp(0.01, 1.0),
            free_workers: bu.num("free_workers")?,
            input_short: bu.num("input_short")?,
            busy_min: bu.num("busy_min")?,
            tax_level: b.sub("fiscal")?.num("tax_level")?,
            change_cost: b.sub("fiscal")?.num("change_cost")?,
            tighten_only: b.sub("fiscal")?.num("tighten_only")?,
            budget_days: b.sub("fiscal")?.num("budget_days")?.max(0.0) as u32,
            budget_spending: b.sub("fiscal")?.num("budget_spending")? > 0.5,
            emergency_budget_days: b.sub("fiscal")?.num("emergency_budget_days")?.max(0.0) as u32,
            debt_budget_days: b.sub("fiscal")?.num("debt_budget_days")?.max(0.0) as u32,
            debt_budget_need: b.sub("fiscal")?.num("debt_budget_need")?,
            tax_max_points: b.sub("fiscal")?.num("max_points")?.clamp(1.0, 5.0) as i32,
            yield_ref: b.sub("fiscal")?.num("yield_ref")?,
            income_tax_max: b.sub("fiscal")?.num("income_tax_max")?,
            corporate_tax_max: b.sub("fiscal")?.num("corporate_tax_max")?,
            sales_tax_max: b.sub("fiscal")?.num("sales_tax_max")?,
            tariff_max: b.sub("fiscal")?.num("tariff_max")?,
            welfare_floor: b.sub("fiscal")?.num("welfare_floor")?,
            welfare_max: b.sub("fiscal")?.num("welfare_max")?,
            welfare_level: b.sub("fiscal")?.num("welfare_level")?,
            rate_from: b.sub("money")?.num("rate_from")?,
            rate_release: b.sub("money")?.num("rate_release")?,
            close_idle: cl.num("idle")?,
            close_days: cl.num("days")?.max(1.0) as u32,
            research_horizon_ref: re.num("horizon_ref")?,
            research_max: re.num("max")?,
            arms_max: ar.num("max")?,
            works_step: b.sub("works")?.num("step")?.clamp(1.0, 10.0) as i32,
            works_max: b.sub("works")?.num("max")?,
            works_debt: b.sub("works")?.num("debt")?,
            arms_commit_days: ar.num("commit_days")?.max(0.0) as u32,
            intel,
            army_value: pw.num("army_value")?,
            threat_border: th.num("border")?,
            threat_continent: th.num("continent")?,
            threat_far: th.num("far")?,
            threat_calm: th.num("calm")?,
            threat_ok: th.num("ok")?,
            threat_span: th.num("span")?,
            hegemon_share: he.num("share")?,
            hegemon_lead: he.num("lead")?,
            hegemon_pressure: he.num("pressure")?,
            opp_reach: op.num("reach")?,
            opp_unemployment: op.num("unemployment")?,
            opp_stability: op.num("stability")?,
            opp_gdp_drop: op.num("gdp_drop")?,
            mem_decay: me.num("decay")?,
            mem_embargo: me.num("embargo")?,
            mem_lift: me.num("lift")?,
            mem_kept: me.num("kept")?,
            mem_broken: me.num("broken")?,
            mem_help: me.num("help")?,
            mem_partner: me.num("partner")?,
            mem_log: me.num("log")?.max(1.0) as usize,
            difficulties,
            default_traits: Traits::default(),
            traits: BTreeMap::new(),
        }
        .with_personality(P, personality)
    }

    fn with_personality(mut self, file: &'static str, v: &Value) -> R<AiData> {
        let p = Table::of(file, "", v)?;
        p.only(&["default", "leader", "ideology", "history", "impact", "fate"])?;
        let read_traits = |t: &Table, base: bool| -> R<Traits> {
            t.only(&[
                "need",
                "like",
                "goal",
                "risk",
                "horizon_years",
                "noise",
                "research_max",
                "arms_floor",
                "tax_floor",
                "embargo",
                "overlord",
                "hegemon",
                "reach",
                "research_belief",
            ])?;
            let d = |key: &str, default: f64| -> R<f64> { if base { t.num(key) } else { t.num_or(key, default) } };
            Ok(Traits {
                need: t.named("need", &NEEDS, 0.0)?,
                like: t.named("like", &CATEGORIES, 0.0)?,
                goal: t.named("goal", &GOALS, 0.0)?,
                risk: d("risk", 0.0)?,
                horizon_years: d("horizon_years", 0.0)?,
                noise: d("noise", 0.0)?,
                research_max: d("research_max", 0.0)?,
                arms_floor: d("arms_floor", 0.0)?,
                tax_floor: d("tax_floor", 0.0)?,
                embargo: d("embargo", 0.0)?,
                overlord: d("overlord", 0.0)?,
                hegemon: d("hegemon", 0.0)?,
                reach: d("reach", 0.0)?,
                research_belief: if base { t.num("research_belief")? } else { t.num_or("research_belief", 0.0)? },
            })
        };
        self.default_traits = read_traits(&p.sub("default")?, true)?;
        for wheel in ["leader", "ideology", "history", "impact", "fate"] {
            let Some(v) = p.t.get(wheel) else { continue };
            let w = Table::of(file, wheel, v)?;
            for (option, v) in w.t {
                let t = Table::of(file, &format!("{wheel}.{option}"), v)?;
                self.traits.insert(format!("{wheel}.{option}"), read_traits(&t, false)?);
            }
        }
        Ok(self)
    }

    /// The difficulty a name stands for (unknown names: normal).
    pub fn difficulty(&self, name: &str) -> Difficulty {
        self.difficulties.get(name).or_else(|| self.difficulties.get("normal")).cloned().unwrap_or_default()
    }

    /// An action's effects (all zero for a name brain.toml doesn't have).
    pub fn effect(&self, name: &str) -> NeedVec {
        self.actions.get(name).copied().unwrap_or([0.0; 9])
    }
}
