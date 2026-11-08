//! Under the island and on it: tunnels that wind through the rock (a few
//! open at the surface), one big cavern deep down with a ruined brick room
//! in it, ore along the cave walls, and abandoned duck houses on the land,
//! falling apart, each with a chest someone left behind.
//!
//! All from the island's seed, so the same island has the same ruins.

use super::*;

/// How many abandoned houses stand on the land (if there's room).
pub const HOUSES: usize = 3;

/// A number from 0 to 1 for (a, b, c) on this island, for this purpose.
fn roll(a: i32, b: i32, c: i32, seed: u32, purpose: u32) -> f32 {
    random01(a, b, c, seed.wrapping_add(purpose))
}

impl World {
    /// Tunnels: worms that wander through the rock, carving as they go.
    /// The first two may break out at the surface (cave mouths).
    pub fn dig_tunnels(&mut self, height: &dyn Fn(i32, i32) -> i32) {
        let seed = self.seed;
        for worm in 0..9 {
            let r = |k: i32| roll(worm, k, 0, seed, 50);
            let mut at = vec3(8.0 + r(1) * 48.0, 5.0 + r(2) * 16.0, 8.0 + r(3) * 48.0);
            let mut yaw = r(4) * TAU;
            let mut pitch = (r(5) - 0.5) * 0.5;
            let mouth = worm < 2;
            if mouth {
                at.y = 16.0 + r(6) * 8.0; // starts high, near the top
                pitch = -0.35; // heads down
            }
            let steps = 40 + (r(7) * 50.0) as i32;
            for step in 0..steps {
                let s = |k: i32| roll(worm, step, k, seed, 51);
                yaw += (s(1) - 0.5) * 0.7;
                pitch = (pitch + (s(2) - 0.5) * 0.3).clamp(-0.55, 0.45);
                at += vec3(yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos());
                if at.y < 3.0 {
                    pitch = pitch.abs();
                }
                let radius = 1.1 + 0.9 * s(3);
                self.carve_ball(at, vec3(radius, radius * 0.85, radius), height, mouth);
            }
        }
    }

    /// The cavern: a wide low hall deep under the highest land, with a
    /// ruined brick room on its floor.
    pub fn hollow_out_cavern(&mut self, height: &dyn Fn(i32, i32) -> i32) -> Option<[i32; 3]> {
        // Under the highest ground near the middle (it has the most rock).
        let mut best: Option<(i32, i32, i32)> = None;
        for z in (16..48).step_by(2) {
            for x in (16..48).step_by(2) {
                let h = height(x, z);
                if best.is_none_or(|(b, _, _)| h > b) {
                    best = Some((h, x, z));
                }
            }
        }
        let (h, cx, cz) = best?;
        if h < SEA + 8 {
            return None;
        }
        let middle = vec3(cx as f32, 6.5, cz as f32);
        for y in 2..12 {
            for z in cz - 9..=cz + 9 {
                for x in cx - 10..=cx + 10 {
                    let d = vec3(x as f32, y as f32, z as f32) - middle;
                    let rough = roll(x, y, z, self.seed, 52) * 0.25;
                    let inside = (d.x / 9.0).powi(2) + (d.y / 4.0).powi(2) + (d.z / 8.0).powi(2);
                    if inside < 1.0 - rough && y >= 3 {
                        self.carve(x, y, z, height, false);
                    }
                }
            }
        }
        // The brick room on its floor: walls with holes, no roof left,
        // a chest in the corner.
        let floor = 3;
        let (x0, z0) = (cx - 2, cz - 2);
        for z in z0..=z0 + 4 {
            for x in x0..=x0 + 4 {
                self.grid.set(x, floor - 1, z, Block::Brick as BlockId);
                let wall = x == x0 || x == x0 + 4 || z == z0 || z == z0 + 4;
                for y in floor..floor + 3 {
                    let crumbled = roll(x, y, z, self.seed, 53) < 0.25 + 0.15 * (y - floor) as f32;
                    let doorway = z == z0 + 4 && x == x0 + 2 && y < floor + 2;
                    let block = if wall && !crumbled && !doorway {
                        Block::Brick
                    } else {
                        Block::Air
                    };
                    self.grid.set(x, y, z, block as BlockId);
                }
            }
        }
        let chest = [x0 + 1, floor, z0 + 1];
        self.grid.set(chest[0], chest[1], chest[2], Block::Chest as BlockId);
        self.fill_chest(chest, true);
        Some(chest)
    }

