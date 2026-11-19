//! Who owns what: the nations laid on the map from their wheel results.
//!
//! 1. The player and the rivals from the rival reveal are placed first,
//!    hardest first (island nations, then landlocked ones, then the rest,
//!    bigger before smaller; the player gets no favours):
//!    - the Continent wheel says where;
//!    - Country Size says how many provinces, as a share of this map's land;
//!    - Landlocked / Coastal / Peninsula / Island Nation says what its
//!      border must be like;
//!    - Land Type makes its own terrain more attractive.
//! 2. Extra nations then spin the same 16 wheels (instantly) for the biggest
//!    stretch of empty land, again and again, until the land is covered or
//!    there are `max_nations`.
//! 3. Land still empty after that joins a neighbouring nation where its
//!    border rules allow; what's left is unclaimed.

use duckforge::prelude::*;

use crate::country::{roll_country, Country};
use crate::data::Data;
use crate::fate::Landed;
use crate::mapdata::{Atlas, WorldMap};

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct World {
    /// Index into `data.maps` (and `atlas.maps`).
    pub map: usize,
    /// Every nation: the player first, then the rivals from the reveal, then
    /// the nations made to fill the land.
    pub nations: Vec<Country>,
    /// The nation that owns each province, if any.
    pub owner: Vec<Option<usize>>,
    /// Each nation's capital province.
    pub capital: Vec<usize>,
    /// Each nation's border colour.
    #[serde(with = "crate::save_codec::colors")]
    pub color: Vec<Color>,
    /// Why a nation isn't quite where its wheels said, if it isn't.
    pub notes: Vec<Option<String>>,
    /// How many nations came from the reveal (the player included); the rest filled the land.
    pub main_count: usize,
}

/// The four border kinds of the Landlocked or Coastal wheel.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coast {
    Landlocked,
    Coastal,
    Peninsula,
    Island,
}

impl Coast {
    pub fn of(data: &Data, c: &Country) -> Coast {
        match c.option_id(data, "coast").as_str() {
            "landlocked" => Coast::Landlocked,
            "peninsula" => Coast::Peninsula,
            "island" => Coast::Island,
            _ => Coast::Coastal,
        }
    }
}

/// How many provinces a nation's Country Size gives it on this map.
pub fn target_size(data: &Data, atlas: &Atlas, map: &WorldMap, c: &Country) -> usize {
    let share = atlas.settings.size_share.get(&c.option_id(data, "size")).copied().unwrap_or(0.03);
    ((share * map.provinces.len() as f64).round() as usize).max(atlas.settings.min_provinces)
}

impl World {
    pub fn provinces_of(&self, nation: usize) -> Vec<usize> {
        (0..self.owner.len()).filter(|&p| self.owner[p] == Some(nation)).collect()
    }

    /// The nation touches the sea (one of its provinces is coastal).
    pub fn touches_sea(&self, map: &WorldMap, nation: usize) -> bool {
        self.provinces_of(nation).iter().any(|&p| map.provinces[p].coastal)
    }

    /// The other nations it shares a land border with.
    pub fn land_neighbours(&self, map: &WorldMap, nation: usize) -> Vec<usize> {
        let mut out = Vec::new();
        for p in self.provinces_of(nation) {
            for &q in &map.provinces[p].neighbours {
                if let Some(o) = self.owner[q] {
                    if o != nation && !out.contains(&o) {
                        out.push(o);
                    }
                }
            }
        }
        out.sort();
        out
    }

    /// How much of the map's land is owned, from 0 to 1.
    pub fn coverage(&self) -> f64 {
        self.owner.iter().filter(|o| o.is_some()).count() as f64 / self.owner.len() as f64
    }

