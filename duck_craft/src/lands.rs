//! Past the island: a dark sea, and three lands across it, each wrong in
//! its own way. A village where no one is (the doors all open, the beds
//! made), a grey forest whose fog never lifts, and a land that has come
//! apart: ground hanging in the air, holes to the bottom of the world,
//! blocks that aren't anything.
//!
//! The island is the old 64-block one, in the middle, grown from the same
//! noise as before (in its own corner of it), so it's the island you know.

use super::*;

/// The old island's side, and how far its corner is from the world's.
pub const ISLAND: i32 = 64;
pub const SHIFT: i32 = (SIZE as i32 - ISLAND) / 2;

/// How far the other lands' middles are from the island's, and how big.
const LAND_DISTANCE: f32 = 66.0;
const LAND_RADIUS: f32 = 23.0;
/// Blocks over which a shore sinks into the sea.
const SHORE: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Land {
    Island,
    Village,
    Forest,
    Broken,
}

impl Land {
    /// The lands across the sea, in `middles`' order.
    pub const AWAY: [Land; 3] = [Land::Village, Land::Forest, Land::Broken];

    pub fn name(self) -> &'static str {
        match self {
            Land::Island => "the island",
            Land::Village => "the village",
            Land::Forest => "the grey forest",
            Land::Broken => "the broken land",
        }
    }

    /// How far you see there, as a share of anywhere else: the village and
    /// the forest are always in fog.
    pub fn sight(self) -> f32 {
        match self {
            Land::Island => 1.0,
            Land::Village => 0.42,
            Land::Forest => 0.26,
            Land::Broken => 0.55,
        }
    }

    /// How uneasy it is just being there (the static's floor).
    pub fn unease(self) -> f32 {
        match self {
            Land::Island => 0.0,
            Land::Village => 0.1,
            Land::Forest => 0.18,
            Land::Broken => 0.42,
        }
    }
}

/// The middle of each land across the sea, in `Land::AWAY`'s order: a
/// third of a turn apart, turned by the seed.
pub fn middles(seed: u32) -> [Vec2; 3] {
    let turn = random01(1, 2, 3, seed.wrapping_add(90)) * TAU;
    let c = SIZE as f32 / 2.0;
    std::array::from_fn(|i| {
        let a = turn + i as f32 * TAU / 3.0;
        vec2(c + a.sin() * LAND_DISTANCE, c + a.cos() * LAND_DISTANCE)
    })
}

/// Which land (x, z) is part of (or the nearest, out at sea), and how far
/// in: 0 at sea, 1 well inland.
pub fn land_at(x: i32, z: i32, seed: u32) -> (Land, f32) {
    let (lx, lz) = (x - SHIFT, z - SHIFT);
    let edge = lx.min(lz).min(ISLAND - 1 - lx).min(ISLAND - 1 - lz);
    if edge >= 0 {
        return (Land::Island, smooth((edge as f32 / 12.0).min(1.0)));
    }
    let (fx, fz) = (x as f32, z as f32);
    let mut best = (Land::Village, f32::MIN);
    for (i, middle) in middles(seed).into_iter().enumerate() {
        let wobble = 0.85 + 0.3 * noise2(fx / 11.0, fz / 11.0, seed + 91 + i as u32);
        let inside = LAND_RADIUS * wobble - vec2(fx, fz).distance(middle);
        if inside > best.1 {
            best = (Land::AWAY[i], inside);
        }
    }
    (best.0, smooth((best.1 / SHORE).clamp(0.0, 1.0)))
}

/// The land whose air (x, z) is in, and how much: its fog reaches out
/// over the sea, thickening to the shore (the island has none).
pub fn haze(x: f32, z: f32, seed: u32) -> (Land, f32) {
    let reach = 14.0;
    let mut best = (Land::Island, 0.0);
    for (i, middle) in middles(seed).into_iter().enumerate() {
        let d = vec2(x, z).distance(middle);
        let t = smooth(((LAND_RADIUS + reach - d) / reach).clamp(0.0, 1.0));
        if t > best.1 {
            best = (Land::AWAY[i], t);
        }
    }
    best
}

