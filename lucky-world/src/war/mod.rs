//! War (docs/WAR.md): armies of divisions raised from the people and
//! equipped from the economy's stockpile, fought hour by hour with
//! Lanchester's laws and dice, supplied along roads, rail and sea lanes,
//! in the air and at sea, up an escalation ladder to a peace deal.
//!
//! Nothing here draws. `Wars::day` runs after the economy's day and the
//! brains' day; it fights 24 hours of battles, then a day of everything
//! else. The same world and seed always give the same war.
//!
//! **Factions.** Nations are factions `0..n`; the rebels of nation `i` (in
//! a civil war) are faction `n + i`. Everything that fights belongs to a
//! faction.

pub mod air;
pub mod data;
pub mod escalation;
pub mod land;
pub mod peace;
pub mod player;
pub mod report;
pub mod sea;
pub mod supply;
#[cfg(test)]
mod tests;

use duckforge::prelude::Rng;

use crate::econ::{EconData, Economy};
use crate::mapdata::Atlas;
use crate::world::World;
pub use data::WarData;

pub type Faction = usize;

/// The fronts a nation's divisions are grouped into, by compass direction
/// from its capital.
pub const FRONTS: [&str; 5] = ["Northern", "Eastern", "Southern", "Western", "Home"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Infantry,
    Armoured,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Aggressive,
    Balanced,
    Cautious,
}