    /// Lays the player and the rivals on the map, then fills the empty land.
    pub fn build(data: &Data, atlas: &Atlas, player: Country, rivals: Vec<Country>, rng: &mut Rng) -> World {
        let map_index = player.map;
        let map = atlas.map(map_index);
        let mut world = World {
            map: map_index,
            nations: Vec::new(),
            owner: vec![None; map.provinces.len()],
            capital: Vec::new(),
            color: Vec::new(),
            notes: Vec::new(),
            main_count: 0,
        };
        world.nations.push(player);
        world.nations.extend(rivals);
        world.nations.truncate(atlas.settings.max_nations);
        world.main_count = world.nations.len();
        let n = world.nations.len();
        world.capital = vec![usize::MAX; n];
        world.notes = vec![None; n];

        // Hardest borders first, then the biggest; ties keep the reveal's order.
        let mut order: Vec<usize> = (0..n).collect();
        let rank = |c: &Country| match Coast::of(data, c) {
            Coast::Island => 0,
            Coast::Landlocked => 1,
            Coast::Peninsula => 2,
            Coast::Coastal => 3,
        };
        order.sort_by_key(|&i| (rank(&world.nations[i]), std::cmp::Reverse(target_size(data, atlas, map, &world.nations[i])), i));
        for i in order {
            world.place(data, atlas, i, None, rng);
        }
        world.fill(data, atlas, rng);
        world.absorb_leftovers(data, map);
        world.color = (0..world.nations.len()).map(|i| atlas.settings.nation_colors[i % atlas.settings.nation_colors.len()]).collect();
        world
    }

    /// Places one nation: its capital, then its provinces around it. `region`
    /// limits it to some provinces (an empty stretch being filled).
    fn place(&mut self, data: &Data, atlas: &Atlas, nation: usize, region: Option<&[bool]>, rng: &mut Rng) {
        let map = atlas.map(self.map);
        let settings = &atlas.settings.placement;
        let country = &self.nations[nation];
        let want_continent = country.landed[0].option;
        let mut coast = Coast::of(data, country);
        let size = target_size(data, atlas, map, country);
        let land_type = country.option_id(data, "land");
        let mut note: Option<String> = None;

        let free = |owner: &[Option<usize>], p: usize| owner[p].is_none() && region.is_none_or(|r| r[p]);

        // Capitals already down, for spreading out.
        let capitals: Vec<(f32, f32)> = self
            .capital
            .iter()
            .filter(|&&c| c != usize::MAX)
            .map(|&c| map.provinces[c].middle)
            .collect();
        let diagonal = ((map.width * map.width) as f32 + (map.height * map.height) as f32).sqrt();

        let mut chosen: Option<(usize, Vec<usize>)> = None;
        // Try the wheel's continent first, then any continent.
        for any_continent in [false, true] {
            let candidates: Vec<usize> = (0..map.provinces.len())
                .filter(|&p| free(&self.owner, p))
                .filter(|&p| any_continent || map.provinces[p].continent == want_continent)
                .filter(|&p| match coast {
                    Coast::Landlocked => !map.provinces[p].coastal,
                    Coast::Coastal | Coast::Peninsula => map.provinces[p].coastal,
                    Coast::Island => map.provinces[p].coastal && self.island_fits(map, p, size, settings.island_slack, region),
                })
                .collect();
            if candidates.is_empty() {
                continue;
            }
            let mut best: Option<(f64, usize)> = None;
            for &p in &candidates {
                let prov = &map.provinces[p];
                let terrain_ok = atlas.terrains[prov.terrain].land_type.as_deref() == Some(land_type.as_str());
                let spread = capitals
                    .iter()
                    .map(|&(x, y)| ((x - prov.middle.0).powi(2) + (y - prov.middle.1).powi(2)).sqrt())
                    .fold(diagonal, f32::min)
                    / diagonal;
                let room = match coast {
                    Coast::Island => {
                        let s = self.landmass_size(map, p) as f64;
                        1.0 - (s - size as f64).abs() / s.max(size as f64)
                    }
                    _ => self.room(map, p, size, coast, want_continent, any_continent, region) as f64 / size as f64,
                };
                let shape = match coast {
                    Coast::Peninsula => prov.sea_edges as f64 / (prov.sea_edges + prov.land_edges).max(1) as f64,
                    _ => 0.0,
                };
                let score = settings.terrain_match * terrain_ok as u8 as f64
                    + settings.spread * spread as f64
                    + settings.room * room
                    + 2.0 * shape
                    + settings.jitter * rng.next_f32() as f64;
                if best.is_none_or(|(b, _)| score > b) {
                    best = Some((score, p));
                }
            }
            let capital = best.expect("candidates isn't empty").1;
            if any_continent {
                let here = &data.maps[self.map].continents[map.provinces[capital].continent].label;
                let wanted = &data.maps[self.map].continents[want_continent].label;
                note = Some(format!("Settled in {here}: {wanted} had no room left"));
            }
            let provinces = self.grow(atlas, map, capital, size, coast, &land_type, region);
            chosen = Some((capital, provinces));
            break;
        }

        // No land fits its border: fate redraws it, and its card says so.
        if chosen.is_none() {
            let fallback = match coast {
                Coast::Island | Coast::Peninsula => Some(Coast::Coastal),
                Coast::Landlocked | Coast::Coastal => None,
            };
            let candidates: Vec<usize> = (0..map.provinces.len()).filter(|&p| free(&self.owner, p)).collect();
            let pick = candidates
                .iter()
                .copied()
                .filter(|&p| map.provinces[p].continent == want_continent)
                .chain(candidates.iter().copied())
                .next();
            if let Some(capital) = pick {
                let new_coast = fallback.unwrap_or(if map.provinces[capital].coastal { Coast::Coastal } else { Coast::Landlocked });
                let reason = match coast {
                    Coast::Island => "no free island was the right size",
                    Coast::Landlocked => "no inland land was free",
                    _ => "no free coast was left",
                };
                self.set_coast(data, nation, new_coast);
                coast = new_coast;
                note = Some(format!("Fate redrew its borders: {reason}"));
                let provinces = self.grow(atlas, map, capital, size, coast, &land_type, region);
                chosen = Some((capital, provinces));
            }
        }

        if let Some((capital, provinces)) = chosen {
            for p in provinces {
                self.owner[p] = Some(nation);
            }
            self.capital[nation] = capital;
        } else {
            note = Some("There was no land left for it".to_string());
        }
        self.notes[nation] = note;
    }