/// The ground's height at (x, z).
pub fn ground_height(x: i32, z: i32, seed: u32) -> i32 {
    let (land, inland) = land_at(x, z, seed);
    if land == Land::Island {
        return island_height(x - SHIFT, z - SHIFT, seed);
    }
    let (fx, fz) = (x as f32, z as f32);
    let base = 0.5 * noise2(fx / 22.0, fz / 22.0, seed)
        + 0.3 * noise2(fx / 11.0, fz / 11.0, seed + 1)
        + 0.2 * noise2(fx / 5.0, fz / 5.0, seed + 2);
    let ground = match land {
        Land::Village => 13.8 + base * 3.5,
        Land::Forest => 13.2 + base * 9.0,
        _ => {
            // Blocky steps, up and down, three blocks square.
            let step = match hash(x.div_euclid(3), 7, z.div_euclid(3), seed + 93) % 9 {
                0 => 5.0,
                1 => -3.0,
                2 => 2.0,
                _ => 0.0,
            };
            13.0 + base * 12.0 + step
        }
    };
    let seabed = 4.0 + 2.0 * noise2(fx / 9.0, fz / 9.0, seed + 92);
    let mut h = lerp(seabed, ground, inland);
    let edge = x.min(z).min(SIZE as i32 - 1 - x).min(SIZE as i32 - 1 - z) as f32;
    if edge < 10.0 {
        h = lerp(3.0, h, smooth(edge / 10.0));
    }
    (h.round() as i32).clamp(2, TALL as i32 - 12)
}

/// The old island's ground, in its own corner (x and z 0 to 63).
fn island_height(x: i32, z: i32, seed: u32) -> i32 {
    let (fx, fz) = (x as f32, z as f32);
    let base = 0.5 * noise2(fx / 22.0, fz / 22.0, seed)
        + 0.3 * noise2(fx / 11.0, fz / 11.0, seed + 1)
        + 0.2 * noise2(fx / 5.0, fz / 5.0, seed + 2);
    let hills = (noise2(fx / 16.0, fz / 16.0, seed + 3) - 0.55).max(0.0) * 34.0;
    let mut h = 6.5 + base * 17.0 + hills;
    // It sinks under the sea near its edges.
    let edge = x.min(z).min(ISLAND - 1 - x).min(ISLAND - 1 - z) as f32;
    if edge < 12.0 {
        h = lerp(5.0, h, smooth(edge / 12.0));
    }
    (h.round() as i32).clamp(2, TALL as i32 - 12)
}

/// What the top of the ground is on this land (above the beach).
pub fn surface(land: Land, x: i32, z: i32, seed: u32) -> Block {
    match land {
        Land::Island | Land::Village => Block::Grass,
        Land::Forest => Block::DeadGrass,
        Land::Broken => {
            let r = random01(x, 3, z, seed + 94);
            if r < 0.14 {
                Block::Glitch
            } else if r < 0.3 {
                Block::Stone
            } else {
                Block::DeadGrass
            }
        }
    }
}

impl World {
    /// The lands across the sea: the village's houses, the forest's dead
    /// trees, the broken land's floating ground and holes.
    pub fn build_lands(&mut self, height: &dyn Fn(i32, i32) -> i32) {
        let middles = middles(self.seed);
        self.build_village(middles[0], height);
        self.grow_dead_forest(middles[1], height);
        self.break_land(middles[2], height);
    }

    /// A number from 0 to 1 for (a, b, c), for this purpose.
    fn chance(&self, a: i32, b: i32, c: i32, purpose: u32) -> f32 {
        random01(a, b, c, self.seed.wrapping_add(purpose))
    }

