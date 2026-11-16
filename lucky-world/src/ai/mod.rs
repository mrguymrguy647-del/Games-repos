//! The NPC brains (docs/AI.md): every nation the player doesn't run thinks
//! in three layers (strategy every few months, planning weekly, battles as a
//! hook) and acts only through the player's own levers.
//!
//! Nothing here draws. `Brains::day` is called once after each economy
//! tick. The same world and seed always give the same decisions.

pub mod battle;
pub mod data;
pub mod memory;
pub mod planning;
pub mod report;
pub mod strategy;
#[cfg(test)]
mod tests;

use duckforge::prelude::Rng;

use crate::country::Country;
use crate::econ::{EconData, Economy};
use crate::mapdata::Atlas;
use crate::world::World;
pub use data::AiData;
use data::{Difficulty, NeedVec, Traits, CATEGORIES, GOALS, NEEDS};
pub use memory::Relation;
pub use planning::Act;

/// A brain's personality: its traits from the wheels, and their names.
#[derive(Debug, Clone)]
pub struct Personality {
    /// The combined traits (default plus every wheel's change, clamped).
    pub t: Traits,
    /// "Warlord", "Democracy"... (for the spectator panel).
    pub labels: Vec<String>,
}

impl Personality {
    /// Builds a personality from a nation's wheels.
    pub fn of(ai: &AiData, gd: &crate::data::Data, c: &Country) -> Personality {
        let mut t = ai.default_traits.clone();
        t.need = [1.0; 9];
        let mut labels = Vec::new();
        for wheel in ["leader", "ideology", "history", "impact", "fate"] {
            let option = c.option_id(gd, wheel);
            if wheel == "leader" || wheel == "ideology" {
                labels.push(c.label(gd, wheel));
            }
            let Some(d) = ai.traits.get(&format!("{wheel}.{option}")) else { continue };
            for k in 0..9 {
                t.need[k] += d.need[k];
            }
            for k in 0..CATEGORIES.len() {
                t.like[k] += d.like[k];
            }
            for k in 0..GOALS.len() {
                t.goal[k] += d.goal[k];
            }
            t.risk += d.risk;
            t.horizon_years += d.horizon_years;
            t.noise += d.noise;
            t.research_max += d.research_max;
            t.arms_floor = t.arms_floor.max(d.arms_floor);
            t.tax_floor = t.tax_floor.max(d.tax_floor);
            t.embargo += d.embargo;
            t.overlord += d.overlord;
            t.hegemon += d.hegemon;
            t.reach += d.reach;
            t.research_belief += d.research_belief;
        }
        for k in 0..9 {
            t.need[k] = t.need[k].clamp(0.2, 3.0);
        }
        t.risk = t.risk.clamp(0.0, 0.8);
        t.horizon_years = t.horizon_years.clamp(2.0, 25.0);
        t.noise = t.noise.clamp(0.0, 0.2);
        t.research_max = (ai.research_max + t.research_max).clamp(0.0, 0.05);
        t.research_belief = t.research_belief.clamp(0.2, 3.0);
        Personality { t, labels }
    }
}

/// What a brain believes about another nation (its own: the truth).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Estimate {
    /// W$ a year.
    pub gdp: f64,
    pub soldiers: f64,
    /// The stockpile's worth at world prices (W$).
    pub stockpile: f64,
    pub unemployment: f64,
    pub stability: f64,
    pub inflation: f64,
    /// Public: its credit rating is junk (BB or worse).
    pub junk: bool,
    /// Public: under a fifth of its starting GDP (the wheels are public).
    pub collapsed: bool,
}

impl Estimate {
    pub fn power(&self, ai: &AiData) -> f64 {
        self.gdp + ai.army_value * self.soldiers + self.stockpile
    }
}

/// A weak neighbour, noted for the war phase.
#[derive(Debug, Clone, PartialEq)]
pub struct Opportunity {
    pub target: usize,
    pub day: u32,
    /// My power over theirs.
    pub ratio: f64,
    pub reasons: Vec<String>,
}

/// One option the brain weighed this week.
#[derive(Debug, Clone, PartialEq)]
pub struct Considered {
    pub act: Act,
    pub label: String,
    /// How much it helps (what must beat holding still).
    pub score: f64,
    /// The score as this brain judged it, with its misjudgement (difficulty
    /// and personality noise): it ranks the options, never makes a bad one good.
    pub rank: f64,
    /// The two needs that weighed most for it, in words.
    pub reasons: Vec<String>,
}