    /// Changes a nation's Landlocked or Coastal result (when no land fits it).
    fn set_coast(&mut self, data: &Data, nation: usize, coast: Coast) {
        let w = data.wheel("coast");
        let id = match coast {
            Coast::Landlocked => "landlocked",
            Coast::Coastal => "coastal",
            Coast::Peninsula => "peninsula",
            Coast::Island => "island",
        };
        if let Some(o) = data.wheels[w].options.iter().position(|o| o.id == id) {
            self.nations[nation].landed[w] = Landed {
                option: o,
                value: 0.0,
                level: data.wheels[w].options[o].level,
            };
        }
    }

    fn landmass_size(&self, map: &WorldMap, p: usize) -> usize {
        map.provinces[p].landmasses.iter().map(|&l| map.provinces_on(l).len()).max().unwrap_or(1)
    }

    /// An island nation takes a whole island: every province on it must be
    /// free (and in the region), and the island not too big for the nation.
    fn island_fits(&self, map: &WorldMap, p: usize, size: usize, slack: f64, region: Option<&[bool]>) -> bool {
        let mut all = Vec::new();
        for &l in &map.provinces[p].landmasses {
            for q in map.provinces_on(l) {
                if !all.contains(&q) {
                    all.push(q);
                }
            }
        }
        all.iter().all(|&q| self.owner[q].is_none() && region.is_none_or(|r| r[q])) && (all.len() as f64) <= slack * size as f64
    }

    fn may_take(&self, map: &WorldMap, p: usize, coast: Coast, region: Option<&[bool]>) -> bool {
        self.owner[p].is_none() && region.is_none_or(|r| r[p]) && !(coast == Coast::Landlocked && map.provinces[p].coastal)
    }

    /// How many provinces (up to `size`) it could grow to from `start`.
    #[allow(clippy::too_many_arguments)]
    fn room(&self, map: &WorldMap, start: usize, size: usize, coast: Coast, continent: usize, any: bool, region: Option<&[bool]>) -> usize {
        let mut seen = vec![start];
        let mut i = 0;
        while i < seen.len() && seen.len() < size {
            for &q in &map.provinces[seen[i]].neighbours {
                if !seen.contains(&q) && self.may_take(map, q, coast, region) && (any || map.provinces[q].continent == continent) {
                    seen.push(q);
                }
            }
            i += 1;
        }
        seen.len().min(size)
    }

