//! The 3D world map: hex terrain, borders, rivers, forests and mountains,
//! cities, capitals and landmarks; an orbit camera; picking a province with
//! the mouse; and the minimap.
//!
//! The provinces are hexes, but the ground isn't drawn as a board. Each hex
//! is cut into 24 small triangles whose corners are shared with its
//! neighbours: heights and colours are worked out at those corners (the
//! middle of a hex keeps its own height and colour, a corner takes the
//! average of the three hexes round it), so land rolls from one hex into the
//! next, slopes down to the shore, and colours run into each other. The
//! engine paints each triangle one flat colour, so the light (from the
//! north-west), the sand along the shore, the snow and each nation's tint are
//! all baked into those colours. How it looks is in `data/map_style.toml`.
//!
//! Picking doesn't look at any of that: `pick` uses each hex's own height
//! (`heights`), as it always did.
//!
//! Everything is built once into a few big meshes that never change while
//! you look (the graphics card keeps them), and nothing here turns on bloom,
//! supersampling, lamps or shadows: those would move the 3D onto the CPU
//! (ISSUES.md, Duck Forge item 15). `LUCKY_MAP_STATS=1` prints the triangle
//! count and the build time of each map.

use std::collections::BTreeMap;
use std::f32::consts::PI;

use duckforge::prelude::*;

use crate::hex;
use crate::mapdata::{Atlas, TerrainKind, WorldMap};
use crate::world::World;

/// Cells per terrain chunk along each side (chunks off screen are skipped).
const CHUNK: usize = 12;
/// The sea's floor: one big plane under everything.
const SEA_LEVEL: f32 = -0.08;
/// The surface of every sea and lake, and the height of every shore.
const WATER: f32 = -0.05;
/// Rings of open sea made up round the map, so coasts and the sea's colours
/// run on past its edge (even, so odd rows stay odd). Land comes within a
/// hex of the edge, and the shelf reaches 3 hexes out, so with 4 rings the
/// sea has turned deep again by the last ring and meets the plain sea
/// beyond without a seam.
const PAD: usize = 4;
/// How far in from a hex's edge its inner ring of corners is (half-way to the middle).
const INNER: f32 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Terrain tinted with each nation's colour.
    Political,
    /// Terrain only.
    Terrain,
}

/// The models dotted on the map, from assets/models/ (Duck Mold).
pub struct Models {
    pub capital: Mesh,
    /// The capital's roofs (triangle range), recoloured per nation.
    pub capital_roofs: Option<std::ops::Range<usize>>,
    pub town: Mesh,
    pub decor: Vec<(String, Mesh)>,
    pub landmarks: Vec<(String, Mesh)>,
    /// The war's pieces (soldier, tank, jet, warship, transport), each with
    /// the triangles recoloured per nation (uniform, hull, wings, flag).
    pub war: Vec<(Mesh, Option<std::ops::Range<usize>>)>,
}

/// A war piece on the map (`Models::war` in this order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece {
    Soldier,
    Tank,
    Jet,
    Warship,
    Transport,
}

/// One model on the war layer: where it stands, in whose colour.
#[derive(Debug, Clone, Copy)]
pub struct WarPiece {
    pub piece: Piece,
    /// A land cell to stand on (its height), or none: at sea level.
    pub cell: Option<usize>,
    /// Map-plane position.
    pub at: (f32, f32),
    pub lift: f32,
    pub color: Color,
    pub turn: f32,
}

/// Everything the war layer shows (rebuilt when the war moves).
#[derive(Debug, Clone, Default)]
pub struct WarScene {
    pub pieces: Vec<WarPiece>,
    /// Province pairs held by enemies at war (the front line between them).
    pub fronts: Vec<(usize, usize)>,
    /// Provinces held by someone other than their owner, and the holder's colour.
    pub occupied: Vec<(usize, Color)>,
    /// Attacks under way: from province, to province, colour.
    pub arrows: Vec<(usize, usize, Color)>,
}

const DECOR: [&str; 7] = ["tree", "pine", "palm", "cactus", "peak", "hill", "reeds"];
const LANDMARKS: [&str; 6] = ["lighthouse", "pyramid", "great_tower", "temple", "arch", "colossus"];

impl Models {
    pub fn load() -> Result<Models, duckforge::LoadError> {
        let path = |name: &str| format!("assets/models/{name}.glb");
        let capital_model = crate::files::load(path("capital"), |p| duckforge::glb::GlbModel::load(p))?;
        let capital_roofs = capital_model.part("roofs").map(|p| p.range());
        let mut decor = Vec::new();
        for name in DECOR {
            decor.push((name.to_string(), crate::files::load(path(name), |p| Mesh::load_glb(p))?));
        }
        let mut landmarks = Vec::new();
        for name in LANDMARKS {
            landmarks.push((name.to_string(), crate::files::load(path(name), |p| Mesh::load_glb(p))?));
        }
        let mut war = Vec::new();
        for (name, part) in [("soldier", "uniform"), ("tank", "hull"), ("jet", "wings"), ("warship", "flag"), ("transport", "flag")] {
            let m = crate::files::load(path(name), |p| duckforge::glb::GlbModel::load(p))?;
            let range = m.part(part).map(|p| p.range());
            war.push((m.to_mesh(), range));
        }
        Ok(Models {
            capital: capital_model.to_mesh(),
            capital_roofs,
            town: crate::files::load(path("town"), |p| Mesh::load_glb(p))?,
            decor,
            landmarks,
            war,
        })
    }

    fn decor(&self, name: &str) -> Option<&Mesh> {
        self.decor.iter().find(|(n, _)| n == name).map(|(_, m)| m)
    }

    fn landmark(&self, name: &str) -> Option<&Mesh> {
        self.landmarks.iter().find(|(n, _)| n == name).map(|(_, m)| m)
    }
}

// ---------------------------------------------------------------------------
// The style: data/map_style.toml
// ---------------------------------------------------------------------------

/// A "#rrggbb" colour in the style file.
#[derive(Debug, Clone, Copy)]
struct Hex(Color);

impl<'de> serde::Deserialize<'de> for Hex {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Hex, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(d)?;
        crate::data::parse_color(&text).map(Hex).ok_or_else(|| serde::de::Error::custom(format!("\"{text}\" isn't a colour like \"#e2b84a\"")))
    }
}