/// What a brain thought about at its last planning.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Thinking {
    pub day: u32,
    /// The best options, best first (the spectator shows three).
    pub options: Vec<Considered>,
    /// What it did.
    pub taken: Vec<String>,
}

/// A decision, for the log and the tuning tool.
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub day: u32,
    pub act: Act,
    pub label: String,
    pub score: f64,
    /// It undid a move made within `flipflop_days`.
    pub flip: bool,
    /// What the economy said back.
    pub result: Result<String, String>,
}

/// One nation's brain.
#[derive(Debug, Clone)]
pub struct Brain {
    pub nation: usize,
    pub personality: Personality,
    /// How wrong its estimates of others can be (its secret service).
    pub intel_error: f64,
    /// Spies' intel on each nation: a factor on `intel_error` (1: none;
    /// diplomacy narrows it after a successful mission).
    pub intel_factor: Vec<f64>,
    pub secret_service: String,
    /// The goal (index into `GOALS`) and why.
    pub goal: usize,
    pub goal_why: String,
    pub needs: NeedVec,
    pub need_words: Vec<String>,
    /// Each good's shortage, averaged over the last few months (does it last?).
    pub shortage_trend: Vec<f64>,
    /// Its beliefs about every nation (the last four quarters' too).
    pub estimates: Vec<Estimate>,
    pub history: Vec<Vec<Estimate>>,
    pub threat: Option<usize>,
    pub threat_level: f64,
    pub hegemon: Option<usize>,
    /// Who it plans against (a hegemon, or a hostile strong nation).
    pub counter: Option<usize>,
    /// Others threatened by the same hegemon (a hook for diplomacy).
    pub alliance_wishes: Vec<usize>,
    pub opportunities: Vec<Opportunity>,
    /// Who it would fight first (a hook for the war phase).
    pub war_targets: Vec<usize>,
    pub relations: Vec<Relation>,
    pub thinking: Thinking,
    pub log: Vec<Decision>,
    /// Each lever's last move: (key, day, direction).
    pub moves: Vec<(u64, u32, i8)>,
    /// The brain's own randomness (its noise and its spies' errors).
    pub rng: Rng,
    /// Counts for the tuning tool.
    pub stats: Stats,
    memory: memory::Watch,
}

/// What a brain has done, for the tuning tool (docs/AI.md, section 11).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stats {
    pub taken: u32,
    pub failed: u32,
    /// Moves undone within `flipflop_days`, and within `QUICK_DAYS`.
    pub flips: u32,
    pub quick_flips: u32,
    /// Flips by action (brain.toml's names).
    pub flips_by: std::collections::BTreeMap<&'static str, u32>,
    /// Decisions by action.
    pub taken_by: std::collections::BTreeMap<&'static str, u32>,
    pub builds: u32,
    /// Builds still mostly idle a year later (and by "model: building").
    pub unneeded: u32,
    pub unneeded_by: std::collections::BTreeMap<String, u32>,
    pub defaults: u32,
    /// Builds waiting for their year-later check: (day, building kind).
    pub checks: Vec<(u32, usize)>,
}

/// Each pair of nations' proximity: border, continent or far.
#[derive(Debug, Clone)]
pub struct Geo {
    pub proximity: Vec<Vec<f64>>,
}

impl Geo {
    pub fn new(ai: &AiData, atlas: &Atlas, world: &World) -> Geo {
        let map = atlas.map(world.map);
        let count = world.nations.len();
        let mut p = vec![vec![ai.threat_far; count]; count];
        let continent = |n: usize| map.provinces[world.capital[n]].continent;
        for (a, row) in p.iter_mut().enumerate() {
            for (b, x) in row.iter_mut().enumerate() {
                if a != b && continent(a) == continent(b) {
                    *x = ai.threat_continent;
                }
            }
        }
        for (q, prov) in map.provinces.iter().enumerate() {
            let Some(a) = world.owner[q] else { continue };
            for &r in &prov.neighbours {
                if let Some(b) = world.owner[r] {
                    if a != b {
                        p[a][b] = ai.threat_border;
                    }
                }
            }
        }
        for (a, row) in p.iter_mut().enumerate() {
            row[a] = 0.0;
        }
        Geo { proximity: p }
    }
}