    /// The provinces a nation takes, growing from its capital: compact,
    /// on its own continent first, preferring its own terrain.
    #[allow(clippy::too_many_arguments)]
    fn grow(&self, atlas: &Atlas, map: &WorldMap, capital: usize, size: usize, coast: Coast, land_type: &str, region: Option<&[bool]>) -> Vec<usize> {
        let continent = map.provinces[capital].continent;
        let mut owned = vec![capital];
        let target = if coast == Coast::Island {
            // The whole island, whatever its size.
            let mut all = Vec::new();
            for &l in &map.provinces[capital].landmasses {
                for q in map.provinces_on(l) {
                    if !all.contains(&q) && self.may_take(map, q, coast, region) {
                        all.push(q);
                    }
                }
            }
            all.len().max(1)
        } else {
            size
        };
        let (cx, cy) = map.provinces[capital].middle;
        while owned.len() < target {
            let mut best: Option<(f64, usize)> = None;
            for &p in &owned {
                for &q in &map.provinces[p].neighbours {
                    if owned.contains(&q) || !self.may_take(map, q, coast, region) {
                        continue;
                    }
                    let prov = &map.provinces[q];
                    let touching = prov.neighbours.iter().filter(|n| owned.contains(n)).count() as f64;
                    let terrain_ok = atlas.terrains[prov.terrain].land_type.as_deref() == Some(land_type);
                    let far = ((prov.middle.0 - cx).powi(2) + (prov.middle.1 - cy).powi(2)).sqrt() as f64;
                    let score = 2.0 * touching + 1.0 * terrain_ok as u8 as f64 + 3.0 * (prov.continent == continent) as u8 as f64
                        + if coast == Coast::Peninsula && prov.coastal { 2.0 } else { 0.0 }
                        - 0.35 * far;
                    if best.is_none_or(|(b, bq)| score > b || (score == b && q < bq)) {
                        best = Some((score, q));
                    }
                }
            }
            match best {
                Some((_, q)) => owned.push(q),
                None => break,
            }
        }
        owned
    }

    /// Extra nations for the empty land, biggest stretch first.
    fn fill(&mut self, data: &Data, atlas: &Atlas, rng: &mut Rng) {
        let map = atlas.map(self.map);
        let settings = &atlas.settings;
        let mut hopeless = vec![false; map.provinces.len()];
        while self.nations.len() < settings.max_nations {
            let Some(region) = self.biggest_empty(map, &hopeless) else { break };
            let in_region: Vec<bool> = (0..map.provinces.len()).map(|p| region.contains(&p)).collect();
            // Its continent: the one most of the stretch is on.
            let mut counts = vec![0usize; data.maps[self.map].continents.len()];
            for &p in &region {
                counts[map.provinces[p].continent] += 1;
            }
            let continent = (0..counts.len()).max_by_key(|&k| (counts[k], std::cmp::Reverse(k))).expect("continents");
            let has_inland = region.iter().any(|&p| !map.provinces[p].coastal);
            let has_coast = region.iter().any(|&p| map.provinces[p].coastal);
            let whole_island = region.iter().all(|&p| {
                map.provinces[p].landmasses.iter().all(|&l| map.provinces_on(l).iter().all(|q| region.contains(q)))
            });

            // Spin until a nation fits this land (its border, then its size).
            let start = vec![Landed {
                option: continent,
                value: 0.0,
                level: continent as f64,
            }];
            let mut best: Option<(usize, Country)> = None;
            for _ in 0..settings.placement.fill_tries.max(1) {
                let c = roll_country(data, self.map, start.clone(), &self.nations, false, rng);
                let fits_border = match Coast::of(data, &c) {
                    Coast::Landlocked => has_inland,
                    Coast::Island => whole_island,
                    Coast::Coastal | Coast::Peninsula => has_coast,
                };
                if !fits_border {
                    continue;
                }
                let size = target_size(data, atlas, map, &c);
                let miss = size.abs_diff(region.len().min(size.max(1)));
                let over = size.saturating_sub(region.len());
                if best.as_ref().is_none_or(|(b, _)| over + miss < *b) {
                    best = Some((over + miss, c));
                }
                if size <= region.len() {
                    break;
                }
            }
            let Some((_, country)) = best else {
                // No roll fits this land at all: leave it for its neighbours.
                for &p in &region {
                    hopeless[p] = true;
                }
                continue;
            };
            let i = self.nations.len();
            self.nations.push(country);
            self.capital.push(usize::MAX);
            self.notes.push(None);
            self.place(data, atlas, i, Some(&in_region), rng);
            if self.capital[i] == usize::MAX {
                // It couldn't settle after all: take it back, and don't try here again.
                self.nations.pop();
                self.capital.pop();
                self.notes.pop();
                for &p in &region {
                    hopeless[p] = true;
                }
            }
        }
    }