    /// Ore along the cave walls: coal up to y 30, gold deep, in little
    /// veins (a few blocks each), so caves are worth going into.
    pub fn lay_ore_veins(&mut self) {
        let seed = self.seed;
        for y in 2..30 {
            for z in 1..SIZE as i32 - 1 {
                for x in 1..SIZE as i32 - 1 {
                    if self.get(x, y, z) != Block::Stone {
                        continue;
                    }
                    let by_air = DIRECTIONS
                        .iter()
                        .any(|&(dx, dy, dz)| self.get(x + dx, y + dy, z + dz) == Block::Air);
                    if !by_air {
                        continue;
                    }
                    let r = roll(x, y, z, seed, 54);
                    let (ore, size) = if y < 12 && r < 0.012 {
                        (Block::GoldOre, 2 + (roll(x, y, z, seed, 55) * 3.0) as i32)
                    } else if r < 0.03 {
                        (Block::CoalOre, 3 + (roll(x, y, z, seed, 55) * 4.0) as i32)
                    } else {
                        continue;
                    };
                    // A little vein, wandering into the stone.
                    let [mut vx, mut vy, mut vz] = [x, y, z];
                    for k in 0..size {
                        if self.get(vx, vy, vz) == Block::Stone {
                            self.grid.set(vx, vy, vz, ore as BlockId);
                        }
                        let (dx, dy, dz) = DIRECTIONS
                            [(roll(vx, vy, vz + k, seed, 56) * DIRECTIONS.len() as f32) as usize
                                % DIRECTIONS.len()];
                        (vx, vy, vz) = (vx + dx, vy + dy, vz + dz);
                    }
                }
            }
        }
    }

    /// Abandoned duck houses on fairly flat land: cobblestone and plank
    /// walls with holes in, roofs half gone, the door left open, leaves
    /// growing over them, and a chest inside. Returns the chests' cells.
    pub fn build_ruins(&mut self, height: &dyn Fn(i32, i32) -> i32, spawn_near: (i32, i32)) -> Vec<[i32; 3]> {
        let seed = self.seed;
        let mut chests: Vec<[i32; 3]> = Vec::new();
        let mut tries = 0;
        while chests.len() < HOUSES && tries < 400 {
            tries += 1;
            let x0 = 4 + (roll(tries, 0, 0, seed, 60) * (SIZE as f32 - 14.0)) as i32;
            let z0 = 4 + (roll(tries, 1, 0, seed, 60) * (SIZE as f32 - 14.0)) as i32;
            let (w, d) = (6, 5);
            // Not right on top of the start, nor another house.
            let (sx, sz) = spawn_near;
            if (x0 + 3 - sx).abs() < 8 && (z0 + 2 - sz).abs() < 8 {
                continue;
            }
            if chests
                .iter()
                .any(|c| (c[0] - x0).abs() < 12 && (c[2] - z0).abs() < 12)
            {
                continue;
            }
            // Flat-ish grass, above the sea, nothing growing there.
            let mut floor = i32::MIN;
            let mut low = i32::MAX;
            let mut fine = true;
            for z in z0..z0 + d {
                for x in x0..x0 + w {
                    let h = height(x, z);
                    floor = floor.max(h);
                    low = low.min(h);
                    if h <= SEA + 1 {
                        fine = false;
                    }
                    for y in h + 1..h + 6 {
                        if matches!(self.get(x, y, z), Block::Log | Block::Leaves) {
                            fine = false;
                        }
                    }
                }
            }
            if !fine || floor - low > 3 {
                continue;
            }
            let base = floor; // the floor's height (filled up to it)
            for z in z0..z0 + d {
                for x in x0..x0 + w {
                    for y in height(x, z)..base {
                        self.grid.set(x, y, z, Block::Cobblestone as BlockId);
                    }
                    let rotten = roll(x, base, z, seed, 61) < 0.3;
                    let block = if rotten { Block::Dirt } else { Block::Planks };
                    self.grid.set(x, base, z, block as BlockId);
                    let wall = x == x0 || x == x0 + w - 1 || z == z0 || z == z0 + d - 1;
                    for y in base + 1..=base + 3 {
                        let doorway = z == z0 + d - 1 && x == x0 + 2 && y <= base + 2;
                        let crumbled =
                            roll(x, y, z, seed, 62) < 0.2 + 0.18 * (y - base - 1) as f32;
                        let block = if doorway {
                            if y == base + 1 {
                                Block::DoorLowOpen
                            } else {
                                Block::DoorHighOpen
                            }
                        } else if wall && !crumbled {
                            if roll(x, y, z, seed, 63) < 0.35 {
                                Block::Planks
                            } else {
                                Block::Cobblestone
                            }
                        } else {
                            Block::Air
                        };
                        self.grid.set(x, y, z, block as BlockId);
                    }
                    // What's left of the roof.
                    let roof = if roll(x, base + 4, z, seed, 64) < 0.45 {
                        Block::Planks
                    } else {
                        Block::Air
                    };
                    self.grid.set(x, base + 4, z, roof as BlockId);
                    // Leaves creeping over the walls.
                    if wall && roll(x, base + 5, z, seed, 65) < 0.18 {
                        let y = base + 2 + (roll(x, 0, z, seed, 66) * 2.0) as i32;
                        if self.get(x, y, z) == Block::Air {
                            self.grid.set(x, y, z, Block::Leaves as BlockId);
                        }
                    }
                }
            }
            // An old bed, and the chest.
            self.grid.set(x0 + w - 2, base + 1, z0 + 1, Block::Bed as BlockId);
            let chest = [x0 + 1, base + 1, z0 + 1];
            self.grid.set(chest[0], chest[1], chest[2], Block::Chest as BlockId);
            self.fill_chest(chest, false);
            chests.push(chest);
        }
        chests
    }

