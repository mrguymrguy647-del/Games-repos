//! Reads `data/diplomacy/diplomacy.toml` (docs/DIPLOMACY.md). Unknown keys
//! are an error, so a typo is one clear sentence naming the file and key.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relations {
    pub same_ideology: f64,
    pub other_ideology: f64,
    pub opposed_ideology: f64,
    pub friends_bonus: f64,
    pub opposed: Vec<[String; 2]>,
    pub ideological_friends: Vec<String>,
    pub border: f64,
    pub trade: f64,
    pub trade_max: f64,
    pub threat: f64,
    pub hegemon: f64,
    pub reputation: f64,
    pub half_life: f64,
    pub treaty: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Events {
    pub invaded_us: f64,
    pub fought_us: f64,
    pub side_by_side: f64,
    pub made_peace: f64,
    pub broke_treaty_with_us: f64,
    pub broke_treaty_with_ally: f64,
    pub refused_call: f64,
    pub came_to_aid: f64,
    pub caught_spy: f64,
    pub embargoed_us: f64,
    pub lifted_embargo: f64,
    pub armed_enemy: f64,
    pub sold_arms: f64,
    pub lent_money: f64,
    pub defaulted: f64,
    pub protected: f64,
    pub neglected: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reputation {
    pub start: f64,
    pub leader: BTreeMap<String, f64>,
    pub break_cost: BTreeMap<String, f64>,
    pub honour_gain: f64,
    pub recover: f64,
    pub incident: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Treaties {
    pub years: BTreeMap<String, f64>,
    pub trade_pref: f64,
    pub arms_markup: f64,
    pub arms_share: f64,
    pub loan_rate: f64,
    pub loan_risk: f64,
    pub loan_years: f64,
    pub loan_size: f64,
    pub default_months: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Answers {
    pub relations_weight: f64,
    pub trust_weight: f64,
    pub accept_threshold: f64,
    pub propose_threshold: f64,
    pub trade_gain: f64,
    pub planned_cost: f64,
    pub designs_cost: f64,
    pub shared_threat: f64,
    pub friends: f64,
    pub dragged: f64,
    pub alliance_drag: f64,
    pub access_cost: f64,
    pub guarantee_gain: f64,
    pub arms_enemy_cost: f64,
    pub sanction_hate: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct War {
    pub honour_threshold: f64,
    pub offensive_threshold: f64,
    pub exhaustion_cost: f64,
    pub busy_cost: f64,
    pub player_days: u32,
    pub break_desire: f64,
    pub deterrence: f64,
    pub alliance_years: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Talks {
    pub concession: f64,
    pub give: f64,
    pub give_weeks: f64,
    pub greed: f64,
    pub ally_reparations: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vassals {
    pub base: f64,
    pub relations: f64,
    pub tribute: f64,
    pub protected: f64,
    pub neglected: f64,
    pub speed: f64,
    pub movement: f64,
    pub movement_stability: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spies {
    pub agents: BTreeMap<String, u32>,
    pub skill: BTreeMap<String, f64>,
    pub ci_base: BTreeMap<String, f64>,
    pub max_factor: f64,
    pub min_max: u32,
    pub train_days: u32,
    pub train_cost: f64,
    pub upkeep: f64,
    pub skill_gain: f64,
    pub skill_max: f64,
    pub ci_log: f64,
    pub per_agent: f64,
    pub k: f64,
    pub ci_share: f64,
    pub launch_chance: f64,
    pub intel_factor: f64,
    pub intel_years: f64,
    pub intel_floor: f64,
    pub sabotage_buildings: f64,
    pub sabotage_rail: f64,
    pub sabotage_power: f64,
    pub steal_share: f64,
    pub unrest: f64,
    pub revolt_stability: f64,
    pub revolt_fallback: f64,
    pub fund_share: f64,
    pub meddle: f64,
    pub meddle_days: u32,
    pub bribe_skill: f64,
    pub bribe_days: u32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mission {
    pub ease: f64,
    pub exposure: f64,
    pub days: u32,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trait {
    #[serde(default)]
    pub honour: f64,
    #[serde(default)]
    pub love: f64,
    #[serde(default)]
    pub protection: f64,
    #[serde(default)]
    pub spies: f64,
}

/// Everything in `data/diplomacy/diplomacy.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiploData {
    pub relations: Relations,
    pub events: Events,
    pub reputation: Reputation,
    pub treaties: Treaties,
    pub answers: Answers,
    pub war: War,
    pub talks: Talks,
    pub vassals: Vassals,
    pub spies: Spies,
    pub missions: BTreeMap<String, Mission>,
    pub personality: BTreeMap<String, Trait>,
}

impl DiploData {
    pub fn load() -> Result<DiploData, String> {
        let path = "data/diplomacy/diplomacy.toml";
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: can't be read ({e}); run the game from the lucky-world folder"))?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<DiploData, String> {
        let d: DiploData = toml::from_str(text).map_err(|e| {
            let line = e.span().map_or(0, |s| text[..s.start.min(text.len())].lines().count());
            format!("data/diplomacy/diplomacy.toml, line {line}: {}", e.message())
        })?;
        for m in super::spies::Kind::ALL {
            if !d.missions.contains_key(m.key()) {
                return Err(format!("data/diplomacy/diplomacy.toml: [missions] has no \"{}\"", m.key()));
            }
        }
        for k in super::Kind::ALL {
            if k.lasts() && !d.treaties.years.contains_key(k.key()) {
                return Err(format!("data/diplomacy/diplomacy.toml: [treaties] years has no \"{}\"", k.key()));
            }
        }
        for w in ["none", "amateur", "competent", "feared", "legendary"] {
            if !d.spies.agents.contains_key(w) || !d.spies.skill.contains_key(w) || !d.spies.ci_base.contains_key(w) {
                return Err(format!("data/diplomacy/diplomacy.toml: [spies] is missing the secret service \"{w}\""));
            }
        }
        if !d.personality.contains_key("default") {
            return Err("data/diplomacy/diplomacy.toml: [personality] has no \"default\"".to_string());
        }
        Ok(d)
    }

    /// A nation's diplomatic personality from its leader and ideology.
    pub fn traits(&self, leader: &str, ideology: &str) -> Trait {
        let mut t = self.personality["default"];
        if let Some(l) = self.personality.get(leader) {
            if l.honour > 0.0 {
                t.honour = l.honour;
            }
            t.love += l.love;
            t.protection += l.protection;
            t.spies += l.spies;
        }
        if let Some(i) = self.personality.get(ideology) {
            t.honour += i.honour;
            t.love += i.love;
            t.protection += i.protection;
            t.spies += i.spies;
        }
        t.honour = t.honour.clamp(0.05, 1.0);
        t
    }

    pub fn mission(&self, k: super::spies::Kind) -> Mission {
        self.missions[k.key()]
    }

    pub fn years(&self, k: super::Kind) -> f64 {
        self.treaties.years.get(k.key()).copied().unwrap_or(5.0)
    }

    pub fn break_cost(&self, k: super::Kind) -> f64 {
        self.reputation.break_cost.get(k.key()).copied().unwrap_or(5.0)
    }

    pub fn treaty_relations(&self, k: super::Kind) -> f64 {
        self.relations.treaty.get(k.key()).copied().unwrap_or(0.0)
    }
}