    /// The largest stretch of free land (joined by land), if any.
    fn biggest_empty(&self, map: &WorldMap, skip: &[bool]) -> Option<Vec<usize>> {
        let mut seen = vec![false; map.provinces.len()];
        let mut best: Option<Vec<usize>> = None;
        for start in 0..map.provinces.len() {
            if seen[start] || self.owner[start].is_some() || skip[start] {
                continue;
            }
            let mut part = vec![start];
            seen[start] = true;
            let mut i = 0;
            while i < part.len() {
                for &q in &map.provinces[part[i]].neighbours {
                    if !seen[q] && self.owner[q].is_none() && !skip[q] {
                        seen[q] = true;
                        part.push(q);
                    }
                }
                i += 1;
            }
            if best.as_ref().is_none_or(|b| part.len() > b.len()) {
                best = Some(part);
            }
        }
        best
    }

    /// Empty land left over joins the neighbour it borders most, where that
    /// neighbour's border rules allow (a landlocked nation never takes coast).
    fn absorb_leftovers(&mut self, data: &Data, map: &WorldMap) {
        loop {
            let mut changed = false;
            for p in 0..map.provinces.len() {
                if self.owner[p].is_some() {
                    continue;
                }
                let mut best: Option<(usize, usize, usize)> = None; // (shared, -size, nation)
                for &q in &map.provinces[p].neighbours {
                    let Some(o) = self.owner[q] else { continue };
                    if Coast::of(data, &self.nations[o]) == Coast::Landlocked && map.provinces[p].coastal {
                        continue;
                    }
                    let shared = map.provinces[p].neighbours.iter().filter(|&&n| self.owner[n] == Some(o)).count();
                    let size = self.provinces_of(o).len();
                    let better = match best {
                        None => true,
                        Some((s, sz, n)) => shared > s || (shared == s && (size < sz || (size == sz && o < n))),
                    };
                    if better {
                        best = Some((shared, size, o));
                    }
                }
                if let Some((_, _, o)) = best {
                    self.owner[p] = Some(o);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        // Islets nobody borders by land go to the nearest nation across the
        // sea (an overseas province), if one is close enough.
        for p in 0..map.provinces.len() {
            if self.owner[p].is_some() || !map.provinces[p].coastal {
                continue;
            }
            let (x, y) = map.provinces[p].middle;
            let mut best: Option<(f32, usize)> = None;
            for q in 0..map.provinces.len() {
                let Some(o) = self.owner[q] else { continue };
                if Coast::of(data, &self.nations[o]) == Coast::Landlocked {
                    continue;
                }
                let (qx, qy) = map.provinces[q].middle;
                let d = (qx - x).powi(2) + (qy - y).powi(2);
                if best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, o));
                }
            }
            if let Some((d, o)) = best {
                if d.sqrt() <= OVERSEAS_REACH {
                    for q in self.unowned_landmass_of(map, p) {
                        self.owner[q] = Some(o);
                    }
                }
            }
        }
    }

    /// The free provinces on the landmass(es) of `p`, `p` included.
    fn unowned_landmass_of(&self, map: &WorldMap, p: usize) -> Vec<usize> {
        let mut out = vec![p];
        let mut i = 0;
        while i < out.len() {
            for &q in &map.provinces[out[i]].neighbours {
                if self.owner[q].is_none() && !out.contains(&q) {
                    out.push(q);
                }
            }
            i += 1;
        }
        out
    }
}

/// How far (in hexes) an unclaimed islet can be from a nation's land and
/// still join it overseas.
const OVERSEAS_REACH: f32 = 14.0;
