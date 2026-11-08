//! How the Hollows find their way to the duck: a map, made a few times a
//! second, of how many steps each place they could stand is from the duck
//! (a "flow field"). Every Hollow just steps to the neighbour with fewer
//! steps, so one map serves them all, and they go round walls, up steps,
//! down drops and through gaps instead of walking into the nearest wall.
//!
//! Lit places (near a lamp or a torch) aren't on the map at all, so the
//! Hollows stop where the light starts and wait there. Shut doors are
//! solid, so they aren't either.
//!
//! There are two maps: one for walkers (two blocks tall, up one step at a
//! time) and one for crawlers (one block tall, and they climb walls).

use super::*;
use std::collections::VecDeque;

/// How far the map reaches from the duck, in blocks (each way).
pub const REACH: i32 = 30;
const UNSEEN: u16 = u16::MAX;
/// The farthest a walker drops down in one step.
const MAX_DROP: i32 = 3;

#[derive(Debug, Clone)]
pub struct Field {
    steps: Vec<u16>,
    /// The cells written last time, to wipe before the next.
    written: Vec<usize>,
    /// The duck's cell the map was made for.
    pub goal: Option<[i32; 3]>,
    /// For crawlers: one block tall, and they climb.
    climbing: bool,
}

fn index(x: i32, y: i32, z: i32) -> Option<usize> {
    World::inside(x, y, z).then(|| (y as usize * SIZE + z as usize) * SIZE + x as usize)
}

const SIDES: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

impl Field {
    pub fn new(climbing: bool) -> Field {
        Field {
            steps: vec![UNSEEN; SIZE * TALL * SIZE],
            written: Vec::new(),
            goal: None,
            climbing,
        }
    }

    /// Steps from (x, y, z) to the duck, if the map reaches it.
    pub fn steps(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        index(x, y, z)
            .map(|i| self.steps[i])
            .filter(|&s| s != UNSEEN)
    }

    /// Room for the body here (two cells for a walker, one for a crawler).
    pub fn fits(&self, world: &World, x: i32, y: i32, z: i32) -> bool {
        World::inside(x, y, z)
            && !world.get(x, y, z).solid()
            && (self.climbing || !world.get(x, y + 1, z).solid())
    }

    /// Somewhere it can be: room, and something to stand on (or water to
    /// swim in; a crawler can also cling to a wall).
    pub fn stands(&self, world: &World, x: i32, y: i32, z: i32) -> bool {
        self.fits(world, x, y, z)
            && (world.get(x, y - 1, z).solid()
                || world.get(x, y, z) == Block::Water
                || (self.climbing && self.clings(world, x, y, z)))
    }

    /// A wall beside it, to climb.
    fn clings(&self, world: &World, x: i32, y: i32, z: i32) -> bool {
        SIDES
            .iter()
            .any(|&(dx, dz)| world.get(x + dx, y, z + dz).solid())
    }

    /// Whether something at `from` can get to the next-door `to` in one
    /// step: along, up one (with room to jump), down a drop, or, for a
    /// crawler, straight up or down a wall.
    pub fn step_ok(&self, world: &World, from: [i32; 3], to: [i32; 3]) -> bool {
        let [fx, fy, fz] = from;
        let [tx, ty, tz] = to;
        if !self.stands(world, tx, ty, tz) {
            return false;
        }
        let dy = ty - fy;
        if fx == tx && fz == tz {
            // Straight up or down: only crawlers, along a wall.
            return self.climbing && dy.abs() == 1;
        }
        match dy {
            0 => true,
            // Up a step: room over its head to jump.
            1 => self.climbing || !world.get(fx, fy + 2, fz).solid(),
            // Down: nothing in the way over where it lands.
            d if d < 0 && -d <= MAX_DROP => {
                (ty..=fy).all(|y| !world.get(tx, y, tz).solid())
                    && (self.climbing || !world.get(tx, fy + 1, tz).solid())
            }
            _ => false,
        }
    }

    /// Makes the map again, out from the duck's cell `goal`, leaving out
    /// the cells `avoid` says to (lit ones).
    pub fn build(&mut self, world: &World, goal: [i32; 3], avoid: &dyn Fn(i32, i32, i32) -> bool) {
        for &i in &self.written {
            self.steps[i] = UNSEEN;
        }
        self.written.clear();
        self.goal = Some(goal);
        // The duck may be in the air: start from the floor under it.
        let [gx, mut gy, gz] = goal;
        while gy > 1 && !self.stands(world, gx, gy, gz) && goal[1] - gy < 6 {
            gy -= 1;
        }
        let Some(start) = index(gx, gy, gz) else {
            return;
        };
        self.steps[start] = 0;
        self.written.push(start);
        let mut queue = VecDeque::from([[gx, gy, gz]]);
        while let Some(at) = queue.pop_front() {
            let here = self.steps[index(at[0], at[1], at[2]).unwrap()];
            if here >= (REACH * 3) as u16 {
                continue;
            }
            // Every cell from which a Hollow could step to `at`.
            let mut froms = Vec::with_capacity(24);
            for (dx, dz) in SIDES {
                for dy in -1..=MAX_DROP {
                    froms.push([at[0] + dx, at[1] + dy, at[2] + dz]);
                }
            }
            if self.climbing {
                froms.push([at[0], at[1] - 1, at[2]]);
                froms.push([at[0], at[1] + 1, at[2]]);
            }
            for from in froms {
                let [x, y, z] = from;
                if (x - gx).abs() > REACH || (z - gz).abs() > REACH {
                    continue;
                }
                let Some(i) = index(x, y, z) else {
                    continue;
                };
                if self.steps[i] != UNSEEN || !self.stands(world, x, y, z) || avoid(x, y, z) {
                    continue;
                }
                if !self.step_ok(world, from, at) {
                    continue;
                }
                self.steps[i] = here + 1;
                self.written.push(i);
                queue.push_back(from);
            }
        }
    }

    /// The best next cell from `at`: the neighbour it can step to with the
    /// fewest steps left (fewer than here).
    pub fn next_from(&self, world: &World, at: [i32; 3]) -> Option<[i32; 3]> {
        let here = self.steps(at[0], at[1], at[2])?;
        let mut best: Option<([i32; 3], u16)> = None;
        let mut consider = |to: [i32; 3]| {
            if let Some(s) = self.steps(to[0], to[1], to[2])
                && s < here
                && best.is_none_or(|(_, b)| s < b)
                && self.step_ok(world, at, to)
            {
                best = Some((to, s));
            }
        };
        for (dx, dz) in SIDES {
            for dy in -MAX_DROP..=1 {
                consider([at[0] + dx, at[1] + dy, at[2] + dz]);
            }
        }
        if self.climbing {
            consider([at[0], at[1] + 1, at[2]]);
            consider([at[0], at[1] - 1, at[2]]);
        }
        best.map(|(cell, _)| cell)
    }
}

/// Whether nothing solid and see-through-proof stands between two points
/// (sampled every quarter block).
pub fn clear_line(world: &World, from: Vec3, to: Vec3) -> bool {
    let d = to - from;
    let steps = (d.length() * 4.0).ceil() as i32;
    (1..steps).all(|i| {
        let p = from + d * (i as f32 / steps as f32);
        !world
            .get(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
            .opaque()
    })
}