    /// A ring of houses round a well, paths out from it. Everything left
    /// as if they all walked out at once: doors open, beds made, chests
    /// full, not one torch.
    fn build_village(&mut self, middle: Vec2, height: &dyn Fn(i32, i32) -> i32) {
        let [mx, mz] = [middle.x as i32, middle.y as i32];
        let ground = height(mx, mz).max(SEA + 2);
        // Paths: six spokes of cobblestone, worn through in places.
        for spoke in 0..6 {
            let a = spoke as f32 * TAU / 6.0 + 0.3;
            for step in 2..17 {
                let x = mx + (a.sin() * step as f32).round() as i32;
                let z = mz + (a.cos() * step as f32).round() as i32;
                let h = height(x, z);
                if self.get(x, h, z) == Block::Grass && self.chance(x, h, z, 100) < 0.85 {
                    self.grid.set(x, h, z, Block::Cobblestone as BlockId);
                }
            }
        }
        // The well: a ring of cobblestone, dark water in it.
        for dz in -2..=2i32 {
            for dx in -2..=2i32 {
                let (x, z) = (mx + dx, mz + dz);
                for y in ground - 3..=ground + 4 {
                    self.grid.set(x, y, z, Block::Air as BlockId);
                }
                for y in height(x, z).min(ground - 3)..ground - 3 {
                    self.grid.set(x, y, z, Block::Cobblestone as BlockId);
                }
                let ring = dx.abs() == 2 || dz.abs() == 2;
                if ring {
                    for y in ground - 3..=ground + 1 {
                        self.grid.set(x, y, z, Block::Cobblestone as BlockId);
                    }
                } else {
                    self.grid.set(x, ground - 3, z, Block::Cobblestone as BlockId);
                    for y in ground - 2..=ground - 1 {
                        self.grid.set(x, y, z, Block::Water as BlockId);
                    }
                    self.grid.set(x, ground, z, Block::Air as BlockId);
                }
            }
        }
        // The houses, round it, each door facing the well.
        for house in 0..6 {
            let a = house as f32 * TAU / 6.0 + (self.chance(house, 0, 0, 101) - 0.5) * 0.5;
            let d = 10.0 + self.chance(house, 1, 0, 101) * 3.0;
            let cx = mx + (a.sin() * d).round() as i32;
            let cz = mz + (a.cos() * d).round() as i32;
            let tall = house == 0;
            self.build_house(cx, cz, (mx - cx, mz - cz), tall, height);
        }
    }

    /// One village house, 5 by 5 (7 by 5 and brick if `tall`), centred on
    /// (cx, cz), its door on the side facing `toward`.
    fn build_house(&mut self, cx: i32, cz: i32, toward: (i32, i32), tall: bool, height: &dyn Fn(i32, i32) -> i32) {
        let (hw, hd) = if tall { (3, 2) } else { (2, 2) };
        let base = height(cx, cz).max(SEA + 2);
        let top = base + if tall { 6 } else { 4 };
        // Which wall the door is in: the one facing the well.
        let door = if toward.0.abs() > toward.1.abs() {
            (toward.0.signum() * hw, 0)
        } else {
            (0, toward.1.signum() * hd)
        };
        for z in cz - hd..=cz + hd {
            for x in cx - hw..=cx + hw {
                // Level ground: stone under, nothing over.
                for y in height(x, z).min(base)..base {
                    self.grid.set(x, y, z, Block::Cobblestone as BlockId);
                }
                for y in base + 1..=top + 2 {
                    self.grid.set(x, y, z, Block::Air as BlockId);
                }
                self.grid.set(x, base, z, Block::Planks as BlockId);
                let (ex, ez) = ((x - cx).abs() == hw, (z - cz).abs() == hd);
                let wall = ex || ez;
                let corner = ex && ez;
                for y in base + 1..top {
                    let doorway = (x - cx, z - cz) == door && y <= base + 2;
                    let window = wall && !corner && y == base + 2 && (x + z) % 2 == 0;
                    let block = if doorway {
                        if y == base + 1 { Block::DoorLowOpen } else { Block::DoorHighOpen }
                    } else if !wall {
                        Block::Air
                    } else if window {
                        // Most of the glass is still there.
                        if self.chance(x, y, z, 102) < 0.7 { Block::Glass } else { Block::Air }
                    } else if corner {
                        Block::Log
                    } else if tall {
                        Block::Brick
                    } else {
                        Block::Planks
                    };
                    self.grid.set(x, y, z, block as BlockId);
                }
                // The roof, a hole or two in it.
                let roof = if self.chance(x, top, z, 103) < 0.88 { Block::Planks } else { Block::Air };
                self.grid.set(x, top, z, roof as BlockId);
            }
        }
        // Inside: a bed and a chest, as they were left.
        let inside = |dx: i32, dz: i32| [cx + dx, base + 1, cz + dz];
        let (bed, chest) = if door.0 != 0 {
            (inside(-door.0.signum(), hd - 1), inside(-door.0.signum(), 1 - hd))
        } else {
            (inside(hw - 1, -door.1.signum()), inside(1 - hw, -door.1.signum()))
        };
        self.grid.set(bed[0], bed[1], bed[2], Block::Bed as BlockId);
        self.grid.set(chest[0], chest[1], chest[2], Block::Chest as BlockId);
        self.fill_chest(chest, false);
    }