/// `data/map_style.toml`: every colour and size of the map's look (the file
/// says what each one does).
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Style {
    land: LandStyle,
    water: WaterStyle,
    borders: BorderStyle,
    lines: LineStyle,
    #[serde(default)]
    terrain: BTreeMap<String, LookFile>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LandStyle {
    tint: f32,
    tint_dark: f32,
    tint_light: f32,
    minimap_tint: f32,
    unowned: f32,
    overlay_relief: f32,
    sun: [f32; 3],
    relief: f32,
    blend: f32,
    sand: Hex,
    sand_width: f32,
    snow: Hex,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WaterStyle {
    deep: Hex,
    mid: Hex,
    shelf: Hex,
    coast: Hex,
    lake: Hex,
    lake_shore: Hex,
    swell: f32,
    river: Hex,
    river_bank: Hex,
    river_source: f32,
    river_mouth: f32,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BorderStyle {
    nation: f32,
    nation_dark: f32,
    coast: f32,
    edge: Hex,
    gold: Hex,
    gold_light: Hex,
    gold_widths: [f32; 3],
    selected: Hex,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LineStyle {
    color: Hex,
    near: f32,
    far: f32,
    near_distance: f32,
    far_distance: f32,
    widths: [f32; 3],
    switch: [f32; 2],
}

/// One terrain's table in the style file.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct LookFile {
    tone: f32,
    color: Option<Hex>,
    patch: Option<Hex>,
    patches: f32,
    grain: f32,
    rough: f32,
    relief: f32,
    ridge: f32,
    dunes: f32,
    snow: Option<f32>,
    shore: Option<Hex>,
    bushes: f32,
    bush: Option<Hex>,
}

impl Default for LookFile {
    fn default() -> Self {
        LookFile {
            tone: 0.8,
            color: None,
            patch: None,
            patches: 0.0,
            grain: 0.05,
            rough: 0.008,
            relief: 3.0,
            ridge: 0.0,
            dunes: 0.0,
            snow: None,
            shore: None,
            bushes: 0.0,
            bush: None,
        }
    }
}

/// How one terrain is painted (a `LookFile` with its colours worked out).
#[derive(Debug, Clone)]
struct Look {
    color: [f32; 3],
    patch: [f32; 3],
    patches: f32,
    grain: f32,
    rough: f32,
    relief: f32,
    ridge: f32,
    dunes: f32,
    /// The height it is snowy above (very high: never).
    snow: f32,
    shore: [f32; 3],
    bushes: f32,
    bush: Color,
}

impl Style {
    /// Reads `data/map_style.toml`. A mistake in it is said once, and the
    /// copy built into the game is used instead.
    fn load() -> Style {
        const BUILT_IN: &str = include_str!("../data/map_style.toml");
        static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let path = std::path::Path::new(crate::data::DATA_DIR).join("map_style.toml");
        if let Ok(text) = crate::files::read_text(&path) {
            match toml::from_str::<Style>(&text) {
                Ok(style) => return style,
                Err(e) => {
                    if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                        let why = e.to_string().split_whitespace().collect::<Vec<_>>().join(" ");
                        eprintln!("{}: {why}: the map is drawn in the built-in style instead", path.display());
                    }
                }
            }
        }
        toml::from_str(BUILT_IN).expect("the data/map_style.toml built into the game is valid")
    }

    /// Each terrain's look, in the atlas's order.
    fn looks(&self, atlas: &Atlas) -> Vec<Look> {
        let plain = LookFile::default();
        atlas
            .terrains
            .iter()
            .map(|t| {
                let file = self.terrain.get(&t.letter.to_string()).unwrap_or(&plain);
                let color = match file.color {
                    Some(c) => rgb(c.0),
                    None => scale3(rgb(t.color), file.tone),
                };
                Look {
                    color,
                    patch: file.patch.map_or(color, |c| rgb(c.0)),
                    patches: file.patches.clamp(0.0, 1.0),
                    grain: file.grain.clamp(0.0, 0.6),
                    rough: file.rough.clamp(0.0, 0.2),
                    relief: file.relief.max(0.0),
                    ridge: file.ridge.clamp(0.0, 0.3),
                    dunes: file.dunes.clamp(0.0, 0.1),
                    snow: file.snow.unwrap_or(f32::MAX),
                    shore: file.shore.map_or(rgb(self.land.sand.0), |c| rgb(c.0)),
                    bushes: file.bushes.clamp(0.0, 8.0),
                    bush: file.bush.map_or(paint(color, 0.8), |c| c.0),
                }
            })
            .collect()
    }

    /// Where the light comes from, in the world (length 1).
    fn sun(&self) -> Vec3 {
        let [x, y, z] = self.land.sun;
        let sun = vec3(x, y.max(0.05), z);
        sun * (1.0 / sun.length())
    }
}

// ---------------------------------------------------------------------------
// Small tools: steady noise and colours as three numbers
// ---------------------------------------------------------------------------

/// A small, steady hash: the same hex always gets the same jitter.
fn hash(a: usize, b: usize, salt: usize) -> f32 {
    let mut n = (a as u32).wrapping_mul(374_761_393) ^ (b as u32).wrapping_mul(668_265_263) ^ (salt as u32).wrapping_mul(2_246_822_519);
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    ((n ^ (n >> 16)) & 0xffff) as f32 / 65535.0
}

/// Smooth noise over the map plane, 0 to 1: the same place always gets the
/// same value, and nearby places get nearly the same.
fn noise(x: f32, y: f32, salt: usize) -> f32 {
    // Moved clear of zero (the plane goes a little negative), so cutting off
    // the fraction is rounding down: `floor` is a slow library call here, and
    // this runs tens of thousands of times per map.
    let (x, y) = (x + 4096.0, y + 4096.0);
    let (ix, iy) = (x as usize, y as usize);
    let ease = |t: f32| t * t * (3.0 - 2.0 * t);
    let (u, v) = (ease(x - ix as f32), ease(y - iy as f32));
    let a = hash(ix, iy, salt);
    let b = hash(ix + 1, iy, salt);
    let c = hash(ix, iy + 1, salt);
    let d = hash(ix + 1, iy + 1, salt);
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

/// 0 below `from`, 1 above `to`, smooth between.
fn smooth(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Where the land's second colour shows (0 to 1), in soft patches.
fn patch_noise(x: f32, y: f32) -> f32 {
    smooth(0.3, 0.72, 0.65 * noise(x * 0.6, y * 0.6, 9) + 0.35 * noise(x * 1.7, y * 1.7, 10))
}

/// Long, winding ripples across the plane, -1 to 1 (dunes). A ripple is
/// about 1.3 hexes long: the ground's corners are about 0.3 apart, and
/// shorter ripples fall between them and show as stripes (aliasing).
fn ripple(x: f32, y: f32) -> f32 {
    (x * 2.9 + y * 3.8 + 4.0 * noise(x * 0.45, y * 0.45, 13)).sin()
}

fn shade(c: Color, k: f32) -> Color {
    Color::rgb(
        (c.r as f32 * k).clamp(0.0, 255.0) as u8,
        (c.g as f32 * k).clamp(0.0, 255.0) as u8,
        (c.b as f32 * k).clamp(0.0, 255.0) as u8,
    )
}

fn rgb(c: Color) -> [f32; 3] {
    [c.r as f32, c.g as f32, c.b as f32]
}

fn scale3(c: [f32; 3], k: f32) -> [f32; 3] {
    [c[0] * k, c[1] * k, c[2] * k]
}

fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// A colour times the light on it, as the engine's colour.
fn paint(c: [f32; 3], light: f32) -> Color {
    Color::rgb((c[0] * light).clamp(0.0, 255.0) as u8, (c[1] * light).clamp(0.0, 255.0) as u8, (c[2] * light).clamp(0.0, 255.0) as u8)
}

/// A colour pushed away from grey (`k` above 1) and darkened by `dark`.
fn vivid(c: Color, k: f32, dark: f32) -> Color {
    let c = rgb(c);
    let grey = 0.3 * c[0] + 0.55 * c[1] + 0.15 * c[2];
    paint(mix3([grey; 3], c, k), dark)
}

/// A nation's colour over the land: dark land keeps it darker, pale land paler.
fn tint(style: &LandStyle, land: [f32; 3], nation: Color, strength: f32) -> [f32; 3] {
    let light = (0.3 * land[0] + 0.55 * land[1] + 0.15 * land[2]) / 128.0;
    let k = style.tint_dark * (1.0 + style.tint_light * (light - 1.0)).clamp(0.6, 1.25);
    mix3(land, scale3(rgb(nation), k), strength)
}

fn mix(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a + (b - a) * t
}

/// How lit a triangle of ground is: 1 on the flat, more where it faces the
/// sun, less where it faces away. `steep` makes slopes count for more than
/// they really are (the land is nearly flat).
fn slope_light(a: Vec3, b: Vec3, c: Vec3, steep: f32, sun: Vec3, gain: f32) -> f32 {
    let (u, w) = (b - a, c - a);
    let n = vec3(u.x, u.y * steep, u.z).cross(vec3(w.x, w.y * steep, w.z));
    let len = n.length();
    if len < 1e-12 {
        return 1.0;
    }
    let facing = n.dot(sun) / len * n.y.signum();
    (1.0 + gain * (facing - sun.y)).clamp(0.62, 1.3)
}

// ---------------------------------------------------------------------------
// The field: every hex's own numbers, and the corners hexes share
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Deep,
    Shallow,
    Lake,
    Land,
}

/// One hex of the padded grid.
#[derive(Debug, Clone, Copy)]
struct Cell {
    kind: Kind,
    /// Its terrain (an index into the looks).
    look: usize,
    /// Land: the height of its middle (the height picking uses).
    h: f32,
    /// Land: its own little change of brightness.
    tone: f32,
    /// Water: how light it is, from 0 (open sea) to 1 (at the shore).
    s: f32,
    /// Land: how much of its roughness it keeps (0 under a city: flat).
    calm: f32,
    /// Its index in the map (`usize::MAX` for the made-up sea round it).
    actual: usize,
}

const OPEN_SEA: Cell = Cell {
    kind: Kind::Deep,
    look: 0,
    h: 0.0,
    tone: 1.0,
    s: 0.0,
    calm: 1.0,
    actual: usize::MAX,
};

/// A corner of the ground, shared by the hexes that meet there.
#[derive(Debug, Clone, Copy, Default)]
struct Vx {
    /// On the map plane.
    x: f32,
    py: f32,
    /// Its height in the world.
    y: f32,
    /// The land's colour here, before the light.
    alb: [f32; 3],
    /// How light the water is here (1 at a shore).
    s: f32,
    /// Where the land's second colour shows here (`patch_noise`).
    n: f32,
    /// It touches water: the shore.
    wet: bool,
}

/// The neighbour of a cell in direction `d`: 0 east, then clockwise on the
/// map (south-east, south-west, west, north-west, north-east). A hex's edge
/// `k` (from corner `k` to `k + 1`) faces direction `k + 1`.
fn step(c: i32, r: i32, d: usize) -> (i32, i32) {
    let odd = r & 1;
    match d {
        0 => (c + 1, r),
        1 => (c + odd, r + 1),
        2 => (c + odd - 1, r + 1),
        3 => (c - 1, r),
        4 => (c + odd - 1, r - 1),
        _ => (c + odd, r - 1),
    }
}

/// The map cell next to `(c, r)` across its edge `k`, if it's on the map.
fn across(map: &WorldMap, c: usize, r: usize, k: usize) -> Option<usize> {
    let (nc, nr) = step(c as i32, r as i32, (k + 1) % 6);
    if nc < 0 || nr < 0 || nc as usize >= map.width || nr as usize >= map.height {
        return None;
    }
    debug_assert_eq!(hex::shared_edge(hex::center(c, r), hex::center(nc as usize, nr as usize)).0, k);
    Some(map.index(nc as usize, nr as usize))
}

fn land_colour(look: &Look, tone: f32, n: f32) -> [f32; 3] {
    mix3(scale3(look.color, tone), look.patch, look.patches * n)
}

/// The map with `PAD` rings of open sea round it: each hex's numbers, and
/// the corners and edge middles hexes share, each worked out once (so two
/// hexes always agree, to the last bit, about the corner between them).
struct Field<'a> {
    style: &'a Style,
    looks: &'a [Look],
    pw: usize,
    ph: usize,
    cells: Vec<Cell>,
    /// Each hex's top (north) corner, shared with its north-west and north-east neighbours...
    top: Vec<Vx>,
    /// ...its bottom corner, shared with the south-east and south-west ones...
    bottom: Vec<Vx>,
    /// ...and the middles of its east, south-east and south-west edges.
    mid: Vec<[Vx; 3]>,
}

impl<'a> Field<'a> {
    fn new(style: &'a Style, looks: &'a [Look], atlas: &Atlas, map: &WorldMap, heights: &[f32], calm: &[f32]) -> Field<'a> {
        let (pw, ph) = (map.width + 2 * PAD, map.height + 2 * PAD);
        let mut cells = vec![OPEN_SEA; pw * ph];
        for i in 0..map.cells.len() {
            let (c, r) = map.cell(i);
            let t = atlas.terrain_of(map, i);
            cells[(r + PAD) * pw + c + PAD] = Cell {
                kind: match t.kind {
                    TerrainKind::Land => Kind::Land,
                    TerrainKind::Lake => Kind::Lake,
                    TerrainKind::Sea if t.letter == '~' => Kind::Deep,
                    TerrainKind::Sea => Kind::Shallow,
                },
                look: map.cells[i],
                h: heights[i],
                tone: 0.96 + 0.08 * hash(c, r, 2),
                s: 0.0,
                calm: calm[i],
                actual: i,
            };
        }
        // How many hexes each bit of water is from land (up to 3).
        let mut far = vec![u8::MAX; pw * ph];
        let mut queue: Vec<usize> = (0..pw * ph).filter(|&i| cells[i].kind == Kind::Land).collect();
        for &i in &queue {
            far[i] = 0;
        }
        let mut head = 0;
        while head < queue.len() {
            let i = queue[head];
            head += 1;
            if far[i] >= 3 {
                continue;
            }
            for d in 0..6 {
                let (c, r) = step((i % pw) as i32, (i / pw) as i32, d);
                if c >= 0 && r >= 0 && (c as usize) < pw && (r as usize) < ph {
                    let j = r as usize * pw + c as usize;
                    if far[j] == u8::MAX {
                        far[j] = far[i] + 1;
                        queue.push(j);
                    }
                }
            }
        }
        for (cell, &d) in cells.iter_mut().zip(&far) {
            cell.s = match (cell.kind, d) {
                (Kind::Deep, 1) => 0.5,
                (Kind::Deep, 2) => 0.24,
                (Kind::Deep, 3) => 0.08,
                (Kind::Shallow, 1) => 0.8,
                (Kind::Shallow, 2) => 0.66,
                (Kind::Shallow, _) => 0.55,
                (Kind::Lake, _) => 0.5,
                _ => 0.0,
            };
        }
        // The shared corners.
        let get = |c: i32, r: i32| (c >= 0 && r >= 0 && (c as usize) < pw && (r as usize) < ph).then(|| cells[r as usize * pw + c as usize]);
        let mut top = vec![Vx::default(); pw * ph];
        let mut bottom = vec![Vx::default(); pw * ph];
        let mut mid = vec![[Vx::default(); 3]; pw * ph];
        for pr in 0..ph as i32 {
            for pc in 0..pw as i32 {
                let i = pr as usize * pw + pc as usize;
                let (x, y) = centre(pc as usize, pr as usize);
                let me = Some(cells[i]);
                let near = |d: usize| {
                    let (c, r) = step(pc, pr, d);
                    get(c, r)
                };
                top[i] = shared(looks, &[me, near(4), near(5)], x, y - hex::RADIUS);
                bottom[i] = shared(looks, &[me, near(1), near(2)], x, y + hex::RADIUS);
                mid[i] = [
                    shared(looks, &[me, near(0)], x + 0.5, y),
                    shared(looks, &[me, near(1)], x + 0.25, y + hex::ROW / 2.0),
                    shared(looks, &[me, near(2)], x - 0.25, y + hex::ROW / 2.0),
                ];
            }
        }
        Field {
            style,
            looks,
            pw,
            ph,
            cells,
            top,
            bottom,
            mid,
        }
    }

    /// A hex's 19 corners: its middle (0), its six corners (1 to 6, in
    /// `hex::corner`'s order), the middles of its six edges (7 to 12: edge
    /// `k` runs from corner `k` to corner `k + 1`) and an inner ring half-way
    /// from the middle to each corner (13 to 18). Not for the outermost ring
    /// of the padded grid.
    fn corners(&self, pc: usize, pr: usize) -> [Vx; 19] {
        let i = pr * self.pw + pc;
        let me = self.cells[i];
        let look = &self.looks[me.look];
        let land = me.kind == Kind::Land;
        let near = |d: usize| {
            let (c, r) = step(pc as i32, pr as i32, d);
            r as usize * self.pw + c as usize
        };
        let (x, y) = centre(pc, pr);
        let n = if land { patch_noise(x, y) } else { 0.0 };
        let mut v = [Vx::default(); 19];
        v[0] = Vx {
            x,
            py: y,
            y: if land { me.h } else { WATER },
            alb: if land { land_colour(look, me.tone, n) } else { [0.0; 3] },
            s: me.s,
            n,
            wet: !land,
        };
        v[1] = self.top[near(1)];
        v[2] = self.bottom[i];
        v[3] = self.top[near(2)];
        v[4] = self.bottom[near(4)];
        v[5] = self.top[i];
        v[6] = self.bottom[near(5)];
        for k in 0..6 {
            let d = (k + 1) % 6;
            v[7 + k] = if d < 3 { self.mid[i][d] } else { self.mid[near(d)][d - 3] };
        }
        if land {
            // Its own colour, running into its neighbours' at the edges.
            for q in &mut v[1..13] {
                q.alb = mix3(land_colour(look, me.tone, q.n), q.alb, self.style.land.blend.clamp(0.0, 1.0));
            }
        }
        for k in 0..6 {
            let corner = v[1 + k];
            let (ix, iy) = ((x + corner.x) / 2.0, (y + corner.py) / 2.0);
            let n = if land { patch_noise(ix, iy) } else { 0.0 };
            let mut q = Vx {
                x: ix,
                py: iy,
                y: WATER,
                alb: [0.0; 3],
                s: (me.s + corner.s) / 2.0,
                n,
                wet: !land,
            };
            if land {
                q.y = if me.calm == 0.0 {
                    me.h
                } else {
                    (me.h + corner.y) / 2.0 + me.calm * (look.rough * (2.0 * hash(pc, pr, 70 + k) - 1.0) + look.dunes * ripple(ix, iy))
                };
                q.alb = mix3(land_colour(look, me.tone, n), corner.alb, 0.5);
            }
            v[13 + k] = q;
        }
        v
    }
}

/// Where a padded cell's middle is on the map plane.
fn centre(pc: usize, pr: usize) -> (f32, f32) {
    let (x, y) = hex::center(pc, pr);
    (x - PAD as f32, y - PAD as f32 * hex::ROW)
}

/// The corner at `(x, py)` shared by the cells `round` it (`None`: off the
/// padded grid, open sea).
fn shared(looks: &[Look], round: &[Option<Cell>], x: f32, py: f32) -> Vx {
    // Only land is patchy.
    let n = if round.iter().any(|c| c.is_some_and(|c| c.kind == Kind::Land)) { patch_noise(x, py) } else { 0.0 };
    let mut v = Vx {
        x,
        py,
        y: WATER,
        alb: [0.0; 3],
        s: 0.0,
        n,
        wet: false,
    };
    let (mut land, mut h, mut rough, mut ridge, mut dunes) = (0.0f32, 0.0f32, f32::MAX, f32::MAX, f32::MAX);
    for cell in round {
        match cell {
            Some(c) if c.kind == Kind::Land => {
                let look = &looks[c.look];
                v.alb = add3(v.alb, land_colour(look, c.tone, n));
                land += 1.0;
                h += c.h;
                rough = rough.min(look.rough * c.calm);
                ridge = ridge.min(look.ridge * c.calm);
                dunes = dunes.min(look.dunes * c.calm);
            }
            Some(c) => {
                v.s += c.s;
                v.wet = true;
            }
            None => v.wet = true,
        }
    }
    if land == 0.0 {
        v.s /= round.len() as f32;
        return v;
    }
    v.alb = scale3(v.alb, 1.0 / land);
    if v.wet {
        v.s = 1.0;
    } else {
        v.y = h / land + ridge + rough * (2.0 * noise(x * 3.3, py * 3.3, 5) - 1.0) + dunes * ripple(x, py);
    }
    v
}

/// The ground of one chunk of the map, painted both ways, with what is
/// needed to paint it again (a nation's tint, an overlay) without building it.
#[derive(Default)]
struct Chunk {
    /// [Nations view, Terrain view]: the same triangles in different colours.
    meshes: [Mesh; 2],
    /// Each triangle's colour before the light and any tint...
    alb: Vec<[f32; 3]>,
    /// ...how lit it is...
    light: Vec<f32>,
    /// ...and the map cell it belongs to.
    cell: Vec<u32>,
}

/// What every triangle of one hex has in common.
struct Pen<'a> {
    cell: u32,
    look: &'a Look,
    snow: [f32; 3],
    sun: Vec3,
    gain: f32,
}