/// Every brain in a world.
#[derive(Debug, Clone)]
pub struct Brains {
    pub data: AiData,
    pub difficulty: Difficulty,
    pub difficulty_name: String,
    /// One per nation; `None` for nations run by a person.
    pub brains: Vec<Option<Brain>>,
    pub geo: Geo,
}

impl Brains {
    /// Gives a brain to every nation `thinks(i)` picks, and hands it the
    /// state's levers (its taxes from day 1; the bank keeps the rate until
    /// the brain takes it).
    pub fn new(
        ai: AiData,
        gd: &crate::data::Data,
        atlas: &Atlas,
        world: &World,
        e: &mut Economy,
        seed: u32,
        thinks: impl Fn(usize) -> bool,
    ) -> Brains {
        let difficulty_name = ai.difficulty_name.clone();
        let difficulty = ai.difficulty(&difficulty_name);
        let geo = Geo::new(&ai, atlas, world);
        let count = e.nations.len();
        let brains = (0..count)
            .map(|i| {
                if !thinks(i) {
                    return None;
                }
                let c = &world.nations[i];
                let personality = Personality::of(&ai, gd, c);
                let secret_service = c.option_id(gd, "secret_service");
                let intel_error = ai.intel.get(&secret_service).copied().unwrap_or(0.2);
                let n = &mut e.nations[i];
                n.brain = true;
                n.manual_taxes = true;
                Some(Brain {
                    nation: i,
                    personality,
                    intel_error,
                    intel_factor: vec![1.0; count],
                    secret_service,
                    goal: 0,
                    goal_why: "Starting out".to_string(),
                    needs: [0.0; 9],
                    need_words: vec![String::new(); NEEDS.len()],
                    shortage_trend: Vec::new(),
                    estimates: vec![Estimate::default(); count],
                    history: Vec::new(),
                    threat: None,
                    threat_level: 0.0,
                    hegemon: None,
                    counter: None,
                    alliance_wishes: Vec::new(),
                    opportunities: Vec::new(),
                    war_targets: Vec::new(),
                    relations: vec![Relation::default(); count],
                    thinking: Thinking::default(),
                    log: Vec::new(),
                    moves: Vec::new(),
                    rng: Rng::new(seed ^ 0x0b4a_1000 ^ (i as u32).wrapping_mul(0x9e37_79b9)),
                    stats: Stats::default(),
                    memory: memory::Watch::new(count),
                })
            })
            .collect();
        Brains { data: ai, difficulty, difficulty_name, brains, geo }
    }

    /// Switches every brain to another difficulty (easy, normal, hard).
    pub fn set_difficulty(&mut self, name: &str) {
        self.difficulty = self.data.difficulty(name);
        self.difficulty_name = name.to_string();
    }

    /// One day's thinking, after the economy's day: each brain's strategy
    /// and planning come round on its own days.
    pub fn day(&mut self, e: &mut Economy, ed: &EconData) {
        let day = e.day;
        let (sd, pd) = (self.data.strategy_days, self.data.planning_days);
        for i in 0..self.brains.len() {
            let Some(brain) = self.brains[i].as_mut() else { continue };
            // A default today (the news says so): counted for the tuning tool.
            for item in e.news.iter().rev().take_while(|x| x.day == day) {
                if item.nation == Some(i) && item.text.contains(" defaults") {
                    brain.stats.defaults += 1;
                }
            }
            let first = brain.estimates.iter().all(|x| x.gdp == 0.0);
            if first || (day + i as u32 * 3).is_multiple_of(sd) {
                strategy::think(brain, &self.data, &self.geo, e, ed);
            }
            if (day + i as u32).is_multiple_of(pd) {
                memory::watch(brain, &self.data, e, ed);
                planning::plan(brain, &self.data, &self.difficulty, e, ed);
            }
            // Battles: the war phase's hook. Nobody fights yet.
            let orders = battle::BattlePlanner::plan(&battle::NoBattles, &battle::BattleView { nation: i, ..battle::BattleView::default() });
            debug_assert!(orders.is_empty());
        }
    }

    pub fn brain(&self, nation: usize) -> Option<&Brain> {
        self.brains.get(nation).and_then(|b| b.as_ref())
    }
}

/// A goal's name in words.
pub fn goal_name(goal: usize) -> &'static str {
    match goal {
        0 => "Grow",
        1 => "Get rich",
        2 => "Dominate",
        _ => "Survive",
    }
}
