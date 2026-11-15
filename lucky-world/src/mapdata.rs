//! The three maps, read from `data/maps/`, and the world settings from
//! `data/world.toml` and `data/terrain.toml`.
//!
//! A map is a terrain grid (one letter per hex) and a list of provinces,
//! each with the hex it is centred on. Every land hex belongs to the province
//! it is nearest to walking over land, so moving a province's `at` moves its
//! borders. From that, each province knows its hexes, terrain, coast,
//! neighbours, rivers and which landmass (island) it is on.

use std::collections::{BTreeMap, VecDeque};
use std::path::Path;

use duckforge::prelude::Color;
use serde::Deserialize;

use crate::data::{parse_color, Data, DataError};
use crate::hex;

// ---------------------------------------------------------------------------
// The files as written
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TerrainFile {
    terrain: BTreeMap<String, TerrainEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TerrainEntry {
    name: String,
    kind: String,
    color: String,
    height: f32,
    #[serde(default)]
    land_type: Option<String>,
    #[serde(default)]
    decor: Option<String>,
    #[serde(default)]
    decor_count: f32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorldFile {
    max_nations: usize,
    min_provinces: usize,
    size_share: BTreeMap<String, f64>,
    placement: Placement,
    nation_colors: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub terrain_match: f64,
    pub spread: f64,
    pub room: f64,
    pub jitter: f64,
    pub island_slack: f64,
    pub fill_tries: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MapFile {
    #[serde(default, rename = "province")]
    provinces: Vec<ProvinceEntry>,
    #[serde(default, rename = "river")]
    rivers: Vec<RiverEntry>,
    #[serde(default, rename = "landmark")]
    landmarks: Vec<LandmarkEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProvinceEntry {
    id: String,
    name: String,
    continent: String,
    at: [usize; 2],
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RiverEntry {
    name: String,
    cells: Vec<[usize; 2]>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LandmarkEntry {
    kind: String,
    name: String,
    at: [usize; 2],
}

// ---------------------------------------------------------------------------
// As the game uses them
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainKind {
    Sea,
    Lake,
    Land,
}

#[derive(Debug, Clone)]
pub struct Terrain {
    pub letter: char,
    pub name: String,
    pub kind: TerrainKind,
    pub color: Color,
    pub height: f32,
    pub land_type: Option<String>,
    pub decor: Option<String>,
    pub decor_count: f32,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub max_nations: usize,
    pub min_provinces: usize,
    pub size_share: BTreeMap<String, f64>,
    pub placement: Placement,
    pub nation_colors: Vec<Color>,
}

#[derive(Debug, Clone)]
pub struct Province {
    pub id: String,
    pub name: String,
    /// Index into the map's continents (`data.maps[map].continents`).
    pub continent: usize,
    /// The hex it is centred on: (column, row).
    pub at: (usize, usize),
    /// Its hexes, as indexes into the grid (row * width + column).
    pub cells: Vec<usize>,
    /// The terrain most of its hexes have (an index into `Atlas::terrains`).
    pub terrain: usize,
    /// It touches the sea (a lake doesn't count).
    pub coastal: bool,
    /// How many of its hex edges face the sea.
    pub sea_edges: usize,
    /// How many of its hex edges face land outside it.
    pub land_edges: usize,
    /// Provinces it shares a land border with, in order.
    pub neighbours: Vec<usize>,
    /// A river runs through it.
    pub river: bool,
    /// It touches a lake.
    pub lake: bool,
    /// The landmasses (islands) its hexes are on.
    pub landmasses: Vec<usize>,
    /// The middle of its hexes on the map plane.
    pub middle: (f32, f32),
}

#[derive(Debug, Clone)]
pub struct River {
    pub name: String,
    pub cells: Vec<(usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct Landmark {
    pub kind: String,
    pub name: String,
    pub at: (usize, usize),
}

/// One map, ready to play on.
#[derive(Debug, Clone)]
pub struct WorldMap {
    /// Index into `data.maps`.
    pub map: usize,
    pub id: String,
    pub width: usize,
    pub height: usize,
    /// The terrain of every hex (index into `Atlas::terrains`), row by row.
    pub cells: Vec<usize>,
    /// The province each land hex belongs to.
    pub province_of: Vec<Option<usize>>,
    /// The landmass each land hex is on.
    pub landmass_of: Vec<Option<usize>>,
    pub landmass_count: usize,
    pub provinces: Vec<Province>,
    pub rivers: Vec<River>,
    pub landmarks: Vec<Landmark>,
}

impl WorldMap {
    pub fn index(&self, c: usize, r: usize) -> usize {
        r * self.width + c
    }

    pub fn cell(&self, i: usize) -> (usize, usize) {
        (i % self.width, i / self.width)
    }

    /// The provinces on one landmass.
    pub fn provinces_on(&self, landmass: usize) -> Vec<usize> {
        (0..self.provinces.len()).filter(|&p| self.provinces[p].landmasses.contains(&landmass)).collect()
    }

    /// The middle of the map plane, which the 3D world is centred on.
    pub fn plane_middle(&self) -> (f32, f32) {
        (self.width as f32 / 2.0, (self.height as f32 - 1.0) * hex::ROW / 2.0)
    }
}

/// Every map and the terrain and world settings.
#[derive(Debug, Clone)]
pub struct Atlas {
    pub terrains: Vec<Terrain>,
    pub settings: Settings,
    pub maps: Vec<WorldMap>,
}

fn err<T>(text: impl Into<String>) -> Result<T, DataError> {
    Err(DataError(text.into()))
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Atlas {
    pub fn load(data: &Data) -> Result<Atlas, DataError> {
        Self::load_from(Path::new(crate::data::DATA_DIR), data)
    }

    pub fn load_from(dir: &Path, data: &Data) -> Result<Atlas, DataError> {
        let read = |name: &str| -> Result<String, DataError> {
            let path = dir.join(name);
            std::fs::read_to_string(&path).map_err(|e| {
                DataError(format!("couldn't read {} ({e}); run the game from the lucky-world folder", path.display()))
            })
        };
        let terrains = parse_terrain(&read("terrain.toml")?)?;
        let settings = parse_settings(&read("world.toml")?, data)?;
        let mut maps = Vec::new();
        for (m, def) in data.maps.iter().enumerate() {
            let grid = read(&format!("maps/{}.txt", def.id))?;
            let file = read(&format!("maps/{}.toml", def.id))?;
            maps.push(parse_map(data, m, &terrains, &grid, &file)?);
        }
        Ok(Atlas {
            terrains,
            settings,
            maps,
        })
    }

    pub fn terrain_of(&self, map: &WorldMap, cell: usize) -> &Terrain {
        &self.terrains[map.cells[cell]]
    }

    /// The map the game's `data.maps[index]` is drawn on.
    pub fn map(&self, index: usize) -> &WorldMap {
        &self.maps[index]
    }
}

fn parse_terrain(text: &str) -> Result<Vec<Terrain>, DataError> {
    let t = "data/terrain.toml";
    let file: TerrainFile = toml::from_str(text).map_err(|e| DataError(format!("{t}: {}", one_line(&e.to_string()))))?;
    let mut out = Vec::new();
    for (key, e) in file.terrain {
        let mut chars = key.chars();
        let (Some(letter), None) = (chars.next(), chars.next()) else {
            return err(format!("{t}: terrain \"{key}\" must be one letter"));
        };
        if letter.is_whitespace() || !letter.is_ascii_graphic() {
            return err(format!("{t}: terrain \"{key}\" must be a visible ASCII letter"));
        }
        let kind = match e.kind.as_str() {
            "sea" => TerrainKind::Sea,
            "lake" => TerrainKind::Lake,
            "land" => TerrainKind::Land,
            other => return err(format!("{t}: terrain \"{key}\": kind \"{other}\" must be sea, lake or land")),
        };
        let color = parse_color(&e.color).ok_or_else(|| DataError(format!("{t}: terrain \"{key}\": bad colour \"{}\"", e.color)))?;
        out.push(Terrain {
            letter,
            name: e.name,
            kind,
            color,
            height: e.height,
            land_type: e.land_type,
            decor: e.decor,
            decor_count: e.decor_count.max(0.0),
        });
    }
    if !out.iter().any(|t| t.kind == TerrainKind::Sea) || !out.iter().any(|t| t.kind == TerrainKind::Land) {
        return err(format!("{t}: there must be at least one sea terrain and one land terrain"));
    }
    Ok(out)
}

fn parse_settings(text: &str, data: &Data) -> Result<Settings, DataError> {
    let w = "data/world.toml";
    let file: WorldFile = toml::from_str(text).map_err(|e| DataError(format!("{w}: {}", one_line(&e.to_string()))))?;
    if file.max_nations < 2 {
        return err(format!("{w}: max_nations must be at least 2"));
    }
    let size = &data.wheels[data.wheel("size")];
    for o in &size.options {
        match file.size_share.get(&o.id) {
            Some(s) if *s > 0.0 && *s < 1.0 => {}
            _ => return err(format!("{w}: [size_share] needs {} = (a share between 0 and 1) for the Country Size option \"{}\"", o.id, o.id)),
        }
    }
    let mut colors = Vec::new();
    for c in &file.nation_colors {
        colors.push(parse_color(c).ok_or_else(|| DataError(format!("{w}: bad nation colour \"{c}\"")))?);
    }
    if colors.len() < file.max_nations {
        return err(format!(
            "{w}: there are {} nation_colors but max_nations is {}: every nation needs its own colour",
            colors.len(),
            file.max_nations
        ));
    }
    Ok(Settings {
        max_nations: file.max_nations,
        min_provinces: file.min_provinces.max(1),
        size_share: file.size_share,
        placement: file.placement,
        nation_colors: colors,
    })
}

/// Reads one map from its grid and province file.
pub fn parse_map(data: &Data, m: usize, terrains: &[Terrain], grid: &str, file: &str) -> Result<WorldMap, DataError> {
    let def = &data.maps[m];
    let g = format!("data/maps/{}.txt", def.id);
    let f = format!("data/maps/{}.toml", def.id);

    // The grid.
    let rows: Vec<&str> = grid.lines().map(|l| l.trim_end_matches('\r')).filter(|l| !l.is_empty()).collect();
    if rows.len() < 4 {
        return err(format!("{g}: the map needs at least 4 rows"));
    }
    let width = rows[0].chars().count();
    let mut cells = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        if row.chars().count() != width {
            return err(format!(
                "{g}, line {}: it is {} letters long, but the first line is {}; every row must be as long",
                r + 1,
                row.chars().count(),
                width
            ));
        }
        for (c, ch) in row.chars().enumerate() {
            match terrains.iter().position(|t| t.letter == ch) {
                Some(t) => cells.push(t),
                None => {
                    return err(format!(
                        "{g}, line {}, column {}: \"{ch}\" isn't a terrain in data/terrain.toml",
                        r + 1,
                        c + 1
                    ));
                }
            }
        }
    }
    let height = rows.len();
    let is_land = |i: usize| terrains[cells[i]].kind == TerrainKind::Land;
    let is_sea = |i: usize| terrains[cells[i]].kind == TerrainKind::Sea;
    let is_lake = |i: usize| terrains[cells[i]].kind == TerrainKind::Lake;
    let idx = |c: usize, r: usize| r * width + c;
    let nbrs = |i: usize| hex::neighbours(i % width, i / width, width, height).map(move |(c, r)| r * width + c);

    // The province file.
    let file: MapFile = toml::from_str(file).map_err(|e| DataError(format!("{f}: {}", one_line(&e.to_string()))))?;
    if file.provinces.is_empty() {
        return err(format!("{f}: the map has no provinces"));
    }
    let mut provinces: Vec<Province> = Vec::new();
    for p in &file.provinces {
        let here = format!("{f}, province \"{}\"", p.id);
        if provinces.iter().any(|q| q.id == p.id) {
            return err(format!("{f}: two provinces are called \"{}\"", p.id));
        }
        let continent = def.continents.iter().position(|c| c.id == p.continent).ok_or_else(|| {
            let names: Vec<_> = def.continents.iter().map(|c| c.id.as_str()).collect();
            DataError(format!(
                "{here}: continent \"{}\" isn't one of this map's ({})",
                p.continent,
                names.join(", ")
            ))
        })?;
        let [c, r] = p.at;
        if c >= width || r >= height {
            return err(format!("{here}: at = [{c}, {r}] is off the map ({width} x {height})"));
        }
        if !is_land(idx(c, r)) {
            return err(format!(
                "{here}: at = [{c}, {r}] is {} in {g}; a province is centred on land",
                terrains[cells[idx(c, r)]].name
            ));
        }
        if let Some(q) = provinces.iter().find(|q| q.at == (c, r)) {
            return err(format!("{here}: at = [{c}, {r}] is also where \"{}\" is centred", q.id));
        }
        provinces.push(Province {
            id: p.id.clone(),
            name: p.name.clone(),
            continent,
            at: (c, r),
            cells: Vec::new(),
            terrain: 0,
            coastal: false,
            sea_edges: 0,
            land_edges: 0,
            neighbours: Vec::new(),
            river: false,
            lake: false,
            landmasses: Vec::new(),
            middle: (0.0, 0.0),
        });
    }
    for (k, cont) in def.continents.iter().enumerate() {
        if !provinces.iter().any(|p| p.continent == k) {
            return err(format!("{f}: the continent \"{}\" has no provinces, so nobody could be placed there", cont.id));
        }
    }

    // Landmasses: hexes joined by land.
    let n = cells.len();
    let mut landmass_of = vec![None; n];
    let mut landmass_count = 0;
    for start in 0..n {
        if !is_land(start) || landmass_of[start].is_some() {
            continue;
        }
        let mut stack = vec![start];
        landmass_of[start] = Some(landmass_count);
        while let Some(i) = stack.pop() {
            for j in nbrs(i) {
                if is_land(j) && landmass_of[j].is_none() {
                    landmass_of[j] = Some(landmass_count);
                    stack.push(j);
                }
            }
        }
        landmass_count += 1;
    }

    // Each land hex to the province nearest over land (all grow at once,
    // one step at a time; a tie goes to the province listed first).
    let mut province_of: Vec<Option<usize>> = vec![None; n];
    let mut queue = VecDeque::new();
    for (p, prov) in provinces.iter().enumerate() {
        let i = idx(prov.at.0, prov.at.1);
        province_of[i] = Some(p);
        queue.push_back(i);
    }
    while let Some(i) = queue.pop_front() {
        let p = province_of[i];
        for j in nbrs(i) {
            if is_land(j) && province_of[j].is_none() {
                province_of[j] = p;
                queue.push_back(j);
            }
        }
    }
    // Land no province could walk to (a rock with no province of its own)
    // goes to the nearest province over the sea.
    for i in 0..n {
        if is_land(i) && province_of[i].is_none() {
            let here = hex::center(i % width, i / width);
            let nearest = (0..provinces.len())
                .min_by(|&a, &b| {
                    let d = |p: usize| {
                        let (x, y) = hex::center(provinces[p].at.0, provinces[p].at.1);
                        (x - here.0).powi(2) + (y - here.1).powi(2)
                    };
                    d(a).total_cmp(&d(b))
                })
                .expect("there are provinces");
            province_of[i] = Some(nearest);
        }
    }

    // What each province is like.
    let mut counts = vec![vec![0usize; terrains.len()]; provinces.len()];
    for i in 0..n {
        let Some(p) = province_of[i] else { continue };
        provinces[p].cells.push(i);
        counts[p][cells[i]] += 1;
        if let Some(l) = landmass_of[i] {
            if !provinces[p].landmasses.contains(&l) {
                provinces[p].landmasses.push(l);
            }
        }
        for j in nbrs(i) {
            if is_sea(j) {
                provinces[p].coastal = true;
                provinces[p].sea_edges += 1;
            } else if is_lake(j) {
                provinces[p].lake = true;
            } else if let Some(q) = province_of[j] {
                if q != p {
                    provinces[p].land_edges += 1;
                    if !provinces[p].neighbours.contains(&q) {
                        provinces[p].neighbours.push(q);
                    }
                }
            }
        }
    }
    for (p, prov) in provinces.iter_mut().enumerate() {
        prov.neighbours.sort();
        prov.landmasses.sort();
        let mut best = 0;
        for (t, &k) in counts[p].iter().enumerate() {
            if k > counts[p][best] {
                best = t;
            }
        }
        prov.terrain = best;
        let (sx, sy) = prov.cells.iter().fold((0.0, 0.0), |(sx, sy), &i| {
            let (x, y) = hex::center(i % width, i / width);
            (sx + x, sy + y)
        });
        prov.middle = (sx / prov.cells.len() as f32, sy / prov.cells.len() as f32);
    }

    // Rivers.
    let mut rivers = Vec::new();
    for rv in &file.rivers {
        let here = format!("{f}, river \"{}\"", rv.name);
        if rv.cells.len() < 2 {
            return err(format!("{here}: a river needs at least two hexes"));
        }
        let mut path = Vec::new();
        for (k, &[c, r]) in rv.cells.iter().enumerate() {
            if c >= width || r >= height {
                return err(format!("{here}: [{c}, {r}] is off the map"));
            }
            if let Some(&(pc, pr)) = path.last() {
                if hex::distance((pc, pr), (c, r)) != 1 {
                    return err(format!("{here}: [{pc}, {pr}] and [{c}, {r}] aren't next to each other"));
                }
            }
            let i = idx(c, r);
            if !is_land(i) && k + 1 != rv.cells.len() {
                return err(format!("{here}: [{c}, {r}] is water; only a river's last hex (its mouth) may be"));
            }
            if let Some(p) = province_of[i] {
                provinces[p].river = true;
            }
            path.push((c, r));
        }
        rivers.push(River {
            name: rv.name.clone(),
            cells: path,
        });
    }

    // Landmarks.
    let mut landmarks = Vec::new();
    for l in &file.landmarks {
        let [c, r] = l.at;
        if c >= width || r >= height || !is_land(idx(c, r)) {
            return err(format!("{f}, landmark \"{}\": at = [{c}, {r}] must be a land hex", l.name));
        }
        landmarks.push(Landmark {
            kind: l.kind.clone(),
            name: l.name.clone(),
            at: (c, r),
        });
    }

    Ok(WorldMap {
        map: m,
        id: def.id.clone(),
        width,
        height,
        cells,
        province_of,
        landmass_of,
        landmass_count,
        provinces,
        rivers,
        landmarks,
    })
}