impl Chunk {
    /// Adds a triangle of ground (corners already in the Terrain mesh).
    fn ground(&mut self, [a, b, c]: [usize; 3], mut alb: [f32; 3], pen: &Pen) {
        let mesh = &mut self.meshes[1];
        let (pa, pb, pc) = (mesh.vertices[a], mesh.vertices[b], mesh.vertices[c]);
        let height = (pa.y + pb.y + pc.y) / 3.0;
        if height > pen.look.snow {
            alb = mix3(alb, pen.snow, smooth(pen.look.snow, pen.look.snow + 0.07, height));
        }
        let grain = 1.0 + pen.look.grain * (hash(self.cell.len(), pen.cell as usize, 3) - 0.5);
        let light = slope_light(pa, pb, pc, pen.look.relief, pen.sun, pen.gain) * grain;
        mesh.add_triangle(a, b, c, paint(alb, light));
        self.alb.push(alb);
        self.light.push(light);
        self.cell.push(pen.cell);
    }
}

/// A band along one hex edge, lying on the ground: from `w0` to `w1` in from
/// the edge (in hexes, up to `INNER`), lifted by `lift`. `p` is the edge as
/// `edge_points` gives it. The band is cut where the ground's triangles
/// meet, so it lies flat on them whatever the slopes.
fn band(mesh: &mut Mesh, p: &[Vec3; 5], w0: f32, w1: f32, lift: f32, color: Color) {
    let [k0, m, k1, a0, a1] = *p;
    let up = vec3(0.0, lift, 0.0);
    let line = |w: f32| {
        let u = (w / INNER).clamp(0.0, 1.0);
        [mix(k0, a0, u) + up, mix(m, a0, u) + up, mix(m, a1, u) + up, mix(k1, a1, u) + up]
    };
    let o = mesh.vertices.len();
    mesh.vertices.extend(line(w0));
    mesh.vertices.extend(line(w1));
    let i = o + 4;
    mesh.add_triangle(i, o, o + 1, color);
    mesh.add_triangle(i, o + 1, i + 1, color);
    if w0 > 0.0 {
        mesh.add_triangle(i + 1, o + 1, o + 2, color);
    }
    mesh.add_triangle(i + 1, o + 2, i + 2, color);
    mesh.add_triangle(i + 2, o + 2, o + 3, color);
    mesh.add_triangle(i + 2, o + 3, i + 3, color);
}

/// `mesh.merge(model, transform)` for a plain model (no picture, nothing
/// glowing), without the empty texture point `merge` adds for every
/// triangle: with tens of thousands of trees, that list is megabytes to
/// fill (and for the graphics card's cache to fingerprint each frame).
fn add_model(mesh: &mut Mesh, model: &Mesh, transform: &Transform) {
    let plain = model.texture.is_none() && !model.glowing && !mesh.glowing && model.glow.iter().all(|&g| g == 0.0);
    if !plain {
        mesh.merge(model, transform);
        return;
    }
    let base = mesh.vertices.len();
    mesh.vertices.extend(model.vertices.iter().map(|&v| transform.apply(v)));
    mesh.triangles.extend(model.triangles.iter().map(|t| duckforge::render3d::Triangle {
        corners: t.corners.map(|i| i + base),
        color: t.color,
    }));
}

/// A small bush: a five-sided cone standing at `at`.
fn bush(mesh: &mut Mesh, at: Vec3, radius: f32, height: f32, turn: f32, color: Color) {
    let top = mesh.add_vertex(at + vec3(0.0, height, 0.0));
    let ring: Vec<usize> = (0..5)
        .map(|k| {
            let a = turn + k as f32 * 2.0 * PI / 5.0;
            mesh.add_vertex(at + vec3(a.cos() * radius, -0.01, a.sin() * radius))
        })
        .collect();
    for k in 0..5 {
        let (a, b) = (ring[k], ring[(k + 1) % 5]);
        // Whichever way round faces out.
        let (pt, pa, pb) = (mesh.vertices[top], mesh.vertices[a], mesh.vertices[b]);
        let out = (pa + pb) * 0.5 - at;
        if (pa - pt).cross(pb - pt).dot(out) > 0.0 {
            mesh.add_triangle(top, a, b, color);
        } else {
            mesh.add_triangle(top, b, a, color);
        }
    }
}