    /// What someone left: food, coal, torches, a little gold; deep down,
    /// more gold, and a blade.
    fn fill_chest(&mut self, cell: [i32; 3], deep: bool) {
        let seed = self.seed;
        let [x, y, z] = cell;
        let r = |k: i32| roll(x + k * 7, y, z, seed, 70);
        let amount = |k: i32, low: u32, high: u32| low + (r(k) * (high - low + 1) as f32) as u32;
        let mut loot: Vec<(Item, u32)> = if deep {
            vec![
                (Item::Gold, amount(1, 2, 5)),
                (Item::Blade, 1),
                (Item::Torch, amount(2, 4, 8)),
                (Item::Coal, amount(3, 4, 8)),
                (Item::Bread, amount(4, 1, 2)),
            ]
        } else {
            vec![
                (Item::Bread, amount(1, 0, 3)),
                (Item::Apple, amount(2, 1, 4)),
                (Item::Coal, amount(3, 2, 6)),
                (Item::Torch, amount(4, 2, 5)),
                (Item::Seeds, amount(5, 0, 6)),
                (Item::Gold, amount(6, 0, 2)),
            ]
        };
        loot.retain(|&(_, n)| n > 0);
        let mut slots = vec![None; furniture::CHEST_SLOTS];
        for (k, (item, count)) in loot.into_iter().enumerate() {
            // Scattered about the chest, not neatly at the front.
            let mut slot = (r(10 + k as i32) * furniture::CHEST_SLOTS as f32) as usize % furniture::CHEST_SLOTS;
            while slots[slot].is_some() {
                slot = (slot + 1) % furniture::CHEST_SLOTS;
            }
            slots[slot] = Some(Stack::new(item, count));
        }
        self.chests.insert(cell, slots);
    }

    /// Clears a ball (radii `size`) round `at`, cell by cell.
    fn carve_ball(&mut self, at: Vec3, size: Vec3, height: &dyn Fn(i32, i32) -> i32, mouth: bool) {
        let reach = size.x.max(size.y).max(size.z).ceil() as i32;
        let [cx, cy, cz] = [at.x, at.y, at.z].map(|v| v.floor() as i32);
        for y in cy - reach..=cy + reach {
            for z in cz - reach..=cz + reach {
                for x in cx - reach..=cx + reach {
                    let d = vec3(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5) - at;
                    let inside = (d.x / size.x).powi(2) + (d.y / size.y).powi(2) + (d.z / size.z).powi(2);
                    if inside < 1.0 {
                        self.carve(x, y, z, height, mouth);
                    }
                }
            }
        }
    }

    /// Makes one cell air, if it's rock under land: never bedrock, never
    /// next to the sea (no holes for it to pour into), and only through
    /// the surface if this is a cave's mouth.
    fn carve(&mut self, x: i32, y: i32, z: i32, height: &dyn Fn(i32, i32) -> i32, mouth: bool) {
        if !World::inside(x, y, z) || y < 2 || x < 1 || z < 1 || x >= SIZE as i32 - 1 || z >= SIZE as i32 - 1 {
            return;
        }
        let h = height(x, z);
        if h <= SEA + 2 || y > h || (!mouth && y > h - 2) {
            return;
        }
        let wet = DIRECTIONS
            .iter()
            .any(|&(dx, dy, dz)| self.get(x + dx, y + dy, z + dz) == Block::Water);
        if wet || self.get(x, y, z) == Block::Bedrock {
            return;
        }
        self.grid.set(x, y, z, Block::Air as BlockId);
    }
}
