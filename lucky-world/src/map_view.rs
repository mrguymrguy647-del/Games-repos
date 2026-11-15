//! The 3D world map: hex terrain, borders, rivers, forests and mountains,
//! cities, capitals and landmarks; an orbit camera; picking a province with
//! the mouse; and the minimap.
//!
//! Everything is built once into a few big meshes that never change while
//! you look (the graphics card keeps them), and nothing here turns on bloom,
//! supersampling, lamps or shadows: those would move the 3D onto the CPU
//! (ISSUES.md, Duck Forge item 15).

use std::f32::consts::PI;

use duckforge::prelude::*;

use crate::hex;
use crate::mapdata::{Atlas, TerrainKind, WorldMap};
use crate::world::World;

/// Cells per terrain chunk along each side (chunks off screen are skipped).
const CHUNK: usize = 12;
const SEA_LEVEL: f32 = -0.08;

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
}

const DECOR: [&str; 7] = ["tree", "pine", "palm", "cactus", "peak", "hill", "reeds"];
const LANDMARKS: [&str; 6] = ["lighthouse", "pyramid", "great_tower", "temple", "arch", "colossus"];

impl Models {
    pub fn load() -> Result<Models, duckforge::LoadError> {
        let path = |name: &str| format!("assets/models/{name}.glb");
        let capital_model = duckforge::glb::GlbModel::load(path("capital"))?;
        let capital_roofs = capital_model.part("roofs").map(|p| p.range());
        let mut decor = Vec::new();
        for name in DECOR {
            decor.push((name.to_string(), Mesh::load_glb(path(name))?));
        }
        let mut landmarks = Vec::new();
        for name in LANDMARKS {
            landmarks.push((name.to_string(), Mesh::load_glb(path(name))?));
        }
        Ok(Models {
            capital: capital_model.to_mesh(),
            capital_roofs,
            town: Mesh::load_glb(path("town"))?,
            decor,
            landmarks,
        })
    }

    fn decor(&self, name: &str) -> Option<&Mesh> {
        self.decor.iter().find(|(n, _)| n == name).map(|(_, m)| m)
    }

    fn landmark(&self, name: &str) -> Option<&Mesh> {
        self.landmarks.iter().find(|(n, _)| n == name).map(|(_, m)| m)
    }
}

/// A small, steady hash: the same hex always gets the same jitter.
fn hash(a: usize, b: usize, salt: usize) -> f32 {
    let mut n = (a as u32).wrapping_mul(374_761_393) ^ (b as u32).wrapping_mul(668_265_263) ^ (salt as u32).wrapping_mul(2_246_822_519);
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    ((n ^ (n >> 16)) & 0xffff) as f32 / 65535.0
}

fn shade(c: Color, k: f32) -> Color {
    Color::rgb(
        (c.r as f32 * k).clamp(0.0, 255.0) as u8,
        (c.g as f32 * k).clamp(0.0, 255.0) as u8,
        (c.b as f32 * k).clamp(0.0, 255.0) as u8,
    )
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
    /// Terrain in chunks, coloured both ways: [political, terrain].
    chunks: Vec<[Mesh; 2]>,
    sea: Mesh,
    water: Mesh,
    decor: Mesh,
    rivers: Mesh,
    borders: Mesh,
    lines: Mesh,
    cities: Mesh,
    highlight: Option<Mesh>,
    /// An economy overlay (resources, industry): land coloured by province,
    /// drawn instead of the terrain while it's set.
    overlay: Option<Vec<Mesh>>,
    pub orbit: Orbit,
    /// The map's extent in the world: (min x, min z, max x, max z).
    pub bounds: (f32, f32, f32, f32),
    minimap: [Sprite; 2],
    /// Where each nation's name and flag go: (nation, ground point).
    pub label_points: Vec<(usize, Vec3)>,
    /// Each capital's top, for its flag.
    pub capital_tops: Vec<Vec3>,
    origin: (f32, f32),
    heights: Vec<f32>,
}