/// The camera: it circles a point on the ground.
#[derive(Debug, Clone, Copy)]
pub struct Orbit {
    /// The ground point in the middle of the screen.
    pub target: Vec2,
    /// Turned round the target (0: looking north).
    pub yaw: f32,
    /// How steeply it looks down.
    pub pitch: f32,
    pub distance: f32,
}

impl Orbit {
    pub fn camera(&self) -> Camera3D {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let target = vec3(self.target.x, 0.0, self.target.y);
        let pos = target + vec3(-sy * cp, sp, -cy * cp) * self.distance;
        Camera3D::looking_at(pos, target)
    }
}

pub struct MapView {
    pub mode: Mode,
    style: Style,
    /// The land in chunks, painted both ways.
    chunks: Vec<Chunk>,
    sea: Mesh,
    /// Seas and lakes, in chunks.
    water: Vec<Mesh>,
    decor: Mesh,
    rivers: Mesh,
    borders: Mesh,
    /// The highlighted nation's gold border in three widths, thinnest
    /// first: `draw` picks one with the province lines, so it stays
    /// thicker than any other border from near or far.
    gold: [Mesh; 3],
    highlighted: Option<usize>,
    /// Province lines in three widths, thinnest first: `draw` picks one by
    /// how far away the camera is, and how solid to draw it.
    lines: [Mesh; 3],
    cities: Mesh,
    highlight: Option<Mesh>,
    /// The war layer: armies, fleets and jets...
    armies: Mesh,
    /// ...and front lines, occupation and attack arrows.
    fronts: Mesh,
    /// An economy overlay (resources, industry): land coloured by province,
    /// drawn instead of the terrain while it's set.
    overlay: Option<Vec<Mesh>>,
    pub orbit: Orbit,
    /// The map's extent in the world: (min x, min z, max x, max z).
    pub bounds: (f32, f32, f32, f32),
    minimap: [Sprite; 2],
    /// The map cell under each minimap pixel (`u32::MAX`: none).
    minimap_cells: Vec<u32>,
    /// Where each nation's name and flag go: (nation, ground point).
    pub label_points: Vec<(usize, Vec3)>,
    /// Each capital's top, for its flag.
    pub capital_tops: Vec<Vec3>,
    origin: (f32, f32),
    /// Each hex's own height: what `pick` looks at.
    heights: Vec<f32>,
    /// The ground's height at each hex's 19 corners (`Field::corners`).
    geo: Vec<[f32; 19]>,
    /// Each hex's colour on the minimap, before any tint.
    cell_color: Vec<[f32; 3]>,
}

impl MapView {
    pub fn new(atlas: &Atlas, world: &World, models: &Models) -> MapView {
        let started = std::time::Instant::now();
        let view = Self::build(atlas, world, models);
        if std::env::var_os("LUCKY_MAP_STATS").is_some() {
            eprintln!("map_view: {} triangles, built in {:.1} ms", view.triangles(), started.elapsed().as_secs_f64() * 1000.0);
        }
        view
    }

    fn build(atlas: &Atlas, world: &World, models: &Models) -> MapView {
        let map = atlas.map(world.map);
        let origin = map.plane_middle();
        let mut view = MapView {
            mode: Mode::Political,
            style: Style::load(),
            chunks: Vec::new(),
            sea: Mesh::new(),
            water: Vec::new(),
            decor: Mesh::new(),
            rivers: Mesh::new(),
            borders: Mesh::new(),
            gold: [Mesh::new(), Mesh::new(), Mesh::new()],
            highlighted: Some(0),
            lines: [Mesh::new(), Mesh::new(), Mesh::new()],
            cities: Mesh::new(),
            highlight: None,
            armies: Mesh::new(),
            fronts: Mesh::new(),
            overlay: None,
            orbit: Orbit {
                target: Vec2::ZERO,
                yaw: 0.0,
                pitch: 0.95,
                distance: 16.0,
            },
            bounds: (0.0, 0.0, 0.0, 0.0),
            minimap: [Sprite::from_pixels(1, 1, vec![Color::BLACK]), Sprite::from_pixels(1, 1, vec![Color::BLACK])],
            minimap_cells: Vec::new(),
            label_points: Vec::new(),
            capital_tops: Vec::new(),
            origin,
            heights: Vec::new(),
            geo: Vec::new(),
            cell_color: Vec::new(),
        };
        view.heights = (0..map.cells.len())
            .map(|i| {
                let t = atlas.terrain_of(map, i);
                let (c, r) = map.cell(i);
                if t.kind == TerrainKind::Land { t.height + 0.025 * (hash(c, r, 1) - 0.5) } else { t.height }
            })
            .collect();
        let (w, h) = (map.width as f32, map.height as f32 * hex::ROW);
        view.bounds = (-origin.0, origin.1 - h, w - origin.0, origin.1);
        let looks = view.style.looks(atlas);
        view.build_terrain(atlas, map, world, &looks);
        view.build_lines(atlas, map, world);
        view.build_rivers(map);
        view.build_decor(atlas, map, models, &looks);
        view.build_cities(atlas, map, world, models);
        view.build_minimap(map, world);
        view.build_labels(map, world);
        view.set_highlight(atlas, world, models, Some(0));
        // Start over the player's capital.
        let start = view.label_points.first().map(|(_, p)| vec2(p.x, p.z)).unwrap_or(Vec2::ZERO);
        view.orbit.target = start;
        view
    }

    /// A map-plane point in the world, at height `y`.
    pub fn world(&self, x: f32, plane_y: f32, y: f32) -> Vec3 {
        vec3(x - self.origin.0, y, self.origin.1 - plane_y)
    }

    /// The map-plane point under a world point.
    pub fn plane(&self, p: Vec3) -> (f32, f32) {
        (p.x + self.origin.0, self.origin.1 - p.z)
    }

    fn cell_world(&self, map: &WorldMap, i: usize, lift: f32) -> Vec3 {
        let (c, r) = map.cell(i);
        let (x, y) = hex::center(c, r);
        self.world(x, y, self.heights[i] + lift)
    }

    /// The ground: the land's rolling triangles, then the seas and lakes.
    fn build_terrain(&mut self, atlas: &Atlas, map: &WorldMap, world: &World, looks: &[Look]) {
        // A hex with a city or a landmark is flat; one with a river is calmer.
        let mut calm = vec![1.0f32; map.cells.len()];
        for river in &map.rivers {
            for &(c, r) in &river.cells {
                calm[map.index(c, r)] = 0.3;
            }
        }
        for at in map.provinces.iter().map(|p| p.at).chain(map.landmarks.iter().map(|l| l.at)) {
            calm[map.index(at.0, at.1)] = 0.0;
        }
        let style = self.style.clone();
        let field = Field::new(&style, looks, atlas, map, &self.heights, &calm);
        self.geo = vec![[WATER; 19]; map.cells.len()];
        self.cell_color = vec![[0.0; 3]; map.cells.len()];

        self.chunks.clear();
        for cr in 0..map.height.div_ceil(CHUNK) {
            for cc in 0..map.width.div_ceil(CHUNK) {
                let mut chunk = Chunk::default();
                // Room for it all at once (growing big lists step by step
                // copies them over and over): a hex is 19 corners and 24
                // triangles, and a few more along a shore.
                let land = (cr * CHUNK..((cr + 1) * CHUNK).min(map.height))
                    .flat_map(|r| (cc * CHUNK..((cc + 1) * CHUNK).min(map.width)).map(move |c| (c, r)))
                    .filter(|&(c, r)| field.cells[(r + PAD) * field.pw + c + PAD].kind == Kind::Land)
                    .count();
                chunk.meshes[1].vertices.reserve(land * 24);
                chunk.meshes[1].triangles.reserve(land * 30);
                chunk.alb.reserve(land * 30);
                chunk.light.reserve(land * 30);
                chunk.cell.reserve(land * 30);
                for r in cr * CHUNK..((cr + 1) * CHUNK).min(map.height) {
                    for c in cc * CHUNK..((cc + 1) * CHUNK).min(map.width) {
                        if field.cells[(r + PAD) * field.pw + c + PAD].kind == Kind::Land {
                            self.add_land(&mut chunk, &field, c + PAD, r + PAD);
                        }
                    }
                }
                if !chunk.cell.is_empty() {
                    // The light is in the colours already.
                    chunk.meshes[1].glowing = true;
                    self.chunks.push(chunk);
                }
            }
        }

        // The open sea: one big plane, under the seas and lakes drawn hex by hex.
        let (x0, z0, x1, z1) = self.bounds;
        let m = 400.0;
        let mut sea = Mesh::new();
        let a = sea.add_vertex(vec3(x0 - m, SEA_LEVEL, z0 - m));
        let b = sea.add_vertex(vec3(x0 - m, SEA_LEVEL, z1 + m));
        let c = sea.add_vertex(vec3(x1 + m, SEA_LEVEL, z1 + m));
        let d = sea.add_vertex(vec3(x1 + m, SEA_LEVEL, z0 - m));
        sea.add_quad(a, b, c, d, style.water.deep.0);
        self.sea = sea.glowing();
        self.water.clear();
        for cr in 0..(field.ph - 2).div_ceil(CHUNK) {
            for cc in 0..(field.pw - 2).div_ceil(CHUNK) {
                let mut mesh = Mesh::new();
                // Open sea is 7 corners and 6 triangles a hex, sea by a shore 19 and 24.
                mesh.vertices.reserve(CHUNK * CHUNK * 10);
                mesh.triangles.reserve(CHUNK * CHUNK * 10);
                for pr in 1 + cr * CHUNK..(1 + (cr + 1) * CHUNK).min(field.ph - 1) {
                    for pc in 1 + cc * CHUNK..(1 + (cc + 1) * CHUNK).min(field.pw - 1) {
                        if field.cells[pr * field.pw + pc].kind != Kind::Land {
                            self.add_water(&mut mesh, &field, map, pc, pr);
                        }
                    }
                }
                if !mesh.triangles.is_empty() {
                    self.water.push(mesh.glowing());
                }
            }
        }
        self.paint_nations(map, world);
    }