impl Style {
    pub fn name(self) -> &'static str {
        match self {
            Style::Aggressive => "aggressive",
            Style::Balanced => "balanced",
            Style::Cautious => "cautious",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct General {
    pub name: String,
    pub skill: f64,
    pub style: Style,
    pub alive: bool,
}

/// What a division has gained and lost, in soldiers (docs/WAR.md, section 3):
/// `start + joined − released = soldiers + killed + wounded + captured + surrendered + deserted`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Tally {
    pub start: f64,
    pub joined: f64,
    pub released: f64,
    pub killed: f64,
    pub wounded: f64,
    pub captured: f64,
    pub surrendered: f64,
    pub deserted: f64,
}

impl Tally {
    /// Soldiers the tally says should be alive and in the line.
    pub fn expected(&self) -> f64 {
        self.start + self.joined - self.released - self.killed - self.wounded - self.captured - self.surrendered - self.deserted
    }
}

/// An order from the player (or none: the general decides).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Auto,
    Attack(usize),
    Hold,
    DigIn,
    Retreat,
    Reinforce(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Division {
    pub id: u32,
    pub faction: Faction,
    pub name: String,
    pub kind: Kind,
    pub province: usize,
    pub alive: bool,
    pub soldiers: f64,
    pub tally: Tally,
    pub rifles: f64,
    pub rifles_faulty: f64,
    pub tanks: f64,
    pub tanks_broken: f64,
    pub tanks_faulty: f64,
    /// Rounds (a shell is `rounds_per_ammo / shells_per_ammo` rounds).
    pub ammo: f64,
    pub ammo_faulty: f64,
    /// Tonnes of fuel, soldier-days of food.
    pub fuel: f64,
    pub food: f64,
    pub training: f64,
    pub experience: f64,
    pub morale: f64,
    pub entrench: f64,
    pub general: usize,
    /// Moving: (to, hours left, by sea).
    pub moving: Option<(usize, f64, bool)>,
    pub battle: Option<usize>,
    pub order: Order,
    /// Hours of a landing's penalty left.
    pub landing: f64,
    /// Hours since its last battle (for morale and reports).
    pub idle_hours: f64,
}

/// One side of a battle, for its report.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SideReport {
    pub factions: Vec<Faction>,
    pub divisions: Vec<String>,
    pub generals: Vec<String>,
    pub soldiers_start: f64,
    pub tanks_start: f64,
    pub rounds_fired: f64,
    pub misfires: f64,
    pub jams: f64,
    pub shells_fired: f64,
    pub duds: f64,
    pub hits: f64,
    pub killed: f64,
    pub wounded: f64,
    pub captured: f64,
    pub surrendered: f64,
    pub deserted: f64,
    pub tanks_lost: f64,
    pub tanks_broke_down: f64,
    pub tanks_abandoned: f64,
    pub tanks_captured: f64,
    /// Ammo units (1,000 rounds) and tonnes of fuel used.
    pub ammo_used: f64,
    pub fuel_used: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BattleReport {
    pub province: usize,
    pub province_name: String,
    pub war: usize,
    pub day_start: u32,
    pub day_end: u32,
    pub hours: u32,
    pub terrain: String,
    pub weather: Vec<String>,
    pub attacker: SideReport,
    pub defender: SideReport,
    /// 0: the attacker won (took the province), 1: the defender held.
    pub winner: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Battle {
    pub id: usize,
    pub province: usize,
    pub war: usize,
    /// The factions on each side (attackers first).
    pub sides: [Vec<Faction>; 2],
    pub attackers: Vec<u32>,
    pub from: Vec<usize>,
    pub over: bool,
    pub report: BattleReport,
}

/// The escalation ladder (docs/WAR.md, section 8).
pub const LEVELS: [&str; 5] = ["Cold war", "Border skirmish", "Military operation", "Declared war", "Total war"];

#[derive(Debug, Clone, PartialEq)]
pub enum Goal {
    /// Take these provinces.
    Provinces(Vec<usize>),
    Reparations,
    Vassal,
    Ideology,
    /// A civil war: take the capital.
    Rebellion,
}

impl Goal {
    pub fn words(&self, world: &World, atlas: &Atlas) -> String {
        match self {
            Goal::Provinces(ps) => {
                let map = atlas.map(world.map);
                format!("take {}", ps.iter().map(|&p| map.provinces[p].name.clone()).collect::<Vec<_>>().join(", "))
            }
            Goal::Reparations => "reparations".to_string(),
            Goal::Vassal => "make it a vassal".to_string(),
            Goal::Ideology => "change its government".to_string(),
            Goal::Rebellion => "overthrow the government".to_string(),
        }
    }
}

/// Totals for one side of a war.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Losses {
    pub killed: f64,
    pub wounded: f64,
    pub captured: f64,
    pub deserted: f64,
    pub tanks: f64,
    pub jets: f64,
    pub warships: f64,
    pub transports: f64,
    pub ammo: f64,
    pub battles_won: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct War {
    pub id: usize,
    pub sides: [Vec<Faction>; 2],
    pub level: usize,
    /// The level it was declared at.
    pub start_level: usize,
    pub max_level: usize,
    pub goal: Goal,
    pub cause: String,
    pub start: u32,
    pub end: Option<u32>,
    pub result: Option<String>,
    /// An escalation waiting for an answer: (who, to what level, by when).
    pub pending: Option<(usize, usize, u32)>,
    pub losses: [Losses; 2],
    pub battles: u32,
    pub civil: bool,
    /// Proxy supporters of each side (they send weapons, unseen).
    pub proxies: [Vec<Faction>; 2],
    pub peace: Option<peace::PeaceTerms>,
}

impl War {
    pub fn active(&self) -> bool {
        self.end.is_none()
    }

    /// Which side a faction is on, if any.
    pub fn side_of(&self, f: Faction) -> Option<usize> {
        (0..2).find(|&s| self.sides[s].contains(&f))
    }
}

/// A general's request to the quartermaster (the player's job, section 5).
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub front: usize,
    pub day: u32,
    pub tanks: f64,
    pub rifles: f64,
    /// Tonnes of fuel, and how many weeks that is.
    pub fuel: f64,
    pub fuel_weeks: f64,
    pub jets: f64,
    pub answered: Option<bool>,
}

impl Request {
    pub fn words(&self) -> String {
        let mut parts = Vec::new();
        if self.tanks >= 1.0 {
            parts.push(format!("{:.0} tanks", self.tanks));
        }
        if self.rifles >= 1.0 {
            parts.push(format!("{} rifles", crate::war::report::big(self.rifles)));
        }
        if self.fuel_weeks >= 0.5 {
            parts.push(format!("{:.0} weeks of fuel", self.fuel_weeks));
        }
        if self.jets >= 1.0 {
            parts.push(format!("{:.0} jets", self.jets));
        }
        format!("{} front needs {}", FRONTS[self.front], if parts.is_empty() { "nothing".to_string() } else { parts.join(" and ") })
    }
}

/// Weapons in and out, by good (for the "every shell is accounted for" test).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Flows {
    /// From the stockpile (or the rebels' stores) to the divisions.
    pub issued: Vec<f64>,
    /// Lost on the way to the front.
    pub lost: Vec<f64>,
    /// Ammo fired (in units), tanks destroyed, jets and ships lost.
    pub used: Vec<f64>,
    /// Taken by the enemy (from divisions and stores), and taken from them.
    pub captured_from: Vec<f64>,
    pub captured_by: Vec<f64>,
    /// Back into store (a division disbanded).
    pub returned: Vec<f64>,
    /// Sent to others, received from others (proxy war).
    pub given: Vec<f64>,
    pub received: Vec<f64>,
    /// Bought into the stockpile by the economy, while war watched.
    pub bought: Vec<f64>,
}

impl Flows {
    fn new(goods: usize) -> Flows {
        let z = vec![0.0; goods];
        Flows { issued: z.clone(), lost: z.clone(), used: z.clone(), captured_from: z.clone(), captured_by: z.clone(), returned: z.clone(), given: z.clone(), received: z.clone(), bought: z }
    }
}

/// A faction's war machine: everything that isn't a division.
#[derive(Debug, Clone, PartialEq)]
pub struct Force {
    pub exhaustion: f64,
    /// Wounded soldiers in hospital.
    pub hospital: f64,
    /// Rebels have no economy: what they carry off and capture.
    pub stores: Vec<f64>,
    pub stores_faulty: Vec<f64>,
    pub flows: Flows,
    pub requests: Vec<Request>,
    /// Each front's supply priority: 0 hold back, 1 low, 2 normal, 3 high.
    pub priority: [u8; 5],
    /// Requests answered by priority without asking.
    pub auto: bool,
    /// What answered requests still owe each front: [front][tanks, rifles, fuel].
    pub granted: [[f64; 3]; 5],
    /// Trained pilots (layer 5b).
    pub pilots: f64,
    /// Warships at sea: zone → ships (layer 5b).
    pub fleets: Vec<(usize, f64)>,
    /// Transports busy carrying troops.
    pub transports_busy: f64,
    pub last_war: Option<u32>,
    pub truces: Vec<(Faction, u32)>,
    pub low_stability_days: u32,
    pub war_unrest_days: u32,
    /// The dead (killed, died of wounds or hunger), ever.
    pub dead: f64,
    pub blockaded: f64,
    /// Air superiority over each front, from its point of view (−1..1).
    pub air: [f64; 5],
    pub divisions_raised: u32,
    /// Killed, wounded and captured, ever (and as last counted, for exhaustion).
    pub casualties: f64,
    pub casualties_seen: f64,
}

impl Force {
    fn new(goods: usize) -> Force {
        Force {
            exhaustion: 0.0,
            hospital: 0.0,
            stores: vec![0.0; goods],
            stores_faulty: vec![0.0; goods],
            flows: Flows::new(goods),
            requests: Vec::new(),
            priority: [2; 5],
            auto: true,
            granted: [[0.0; 3]; 5],
            pilots: 0.0,
            fleets: Vec::new(),
            transports_busy: 0.0,
            last_war: None,
            truces: Vec::new(),
            low_stability_days: 0,
            war_unrest_days: 0,
            dead: 0.0,
            blockaded: 0.0,
            air: [0.0; 5],
            divisions_raised: 0,
            casualties: 0.0,
            casualties_seen: 0.0,
        }
    }
}

/// The map as war sees it.
#[derive(Debug, Clone)]
pub struct Geo {
    pub neighbours: Vec<Vec<usize>>,
    pub terrain: Vec<data::Terrain>,
    pub terrain_name: Vec<String>,
    pub hexes: Vec<f64>,
    pub river: Vec<bool>,
    pub coastal: Vec<bool>,
    pub city: Vec<bool>,
    /// 0 at the top (north) of the map, 1 at the bottom.
    pub latitude: Vec<f64>,
    pub middle: Vec<(f32, f32)>,
    pub names: Vec<String>,
    /// Sea zones (layer 5b): each coastal province's zones, and zone links.
    pub zones_of: Vec<Vec<usize>>,
    pub zone_links: Vec<Vec<usize>>,
    pub zone_middle: Vec<(f32, f32)>,
    /// Each zone's coastal provinces.
    pub zone_coast: Vec<Vec<usize>>,
}

/// Something that happened, for the news and the screen.
#[derive(Debug, Clone, PartialEq)]
pub struct WarNews {
    pub day: u32,
    pub factions: Vec<Faction>,
    pub text: String,
}

/// Every army, war and battle in a world.
#[derive(Debug, Clone)]
pub struct Wars {
    pub data: WarData,
    pub nations: usize,
    pub geo: Geo,
    pub divisions: Vec<Division>,
    pub generals: Vec<General>,
    pub wars: Vec<War>,
    pub battles: Vec<Battle>,
    /// Finished battles (all of them; the screen shows the latest).
    pub reports: Vec<BattleReport>,
    pub forces: Vec<Force>,
    /// Who holds each province (a faction), if anyone.
    pub control: Vec<Option<Faction>>,
    pub owner: Vec<Option<usize>>,
    pub capital: Vec<usize>,
    /// Prisoners held: [captor][origin].
    pub prisoners: Vec<Vec<f64>>,
    /// Each faction's names and wheels, for the generals and the AI.
    pub names: Vec<String>,
    pub ideology: Vec<String>,
    pub leader: Vec<String>,
    pub secret_service: Vec<String>,
    pub army_option: Vec<String>,
    pub news: Vec<WarNews>,
    pub rng: Rng,
    next_division: u32,
    /// Factions at war with each other: [a][b] → war index.
    rel: Vec<Vec<Option<usize>>>,
    /// The day's weather in each province: 0 clear, 1 rain, 2 snow, 3 storm.
    pub weather: Vec<u8>,
    /// How wrong each faction's scouts and spies are (AI.md, section 2).
    pub intel_error: Vec<f64>,
    /// Who holds each sea zone, and which are fought over.
    pub zone_control: Vec<Option<Faction>>,
    pub zone_contested: Vec<bool>,
    /// The fleets in each zone today: (faction, naval power).
    pub zone_fleets: Vec<Vec<(Faction, f64)>>,
    /// The brains (or the generals alone, when false) decide wars and peace.
    pub ai: bool,
    /// The player's nation, if a person plays (their war choices are theirs).
    pub player: Option<usize>,
    /// Peace offers waiting for the player: (war, offering side, terms, day).
    pub offers: Vec<(usize, usize, peace::PeaceTerms, u32)>,
    /// Reparations: (payer, payee, share of GDP a year, days left).
    pub reparations: Vec<(usize, usize, f64, u32)>,
    /// A peace deal moved a border (the map must be redrawn).
    pub borders_changed: bool,
    pub day: u32,
}

const FIRST: [&str; 24] = [
    "Aldo", "Bram", "Corin", "Dag", "Ebba", "Faro", "Greta", "Hale", "Ines", "Joran", "Kasra", "Lev", "Mira", "Nils", "Orla", "Pavo", "Quill", "Runa",
    "Soren", "Tamsin", "Ulric", "Vesna", "Wren", "Yara",
];
const LAST: [&str; 24] = [
    "Ashby", "Brask", "Carro", "Dunmor", "Eskel", "Falk", "Gorran", "Halvik", "Irwin", "Jassar", "Kell", "Lorne", "Morrow", "Navar", "Ostrin", "Pell", "Rask",
    "Strand", "Toller", "Ulvane", "Varga", "Wexley", "Yorke", "Zander",
];

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// A normal number from a war rng.
pub fn normal(rng: &mut Rng) -> f64 {
    let u1 = (rng.next_f32() as f64).max(1e-6);
    let u2 = rng.next_f32() as f64;
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Rounds a non-negative amount to whole units, up or down by chance (the
/// dice decide the odd soldier).
pub fn whole(x: f64, rng: &mut Rng) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let f = x.floor();
    f + if (rng.next_f32() as f64) < x - f { 1.0 } else { 0.0 }
}

impl Wars {
    /// Raises every nation's army from its people and stockpile.
    pub fn new(wd: WarData, gd: &crate::data::Data, atlas: &Atlas, world: &World, e: &mut Economy, ed: &EconData, seed: u32) -> Wars {
        let map = atlas.map(world.map);
        let n = world.nations.len();
        let goods = ed.goods.len();
        let geo = Geo::new(&wd, atlas, world, e);
        let opt = |c: &crate::country::Country, w: &str| c.option_id(gd, w);
        let mut names: Vec<String> = world.nations.iter().map(|c| c.name.clone()).collect();
        names.extend(world.nations.iter().map(|c| format!("{} rebels", c.name)));
        let mut ideology: Vec<String> = world.nations.iter().map(|c| opt(c, "ideology")).collect();
        ideology.extend((0..n).map(|_| "rebellion".to_string()));
        let mut leader: Vec<String> = world.nations.iter().map(|c| opt(c, "leader")).collect();
        leader.extend(leader.clone());
        let mut secret_service: Vec<String> = world.nations.iter().map(|c| opt(c, "secret_service")).collect();
        secret_service.extend(secret_service.clone());
        let mut army_option: Vec<String> = world.nations.iter().map(|c| opt(c, "army")).collect();
        army_option.extend(army_option.clone());
        let mut w = Wars {
            data: wd,
            nations: n,
            geo,
            divisions: Vec::new(),
            generals: Vec::new(),
            wars: Vec::new(),
            battles: Vec::new(),
            reports: Vec::new(),
            forces: (0..2 * n).map(|_| Force::new(goods)).collect(),
            control: world.owner.clone(),
            owner: world.owner.clone(),
            capital: world.capital.clone(),
            prisoners: vec![vec![0.0; 2 * n]; 2 * n],
            names,
            ideology,
            leader,
            secret_service,
            army_option,
            news: Vec::new(),
            rng: Rng::new(seed ^ 0x5741_5221),
            next_division: 1,
            rel: vec![vec![None; 2 * n]; 2 * n],
            weather: vec![0; map.provinces.len()],
            intel_error: vec![0.15; 2 * n],
            zone_control: Vec::new(),
            zone_contested: Vec::new(),
            zone_fleets: Vec::new(),
            ai: true,
            player: None,
            offers: Vec::new(),
            reparations: Vec::new(),
            borders_changed: false,
            day: e.day,
        };
        for i in 0..n {
            let soldiers = (e.nations[i].population * e.nations[i].duty).min(e.nations[i].population * w.data.army.max_share);
            w.raise(e, ed, i, soldiers, true);
            w.forces[i].pilots = e.nations[i].stockpile[ed.jets] * w.data.air.pilots_per_jet;
            w.sync_soldiers(e, i);
        }
        w.home_fleets(e, ed);
        w
    }

    pub fn is_rebel(&self, f: Faction) -> bool {
        f >= self.nations
    }

    /// The nation a faction belongs to (rebels: the nation they rose in).
    pub fn nation_of(&self, f: Faction) -> usize {
        if f >= self.nations { f - self.nations } else { f }
    }

    pub fn at_war(&self, a: Faction, b: Faction) -> Option<usize> {
        self.rel[a][b]
    }

    /// Every faction at war with `f`.
    pub fn enemies(&self, f: Faction) -> Vec<Faction> {
        (0..self.rel.len()).filter(|&g| self.rel[f][g].is_some()).collect()
    }

    /// The level of the war between two factions (0: none).
    pub fn level_between(&self, a: Faction, b: Faction) -> usize {
        self.rel[a][b].map_or(0, |k| self.wars[k].level)
    }

    /// Takes each faction's intel error from the brains' data (its secret service).
    pub fn set_intel(&mut self, ai: &crate::ai::AiData) {
        for f in 0..self.intel_error.len() {
            self.intel_error[f] = ai.intel.get(&self.secret_service[f]).copied().unwrap_or(0.15);
        }
    }

    /// The front (by compass) a province is on, from a faction's capital.
    pub fn front_of(&self, f: Faction, q: usize) -> usize {
        let cap = self.capital[self.nation_of(f)];
        let (cx, cy) = self.geo.middle[cap];
        let (x, y) = self.geo.middle[q];
        let (dx, dy) = ((x - cx) as f64, (y - cy) as f64);
        if dx * dx + dy * dy < 6.0 {
            return 4;
        }
        if dy.abs() >= dx.abs() {
            if dy < 0.0 { 0 } else { 2 }
        } else if dx > 0.0 {
            1
        } else {
            3
        }
    }

    /// Air superiority of faction `f` over a province (its front's).
    pub fn air_over(&self, f: Faction, q: usize) -> f64 {
        self.forces[f].air[self.front_of(f, q)]
    }

    pub fn in_any_war(&self, f: Faction) -> bool {
        self.rel[f].iter().any(|x| x.is_some())
    }

    /// The highest level of any war `f` is in.
    pub fn level_of(&self, f: Faction) -> usize {
        self.wars.iter().filter(|w| w.active() && w.side_of(f).is_some()).map(|w| w.level).max().unwrap_or(0)
    }

    /// Friends: on the same side as `f` in some war (and itself).
    pub fn friends(&self, f: Faction) -> Vec<Faction> {
        let mut out = vec![f];
        for w in self.wars.iter().filter(|w| w.active()) {
            if let Some(s) = w.side_of(f) {
                for &g in &w.sides[s] {
                    if !out.contains(&g) {
                        out.push(g);
                    }
                }
            }
        }
        out
    }

    /// Rebuilds who is at war with whom.
    pub fn refresh_relations(&mut self) {
        for row in self.rel.iter_mut() {
            row.iter_mut().for_each(|x| *x = None);
        }
        for (k, w) in self.wars.iter().enumerate() {
            if !w.active() || w.level == 0 {
                continue;
            }
            for &a in &w.sides[0] {
                for &b in &w.sides[1] {
                    self.rel[a][b] = Some(k);
                    self.rel[b][a] = Some(k);
                }
            }
        }
    }

    pub fn say(&mut self, factions: Vec<Faction>, text: String) {
        self.news.push(WarNews { day: self.day, factions, text });
        if self.news.len() > 400 {
            self.news.drain(..100);
        }
    }

    /// A new general for a faction.
    pub fn new_general(&mut self, f: Faction) -> usize {
        let g = &self.data.generals;
        let mean = 1.0
            + match self.leader[f].as_str() {
                "genius" => g.leader_genius,
                "warlord" => g.leader_warlord,
                "coward" => g.leader_coward,
                _ => 0.0,
            };
        let skill = (mean + g.skill_sd * normal(&mut self.rng)).clamp(0.6, 1.4);
        let roll = self.rng.next_f32() as f64;
        let style = match self.leader[f].as_str() {
            "warlord" if roll < 0.6 => Style::Aggressive,
            "coward" if roll < 0.6 => Style::Cautious,
            _ if roll < 0.3 => Style::Aggressive,
            _ if roll < 0.6 => Style::Cautious,
            _ => Style::Balanced,
        };
        let name = format!("{} {}", FIRST[self.rng.next_u32() as usize % FIRST.len()], LAST[self.rng.next_u32() as usize % LAST.len()]);
        self.generals.push(General { name: format!("Gen. {name}"), skill, style, alive: true });
        self.generals.len() - 1
    }

    /// The training a faction's soldiers drill up to.
    pub fn training_cap(&self, e: &Economy, f: Faction) -> f64 {
        let a = &self.data.army;
        let n = &e.nations[self.nation_of(f)];
        (a.training_base + a.training_hdi * n.hdi + self.data.training.get(&self.army_option[f]).copied().unwrap_or(0.0)).clamp(0.1, 1.0)
    }

    /// Raises `soldiers` new soldiers for a faction into new divisions (at
    /// its capital, or spread over its land on day 1), armed from its stockpile.
    pub fn raise(&mut self, e: &mut Economy, ed: &EconData, f: Faction, soldiers: f64, day_one: bool) {
        let size = self.data.army.division_size;
        let count = (soldiers / size).ceil().max(if soldiers >= 500.0 { 1.0 } else { 0.0 }) as usize;
        if count == 0 {
            return;
        }
        let nation = self.nation_of(f);
        let mut places: Vec<usize> = (0..self.control.len()).filter(|&q| self.control[q] == Some(f)).collect();
        if places.is_empty() {
            places.push(self.capital[nation]);
        }
        // Border provinces first on day 1 (where an army stands guard).
        places.sort_by_key(|&q| {
            let border = self.geo.neighbours[q].iter().any(|&r| self.control[r].is_some_and(|c| c != f));
            (!border, q)
        });
        let cap = self.training_cap(e, f);
        let tanks_have = if f < self.nations { e.nations[nation].stockpile[ed.tanks] } else { self.forces[f].stores[ed.tanks] };
        let armour = ((tanks_have / self.data.army.armour_tanks).floor() as usize).min((count as f64 * self.data.army.armour_share).floor() as usize);
        let per = soldiers / count as f64;
        for k in 0..count {
            let kind = if k < armour { Kind::Armoured } else { Kind::Infantry };
            let general = self.new_general(f);
            self.forces[f].divisions_raised += 1;
            let num = self.forces[f].divisions_raised;
            let name = format!("{} {}", ordinal(num), if kind == Kind::Armoured { "Armoured" } else { "Infantry" });
            let province = if day_one { places[k % places.len()] } else { self.base(e, f).unwrap_or(self.capital[nation]) };
            let s = per.round();
            let id = self.next_division;
            self.next_division += 1;
            let morale = 0.55 + 0.3 * cap;
            self.divisions.push(Division {
                id,
                faction: f,
                name,
                kind,
                province,
                alive: true,
                soldiers: s,
                tally: Tally { start: s, ..Tally::default() },
                rifles: 0.0,
                rifles_faulty: 0.0,
                tanks: 0.0,
                tanks_broken: 0.0,
                tanks_faulty: 0.0,
                ammo: 0.0,
                ammo_faulty: 0.0,
                fuel: 0.0,
                food: s * self.data.supply.food_days,
                training: if day_one { cap } else { self.data.army.recruit_training },
                experience: 0.0,
                morale,
                entrench: 0.0,
                general,
                moving: None,
                battle: None,
                order: Order::Auto,
                landing: 0.0,
                idle_hours: 0.0,
            });
            let d = self.divisions.len() - 1;
            // Arm it from the stockpile: rifles, a few days of ammo, tanks.
            let want_rifles = s;
            let rifles = self.take(e, ed, f, ed.rifles, want_rifles);
            self.divisions[d].rifles = rifles.0;
            self.divisions[d].rifles_faulty = rifles.1;
            if kind == Kind::Armoured {
                let t = self.take(e, ed, f, ed.tanks, self.data.army.armour_tanks);
                self.divisions[d].tanks = t.0;
                self.divisions[d].tanks_faulty = t.1;
            }
            let rounds_per_ammo = self.data.combat.rounds_per_ammo;
            let want_ammo = supply::ammo_target(&self.data, &self.divisions[d]) / rounds_per_ammo * 0.5;
            let a = self.take(e, ed, f, ed.ammo, want_ammo);
            self.divisions[d].ammo = a.0 * rounds_per_ammo;
            self.divisions[d].ammo_faulty = a.1;
        }
    }

    /// Takes up to `want` of a weapon out of a faction's stockpile (or
    /// stores): (taken, its faulty share). Counted as issued.
    pub fn take(&mut self, e: &mut Economy, ed: &EconData, f: Faction, g: usize, want: f64) -> (f64, f64) {
        let _ = ed;
        let (have, faulty) = if f < self.nations {
            let n = &mut e.nations[f];
            let q = want.max(0.0).min(n.stockpile[g]);
            n.stockpile[g] -= q;
            (q, n.stockpile_faulty[g])
        } else {
            let s = &mut self.forces[f];
            let q = want.max(0.0).min(s.stores[g]);
            s.stores[g] -= q;
            (q, s.stores_faulty[g])
        };
        self.forces[f].flows.issued[g] += have;
        (have, faulty)
    }

    /// Puts weapons into a faction's stockpile (or stores).
    pub fn store(&mut self, e: &mut Economy, f: Faction, g: usize, q: f64, faulty: f64) {
        if q <= 0.0 {
            return;
        }
        if f < self.nations {
            let n = &mut e.nations[f];
            n.stockpile_faulty[g] = crate::econ::mix_faulty(n.stockpile_faulty[g], n.stockpile[g], faulty, q);
            n.stockpile[g] += q;
        } else {
            let s = &mut self.forces[f];
            s.stores_faulty[g] = crate::econ::mix_faulty(s.stores_faulty[g], s.stores[g], faulty, q);
            s.stores[g] += q;
        }
    }

    /// Tells the economy how many soldiers a nation has (divisions and hospital).
    pub fn sync_soldiers(&self, e: &mut Economy, nation: usize) {
        let s: f64 = self.divisions.iter().filter(|d| d.alive && d.faction == nation).map(|d| d.soldiers).sum::<f64>() + self.forces[nation].hospital;
        e.nations[nation].soldiers = Some(s);
    }

    /// Soldiers, tanks in a faction's divisions.
    pub fn strength(&self, f: Faction) -> (f64, f64) {
        self.divisions.iter().filter(|d| d.alive && d.faction == f).fold((0.0, 0.0), |(s, t), d| (s + d.soldiers, t + d.tanks))
    }

    /// The day's weather everywhere (from a hash: it costs no dice).
    fn roll_weather(&mut self, ed: &EconData) {
        let w = &self.data.weather;
        let in_year = self.day % ed.days_per_year;
        let winter = in_year < 60 || in_year >= 330;
        for q in 0..self.weather.len() {
            let h = hash3(q as u32, self.day, 0x77);
            let t = &self.geo.terrain_name[q];
            let wet = t == "jungle" || t == "marsh";
            let cold = self.geo.latitude[q] < 0.3 || t == "mountains" || t == "glacier" || t == "tundra";
            self.weather[q] = if h < w.storm {
                3
            } else if winter && cold && h < w.storm + w.snow_winter {
                2
            } else if h < w.storm + if wet { w.rain_wet } else { w.rain } + if winter && cold { w.snow_winter } else { 0.0 } {
                1
            } else {
                0
            };
        }
    }

    pub fn weather_name(code: u8) -> &'static str {
        match code {
            1 => "rain",
            2 => "snow",
            3 => "storm",
            _ => "clear",
        }
    }

    /// One day of war, after the economy's day (and the brains').
    pub fn day(&mut self, e: &mut Economy, ed: &EconData, brains: Option<&mut crate::ai::Brains>) {
        self.day = e.day;
        self.roll_weather(ed);
        // What the economy bought into the stockpiles today.
        for i in 0..self.nations {
            for &(g, _) in &ed.military_split {
                self.forces[i].flows.bought[g] += e.nations[i].ledger.stockpiled[g];
            }
        }
        let fighting = self.battles.iter().any(|b| !b.over) || self.divisions.iter().any(|d| d.alive && d.moving.is_some());
        if fighting {
            for _ in 0..24 {
                land::hour(self, e, ed);
            }
        }
        land::finish_battles(self, e, ed);
        supply::day(self, e, ed);
        air::day(self, e, ed);
        sea::day(self, e, ed);
        land::generals(self, e, ed);
        escalation::day(self, e, ed, brains);
        for i in 0..self.nations {
            self.sync_soldiers(e, i);
        }
    }

    /// Who holds the province, in words.
    pub fn holder_name(&self, q: usize) -> String {
        self.control[q].map_or("nobody".to_string(), |f| self.names[f].clone())
    }

    /// Whether a faction still holds its capital (rebels: anything).
    pub fn capital_held(&self, f: Faction) -> bool {
        if self.is_rebel(f) {
            return !self.held(f).is_empty();
        }
        self.control[self.capital[f]] == Some(f)
    }

    /// Where a faction's supplies and recruits start: its capital while it
    /// holds it, else the province it holds where most of its people live.
    pub fn base(&self, e: &Economy, f: Faction) -> Option<usize> {
        let nation = self.nation_of(f);
        let cap = self.capital[nation];
        if self.control[cap] == Some(f) {
            return Some(cap);
        }
        self.held(f).into_iter().max_by(|&a, &b| e.provinces[a].people.total_cmp(&e.provinces[b].people).then(b.cmp(&a)))
    }

    /// Provinces a faction holds.
    pub fn held(&self, f: Faction) -> Vec<usize> {
        (0..self.control.len()).filter(|&q| self.control[q] == Some(f)).collect()
    }

    /// Sets who holds a province, and tells the economy if it is occupied.
    pub fn set_control(&mut self, e: &mut Economy, q: usize, f: Option<Faction>) {
        self.control[q] = f;
        e.provinces[q].occupied_by = match (f, self.owner[q]) {
            (Some(c), Some(o)) if c != o => Some(self.nation_of(c)),
            _ => None,
        };
        e.provinces[q].output_share = if e.provinces[q].occupied_by.is_some() { self.data.combat.occupied_output } else { 1.0 };
    }

    /// Places a warship fleet in each coastal nation's home waters (layer 5b).
    fn home_fleets(&mut self, e: &Economy, ed: &EconData) {
        for i in 0..self.nations {
            let ships = e.nations[i].stockpile[ed.warships];
            if ships <= 0.0 {
                continue;
            }
            if let Some(&z) = self.geo.zones_of[self.capital[i]].first().or_else(|| e.nations[i].provinces.iter().find_map(|&q| self.geo.zones_of[q].first())) {
                self.forces[i].fleets.push((z, ships));
            }
        }
    }
}

/// A steady hash in [0, 1).
pub fn hash3(a: u32, b: u32, c: u32) -> f64 {
    let mut n = a.wrapping_mul(374_761_393) ^ b.wrapping_mul(668_265_263) ^ c.wrapping_mul(2_246_822_519);
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    ((n ^ (n >> 16)) & 0xffffff) as f64 / 16_777_216.0
}

impl Geo {
    pub fn new(wd: &WarData, atlas: &Atlas, world: &World, e: &Economy) -> Geo {
        let map = atlas.map(world.map);
        let count = map.provinces.len();
        let mut city = vec![false; count];
        for (i, &c) in world.capital.iter().enumerate() {
            city[c] = true;
            let _ = i;
        }
        for q in 0..count {
            if e.provinces[q].people >= 0.15 {
                city[q] = true;
            }
        }
        let terrain_name: Vec<String> = map.provinces.iter().map(|p| atlas.terrains[p.terrain].name.to_lowercase()).collect();
        let (zones_of, zone_links, zone_middle, zone_coast) = sea::zones(wd, atlas, world);
        Geo {
            neighbours: map.provinces.iter().map(|p| p.neighbours.clone()).collect(),
            terrain: terrain_name.iter().map(|t| wd.terrain(t)).collect(),
            terrain_name,
            hexes: map.provinces.iter().map(|p| p.cells.len() as f64).collect(),
            river: map.provinces.iter().map(|p| p.river).collect(),
            coastal: map.provinces.iter().map(|p| p.coastal).collect(),
            city,
            latitude: map.provinces.iter().map(|p| p.at.1 as f64 / map.height.max(1) as f64).collect(),
            middle: map.provinces.iter().map(|p| p.middle).collect(),
            names: map.provinces.iter().map(|p| p.name.clone()).collect(),
            zones_of,
            zone_links,
            zone_middle,
            zone_coast,
        }
    }

    /// Breadth-first hops from `from` through provinces `ok` allows.
    pub fn hops(&self, from: usize, ok: impl Fn(usize) -> bool) -> Vec<Option<(u32, usize)>> {
        let mut out: Vec<Option<(u32, usize)>> = vec![None; self.neighbours.len()];
        out[from] = Some((0, from));
        let mut queue = std::collections::VecDeque::from([from]);
        while let Some(q) = queue.pop_front() {
            let (d, _) = out[q].unwrap();
            for &r in &self.neighbours[q] {
                if out[r].is_none() && ok(r) {
                    out[r] = Some((d + 1, q));
                    queue.push_back(r);
                }
            }
        }
        out
    }
}
