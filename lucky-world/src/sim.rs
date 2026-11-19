//! The running world: the economy, the brains, the wars and diplomacy, made
//! from a placed world and stepped one day at a time. The game, the save
//! test and the long-game tools all use exactly this.

use crate::ai::{AiData, Brains};
use crate::diplo::{DiploData, Diplomacy};
use crate::econ::{EconData, Economy};
use crate::mapdata::Atlas;
use crate::war::{WarData, Wars};
use crate::world::World;

/// Everything that thinks and changes once the nations are on the map.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Sim {
    pub economy: Economy,
    pub brains: Brains,
    pub wars: Wars,
    pub diplo: Diplomacy,
}

/// The rules every world is made with (all of `data/`).
pub struct Rules<'a> {
    pub data: &'a crate::data::Data,
    pub atlas: &'a Atlas,
    pub econ: &'a EconData,
    pub ai: &'a AiData,
    pub war: &'a WarData,
    pub diplo: &'a DiploData,
    pub victory: &'a crate::victory::VictoryData,
}

impl Sim {
    /// The economy from the wheels, a brain for every nation but the
    /// player's (all of them when `watching`), armies, and diplomacy.
    pub fn new(r: &Rules, world: &World, seed: u32, watching: bool, difficulty: &str) -> Sim {
        let mut economy = Economy::new(r.data, r.econ, r.atlas, world, seed);
        economy.underdog = crate::econ::underdog::Underdog::start(&economy, r.victory.underdog.clone());
        let mut brains = Brains::new(r.ai.clone(), r.data, r.atlas, world, &mut economy, seed, |i| watching || i != 0);
        brains.set_difficulty(difficulty);
        let mut wars = Wars::new(r.war.clone(), r.data, r.atlas, world, &mut economy, r.econ, seed);
        wars.set_intel(r.ai);
        wars.player = if watching { None } else { Some(0) };
        let diplo = Diplomacy::new(r.diplo.clone(), &wars, &economy, r.ai, seed);
        Sim { economy, brains, wars, diplo }
    }

    /// One day: the economy, then the brains think, then the war (battles
    /// hour by hour), then diplomacy.
    pub fn day(&mut self, ed: &EconData) {
        day(&mut self.economy, ed, Some(&mut self.brains), Some(&mut self.wars), Some(&mut self.diplo));
    }
}

/// One day of a world whose parts may be missing (the game keeps them
/// apart; tools may run an economy alone).
pub fn day(e: &mut Economy, ed: &EconData, mut brains: Option<&mut Brains>, wars: Option<&mut Wars>, diplo: Option<&mut Diplomacy>) {
    e.tick(ed);
    if let Some(b) = brains.as_deref_mut() {
        b.day(e, ed);
    }
    if let Some(w) = wars {
        w.day(e, ed, brains.as_deref_mut());
        if let Some(d) = diplo {
            d.day(e, ed, w, brains.as_deref_mut());
        }
    }
}

/// All of `data/`, loaded once, for tools and tests that make many worlds.
pub struct Bundle {
    pub data: crate::data::Data,
    pub atlas: Atlas,
    pub econ: EconData,
    pub ai: AiData,
    pub war: WarData,
    pub diplo: DiploData,
    pub victory: crate::victory::VictoryData,
}

impl Bundle {
    pub fn load() -> Result<Bundle, Box<dyn std::error::Error>> {
        let data = crate::data::Data::load()?;
        let atlas = Atlas::load(&data)?;
        let econ = EconData::load(&data)?;
        Ok(Bundle { atlas, econ, ai: AiData::load()?, war: WarData::load()?, diplo: DiploData::load()?, victory: crate::victory::VictoryData::load()?, data })
    }

    pub fn rules(&self) -> Rules<'_> {
        Rules { data: &self.data, atlas: &self.atlas, econ: &self.econ, ai: &self.ai, war: &self.war, diplo: &self.diplo, victory: &self.victory }
    }

    /// A world placed as the game places one (the player's wheels rolled at once).
    pub fn world(&self, map: usize, seed: u32) -> World {
        let mut rng = duckforge::prelude::Rng::new(seed);
        let player = crate::country::roll_country(&self.data, map, Vec::new(), &[], true, &mut rng);
        let rivals = crate::country::roll_rivals(&self.data, &player, &mut rng);
        World::build(&self.data, &self.atlas, player, rivals, &mut rng)
    }

    /// A placed world and its running simulation, with a player (nation 0)
    /// who leaves everything to the autopilot.
    pub fn game(&self, map: usize, seed: u32, difficulty: &str) -> (World, Sim) {
        let world = self.world(map, seed);
        let sim = Sim::new(&self.rules(), &world, seed, false, difficulty);
        (world, sim)
    }
}