    /// One hex of land: 24 triangles, or more along a shore (the sand).
    fn add_land(&mut self, chunk: &mut Chunk, field: &Field, pc: usize, pr: usize) {
        let me = field.cells[pr * field.pw + pc];
        let style = &field.style.land;
        let look = &field.looks[me.look];
        let v = field.corners(pc, pr);
        self.geo[me.actual] = v.map(|q| q.y);
        self.cell_color[me.actual] = v[0].alb;
        let base = chunk.meshes[1].vertices.len();
        for q in &v {
            chunk.meshes[1].vertices.push(self.world(q.x, q.py, q.y));
        }
        let pen = Pen {
            cell: me.actual as u32,
            look,
            snow: rgb(style.snow.0),
            sun: field.style.sun(),
            gain: 0.8 * style.relief,
        };
        let colour = |ids: &[usize]| scale3(ids.iter().fold([0.0; 3], |sum, &q| add3(sum, v[q].alb)), 1.0 / ids.len() as f32);
        let sand = (style.sand_width / INNER).clamp(0.05, 0.8);
        for k in 0..6 {
            let k1 = (k + 1) % 6;
            let (ck, ck1, m, a, a1) = (1 + k, 1 + k1, 7 + k, 13 + k, 13 + k1);
            chunk.ground([base, base + a, base + a1], colour(&[0, a, a1]), &pen);
            let (nc, nr) = step(pc as i32, pr as i32, k1);
            let shore = field.cells[nr as usize * field.pw + nc as usize].kind != Kind::Land;
            if !shore {
                for ids in [[a, ck, m], [a, m, a1], [a1, m, ck1]] {
                    let wet = ids.iter().filter(|&&q| v[q].wet).count() as f32 / 3.0;
                    chunk.ground(ids.map(|q| base + q), mix3(colour(&ids), look.shore, 0.5 * wet), &pen);
                }
                continue;
            }
            // Along the water: a strip of sand, then land that is still a little sandy.
            let mesh = &mut chunk.meshes[1];
            let p = |q: usize| mesh.vertices[base + q];
            let line = [mix(p(ck), p(a), sand), mix(p(m), p(a), sand), mix(p(m), p(a1), sand), mix(p(ck1), p(a1), sand)];
            let q = mesh.vertices.len();
            mesh.vertices.extend(line);
            let land = colour(&[a, ck, m, a1, ck1]);
            let beach = mix3(look.shore, land, 0.12);
            for ids in [[q, base + ck, base + m], [q, base + m, q + 1], [q + 1, base + m, q + 2], [q + 2, base + m, base + ck1], [q + 2, base + ck1, q + 3]] {
                chunk.ground(ids, beach, &pen);
            }
            let behind = mix3(land, look.shore, 0.3);
            for ids in [[base + a, q, q + 1], [base + a, q + 1, q + 2], [base + a, q + 2, base + a1], [base + a1, q + 2, q + 3]] {
                chunk.ground(ids, behind, &pen);
            }
        }
    }

    /// One hex of sea or lake: a fan of 6 triangles on the open sea, 24 where
    /// the water lightens towards a shore.
    fn add_water(&mut self, mesh: &mut Mesh, field: &Field, map: &WorldMap, pc: usize, pr: usize) {
        let me = field.cells[pr * field.pw + pc];
        let v = field.corners(pc, pr);
        let lake = me.kind == Kind::Lake;
        let water = &field.style.water;
        let colour = |ids: [usize; 3]| {
            let s = (v[ids[0]].s + v[ids[1]].s + v[ids[2]].s) / 3.0;
            let (x, y) = ((v[ids[0]].x + v[ids[1]].x + v[ids[2]].x) / 3.0, (v[ids[0]].py + v[ids[1]].py + v[ids[2]].py) / 3.0);
            // The sea's drift fades out at the map's edge, into the plain sea beyond.
            let edge = ((x + 0.5).min(map.width as f32 - x).min(y + 0.5).min(map.height as f32 * hex::ROW - y) / 3.0).clamp(0.0, 1.0);
            let drift = 0.6 * noise(x * 0.21, y * 0.21, 21) + 0.4 * noise(x * 0.8, y * 0.8, 22) - 0.5;
            paint(water_colour(water, lake, s), 1.0 + 2.0 * water.swell * drift * edge)
        };
        let base = mesh.vertices.len();
        let open = v[..13].iter().all(|q| q.s == 0.0);
        let (x, y) = (v[0].x, v[0].py);
        if open && (x < -1.1 || x > map.width as f32 + 0.6 || y < -1.1 || y > map.height as f32 * hex::ROW + 0.6) {
            // Open sea off the map, with no drift: the plain sea under it is the same colour.
            return;
        }
        for q in &v[..if open { 7 } else { 19 }] {
            mesh.vertices.push(self.world(q.x, q.py, WATER));
        }
        for k in 0..6 {
            let k1 = (k + 1) % 6;
            if open {
                mesh.add_triangle(base, base + 1 + k, base + 1 + k1, colour([0, 1 + k, 1 + k1]));
                continue;
            }
            let (ck, ck1, m, a, a1) = (1 + k, 1 + k1, 7 + k, 13 + k, 13 + k1);
            for ids in [[0, a, a1], [a, ck, m], [a, m, a1], [a1, m, ck1]] {
                mesh.add_triangle(base + ids[0], base + ids[1], base + ids[2], colour(ids));
            }
        }
        if me.actual != usize::MAX {
            let s = v.iter().map(|q| q.s).sum::<f32>() / 19.0;
            self.cell_color[me.actual] = water_colour(water, lake, s);
        }
    }

    /// Paints the Nations view: each nation's colour as a tint over its land.
    fn paint_nations(&mut self, map: &WorldMap, world: &World) {
        let style = &self.style.land;
        for chunk in &mut self.chunks {
            let [nations, terrain] = &mut chunk.meshes;
            if nations.triangles.len() != terrain.triangles.len() {
                *nations = terrain.clone();
            }
            for (t, triangle) in nations.triangles.iter_mut().enumerate() {
                let owner = map.province_of[chunk.cell[t] as usize].and_then(|p| world.owner[p]);
                let colour = match owner {
                    Some(o) => tint(style, chunk.alb[t], world.color[o], style.tint),
                    None => mix3(chunk.alb[t], [120.0; 3], style.unowned),
                };
                triangle.color = paint(colour, chunk.light[t]);
            }
        }
    }

    /// One hex edge on the ground, for `band`: corner `k`, the edge's middle,
    /// corner `k + 1`, and the inner-ring corners behind those two.
    fn edge_points(&self, map: &WorldMap, i: usize, k: usize) -> [Vec3; 5] {
        let (c, r) = map.cell(i);
        let (x, y) = hex::center(c, r);
        let k1 = (k + 1) % 6;
        let g = &self.geo[i];
        let (ax, ay) = hex::corner(x, y, k);
        let (bx, by) = hex::corner(x, y, k1);
        [
            self.world(ax, ay, g[1 + k]),
            self.world((ax + bx) / 2.0, (ay + by) / 2.0, g[7 + k]),
            self.world(bx, by, g[1 + k1]),
            self.world((x + ax) / 2.0, (y + ay) / 2.0, g[13 + k]),
            self.world((x + bx) / 2.0, (y + by) / 2.0, g[13 + k1]),
        ]
    }

    /// The ground's height at a map-plane point of land hex `i` (a point
    /// outside the hex gets the height of its nearest edge).
    fn surface(&self, map: &WorldMap, i: usize, x: f32, py: f32) -> f32 {
        let (c, r) = map.cell(i);
        let (cx, cy) = hex::center(c, r);
        let (dx, dy) = (x - cx, py - cy);
        let g = &self.geo[i];
        if dx == 0.0 && dy == 0.0 {
            return g[0];
        }
        // Which sixth of the hex, then where in it: `u` towards corner k, `v` towards corner k + 1.
        let k = (((dy.atan2(dx).to_degrees() - 30.0) / 60.0).floor() as i32).rem_euclid(6) as usize;
        let k1 = (k + 1) % 6;
        let (ax, ay) = hex::corner(0.0, 0.0, k);
        let (bx, by) = hex::corner(0.0, 0.0, k1);
        let det = ax * by - ay * bx;
        let mut u = ((dx * by - dy * bx) / det).max(0.0);
        let mut v = ((ax * dy - ay * dx) / det).max(0.0);
        if u + v > 1.0 {
            (u, v) = (u / (u + v), v / (u + v));
        }
        let (hc, hk, hk1, hm, ha, ha1) = (g[0], g[1 + k], g[1 + k1], g[7 + k], g[13 + k], g[13 + k1]);
        if u + v <= 0.5 {
            hc + 2.0 * u * (ha - hc) + 2.0 * v * (ha1 - hc)
        } else if u >= 0.5 {
            (2.0 - 2.0 * u - 2.0 * v) * ha + (2.0 * u - 1.0) * hk + 2.0 * v * hm
        } else if v >= 0.5 {
            (2.0 - 2.0 * u - 2.0 * v) * ha1 + (2.0 * v - 1.0) * hk1 + 2.0 * u * hm
        } else {
            (1.0 - 2.0 * v) * ha + (1.0 - 2.0 * u) * ha1 + (2.0 * u + 2.0 * v - 1.0) * hm
        }
    }

    /// The land's height at a map-plane point (`None` over water or off the map).
    fn land_height(&self, map: &WorldMap, x: f32, py: f32) -> Option<f32> {
        let (c, r) = hex::cell_at(x, py, map.width, map.height)?;
        let i = map.index(c, r);
        map.province_of[i].map(|_| self.surface(map, i, x, py))
    }

