//! Reads `data/war/war.toml` (docs/WAR.md). Unknown keys are an error,
//! so a typo is one clear sentence naming the file and the key.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Army {
    pub division_size: f64,
    pub armour_tanks: f64,
    pub armour_share: f64,
    pub recruit_training: f64,
    pub recruit_per_day: f64,
    pub training_base: f64,
    pub training_hdi: f64,
    pub training_gain: f64,
    pub experience_gain: f64,
    pub max_share: f64,
    pub entrench_gain: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generals {
    pub skill_sd: f64,
    pub leader_genius: f64,
    pub leader_warlord: f64,
    pub leader_coward: f64,
    pub aggressive_ratio: f64,
    pub balanced_ratio: f64,
    pub cautious_ratio: f64,
    pub initiative: f64,
    pub reserve_frontages: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Combat {
    pub frontage_base: f64,
    pub rounds_per_hour: f64,
    pub shells_per_soldier_hour: f64,
    pub shells_per_tank_hour: f64,
    pub rounds_per_ammo: f64,
    pub shells_per_ammo: f64,
    pub p_round: f64,
    pub p_shell: f64,
    pub p_tank_vs_men: f64,
    pub p_tank_vs_tank: f64,
    pub p_at: f64,
    pub defend_stance: f64,
    pub no_ammo_fire: f64,
    pub no_rifle_fire: f64,
    pub luck_sd: f64,
    pub river_penalty: f64,
    pub city_defence: f64,
    pub city_tanks: f64,
    pub entrench_defence: f64,
    pub killed: f64,
    pub wounded: f64,
    pub captured: f64,
    pub jam_per_hour: f64,
    pub breakdown: f64,
    pub fault_breakdown: f64,
    pub no_parts_breakdown: f64,
    pub repair_days: f64,
    pub battle_damage: f64,
    pub road_damage: f64,
    pub amphibious: f64,
    pub occupied_output: f64,
    pub attack_days: f64,
    pub call_off_morale: f64,
    pub flank: f64,
    pub flank_max: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Morale {
    pub shock: f64,
    pub shortage: f64,
    pub exhaustion: f64,
    pub recover: f64,
    pub target_base: f64,
    pub target_training: f64,
    pub target_equipped: f64,
    pub win: f64,
    pub lose: f64,
    pub break_point: f64,
    pub surrender_morale: f64,
    pub surrender_equipped: f64,
    pub pocket_escape: f64,
    pub desert_below: f64,
    pub desertion: f64,
    pub stragglers_min: f64,
    pub stragglers_max: f64,
    pub starving_deaths: f64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Terrain {
    pub defence: f64,
    pub tanks: f64,
    pub frontage: f64,
    #[serde(rename = "move")]
    pub move_days: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Weather {
    pub rain: f64,
    pub rain_wet: f64,
    pub storm: f64,
    pub snow_winter: f64,
    pub rain_fire: f64,
    pub snow_fire: f64,
    pub storm_fire: f64,
    pub rain_air: f64,
    pub snow_air: f64,
    pub storm_air: f64,
    pub storm_sea: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Supply {
    pub hop_base: f64,
    pub hop_roads: f64,
    pub hop_max: f64,
    pub capacity: f64,
    pub capacity_roads: f64,
    pub interdiction: f64,
    pub ammo_days: f64,
    pub food_days: f64,
    pub fuel_days: f64,
    pub food_per_soldier: f64,
    pub fuel_tank_day: f64,
    pub fuel_tank_hour: f64,
    pub parts_tank_day: f64,
    pub request_days: u32,
    pub request_wait: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hospital {
    pub heal_days: f64,
    #[serde(rename = "return")]
    pub back: f64,
    pub die: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Air {
    pub pilot_training_days: f64,
    pub pilots_per_jet: f64,
    pub airbase_roads: f64,
    pub air_kill: f64,
    pub fault_grounding: f64,
    pub jet_fuel_per_day: f64,
    pub support: f64,
    pub support_enemy: f64,
    pub bomb_factories: f64,
    pub bomb_rail: f64,
    pub bomb_city_stability: f64,
    pub aa_loss: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sea {
    pub zone_cells: f64,
    pub sea_kill: f64,
    pub control_ratio: f64,
    pub blockade_cut: f64,
    pub convoy_loss: f64,
    pub ship_fuel_per_day: f64,
    pub transport_soldiers: f64,
    pub transport_tanks: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Levels {
    pub mobilisation: [f64; 5],
    pub arms_share: [f64; 5],
    pub skirmish_divisions: usize,
    pub skirmish_days: u32,
    pub total_conversion: f64,
    pub total_stability: f64,
    pub total_happiness: f64,
    pub total_output: f64,
    pub match_days: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Escalation {
    pub military_rule: f64,
    pub imperialism: f64,
    pub authoritarianism: f64,
    pub monarchy: f64,
    pub theocracy: f64,
    pub tribalism: f64,
    pub communism: f64,
    pub colony: f64,
    pub democracy: f64,
    pub democracy_support: f64,
    pub democracy_drain: f64,
    pub democracy_days: u32,
}

impl Escalation {
    /// How hard an ideology finds it to escalate (1: normal).
    pub fn cost(&self, ideology: &str) -> f64 {
        match ideology {
            "military_rule" => self.military_rule,
            "imperialism" => self.imperialism,
            "authoritarianism" => self.authoritarianism,
            "monarchy" => self.monarchy,
            "theocracy" => self.theocracy,
            "tribalism" => self.tribalism,
            "communism" => self.communism,
            "colony" => self.colony,
            "democracy" => self.democracy,
            _ => 1.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Other {
    pub proxy_share: f64,
    pub civil_war_stability: f64,
    pub civil_war_days: u32,
    pub civil_war_chance: f64,
    pub rebel_share: f64,
    pub civil_war_years: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exhaustion {
    pub losses: f64,
    pub level: [f64; 5],
    pub occupied: f64,
    pub blockaded: f64,
    pub recover: f64,
    pub stability: f64,
    pub output: f64,
    pub prisoner_release_days: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Peace {
    pub white: f64,
    pub reparations: f64,
    pub provinces: f64,
    pub reparations_share: f64,
    pub reparations_years: f64,
    pub accept: f64,
    pub max_war_years: f64,
    pub offer_exhaustion: f64,
    pub min_days: f64,
    pub stake: [f64; 5],
    pub hopeless: [f64; 5],
    pub lost_war_stability: f64,
    pub won_years: [f64; 5],
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ai {
    pub war_threshold: f64,
    pub gang_up_threshold: f64,
    pub cooldown_years: f64,
    pub truce_years: f64,
    pub base_aggression: f64,
    pub leader: BTreeMap<String, f64>,
    pub ideology: BTreeMap<String, f64>,
    pub weakness: f64,
    pub strength: f64,
    pub grudge: f64,
    pub gang_up: f64,
    pub exhaustion_cost: f64,
    pub debt_cost: f64,
    pub busy_cost: f64,
    pub escalate_score: f64,
    pub escalate_chance: f64,
    pub total_war_score: f64,
    pub match_ratio: f64,
    pub declare_aggression: f64,
    pub crusade: BTreeMap<String, f64>,
    pub resources: f64,
    pub revanche: f64,
    pub revanche_years: f64,
    pub coalition_min: usize,
    pub max_side: usize,
    pub max_vassals: usize,
    pub liberty_threshold: f64,
    pub vassal_output: f64,
    pub vassal_stability: f64,
    pub world_at_war_cost: f64,
    pub coalition_years: f64,
    pub operation_aggression: f64,
}

/// Everything in `data/war/war.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WarData {
    pub army: Army,
    pub training: BTreeMap<String, f64>,
    pub generals: Generals,
    pub combat: Combat,
    pub morale: Morale,
    pub terrain: BTreeMap<String, Terrain>,
    pub weather: Weather,
    pub supply: Supply,
    pub hospital: Hospital,
    pub air: Air,
    pub sea: Sea,
    pub levels: Levels,
    pub escalation: Escalation,
    pub other: Other,
    pub exhaustion: Exhaustion,
    pub peace: Peace,
    pub ai: Ai,
}

impl WarData {
    pub fn load() -> Result<WarData, String> {
        let path = "data/war/war.toml";
        let text = crate::files::read_text(path).map_err(|e| format!("{path}: can't be read ({e}); run the game from the lucky-world folder"))?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<WarData, String> {
        let d: WarData = toml::from_str(text).map_err(|e| {
            let line = e.span().map_or(0, |s| text[..s.start.min(text.len())].lines().count());
            format!("data/war/war.toml, line {line}: {}", e.message())
        })?;
        for t in ["plains", "steppe", "desert", "tundra", "forest", "hills", "marsh", "jungle", "glacier", "mountains"] {
            if !d.terrain.contains_key(t) {
                return Err(format!("data/war/war.toml: [terrain] has no \"{t}\""));
            }
        }
        if d.hospital.back + d.hospital.die > 1.0 {
            return Err("data/war/war.toml: hospital.return + hospital.die must be at most 1".to_string());
        }
        if (d.combat.killed + d.combat.wounded + d.combat.captured - 1.0).abs() > 1e-9 {
            return Err("data/war/war.toml: combat.killed + wounded + captured must be 1".to_string());
        }
        Ok(d)
    }

    /// A terrain's numbers by its name in data/terrain.toml (plains if unknown).
    pub fn terrain(&self, name: &str) -> Terrain {
        let key = name.to_lowercase();
        self.terrain.get(&key).or_else(|| self.terrain.get("plains")).copied().unwrap_or(Terrain { defence: 1.0, tanks: 1.0, frontage: 1.0, move_days: 1.0 })
    }
}