    /// Trees with no leaves on grey ground, close together, a few fallen.
    fn grow_dead_forest(&mut self, middle: Vec2, height: &dyn Fn(i32, i32) -> i32) {
        let reach = LAND_RADIUS as i32 + 6;
        let [mx, mz] = [middle.x as i32, middle.y as i32];
        let mut trees: Vec<(i32, i32)> = Vec::new();
        for z in mz - reach..=mz + reach {
            for x in mx - reach..=mx + reach {
                if !World::inside(x, 0, z) {
                    continue;
                }
                let h = height(x, z);
                if self.get(x, h, z) != Block::DeadGrass {
                    continue;
                }
                let room = trees.iter().all(|&(tx, tz)| (tx - x).abs().max((tz - z).abs()) > 2);
                let r = self.chance(x, 0, z, 104);
                if room && r < 0.1 {
                    trees.push((x, z));
                    self.grow_dead_tree(x, h + 1, z);
                } else if r > 0.996 {
                    // A fallen trunk.
                    let along = self.chance(x, 1, z, 104) < 0.5;
                    for i in 0..4 {
                        let (lx, lz) = if along { (x + i, z) } else { (x, z + i) };
                        if self.get(lx, h + 1, lz) == Block::Air && height(lx, lz) == h {
                            self.grid.set(lx, h + 1, lz, Block::DeadLog as BlockId);
                        }
                    }
                }
            }
        }
    }

    /// A bare trunk, five to nine tall, with stubs of branches near the top.
    pub fn grow_dead_tree(&mut self, x: i32, base: i32, z: i32) {
        let tall = 5 + (self.chance(x, base, z, 105) * 5.0) as i32;
        let top = (base + tall - 1).min(TALL as i32 - 2);
        for y in base..=top {
            self.grid.set(x, y, z, Block::DeadLog as BlockId);
        }
        for (i, (dx, dz)) in [(1, 0), (-1, 0), (0, 1), (0, -1)].into_iter().enumerate() {
            if self.chance(x, i as i32, z, 106) < 0.55 {
                let y = top - 1 - (self.chance(x, i as i32, z, 107) * 3.0) as i32;
                // Out one, then (sometimes) up one: a bent stub.
                let up = self.chance(x, i as i32, z, 108) < 0.5;
                let stub = [(x + dx, y, z + dz), (x + dx, y + 1, z + dz)];
                for &(bx, by, bz) in &stub[..1 + up as usize] {
                    if self.get(bx, by, bz) == Block::Air {
                        self.grid.set(bx, by, bz, Block::DeadLog as BlockId);
                    }
                }
            }
        }
    }