    /// Nation borders (each side in its own colour) and province lines.
    fn build_lines(&mut self, atlas: &Atlas, map: &WorldMap, world: &World) {
        let (b, l) = (&self.style.borders, &self.style.lines);
        /// What one hex edge gets: a nation's edge along the water, a
        /// border between nations (and a province line under it), or a
        /// province line.
        enum Edge {
            Coast(usize),
            Border(usize),
            Line,
        }
        let mut edges = Vec::new();
        for i in 0..map.cells.len() {
            let Some(p) = map.province_of[i] else { continue };
            let owner = world.owner[p];
            let (c, r) = map.cell(i);
            for k in 0..6 {
                let j = across(map, c, r, k).filter(|&j| atlas.terrain_of(map, j).kind == TerrainKind::Land);
                let other = j.and_then(|j| map.province_of[j]);
                if j.is_some() && other == Some(p) {
                    continue;
                }
                let edge = match (owner, j) {
                    (Some(o), None) => Edge::Coast(o),
                    (Some(o), Some(_)) if other.and_then(|q| world.owner[q]) != owner => Edge::Border(o),
                    (_, Some(_)) => Edge::Line,
                    (None, None) => continue,
                };
                edges.push((i, k, edge));
            }
        }
        // Room for every band at once (8 corners and 5 triangles each).
        let coasts = edges.iter().filter(|e| matches!(e.2, Edge::Coast(_))).count();
        let nations = edges.iter().filter(|e| matches!(e.2, Edge::Border(_))).count();
        let (lined, banded) = (edges.len() - coasts, coasts + 2 * nations);
        let mut borders = Mesh::new();
        borders.vertices.reserve(8 * banded);
        borders.triangles.reserve(5 * banded);
        let mut lines = [Mesh::new(), Mesh::new(), Mesh::new()];
        for mesh in &mut lines {
            mesh.vertices.reserve(8 * lined);
            mesh.triangles.reserve(5 * lined);
        }
        for (i, k, kind) in edges {
            let edge = self.edge_points(map, i, k);
            match kind {
                Edge::Coast(o) => band(&mut borders, &edge, 0.0, b.coast, 0.012, vivid(world.color[o], 1.15, b.nation_dark)),
                Edge::Border(o) => {
                    band(&mut borders, &edge, 0.0, b.nation, 0.014, vivid(world.color[o], 1.15, b.nation_dark));
                    band(&mut borders, &edge, 0.0, 0.011, 0.017, b.edge.0);
                }
                Edge::Line => {}
            }
            // A province line on every edge between two pieces of land, even
            // under a border: the Terrain view (no borders) shows them all.
            if !matches!(kind, Edge::Coast(_)) {
                for (mesh, &width) in lines.iter_mut().zip(&l.widths) {
                    band(mesh, &edge, 0.0, width, 0.01, l.color.0);
                }
            }
        }
        self.borders = borders.glowing();
        self.lines = lines.map(|mesh| mesh.glowing());
    }

    /// Draws `nation`'s outer border in gold, thicker and brighter than any
    /// other border (`None`: no gold border). It stays until changed, and
    /// follows the nation's land when borders are rebuilt.
    pub fn set_highlight(&mut self, atlas: &Atlas, world: &World, _models: &Models, nation: Option<usize>) {
        self.highlighted = nation;
        let b = &self.style.borders;
        let map = atlas.map(world.map);
        let mut gold = [Mesh::new(), Mesh::new(), Mesh::new()];
        if let Some(n) = nation.filter(|&n| n < world.nations.len()) {
            let mine = |i: usize| map.province_of[i].is_some_and(|p| world.owner[p] == Some(n));
            for i in (0..map.cells.len()).filter(|&i| mine(i)) {
                let (c, r) = map.cell(i);
                for k in 0..6 {
                    if across(map, c, r, k).is_some_and(mine) {
                        continue;
                    }
                    let edge = self.edge_points(map, i, k);
                    for (mesh, &width) in gold.iter_mut().zip(&b.gold_widths) {
                        // Gold, a bright line inside it, and a dark edge outside.
                        let w = width.clamp(0.02, INNER);
                        band(mesh, &edge, 0.0, w, 0.026, b.gold.0);
                        band(mesh, &edge, 0.3 * w, 0.62 * w, 0.029, b.gold_light.0);
                        band(mesh, &edge, 0.0, 0.012, 0.029, shade(b.gold.0, 0.45));
                    }
                }
            }
        }
        self.gold = gold.map(|mesh| mesh.glowing());
    }

    /// Rivers: winding ribbons from hex to hex, wider towards the mouth,
    /// each with a darker bank under it so it shows on any land.
    fn build_rivers(&mut self, map: &WorldMap) {
        let w = &self.style.water;
        let mut mesh = Mesh::new();
        for (n, river) in map.rivers.iter().enumerate() {
            if river.cells.len() < 2 {
                continue;
            }
            // Its course: through each hex a little off its middle...
            let last = river.cells.len() - 1;
            let mut course: Vec<(f32, f32)> = river
                .cells
                .iter()
                .enumerate()
                .map(|(j, &(c, r))| {
                    let (x, y) = hex::center(c, r);
                    let wander = if j == 0 || j == last { 0.0 } else { 0.32 };
                    (x + wander * (hash(c, r, 81 + n) - 0.5), y + wander * (hash(c, r, 91 + n) - 0.5))
                })
                .collect();
            // ...ending a little way into the water it runs into.
            let (lc, lr) = river.cells[last];
            if map.province_of[map.index(lc, lr)].is_none() {
                let (a, b) = (course[last - 1], course[last]);
                course[last] = (a.0 + (b.0 - a.0) * 0.64, a.1 + (b.1 - a.1) * 0.64);
            }
            // A smooth curve through those points.
            let at = |j: isize| course[j.clamp(0, last as isize) as usize];
            let steps = 6;
            let mut points = Vec::with_capacity(last * steps + 1);
            for j in 0..last as isize {
                let (p0, p1, p2, p3) = (at(j - 1), at(j), at(j + 1), at(j + 2));
                for s in 0..steps {
                    let t = s as f32 / steps as f32;
                    let curve = |a: f32, b: f32, c: f32, d: f32| 0.5 * (2.0 * b + (c - a) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t * t + (3.0 * b - a - 3.0 * c + d) * t * t * t);
                    points.push((curve(p0.0, p1.0, p2.0, p3.0), curve(p0.1, p1.1, p2.1, p3.1)));
                }
            }
            points.push(course[last]);
            let count = points.len();
            // Across the river at each point: bank, water, water, bank.
            let rows: Vec<[Vec3; 4]> = (0..count)
                .map(|q| {
                    let (x, y) = points[q];
                    let (before, after) = (points[q.saturating_sub(1)], points[(q + 1).min(count - 1)]);
                    let (tx, ty) = (after.0 - before.0, after.1 - before.1);
                    let len = (tx * tx + ty * ty).sqrt().max(1e-6);
                    let (sx, sy) = (-ty / len, tx / len);
                    let along = q as f32 / (count - 1) as f32;
                    let half = 0.5 * (w.river_source + (w.river_mouth - w.river_source) * along.powf(1.4));
                    let point = |k: f32, lift: f32| {
                        let (x, y) = (x + sx * k, y + sy * k);
                        self.world(x, y, self.land_height(map, x, y).unwrap_or(WATER).max(WATER) + lift)
                    };
                    [point(-half - 0.024, 0.018), point(-half, 0.024), point(half, 0.024), point(half + 0.024, 0.018)]
                })
                .collect();
            for pair in rows.windows(2) {
                for (left, right, color) in [(0, 3, w.river_bank.0), (1, 2, w.river.0)] {
                    let ids = [pair[0][left], pair[0][right], pair[1][right], pair[1][left]].map(|p| mesh.add_vertex(p));
                    // Both ways round, so it shows whichever way the river turns.
                    mesh.add_quad(ids[0], ids[1], ids[2], ids[3], color);
                    mesh.add_quad(ids[0], ids[3], ids[2], ids[1], color);
                }
            }
        }
        self.rivers = mesh.glowing();
    }

    /// Trees, palms, peaks and reeds (the terrain's models), each a little
    /// lighter or darker than the next, and bushes that thicken the forests.
    #[allow(clippy::needless_range_loop)]
    fn build_decor(&mut self, atlas: &Atlas, map: &WorldMap, models: &Models, looks: &[Look]) {
        let mut busy = vec![false; map.cells.len()];
        for p in &map.provinces {
            busy[map.index(p.at.0, p.at.1)] = true;
        }
        for l in &map.landmarks {
            busy[map.index(l.at.0, l.at.1)] = true;
        }
        let mut decor = Mesh::new();
        // Room for at most this many models and bushes, made once.
        let mut hexes_of = vec![0usize; atlas.terrains.len()];
        for &t in &map.cells {
            hexes_of[t] += 1;
        }
        let (mut corners, mut triangles) = (0, 0);
        for ((terrain, look), &hexes) in atlas.terrains.iter().zip(looks).zip(&hexes_of) {
            if let Some(model) = terrain.decor.as_ref().and_then(|name| models.decor(name)) {
                corners += hexes * terrain.decor_count.ceil() as usize * model.vertices.len();
                triangles += hexes * terrain.decor_count.ceil() as usize * model.triangles.len();
            }
            corners += hexes * look.bushes.ceil() as usize * 6;
            triangles += hexes * look.bushes.ceil() as usize * 5;
        }
        decor.vertices.reserve(corners);
        decor.triangles.reserve(triangles);
        for i in 0..map.cells.len() {
            let t = atlas.terrain_of(map, i);
            if t.kind != TerrainKind::Land {
                continue;
            }
            let (c, r) = map.cell(i);
            let (cx, cy) = hex::center(c, r);
            // A point `reach` from the hex's middle, standing on the ground.
            let stand = |angle: f32, reach: f32| {
                let (x, y) = (cx + angle.cos() * reach, cy - angle.sin() * reach);
                self.world(x, y, self.surface(map, i, x, y))
            };
            if let Some(model) = t.decor.as_ref().and_then(|name| models.decor(name)) {
                let whole = t.decor_count.floor() as usize;
                let count = whole + (hash(c, r, 7) < t.decor_count.fract()) as usize;
                for k in 0..count {
                    let a = hash(c, r, 11 + k) * 2.0 * PI;
                    let reach = if busy[i] { 0.3 } else { 0.08 + 0.2 * hash(c, r, 21 + k) };
                    let size = 0.85 + 0.3 * hash(c, r, 31 + k);
                    let turn = hash(c, r, 41 + k) * 2.0 * PI;
                    let from = decor.triangles.len();
                    add_model(&mut decor, model, &Transform::at(stand(a, reach)).rotated(vec3(0.0, turn, 0.0)).sized(size));
                    let tone = 0.84 + 0.26 * hash(c, r, 141 + k);
                    for triangle in &mut decor.triangles[from..] {
                        triangle.color = shade(triangle.color, tone);
                    }
                }
            }
            let look = &looks[map.cells[i]];
            let bushes = look.bushes.floor() as usize + (hash(c, r, 8) < look.bushes.fract()) as usize;
            for k in 0..bushes {
                let a = (k as f32 + hash(c, r, 101 + k)) / bushes as f32 * 2.0 * PI;
                let reach = if busy[i] { 0.4 } else { 0.24 + 0.2 * hash(c, r, 111 + k) };
                let size = 0.75 + 0.5 * hash(c, r, 121 + k);
                bush(&mut decor, stand(a, reach), 0.11 * size, 0.15 * size, a * 3.0, shade(look.bush, 0.82 + 0.3 * hash(c, r, 131 + k)));
            }
        }
        self.decor = decor;
    }