impl MapView {
    pub fn new(atlas: &Atlas, world: &World, models: &Models) -> MapView {
        let map = atlas.map(world.map);
        let origin = map.plane_middle();
        let mut view = MapView {
            mode: Mode::Political,
            chunks: Vec::new(),
            sea: Mesh::new(),
            water: Mesh::new(),
            decor: Mesh::new(),
            rivers: Mesh::new(),
            borders: Mesh::new(),
            lines: Mesh::new(),
            cities: Mesh::new(),
            highlight: None,
            overlay: None,
            orbit: Orbit {
                target: Vec2::ZERO,
                yaw: 0.0,
                pitch: 0.95,
                distance: 16.0,
            },
            bounds: (0.0, 0.0, 0.0, 0.0),
            minimap: [Sprite::from_pixels(1, 1, vec![Color::BLACK]), Sprite::from_pixels(1, 1, vec![Color::BLACK])],
            label_points: Vec::new(),
            capital_tops: Vec::new(),
            origin,
            heights: Vec::new(),
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
        view.build_terrain(atlas, map, world);
        view.build_lines(atlas, map, world);
        view.build_rivers(map);
        view.build_decor(atlas, map, models);
        view.build_cities(atlas, map, world, models);
        view.build_minimap(atlas, map, world);
        view.build_labels(map, world);
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

    fn build_terrain(&mut self, atlas: &Atlas, map: &WorldMap, world: &World) {
        let cols = map.width.div_ceil(CHUNK);
        let rows = map.height.div_ceil(CHUNK);
        for cr in 0..rows {
            for cc in 0..cols {
                let mut meshes = [Mesh::new(), Mesh::new()];
                let mut any = false;
                for r in cr * CHUNK..((cr + 1) * CHUNK).min(map.height) {
                    for c in cc * CHUNK..((cc + 1) * CHUNK).min(map.width) {
                        let i = map.index(c, r);
                        let t = atlas.terrain_of(map, i);
                        if t.kind != TerrainKind::Land {
                            continue;
                        }
                        let jitter = 0.94 + 0.12 * hash(c, r, 2);
                        let base = shade(t.color, jitter);
                        let owner = map.province_of[i].and_then(|p| world.owner[p]);
                        let political = match owner {
                            Some(o) => base.lerp(world.color[o], 0.42),
                            None => base.lerp(Color::rgb(120, 120, 120), 0.5),
                        };
                        for (k, top) in [political, base].into_iter().enumerate() {
                            self.add_hex(&mut meshes[k], map, i, top);
                        }
                        any = true;
                    }
                }
                if any {
                    self.chunks.push(meshes);
                }
            }
        }
        // The sea: one big plane, with lighter shallows and lakes on top.
        let deep = atlas.terrains.iter().find(|t| t.letter == '~').map(|t| t.color).unwrap_or(Color::from_hex(0x1c3c7a));
        let (x0, z0, x1, z1) = self.bounds;
        let m = 400.0;
        let mut sea = Mesh::new();
        let a = sea.add_vertex(vec3(x0 - m, SEA_LEVEL, z0 - m));
        let b = sea.add_vertex(vec3(x0 - m, SEA_LEVEL, z1 + m));
        let c = sea.add_vertex(vec3(x1 + m, SEA_LEVEL, z1 + m));
        let d = sea.add_vertex(vec3(x1 + m, SEA_LEVEL, z0 - m));
        sea.add_quad(a, b, c, d, deep);
        self.sea = sea.glowing();
        let mut water = Mesh::new();
        for i in 0..map.cells.len() {
            let t = atlas.terrain_of(map, i);
            if t.kind != TerrainKind::Land && t.letter != '~' {
                let (c, r) = map.cell(i);
                self.add_top(&mut water, map, i, shade(t.color, 0.97 + 0.06 * hash(c, r, 3)));
            }
        }
        self.water = water.glowing();
    }

    fn corner_world(&self, map: &WorldMap, i: usize, k: usize, y: f32) -> Vec3 {
        let (c, r) = map.cell(i);
        let (x, py) = hex::center(c, r);
        let (cx, cy) = hex::corner(x, py, k);
        self.world(cx, cy, y)
    }

    fn add_top(&self, mesh: &mut Mesh, map: &WorldMap, i: usize, color: Color) {
        let y = self.heights[i];
        let centre = mesh.add_vertex(self.cell_world(map, i, 0.0));
        let corners: Vec<usize> = (0..6).map(|k| mesh.add_vertex(self.corner_world(map, i, k, y))).collect();
        for k in 0..6 {
            mesh.add_triangle(centre, corners[k], corners[(k + 1) % 6], color);
        }
    }

    /// A land hex: its top, and its sides where the ground beside is lower.
    fn add_hex(&self, mesh: &mut Mesh, map: &WorldMap, i: usize, color: Color) {
        self.add_top(mesh, map, i, color);
        let (c, r) = map.cell(i);
        let here = hex::center(c, r);
        let y = self.heights[i];
        let side = shade(color, 0.62);
        let mut lower: Vec<(usize, f32)> = Vec::new();
        for (nc, nr) in hex::neighbours(c, r, map.width, map.height) {
            let j = map.index(nc, nr);
            if self.heights[j] < y - 0.005 {
                let (k0, _) = hex::shared_edge(here, hex::center(nc, nr));
                lower.push((k0, self.heights[j].max(SEA_LEVEL)));
            }
        }
        // The map's own edge.
        if c == 0 || r == 0 || c + 1 == map.width || r + 1 == map.height {
            for k in 0..6 {
                if !lower.iter().any(|&(k0, _)| k0 == k) {
                    lower.push((k, SEA_LEVEL));
                }
            }
        }
        for (k, bottom) in lower {
            let k1 = (k + 1) % 6;
            let a = mesh.add_vertex(self.corner_world(map, i, k, y));
            let b = mesh.add_vertex(self.corner_world(map, i, k1, y));
            let cc = mesh.add_vertex(self.corner_world(map, i, k1, bottom));
            let d = mesh.add_vertex(self.corner_world(map, i, k, bottom));
            // Both ways round, so it shows from any side.
            mesh.add_quad(a, b, cc, d, side);
            mesh.add_quad(a, d, cc, b, side);
        }
    }

    /// A strip along one hex edge, inside the hex: `t` is how far in it reaches.
    #[allow(clippy::too_many_arguments)]
    fn edge_strip(&self, mesh: &mut Mesh, map: &WorldMap, i: usize, k: usize, t: f32, lift: f32, color: Color) {
        let y = self.heights[i] + lift;
        let centre = self.cell_world(map, i, lift);
        let p0 = self.corner_world(map, i, k, y);
        let p1 = self.corner_world(map, i, (k + 1) % 6, y);
        let q0 = p0 + (centre - p0) * t;
        let q1 = p1 + (centre - p1) * t;
        let (a, b, c, d) = (mesh.add_vertex(p0), mesh.add_vertex(p1), mesh.add_vertex(q1), mesh.add_vertex(q0));
        mesh.add_quad(a, b, c, d, color);
        mesh.add_quad(a, d, c, b, color);
    }

    /// Nation borders (both sides in their own colour) and thin province lines.
    fn build_lines(&mut self, atlas: &Atlas, map: &WorldMap, world: &World) {
        let mut borders = Mesh::new();
        let mut lines = Mesh::new();
        for i in 0..map.cells.len() {
            let Some(p) = map.province_of[i] else { continue };
            let owner = world.owner[p];
            let (c, r) = map.cell(i);
            let here = hex::center(c, r);
            for (nc, nr) in hex::neighbours(c, r, map.width, map.height) {
                let j = map.index(nc, nr);
                let (k, _) = hex::shared_edge(here, hex::center(nc, nr));
                let other = map.province_of[j];
                let other_owner = other.and_then(|q| world.owner[q]);
                let water = atlas.terrain_of(map, j).kind != TerrainKind::Land;
                if let Some(o) = owner {
                    if water {
                        self.edge_strip(&mut borders, map, i, k, 0.11, 0.012, shade(world.color[o], 1.05));
                    } else if other_owner != owner {
                        self.edge_strip(&mut borders, map, i, k, 0.2, 0.014, world.color[o]);
                        self.edge_strip(&mut borders, map, i, k, 0.04, 0.016, Color::rgb(20, 14, 34));
                    } else if other != Some(p) {
                        self.edge_strip(&mut lines, map, i, k, 0.05, 0.01, shade(world.color[o], 0.45));
                    }
                } else if !water && other != Some(p) {
                    self.edge_strip(&mut lines, map, i, k, 0.05, 0.01, Color::rgb(60, 60, 60));
                }
            }
        }
        self.borders = borders.glowing();
        self.lines = lines;
    }

    fn build_rivers(&mut self, map: &WorldMap) {
        let mut mesh = Mesh::new();
        let color = Color::from_hex(0x4aa8ec);
        for river in &map.rivers {
            for pair in river.cells.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let (ia, ib) = (map.index(a.0, a.1), map.index(b.0, b.1));
                let lift = 0.018;
                let pa = self.cell_world(map, ia, lift);
                let mut pb = self.cell_world(map, ib, lift);
                if map.province_of[ib].is_none() {
                    // The mouth: stop at the shore, at the land's height.
                    pb = pa + (pb - pa) * 0.6;
                    pb.y = pa.y;
                }
                let dir = vec3(pb.x - pa.x, 0.0, pb.z - pa.z).normalized();
                let side = vec3(-dir.z, 0.0, dir.x) * 0.07;
                let (v0, v1, v2, v3) = (mesh.add_vertex(pa - side), mesh.add_vertex(pa + side), mesh.add_vertex(pb + side), mesh.add_vertex(pb - side));
                mesh.add_quad(v0, v1, v2, v3, color);
                mesh.add_quad(v0, v3, v2, v1, color);
            }
        }
        self.rivers = mesh.glowing();
    }

    #[allow(clippy::needless_range_loop)]
    fn build_decor(&mut self, atlas: &Atlas, map: &WorldMap, models: &Models) {
        let mut busy = vec![false; map.cells.len()];
        for p in &map.provinces {
            busy[map.index(p.at.0, p.at.1)] = true;
        }
        for l in &map.landmarks {
            busy[map.index(l.at.0, l.at.1)] = true;
        }
        let mut decor = Mesh::new();
        for i in 0..map.cells.len() {
            let t = atlas.terrain_of(map, i);
            let Some(name) = &t.decor else { continue };
            let Some(model) = models.decor(name) else { continue };
            let (c, r) = map.cell(i);
            let whole = t.decor_count.floor() as usize;
            let count = whole + (hash(c, r, 7) < t.decor_count.fract()) as usize;
            for k in 0..count {
                let a = hash(c, r, 11 + k) * 2.0 * PI;
                let reach = if busy[i] { 0.3 } else { 0.08 + 0.2 * hash(c, r, 21 + k) };
                let base = self.cell_world(map, i, 0.0);
                let at = base + vec3(a.cos() * reach, 0.0, a.sin() * reach);
                let size = 0.85 + 0.3 * hash(c, r, 31 + k);
                let turn = hash(c, r, 41 + k) * 2.0 * PI;
                decor.merge(model, &Transform::at(at).rotated(vec3(0.0, turn, 0.0)).sized(size));
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
                cities.merge(&capital, &Transform::at(at).rotated(vec3(0.0, turn, 0.0)).sized(0.95));
                self.capital_tops[n] = at + vec3(0.0, 0.85, 0.0);
            } else if world.owner[p].is_some() {
                let terrain = &atlas.terrains[prov.terrain];
                let rough = matches!(terrain.letter, 'M' | 'I');
                if !rough && hash(prov.at.0, prov.at.1, 61) < 0.75 {
                    cities.merge(&models.town, &Transform::at(at).rotated(vec3(0.0, turn, 0.0)).sized(0.9));
                }
            }
        }
        for l in &map.landmarks {
            if let Some(model) = models.landmark(&l.kind) {
                let i = map.index(l.at.0, l.at.1);
                let at = self.cell_world(map, i, 0.0);
                cities.merge(model, &Transform::at(at).sized(1.0));
            }
        }
        self.cities = cities;
    }

    fn build_minimap(&mut self, atlas: &Atlas, map: &WorldMap, world: &World) {
        let k = 4.0;
        let w = ((map.width as f32 + 0.5) * k) as usize;
        let h = (map.height as f32 * hex::ROW * k + k) as usize;
        for (slot, political) in [(0, true), (1, false)] {
            let mut pixels = vec![Color::from_hex(0x10244a); w * h];
            for y in 0..h {
                for x in 0..w {
                    let px = x as f32 / k;
                    let py = y as f32 / k;
                    if let Some((c, r)) = hex::cell_at(px, py, map.width, map.height) {
                        let i = map.index(c, r);
                        let t = atlas.terrain_of(map, i);
                        let mut color = t.color;
                        if political && t.kind == TerrainKind::Land {
                            if let Some(o) = map.province_of[i].and_then(|p| world.owner[p]) {
                                color = t.color.lerp(world.color[o], 0.7);
                            }
                        }
                        pixels[y * w + x] = color;
                    }
                }
            }
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
                let here = hex::center(c, r);
                for (nc, nr) in hex::neighbours(c, r, map.width, map.height) {
                    let j = map.index(nc, nr);
                    if map.province_of[j] != Some(p) {
                        let (k, _) = hex::shared_edge(here, hex::center(nc, nr));
                        self.edge_strip(&mut mesh, map, i, k, 0.16, 0.03, Color::WHITE);
                    }
                }
            }
            mesh.glowing()
        });
    }

    /// Colours the land by province (one colour each, `None` for grey), or
    /// back to the normal map with `None`. Built in chunks like the terrain.
    pub fn set_overlay(&mut self, atlas: &Atlas, world: &World, colors: Option<&[Option<Color>]>) {
        let Some(colors) = colors else {
            self.overlay = None;
            return;
        };
        let map = atlas.map(world.map);
        let cols = map.width.div_ceil(CHUNK);
        let rows = map.height.div_ceil(CHUNK);
        let mut meshes = Vec::new();
        for cr in 0..rows {
            for cc in 0..cols {
                let mut mesh = Mesh::new();
                let mut any = false;
                for r in cr * CHUNK..((cr + 1) * CHUNK).min(map.height) {
                    for c in cc * CHUNK..((cc + 1) * CHUNK).min(map.width) {
                        let i = map.index(c, r);
                        let t = atlas.terrain_of(map, i);
                        if t.kind != TerrainKind::Land {
                            continue;
                        }
                        let grey = shade(t.color, 0.94 + 0.12 * hash(c, r, 2)).lerp(Color::rgb(70, 70, 80), 0.75);
                        let color = map.province_of[i].and_then(|p| colors.get(p).copied().flatten()).unwrap_or(grey);
                        self.add_hex(&mut mesh, map, i, shade(color, 0.95 + 0.1 * hash(c, r, 4)));
                        any = true;
                    }
                }
                if any {
                    meshes.push(mesh);
                }
            }
        }
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
        canvas.set_lighting(Lighting {
            sun_direction: vec3(-0.45, -0.8, 0.4),
            sun_color: Color::from_hex(0xfff4e0),
            sun_strength: 0.42,
            ambient: 0.8,
        });
        canvas.draw_mesh(&self.sea, &Transform::default(), camera);
        canvas.draw_mesh(&self.water, &Transform::default(), camera);
        let slot = match self.mode {
            Mode::Political => 0,
            Mode::Terrain => 1,
        };
        if let Some(overlay) = &self.overlay {
            for mesh in overlay {
                canvas.draw_mesh(mesh, &Transform::default(), camera);
            }
        } else {
            for meshes in &self.chunks {
                canvas.draw_mesh(&meshes[slot], &Transform::default(), camera);
            }
        }
        canvas.draw_mesh(&self.lines, &Transform::default(), camera);
        canvas.draw_mesh(&self.rivers, &Transform::default(), camera);
        canvas.draw_mesh(&self.decor, &Transform::default(), camera);
        canvas.draw_mesh(&self.cities, &Transform::default(), camera);
        if self.mode == Mode::Political || self.overlay.is_some() {
            canvas.draw_mesh(&self.borders, &Transform::default(), camera);
        }
        if let Some(h) = &self.highlight {
            canvas.draw_mesh(h, &Transform::default(), camera);
        }
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

    /// Triangles in all the map's meshes (for checking it stays light).
    pub fn triangles(&self) -> usize {
        self.chunks.iter().map(|c| c[0].triangles.len()).sum::<usize>()
            + self.sea.triangles.len()
            + self.water.triangles.len()
            + self.decor.triangles.len()
            + self.rivers.triangles.len()
            + self.borders.triangles.len()
            + self.lines.triangles.len()
            + self.cities.triangles.len()
    }
}