    /// Ground hanging in the air, holes to the bottom of the world, and
    /// blocks that aren't anything, floating on their own.
    fn break_land(&mut self, middle: Vec2, height: &dyn Fn(i32, i32) -> i32) {
        let spread = LAND_RADIUS * 0.85;
        let place = |w: &World, k: i32, purpose: u32| {
            let a = w.chance(k, 0, 0, purpose) * TAU;
            let d = w.chance(k, 1, 0, purpose).sqrt() * spread;
            ((middle.x + a.sin() * d) as i32, (middle.y + a.cos() * d) as i32)
        };
        // Lumps of ground, torn out and left in the air.
        for k in 0..16 {
            let (x, z) = place(self, k, 110);
            let r = 1.3 + self.chance(k, 2, 0, 110) * 1.6;
            let y = (height(x, z) + 7 + (self.chance(k, 3, 0, 110) * 10.0) as i32).min(TALL as i32 - 5);
            let reach = r.ceil() as i32;
            for dy in -reach..=reach {
                for dz in -reach..=reach {
                    for dx in -reach..=reach {
                        // Flat on top, pointed underneath, like a lump pulled out.
                        let squash = if dy > 0 { 2.5 } else { 1.0 };
                        let inside = ((dx * dx + dz * dz) as f32 + (dy as f32 * squash).powi(2)).sqrt();
                        let (bx, by, bz) = (x + dx, y + dy, z + dz);
                        if inside > r || !World::inside(bx, by, bz) {
                            continue;
                        }
                        let block = if dy >= 0 && inside > r - 1.0 && dy as f32 * squash >= r - 1.5 {
                            Block::DeadGrass
                        } else if self.chance(bx, by, bz, 111) < 0.2 {
                            Block::Glitch
                        } else if dy < -1 {
                            Block::Stone
                        } else {
                            Block::Dirt
                        };
                        self.grid.set(bx, by, bz, block as BlockId);
                    }
                }
            }
            // Its top: grey grass wherever there's open sky.
            for dz in -reach..=reach {
                for dx in -reach..=reach {
                    let (bx, bz) = (x + dx, z + dz);
                    for by in (y - reach..=y + reach).rev() {
                        if World::inside(bx, by, bz) && self.get(bx, by, bz).solid() {
                            if self.get(bx, by, bz) == Block::Dirt {
                                self.grid.set(bx, by, bz, Block::DeadGrass as BlockId);
                            }
                            break;
                        }
                    }
                }
            }
        }
        // Holes down to the bottom of the world (away from the sea).
        let mut holes = 0;
        for k in 0..30 {
            if holes == 4 {
                break;
            }
            let (x, z) = place(self, k, 112);
            let (_, inland) = land_at(x, z, self.seed);
            let r = 1.6 + self.chance(k, 2, 0, 112) * 1.2;
            if inland < 0.99 || height(x, z) <= SEA + 2 {
                continue;
            }
            let reach = r.ceil() as i32;
            let dry = (-reach - 1..=reach + 1).all(|dz| {
                (-reach - 1..=reach + 1).all(|dx| {
                    (1..TALL as i32).all(|y| self.get(x + dx, y, z + dz) != Block::Water)
                })
            });
            if !dry {
                continue;
            }
            holes += 1;
            for dz in -reach..=reach {
                for dx in -reach..=reach {
                    if ((dx * dx + dz * dz) as f32).sqrt() <= r {
                        for y in 1..TALL as i32 {
                            self.grid.set(x + dx, y, z + dz, Block::Air as BlockId);
                        }
                    }
                }
            }
        }
        // Single blocks, floating where nothing holds them: something
        // that isn't anything, a stone, a log.
        for k in 0..40 {
            let (x, z) = place(self, k, 113);
            let y = height(x, z) + 2 + (self.chance(k, 2, 0, 113) * 12.0) as i32;
            if !World::inside(x, y, z) || self.get(x, y, z) != Block::Air {
                continue;
            }
            let block = match (self.chance(k, 3, 0, 113) * 6.0) as i32 {
                0 => Block::Cobblestone,
                1 => Block::Log,
                2 => Block::Leaves,
                _ => Block::Glitch,
            };
            self.grid.set(x, y, z, block as BlockId);
            // Some are columns.
            if block == Block::Glitch && self.chance(k, 4, 0, 113) < 0.4 {
                for dy in 1..3 {
                    if World::inside(x, y + dy, z) {
                        self.grid.set(x, y + dy, z, Block::Glitch as BlockId);
                    }
                }
            }
        }
        // A few dead trees.
        for k in 0..6 {
            let (x, z) = place(self, k, 114);
            let h = height(x, z);
            if matches!(self.get(x, h, z), Block::DeadGrass | Block::Stone) && self.get(x, h + 1, z) == Block::Air {
                self.grow_dead_tree(x, h + 1, z);
            }
        }
    }
}