    fn build_cities(&mut self, atlas: &Atlas, map: &WorldMap, world: &World, models: &Models) {
        let mut cities = Mesh::new();
        let capitals: Vec<usize> = world.capital.clone();
        self.capital_tops = vec![Vec3::ZERO; world.nations.len()];
        for (p, prov) in map.provinces.iter().enumerate() {
            let i = map.index(prov.at.0, prov.at.1);
            let at = self.cell_world(map, i, 0.0);
            let turn = hash(prov.at.0, prov.at.1, 51) * 2.0 * PI;
            if let Some(n) = capitals.iter().position(|&c| c == p) {
                let mut capital = models.capital.clone();
                if let Some(range) = &models.capital_roofs {
                    for t in range.clone() {
                        if let Some(tri) = capital.triangles.get_mut(t) {
                            tri.color = world.color[n];
                        }
                    }
                }
                add_model(&mut cities, &capital, &Transform::at(at).rotated(vec3(0.0, turn, 0.0)).sized(0.95));
                self.capital_tops[n] = at + vec3(0.0, 0.85, 0.0);
            } else if world.owner[p].is_some() {
                let terrain = &atlas.terrains[prov.terrain];
                let rough = matches!(terrain.letter, 'M' | 'I');
                if !rough && hash(prov.at.0, prov.at.1, 61) < 0.75 {
                    add_model(&mut cities, &models.town, &Transform::at(at).rotated(vec3(0.0, turn, 0.0)).sized(0.9));
                }
            }
        }
        for l in &map.landmarks {
            if let Some(model) = models.landmark(&l.kind) {
                let i = map.index(l.at.0, l.at.1);
                let at = self.cell_world(map, i, 0.0);
                add_model(&mut cities, model, &Transform::at(at).sized(1.0));
            }
        }
        self.cities = cities;
    }

    /// The minimap, both ways: each hex in its own colour (the sea's shelf
    /// and all), and in the Nations view tinted like the map, only stronger.
    fn build_minimap(&mut self, map: &WorldMap, world: &World) {
        let k = 4.0;
        let w = ((map.width as f32 + 0.5) * k) as usize;
        let h = (map.height as f32 * hex::ROW * k + k) as usize;
        if self.minimap_cells.len() != w * h {
            self.minimap_cells = minimap_cells(w, h, k, map.width, map.height);
        }
        let style = &self.style.land;
        let terrain: Vec<Color> = self.cell_color.iter().map(|&c| paint(c, 1.0)).collect();
        let nations: Vec<Color> = (0..map.cells.len())
            .map(|i| match map.province_of[i].and_then(|p| world.owner[p]) {
                Some(o) => paint(tint(style, self.cell_color[i], world.color[o], style.minimap_tint), 1.0),
                None => terrain[i],
            })
            .collect();
        let sea = self.style.water.deep.0;
        for (slot, colors) in [nations, terrain].into_iter().enumerate() {
            let pixels = self.minimap_cells.iter().map(|&i| colors.get(i as usize).copied().unwrap_or(sea)).collect();
            self.minimap[slot] = Sprite::from_pixels(w, h, pixels);
        }
    }

    fn build_labels(&mut self, map: &WorldMap, world: &World) {
        self.label_points.clear();
        for n in 0..world.nations.len() {
            let provinces = world.provinces_of(n);
            if provinces.is_empty() {
                continue;
            }
            // The middle of its biggest piece of land (not the sea between islands).
            let capital = world.capital[n];
            let mut piece = vec![capital];
            let mut i = 0;
            while i < piece.len() {
                for &q in &map.provinces[piece[i]].neighbours {
                    if world.owner[q] == Some(n) && !piece.contains(&q) {
                        piece.push(q);
                    }
                }
                i += 1;
            }
            let (mut sx, mut sy, mut k) = (0.0, 0.0, 0.0);
            for &p in &piece {
                let w = map.provinces[p].cells.len() as f32;
                sx += map.provinces[p].middle.0 * w;
                sy += map.provinces[p].middle.1 * w;
                k += w;
            }
            let (x, y) = (sx / k, sy / k);
            self.label_points.push((n, self.world(x, y, 0.35)));
        }
    }

    /// Outlines a province in white (or clears it with `None`).
    pub fn select(&mut self, atlas: &Atlas, world: &World, province: Option<usize>) {
        let map = atlas.map(world.map);
        self.highlight = province.map(|p| {
            let mut mesh = Mesh::new();
            for &i in &map.provinces[p].cells {
                let (c, r) = map.cell(i);
                for k in 0..6 {
                    if across(map, c, r, k).is_none_or(|j| map.province_of[j] != Some(p)) {
                        band(&mut mesh, &self.edge_points(map, i, k), 0.0, 0.04, 0.034, self.style.borders.selected.0);
                    }
                }
            }
            mesh.glowing()
        });
    }

    /// Colours the land by province (one colour each, `None` for grey), or
    /// back to the normal map with `None`. The hills' shading stays, fainter.
    pub fn set_overlay(&mut self, atlas: &Atlas, world: &World, colors: Option<&[Option<Color>]>) {
        let Some(colors) = colors else {
            self.overlay = None;
            return;
        };
        let map = atlas.map(world.map);
        let keep = self.style.land.overlay_relief.clamp(0.0, 1.0);
        let meshes = self
            .chunks
            .iter()
            .map(|chunk| {
                let mut mesh = chunk.meshes[1].clone();
                for (t, triangle) in mesh.triangles.iter_mut().enumerate() {
                    let colour = match map.province_of[chunk.cell[t] as usize].and_then(|p| colors.get(p).copied().flatten()) {
                        Some(c) => scale3(rgb(c), 0.86),
                        None => mix3(chunk.alb[t], [70.0, 70.0, 80.0], 0.75),
                    };
                    triangle.color = paint(colour, 1.0 + (chunk.light[t] - 1.0) * keep);
                }
                mesh
            })
            .collect();
        self.overlay = Some(meshes);
    }

    /// The province under a ray (from the mouse), looking at the hexes' heights.
    pub fn pick(&self, map: &WorldMap, origin: Vec3, direction: Vec3) -> Option<usize> {
        if direction.y.abs() < 1e-4 {
            return None;
        }
        let mut height = 0.1;
        let mut found = None;
        for _ in 0..3 {
            let t = (height - origin.y) / direction.y;
            if t < 0.0 {
                return None;
            }
            let p = origin + direction * t;
            let (x, y) = self.plane(p);
            let (c, r) = hex::cell_at(x, y, map.width, map.height)?;
            let i = map.index(c, r);
            found = map.province_of[i];
            height = self.heights[i].max(0.0);
        }
        found
    }

    /// Where the ground is under a screen ray (at sea level).
    pub fn ground(&self, origin: Vec3, direction: Vec3) -> Option<Vec2> {
        if direction.y > -1e-4 {
            return None;
        }
        let t = (0.0 - origin.y) / direction.y;
        let p = origin + direction * t;
        Some(vec2(p.x, p.z))
    }

    pub fn clamp_target(&mut self) {
        let (x0, z0, x1, z1) = self.bounds;
        self.orbit.target.x = self.orbit.target.x.clamp(x0, x1);
        self.orbit.target.y = self.orbit.target.y.clamp(z0, z1);
        self.orbit.distance = self.orbit.distance.clamp(5.0, 48.0);
        self.orbit.pitch = self.orbit.pitch.clamp(0.55, 1.45);
    }

    pub fn draw(&self, canvas: &mut Canvas, camera: &Camera3D) {
        // The plain look: drawn by the graphics card when there is one.
        canvas.set_supersample(1);
        canvas.set_bloom(None);
        // The ground and the water carry their own light; this lights the
        // models (trees, towns, armies) from the same north-west.
        canvas.set_lighting(Lighting {
            sun_direction: self.style.sun(),
            sun_color: Color::from_hex(0xfff4e0),
            sun_strength: 0.95,
            ambient: 0.58,
        });
        let still = Transform::default();
        canvas.draw_mesh(&self.sea, &still, camera);
        for mesh in &self.water {
            canvas.draw_mesh(mesh, &still, camera);
        }
        if let Some(overlay) = &self.overlay {
            for mesh in overlay {
                canvas.draw_mesh(mesh, &still, camera);
            }
        } else {
            let slot = match self.mode {
                Mode::Political => 0,
                Mode::Terrain => 1,
            };
            for chunk in &self.chunks {
                canvas.draw_mesh(&chunk.meshes[slot], &still, camera);
            }
        }
        canvas.draw_mesh(&self.rivers, &still, camera);
        canvas.draw_mesh(&self.decor, &still, camera);
        canvas.draw_mesh(&self.cities, &still, camera);
        if self.mode == Mode::Political || self.overlay.is_some() {
            canvas.draw_mesh(&self.borders, &still, camera);
        }
        let (lod, solid) = self.line_look(camera);
        canvas.draw_mesh(&self.gold[lod], &still, camera);
        canvas.draw_mesh(&self.fronts, &still, camera);
        canvas.draw_mesh(&self.armies, &still, camera);
        if let Some(h) = &self.highlight {
            canvas.draw_mesh(h, &still, camera);
        }
        // Province lines last, see-through: faint and wide from far away,
        // thin and dark up close.
        canvas.draw_mesh_tinted(&self.lines[lod], &still, camera, Color::rgba(255, 255, 255, (solid * 255.0) as u8));
    }

    /// Which of the three province-line meshes to draw from this camera, and
    /// how solid (0 to 1).
    fn line_look(&self, camera: &Camera3D) -> (usize, f32) {
        let l = &self.style.lines;
        // How far the camera is from the ground it looks at.
        let distance = camera.position.y.max(0.0) / (-camera.pitch.sin()).max(0.05);
        let lod = l.switch.iter().filter(|&&at| distance >= at).count();
        let t = ((distance - l.near_distance) / (l.far_distance - l.near_distance).max(0.1)).clamp(0.0, 1.0);
        // A wider line is drawn fainter, so the change of width doesn't jump.
        let even = (l.widths[0] / l.widths[lod].max(1e-4)).clamp(0.0, 1.0).sqrt();
        (lod, ((l.near + (l.far - l.near) * t) * even).clamp(0.0, 1.0))
    }

    /// Redraws borders, cities, the minimap and the names after provinces
    /// change hands (a peace deal): the camera stays where it is.
    pub fn rebuild_borders(&mut self, atlas: &Atlas, world: &World, models: &Models) {
        let map = atlas.map(world.map);
        self.paint_nations(map, world);
        self.build_lines(atlas, map, world);
        self.build_cities(atlas, map, world, models);
        self.build_minimap(map, world);
        self.build_labels(map, world);
        self.set_highlight(atlas, world, models, self.highlighted);
    }

    /// The war layer: one model per piece, red front lines between enemies,
    /// dots of the holder's colour over occupied land, and attack arrows.
    pub fn set_war(&mut self, atlas: &Atlas, world: &World, models: &Models, scene: &WarScene) {
        let map = atlas.map(world.map);
        let mut armies = Mesh::new();
        for p in &scene.pieces {
            let (mesh, range) = &models.war[p.piece as usize];
            let mut m = mesh.clone();
            if let Some(range) = range {
                for t in range.clone() {
                    if let Some(tri) = m.triangles.get_mut(t) {
                        tri.color = p.color;
                    }
                }
            }
            // On the ground where it stands (or at its hex's height, if it
            // stands past the land's edge); ships on the water.
            let y = match p.cell {
                Some(i) => self.land_height(map, p.at.0, p.at.1).unwrap_or(self.heights[i].max(WATER)),
                None => WATER + 0.01,
            } + p.lift;
            let at = self.world(p.at.0, p.at.1, y);
            let size = match p.piece {
                Piece::Soldier => 1.6,
                Piece::Tank => 1.5,
                Piece::Jet => 1.6,
                Piece::Warship | Piece::Transport => 1.4,
            };
            add_model(&mut armies, &m, &Transform::at(at).rotated(vec3(0.0, p.turn, 0.0)).sized(size));
        }
        self.armies = armies;

        let mut fronts = Mesh::new();
        let red = Color::rgb(235, 40, 40);
        if !scene.fronts.is_empty() {
            let pairs: std::collections::HashSet<(usize, usize)> = scene.fronts.iter().flat_map(|&(a, b)| [(a, b), (b, a)]).collect();
            for i in 0..map.cells.len() {
                let Some(p) = map.province_of[i] else { continue };
                let (c, r) = map.cell(i);
                for k in 0..6 {
                    if let Some(q) = across(map, c, r, k).and_then(|j| map.province_of[j]) {
                        if q != p && pairs.contains(&(p, q)) {
                            band(&mut fronts, &self.edge_points(map, i, k), 0.0, 0.04, 0.034, red);
                        }
                    }
                }
            }
        }
        for &(p, color) in &scene.occupied {
            for &i in &map.provinces[p].cells {
                let (c, r) = map.cell(i);
                if (c + 2 * r) % 3 != 0 {
                    continue;
                }
                // A small hex in the middle, lying on the ground's inner triangles.
                let (x, y) = hex::center(c, r);
                let g = &self.geo[i];
                let v0 = fronts.add_vertex(self.world(x, y, g[0] + 0.025));
                let corners: Vec<usize> = (0..6)
                    .map(|k| {
                        let (qx, qy) = hex::corner(x, y, k);
                        fronts.add_vertex(self.world(x + (qx - x) * 0.42, y + (qy - y) * 0.42, g[0] + (g[13 + k] - g[0]) * 0.84 + 0.025))
                    })
                    .collect();
                for k in 0..6 {
                    fronts.add_triangle(v0, corners[k], corners[(k + 1) % 6], color);
                }
            }
        }
        for &(a, b, color) in &scene.arrows {
            let pa = map.provinces[a].middle;
            let pb = map.provinces[b].middle;
            let from = self.world(pa.0, pa.1, 0.32);
            let to = self.world(pb.0, pb.1, 0.32);
            let d = to - from;
            let len = (d.x * d.x + d.z * d.z).sqrt();
            if len < 0.1 {
                continue;
            }
            let dir = vec3(d.x / len, 0.0, d.z / len);
            let side = vec3(-dir.z, 0.0, dir.x);
            let start = from + dir * (len * 0.15);
            let tip = from + dir * (len * 0.85);
            let neck = tip - dir * 0.45;
            let (w0, w1) = (0.09, 0.26);
            let quad = [start + side * w0, neck + side * w0, neck - side * w0, start - side * w0];
            let ids: Vec<usize> = quad.iter().map(|&v| fronts.add_vertex(v)).collect();
            fronts.add_quad(ids[0], ids[1], ids[2], ids[3], color);
            fronts.add_quad(ids[0], ids[3], ids[2], ids[1], color);
            let h = [fronts.add_vertex(neck + side * w1), fronts.add_vertex(tip), fronts.add_vertex(neck - side * w1)];
            fronts.add_triangle(h[0], h[1], h[2], color);
            fronts.add_triangle(h[0], h[2], h[1], color);
        }
        self.fronts = fronts.glowing();
    }

    /// The ground point of a province's middle (for markers and the camera).
    pub fn province_point(&self, atlas: &Atlas, world: &World, p: usize, lift: f32) -> Vec3 {
        let map = atlas.map(world.map);
        let (x, y) = map.provinces[p].middle;
        let i = map.index(map.provinces[p].at.0, map.provinces[p].at.1);
        self.world(x, y, self.heights[i].max(0.0) + lift)
    }

    pub fn minimap(&self) -> &Sprite {
        &self.minimap[match self.mode {
            Mode::Political => 0,
            Mode::Terrain => 1,
        }]
    }

    /// A world ground point as a minimap pixel, for a minimap drawn at `at`.
    pub fn to_minimap(&self, p: Vec2, at: Vec2) -> Vec2 {
        let (x, y) = self.plane(vec3(p.x, 0.0, p.y));
        at + vec2(x * 4.0, y * 4.0)
    }

    /// The world ground point under a minimap pixel.
    pub fn minimap_to_ground(&self, pixel: Vec2, at: Vec2) -> Vec2 {
        let (x, y) = ((pixel.x - at.x) / 4.0, (pixel.y - at.y) / 4.0);
        let p = self.world(x, y, 0.0);
        vec2(p.x, p.z)
    }

    /// Triangles drawn in one frame (for checking the map stays light): one
    /// view of the land and one width of province lines, plus everything else.
    pub fn triangles(&self) -> usize {
        self.chunks.iter().map(|c| c.meshes[0].triangles.len()).sum::<usize>()
            + self.sea.triangles.len()
            + self.water.iter().map(|m| m.triangles.len()).sum::<usize>()
            + self.decor.triangles.len()
            + self.rivers.triangles.len()
            + self.borders.triangles.len()
            + self.gold[0].triangles.len()
            + self.lines[0].triangles.len()
            + self.cities.triangles.len()
            + self.armies.triangles.len()
            + self.fronts.triangles.len()
    }
}

/// The map index of the cell under each pixel of a `pw` x `ph` minimap drawn
/// `k` pixels to a hex (`u32::MAX`: none). It is `hex::cell_at`'s answer
/// for each pixel, worked out the same way (the nearest cell middle, the
/// first of equals in its order, rows then columns), but a row of pixels at
/// a time and without library calls for rounding: `cell_at` for each of the
/// 40,000 pixels was a fifth of the time it took to build the map.
fn minimap_cells(pw: usize, ph: usize, k: f32, w: usize, h: usize) -> Vec<u32> {
    let limit = hex::RADIUS * hex::RADIUS * 1.01;
    let mut cells = Vec::with_capacity(pw * ph);
    for py in 0..ph {
        let y = py as f32 / k;
        // `cell_at` looks at the three rows round the nearest one (y is at
        // least 0, so adding a half and cutting off the fraction rounds it).
        let r0 = (y / hex::ROW + 0.5) as i32;
        let rows: Vec<(usize, f32, f32)> = (r0 - 1..=r0 + 1)
            .filter(|&r| r >= 0 && (r as usize) < h)
            .map(|r| {
                let (_, cy) = hex::center(0, r as usize);
                (r as usize, 0.5 * (r & 1) as f32, (cy - y) * (cy - y))
            })
            .collect();
        for px in 0..pw {
            let x = px as f32 / k;
            let mut best = (u32::MAX, f32::MAX);
            for &(r, shift, dy) in &rows {
                // In a row only the two columns either side of x can be
                // nearest (`x - shift` is at least -0.5, so this rounds down).
                let c0 = (x - shift + 1.0) as i32 - 1;
                for c in c0..=c0 + 1 {
                    if c < 0 || c as usize >= w {
                        continue;
                    }
                    let cx = c as f32 + shift;
                    let d = (cx - x) * (cx - x) + dy;
                    if d < best.1 {
                        best = ((r * w + c as usize) as u32, d);
                    }
                }
            }
            cells.push(if best.1 <= limit { best.0 } else { u32::MAX });
        }
    }
    cells
}

/// The sea's colour by how near the shore it is (`s`: 0 open sea, 1 at the
/// shore), or a lake's.
fn water_colour(w: &WaterStyle, lake: bool, s: f32) -> [f32; 3] {
    if lake {
        return mix3(rgb(w.lake.0), rgb(w.lake_shore.0), smooth(0.5, 1.0, s));
    }
    let stops = [(0.0, w.deep.0), (0.3, w.mid.0), (0.62, w.shelf.0), (1.0, w.coast.0)];
    for pair in stops.windows(2) {
        let ((s0, c0), (s1, c1)) = (pair[0], pair[1]);
        if s <= s1 {
            return mix3(rgb(c0), rgb(c1), ((s - s0) / (s1 - s0)).clamp(0.0, 1.0));
        }
    }
    rgb(w.coast.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The minimap's quick lookup gives exactly `hex::cell_at`'s cell for
    /// every pixel (ties and the map's edges included).
    #[test]
    fn minimap_cells_are_cell_at() {
        let k = 4.0;
        for (w, h) in [(68, 42), (5, 3), (13, 8)] {
            let pw = ((w as f32 + 0.5) * k) as usize;
            let ph = (h as f32 * hex::ROW * k + k) as usize;
            let cells = minimap_cells(pw, ph, k, w, h);
            assert_eq!(cells.len(), pw * ph);
            for (at, &got) in cells.iter().enumerate() {
                let (x, y) = ((at % pw) as f32 / k, (at / pw) as f32 / k);
                let want = hex::cell_at(x, y, w, h).map_or(u32::MAX, |(c, r)| (r * w + c) as u32);
                assert_eq!(got, want, "{w} x {h} map, pixel ({}, {})", at % pw, at / pw);
            }
        }
    }

    /// Steady noise stays between 0 and 1 and is the same each time.
    #[test]
    fn noise_is_steady() {
        for i in 0..2000 {
            let (x, y) = (i as f32 * 0.37 - 3.0, i as f32 * 0.11 - 2.0);
            let n = noise(x, y, 5);
            assert!((0.0..=1.0).contains(&n), "noise({x}, {y}) = {n}");
            assert_eq!(n, noise(x, y, 5));
        }
    }
}
