//! What lives past the island, and what followed you back to it.
//!
//! The Other: a duck like you, with white eyes and nothing behind them.
//! It stands at the edge of the fog and watches, and is gone when you go
//! to it. While you're away (or just not looking) it opens your doors,
//! puts out your torches, builds things that mean nothing (a pyramid of
//! sand, a tunnel into a hill, a tree with no leaves), strips the leaves
//! off a tree. Some nights it stands behind you. Some mornings it's at
//! the foot of your bed.
//!
//! The Nothing: a black shape the shape of a duck, with slits for eyes.
//! It only moves when you can't see it.
//!
//! The moon, from the fourth night, has a face. Don't look at it.
//!
//! And the game itself goes wrong: it freezes, and comes back saying
//! things it shouldn't. Once you've met the Other, even the title isn't
//! right any more.

use super::*;
use lands::Land;
use std::f32::consts::PI;

/// From which day the Other plays its tricks, and stands behind you.
pub const PRANK_DAY: u32 = 2;
pub const BEHIND_DAY: u32 = 3;
/// From which night the Nothing comes, and the moon has a face.
pub const NOTHING_NIGHT: u32 = 3;
pub const MOON_NIGHT: u32 = 4;
/// Seconds of looking at the moon before it comes for you.
pub const STARE_SECONDS: f32 = 5.0;
/// How much the Nothing hurts, and how much the moon does.
pub const NOTHING_HURTS: u32 = 6;
pub const MOON_HURTS: u32 = 4;
/// How long a line stays in the chat.
const CHAT_SECONDS: f32 = 8.0;

/// What the Other is doing here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visit {
    /// At the edge of the fog, watching.
    Watching,
    /// Right behind you, breathing.
    Behind,
    /// At the foot of the bed you just woke in.
    Bed,
}

#[derive(Debug, Clone)]
pub struct Other {
    pub position: Vec3,
    pub yaw: f32,
    pub visit: Visit,
    /// Seconds it's been looked at.
    seen: f32,
    age: f32,
    /// Come this near and it's gone.
    near: f32,
    told: bool,
}

#[derive(Debug, Clone)]
pub struct Nothing {
    pub position: Vec3,
    pub yaw: f32,
    /// Whether you could see it this frame (it doesn't move if so).
    pub seen: bool,
}

#[cfg(test)]
impl Other {
    pub fn new(position: Vec3, visit: Visit) -> Other {
        Other { position, yaw: 0.0, visit, seen: 0.0, age: 0.0, near: 6.0, told: false }
    }
}

/// Everything haunting the game, and when it comes next.
#[derive(Debug, Clone)]
pub struct Haunt {
    /// Seconds of play (not the day's clock: sleeping doesn't count).
    pub clock: f32,
    pub other_at: f32,
    pub prank_at: f32,
    pub glitch_at: f32,
    pub nothing_at: f32,
    pub other: Option<Other>,
    pub nothings: Vec<Nothing>,
    /// Seconds you've been looking at the moon.
    pub moon_stare: f32,
    /// The night it came for you (it doesn't again that night), and how
    /// long its face still fills the screen.
    moon_night: u32,
    pub moon_jump: f32,
    /// Seconds left of a freeze (the game "not responding").
    pub freeze: f32,
    /// Words over the screen, and for how long.
    pub flash: Option<(String, f32)>,
    /// Lines in the chat: what, and how long ago.
    pub chat: Vec<(String, f32)>,
    /// How many times the Other has come (kept in the save).
    pub met: u32,
    pub pranks: u32,
    lands_seen: [bool; 3],
    /// The last day a line was said in the chat, and the night the moon
    /// was warned about.
    said_day: u32,
    warned_night: u32,
    /// None of it (the tests that aren't about it).
    pub calm: bool,
}

impl Default for Haunt {
    fn default() -> Haunt {
        Haunt {
            clock: 0.0,
            other_at: 150.0,
            prank_at: 120.0,
            glitch_at: 240.0,
            nothing_at: 0.0,
            other: None,
            nothings: Vec::new(),
            moon_stare: 0.0,
            moon_night: 0,
            moon_jump: 0.0,
            freeze: 0.0,
            flash: None,
            chat: Vec::new(),
            met: 0,
            pranks: 0,
            lands_seen: [false; 3],
            said_day: 0,
            warned_night: 0,
            calm: false,
        }
    }
}

impl Haunt {
    pub fn say(&mut self, line: impl Into<String>) {
        self.chat.push((line.into(), 0.0));
        if self.chat.len() > 5 {
            self.chat.remove(0);
        }
    }
}

/// What the game says when it comes back from a freeze.
const ERRORS: [&str; 8] = [
    "ERROR 422: duck not found",
    "duck_craft has stopped responding",
    "chunk failed to save: something is in it",
    "missing texture: you",
    "the island is not empty",
    "ERROR 422: there are two of you",
    "could not unload: it is still watching",
    "world.dat: who is the second player",
];

/// What the Other says in the chat (once a day at most, once you've met).
const WORDS: [&str; 9] = [
    "<???> i see you",
    "<???> this is my island",
    "<???> why did you leave",
    "<???> go back",
    "<???> dont sleep",
    "<???> you left the door open",
    "<???> it is not your bed",
    "<???> nice house",
    "<???> stop",
];

impl DuckCraft {
    pub fn update_haunt(&mut self, ctx: &mut Context) {
        if self.haunt.calm {
            return;
        }
        let dt = ctx.dt();
        self.haunt.clock += dt;
        for line in &mut self.haunt.chat {
            line.1 += dt;
        }
        self.haunt.chat.retain(|l| l.1 < CHAT_SECONDS);
        self.age_flash(dt);
        self.haunt.moon_jump = (self.haunt.moon_jump - dt).max(0.0);
        self.arrive_on_lands();
        self.update_other(ctx);
        self.maybe_prank(ctx);
        self.update_nothings(ctx);
        self.update_moon(ctx);
        self.maybe_glitch(ctx);
        self.maybe_speak(ctx);
        ctx.report("other", self.haunt.other.as_ref().map_or("none", |o| match o.visit {
            Visit::Watching => "watching",
            Visit::Behind => "behind",
            Visit::Bed => "bed",
        }));
        ctx.report("nothings", self.haunt.nothings.len());
        ctx.report("moon_stare", (self.haunt.moon_stare * 10.0).round() / 10.0);
        ctx.report("pranks", self.haunt.pranks);
    }

    fn age_flash(&mut self, dt: f32) {
        if let Some((_, left)) = &mut self.haunt.flash {
            *left -= dt;
            if *left <= 0.0 {
                self.haunt.flash = None;
            }
        }
    }

    /// The game frozen: nothing moves, nothing answers. Except the Nothing,
    /// which can't be seen while the picture's stuck.
    pub fn update_frozen(&mut self, ctx: &mut Context) {
        let dt = ctx.dt();
        self.haunt.freeze = (self.haunt.freeze - dt).max(0.0);
        self.move_nothings(ctx, dt);
        if self.haunt.freeze == 0.0 {
            ctx.play_wav(&self.art.sounds.glitch);
            let error = ERRORS[ctx.rng.range(0.0, ERRORS.len() as f32) as usize % ERRORS.len()];
            self.haunt.flash = Some((error.to_string(), 0.7));
            self.scare = self.scare.max(0.3);
        }
        ctx.report("frozen", self.haunt.freeze > 0.0);
    }

    // ---- Where things can be, and whether you can see them ----

    /// Whether you can see `at`: in front of you (a bit past the screen's
    /// edge counts) with nothing solid between.
    pub fn sees(&self, at: Vec3) -> bool {
        let eye = self.eye();
        let to = at - eye;
        if to.length() < 0.8 {
            return true;
        }
        // (The line stops short of `at`: a block you look at is in view,
        // though the line ends inside it.)
        let short = at - to.normalized() * 0.7;
        to.normalized().dot(self.look_direction()) > 0.6 && hunt::clear_line(&self.world, eye, short)
    }

    /// Where something could stand in column (x, z): on its highest solid
    /// block, if that's ground (not a tree top, not under water).
    pub fn standing_spot(&self, x: i32, z: i32) -> Option<Vec3> {
        if !World::inside(x, 0, z) {
            return None;
        }
        let mut top = TALL as i32 - 2;
        while top > 0 && !self.world.get(x, top, z).solid() {
            top -= 1;
        }
        let ground = self.world.get(x, top, z);
        let wet = self.world.get(x, top + 1, z) == Block::Water;
        if top <= 0 || wet || matches!(ground, Block::Leaves | Block::Log | Block::DeadLog | Block::Glass) {
            return None;
        }
        Some(vec3(x as f32 + 0.5, top as f32 + 1.0, z as f32 + 0.5))
    }

    /// How far you can see from here (the fog's far edge).
    pub fn sight_distance(&self) -> f32 {
        let daylight = self.sky().daylight;
        let (land, haze) = lands::haze(self.position.x, self.position.z, self.world.seed);
        let sight = self.weather.sight() * lerp(1.0, land.sight(), haze);
        ((30.0 + 90.0 * daylight) * sight).max(12.0)
    }

    /// The first time you set foot on each land across the sea.
    fn arrive_on_lands(&mut self) {
        let (land, haze) = lands::haze(self.position.x, self.position.z, self.world.seed);
        let Some(i) = Land::AWAY.iter().position(|&l| l == land) else {
            return;
        };
        if haze < 0.95 || self.haunt.lands_seen[i] {
            return;
        }
        self.haunt.lands_seen[i] = true;
        let line = match land {
            Land::Village => "The doors are open. The beds are made. No one is here.",
            Land::Forest => "The fog here never lifts.",
            _ => "This place didn't load right.",
        };
        let name = land.name();
        let title = name[..1].to_uppercase() + &name[1..];
        self.announce(title, line);
    }

    // ---- The Other ----

    fn update_other(&mut self, ctx: &mut Context) {
        let dt = ctx.dt();
        let day = self.day_number();
        let daylight = self.sky().daylight;
        let (land, haze) = lands::haze(self.position.x, self.position.z, self.world.seed);
        let away = land != Land::Island && haze > 0.5;
        if self.haunt.other.is_none() && self.haunt.clock >= self.haunt.other_at {
            let behind = day >= BEHIND_DAY && daylight < 0.3 && ctx.rng.range(0.0, 1.0) < 0.3;
            let visit = if behind { Visit::Behind } else { Visit::Watching };
            let came = self.screen.is_none() && self.place_other(ctx, visit);
            // Sooner on the other lands, and sooner as the days go on.
            let wait = if away { ctx.rng.range(50.0, 100.0) } else { ctx.rng.range(110.0, 220.0) };
            let wait = if came { wait / (1.0 + 0.15 * day.min(6) as f32) } else { 8.0 };
            self.haunt.other_at = self.haunt.clock + wait;
        }
        let eye = self.eye();
        let look = self.look_direction();
        let moving = vec2(self.velocity.x, self.velocity.z).length() > 0.5;
        let Some(other) = &mut self.haunt.other else {
            return;
        };
        other.age += dt;
        let head = other.position + vec3(0.0, 1.55, 0.0);
        let to = head - eye;
        let distance = vec2(to.x, to.z).length();
        let facing = to.normalized().dot(look);
        let clear = hunt::clear_line(&self.world, eye, head);
        // Always facing you.
        other.yaw = (-to.x).atan2(-to.z);
        let mut scared = false;
        let gone = match other.visit {
            Visit::Watching => {
                if facing > 0.985 && clear {
                    other.seen += dt;
                }
                other.seen > 2.5 || distance < other.near || other.age > 50.0
            }
            Visit::Behind => {
                if facing > 0.6 && clear {
                    scared = true;
                    true
                } else {
                    if other.age > 3.5 && !other.told {
                        other.told = true;
                        self.haunt.chat.push((String::from("<???> turn around"), 0.0));
                    }
                    other.age > 10.0
                }
            }
            Visit::Bed => {
                if facing > 0.9 {
                    other.seen += dt;
                }
                other.seen > 0.9 || moving || other.age > 4.0
            }
        };
        if !gone {
            return;
        }
        let visit = other.visit;
        self.haunt.other = None;
        self.haunt.met += 1;
        match visit {
            Visit::Watching => self.haunt.say("Someone left the island."),
            Visit::Behind if scared => {
                ctx.play_wav(&self.art.sounds.scare);
                self.scare = 1.0;
                self.haunt.flash = Some((String::from("TURN AROUND"), 0.12));
            }
            Visit::Bed => {
                ctx.play_wav(&self.art.sounds.glitch);
                self.haunt.flash = Some((String::from("it is not your bed"), 0.25));
            }
            Visit::Behind => {}
        }
    }

    /// Puts the Other somewhere for this visit. Whether it found a place.
    pub fn place_other(&mut self, ctx: &mut Context, visit: Visit) -> bool {
        let eye = self.eye();
        let feet = self.position;
        for _ in 0..16 {
            let (angle, distance) = match visit {
                Visit::Watching => {
                    let side = if ctx.rng.range(0.0, 1.0) < 0.5 { -1.0 } else { 1.0 };
                    let far = (self.sight_distance() * 0.85).clamp(14.0, 64.0);
                    (self.yaw + side * ctx.rng.range(0.25, 0.7), ctx.rng.range(far * 0.85, far))
                }
                Visit::Behind => (self.yaw + PI + ctx.rng.range(-0.3, 0.3), 2.3),
                Visit::Bed => (self.yaw + ctx.rng.range(-0.2, 0.2), 2.4),
            };
            let x = (feet.x + angle.sin() * distance).floor() as i32;
            let z = (feet.z + angle.cos() * distance).floor() as i32;
            let Some(spot) = self.standing_spot(x, z) else {
                continue;
            };
            let level = (spot.y - feet.y).abs() < 1.6;
            let seen_from_here = hunt::clear_line(&self.world, eye, spot + vec3(0.0, 1.5, 0.0));
            let fits = !self.world.get(x, spot.y as i32 + 1, z).solid();
            let fine = match visit {
                Visit::Watching => seen_from_here,
                _ => level && seen_from_here,
            };
            if !fine || !fits {
                continue;
            }
            let yaw = (feet.x - spot.x).atan2(feet.z - spot.z);
            self.haunt.other = Some(Other {
                position: spot,
                yaw,
                visit,
                seen: 0.0,
                age: 0.0,
                near: (distance * 0.6).max(6.0),
                told: false,
            });
            match visit {
                Visit::Watching => {
                    ctx.play_wav(&self.art.sounds.joined);
                    self.haunt.say("Someone joined the island.");
                }
                Visit::Behind => {
                    let pan = night::pan_toward(feet, self.yaw, spot);
                    ctx.play_wav_panned(&self.art.sounds.breath, pan);
                }
                Visit::Bed => {}
            }
            return true;
        }
        false
    }

    /// On waking (from the second day): sometimes it's there, at the foot
    /// of the bed.
    pub fn at_the_foot_of_the_bed(&mut self, ctx: &mut Context) {
        if self.day_number() >= PRANK_DAY && ctx.rng.range(0.0, 1.0) < 0.6 {
            self.place_other(ctx, Visit::Bed);
        }
    }

    // ---- Its tricks ----

    fn maybe_prank(&mut self, ctx: &mut Context) {
        if self.haunt.clock < self.haunt.prank_at {
            return;
        }
        if self.day_number() < PRANK_DAY {
            self.haunt.prank_at = self.haunt.clock + 30.0;
            return;
        }
        let start = ctx.rng.range(0.0, 6.0) as usize;
        let mut done = false;
        for k in 0..6 {
            done = match (start + k) % 6 {
                0 => self.open_a_door(),
                1 => self.snuff_a_torch(),
                2 => self.sand_pyramid(&mut ctx.rng),
                3 => self.tunnel(&mut ctx.rng),
                4 => self.bare_tree(&mut ctx.rng),
                _ => self.strip_a_tree(),
            };
            if done {
                break;
            }
        }
        if done {
            self.haunt.pranks += 1;
        }
        self.haunt.prank_at = self.haunt.clock + if done { ctx.rng.range(70.0, 140.0) } else { 20.0 };
    }

    /// Unseen, 8 to `far` blocks off.
    fn out_of_sight(&self, at: Vec3, far: f32) -> bool {
        let d = at.distance(self.position);
        d > 8.0 && d < far && !self.sees(at)
    }

    /// Opens one of your shut doors that you can't see.
    pub fn open_a_door(&mut self) -> bool {
        let [px, py, pz] = [self.position.x, self.position.y, self.position.z].map(|v| v.floor() as i32);
        for y in (py - 10).max(1)..(py + 10).min(TALL as i32) {
            for z in pz - 40..=pz + 40 {
                for x in px - 40..=px + 40 {
                    if self.world.get(x, y, z) != Block::DoorLow {
                        continue;
                    }
                    let middle = vec3(x as f32 + 0.5, y as f32 + 1.0, z as f32 + 0.5);
                    if self.out_of_sight(middle, 48.0) && self.swing_door(x, y, z) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Puts out one of your torches that you can't see.
    pub fn snuff_a_torch(&mut self) -> bool {
        let torch = self
            .world
            .plants
            .iter()
            .filter(|&(_, &p)| p == Plant::Torch)
            .map(|(&cell, _)| cell)
            .find(|&[x, y, z]| self.out_of_sight(vec3(x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5), 56.0));
        match torch {
            Some([x, y, z]) => {
                self.world.remove_plant(x, y, z);
                true
            }
            None => false,
        }
    }

    /// Somewhere unseen 14 to 30 blocks away, on open ground: its middle.
    fn hidden_ground(&self, rng: &mut Rng) -> Option<(i32, i32, i32)> {
        for _ in 0..24 {
            let angle = self.yaw + PI + rng.range(-1.6, 1.6);
            let distance = rng.range(14.0, 30.0);
            let x = (self.position.x + angle.sin() * distance).floor() as i32;
            let z = (self.position.z + angle.cos() * distance).floor() as i32;
            let spot = self.standing_spot(x, z)?;
            let y = spot.y as i32;
            let ground = self.world.get(x, y - 1, z);
            if !matches!(ground, Block::Grass | Block::Sand | Block::DeadGrass | Block::Dirt) {
                continue;
            }
            if self.out_of_sight(spot + vec3(0.0, 1.0, 0.0), 40.0) {
                return Some((x, y, z));
            }
        }
        None
    }

    /// A pyramid of sand, five across, on level ground.
    pub fn sand_pyramid(&mut self, rng: &mut Rng) -> bool {
        let Some((x, y, z)) = self.hidden_ground(rng) else {
            return false;
        };
        let level = (-2..=2).all(|dz| {
            (-2..=2).all(|dx| {
                self.world.get(x + dx, y - 1, z + dz).solid()
                    && (0..3).all(|dy| self.world.get(x + dx, y + dy, z + dz) == Block::Air)
            })
        });
        if !level {
            return false;
        }
        for (layer, r) in [2, 1, 0].into_iter().enumerate() {
            for dz in -r..=r {
                for dx in -r..=r {
                    self.world.set(x + dx, y + layer as i32, z + dz, Block::Sand);
                }
            }
        }
        true
    }

    /// A tunnel two by two, straight into a hill.
    pub fn tunnel(&mut self, rng: &mut Rng) -> bool {
        let Some((x, y, z)) = self.hidden_ground(rng) else {
            return false;
        };
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let rises = (2..5).any(|k| self.world.get(x + dx * k, y + 2, z + dz * k).solid());
            if !rises {
                continue;
            }
            let (sx, sz) = (dz.abs(), dx.abs()); // sideways
            for k in 0..14 {
                for (w, h) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (cx, cy, cz) = (x + dx * k + sx * w, y + h, z + dz * k + sz * w);
                    let block = self.world.get(cx, cy, cz);
                    let keep = matches!(block, Block::Bedrock | Block::Water | Block::Chest | Block::Bed)
                        || block.is_door()
                        || DIRECTIONS.iter().any(|&(ax, ay, az)| self.world.get(cx + ax, cy + ay, cz + az) == Block::Water);
                    if !keep && World::inside(cx, cy, cz) {
                        self.world.set(cx, cy, cz, Block::Air);
                    }
                }
            }
            return true;
        }
        false
    }

    /// A tree trunk with no leaves on it.
    pub fn bare_tree(&mut self, rng: &mut Rng) -> bool {
        let Some((x, y, z)) = self.hidden_ground(rng) else {
            return false;
        };
        if self.world.get(x, y - 1, z) != Block::Grass {
            return false;
        }
        let tall = 4 + rng.range(0.0, 3.0) as i32;
        if (0..tall + 1).any(|dy| self.world.get(x, y + dy, z) != Block::Air) {
            return false;
        }
        for dy in 0..tall {
            self.world.set(x, y + dy, z, Block::Log);
        }
        true
    }

    /// Every leaf off one tree you can't see.
    pub fn strip_a_tree(&mut self) -> bool {
        let [px, py, pz] = [self.position.x, self.position.y, self.position.z].map(|v| v.floor() as i32);
        for z in pz - 32..=pz + 32 {
            for x in px - 32..=px + 32 {
                for y in (py - 6).max(1)..(py + 14).min(TALL as i32 - 1) {
                    // A trunk's top, under leaves.
                    if self.world.get(x, y, z) != Block::Log || self.world.get(x, y + 1, z) != Block::Leaves {
                        continue;
                    }
                    let crown = vec3(x as f32 + 0.5, y as f32 + 1.0, z as f32 + 0.5);
                    if !self.out_of_sight(crown, 40.0) {
                        continue;
                    }
                    for cy in y - 2..=y + 2 {
                        for cz in z - 2..=z + 2 {
                            for cx in x - 2..=x + 2 {
                                if self.world.get(cx, cy, cz) == Block::Leaves {
                                    self.world.set(cx, cy, cz, Block::Air);
                                }
                            }
                        }
                    }
                    return true;
                }
            }
        }
        false
    }

    // ---- The Nothing ----

    fn update_nothings(&mut self, ctx: &mut Context) {
        let dt = ctx.dt();
        let night = self.day_number();
        let daylight = self.sky().daylight;
        let (land, haze) = lands::haze(self.position.x, self.position.z, self.world.seed);
        let broken = land == Land::Broken && haze > 0.6;
        let want = if broken {
            2
        } else if daylight < 0.1 && night >= NOTHING_NIGHT {
            (night - NOTHING_NIGHT + 1).min(4) as usize
        } else {
            0
        };
        // Gone when they're not wanted (only while you're not looking),
        // or when you're far.
        let duck = self.position;
        let unwanted = self.haunt.nothings.len() > want;
        let seen: Vec<bool> = self.haunt.nothings.iter().map(|n| self.sees(n.position + vec3(0.0, 1.0, 0.0))).collect();
        let mut i = 0;
        self.haunt.nothings.retain(|n| {
            let keep = !(unwanted && !seen[i]) && n.position.distance(duck) < 64.0;
            i += 1;
            keep
        });
        if self.haunt.nothings.len() < want && self.haunt.clock >= self.haunt.nothing_at {
            self.haunt.nothing_at = self.haunt.clock + 6.0;
            for _ in 0..10 {
                let angle = self.yaw + PI + ctx.rng.range(-1.0, 1.0);
                let distance = ctx.rng.range(18.0, 26.0);
                let x = (duck.x + angle.sin() * distance).floor() as i32;
                let z = (duck.z + angle.cos() * distance).floor() as i32;
                if let Some(spot) = self.standing_spot(x, z) {
                    if !self.sees(spot + vec3(0.0, 1.0, 0.0)) {
                        self.haunt.nothings.push(Nothing { position: spot, yaw: 0.0, seen: false });
                        break;
                    }
                }
            }
        }
        self.move_nothings(ctx, dt);
    }

    /// Each Nothing you can't see comes closer. One that reaches you hurts.
    fn move_nothings(&mut self, ctx: &mut Context, dt: f32) {
        let night = self.day_number();
        let speed = (3.6 + 0.5 * night.saturating_sub(NOTHING_NIGHT) as f32).min(6.0);
        let duck = self.position;
        let frozen = self.haunt.freeze > 0.0;
        let mut touched = None;
        for i in 0..self.haunt.nothings.len() {
            let at = self.haunt.nothings[i].position;
            let seen = !frozen && self.sleeping <= 0.0 && self.sees(at + vec3(0.0, 1.0, 0.0));
            let nothing = &mut self.haunt.nothings[i];
            nothing.seen = seen;
            let to = vec2(duck.x - at.x, duck.z - at.z);
            if !seen {
                nothing.yaw = to.x.atan2(to.y);
                if let Some(next) = nothing_step(&self.world, at, to, speed * dt) {
                    nothing.position = next;
                }
            }
            let close = vec2(duck.x - nothing.position.x, duck.z - nothing.position.z).length();
            if close < 0.8 && (duck.y - nothing.position.y).abs() < 1.8 {
                touched = Some(i);
            }
        }
        if let Some(i) = touched {
            self.haunt.nothings.remove(i);
            ctx.play_wav(&self.art.sounds.scare);
            self.scare = 1.0;
            self.hurt(ctx, NOTHING_HURTS, "the Nothing");
        }
    }

    // ---- The moon ----

    fn update_moon(&mut self, ctx: &mut Context) {
        let dt = ctx.dt();
        let night = self.night_of();
        let sky = self.sky();
        let moon = sky.sun * -1.0;
        let up = night >= MOON_NIGHT && sky.daylight < 0.15 && moon.y > 0.05 && self.weather.grey() < 0.5;
        if up && self.haunt.warned_night != night {
            self.haunt.warned_night = night;
            self.haunt.say("<???> dont look at the moon");
        }
        let watching = up && self.haunt.moon_night != night;
        let looking = watching && self.look_direction().dot(moon) > 0.97;
        self.haunt.moon_stare = if looking {
            self.haunt.moon_stare + dt
        } else {
            (self.haunt.moon_stare - dt * 0.7).max(0.0)
        };
        if self.haunt.moon_stare >= STARE_SECONDS {
            self.haunt.moon_stare = 0.0;
            self.haunt.moon_night = night;
            self.haunt.moon_jump = 0.8;
            ctx.play_wav(&self.art.sounds.scare);
            self.scare = 1.0;
            self.hurt(ctx, MOON_HURTS, "the moon");
        }
    }

    /// Which night it is (a night after midnight is still the night it
    /// started on).
    pub fn night_of(&self) -> u32 {
        let day = self.day_number();
        if self.time_of_day() < 0.25 { day.saturating_sub(1).max(1) } else { day }
    }

    /// Whether the moon has its face tonight (and has it still: after it
    /// comes for you, its eyes are shut till the next night).
    pub fn moon_face(&self) -> Option<bool> {
        let night = self.night_of();
        (night >= MOON_NIGHT).then_some(self.haunt.moon_night != night)
    }

    // ---- The game going wrong ----

    fn maybe_glitch(&mut self, ctx: &mut Context) {
        if self.haunt.clock < self.haunt.glitch_at {
            return;
        }
        let (land, haze) = lands::haze(self.position.x, self.position.z, self.world.seed);
        let broken = land == Land::Broken && haze > 0.6;
        if self.day_number() < PRANK_DAY && !broken {
            self.haunt.glitch_at = self.haunt.clock + 60.0;
            return;
        }
        if self.screen.is_some() || self.vitals.fainted {
            self.haunt.glitch_at = self.haunt.clock + 10.0;
            return;
        }
        if ctx.rng.range(0.0, 1.0) < 0.5 {
            self.haunt.freeze = ctx.rng.range(1.5, 3.0);
        } else {
            ctx.play_wav(&self.art.sounds.glitch);
            let error = ERRORS[ctx.rng.range(0.0, ERRORS.len() as f32) as usize % ERRORS.len()];
            self.haunt.flash = Some((error.to_string(), 0.25));
        }
        self.haunt.glitch_at = self.haunt.clock + if broken { ctx.rng.range(45.0, 100.0) } else { ctx.rng.range(200.0, 380.0) };
    }

    /// Once a day, once you've met it, it says something.
    fn maybe_speak(&mut self, ctx: &mut Context) {
        let day = self.day_number();
        if self.haunt.met < 2 || self.haunt.said_day == day || ctx.rng.range(0.0, 1.0) > dt_chance(ctx.dt(), 240.0) {
            return;
        }
        self.haunt.said_day = day;
        let line = WORDS[ctx.rng.range(0.0, WORDS.len() as f32) as usize % WORDS.len()];
        ctx.play_wav(&self.art.sounds.joined);
        self.haunt.say(line);
    }

    // ---- Drawing ----

    /// The Other and the Nothing, in the world.
    pub fn draw_haunt(&self, canvas: &mut Canvas, camera: &Camera3D) {
        if let Some(other) = &self.haunt.other {
            self.draw_other(canvas, camera, other.position, other.yaw);
        }
        if matches!(self.stage, Stage::Title { .. }) && self.haunt.met > 0 {
            // It's on the title's island now, off to one side (clear of the
            // buttons) as the island first turns into view, facing you.
            let first = self.title_camera(0.0);
            let target = vec2(WIDTH * 0.8, HEIGHT * 0.66);
            let s = self.world.spawn;
            let mut best: Option<(f32, Vec3)> = None;
            for dz in -14..=14 {
                for dx in -14..=14 {
                    let Some(spot) = self.standing_spot(s.x as i32 + dx, s.z as i32 + dz) else {
                        continue;
                    };
                    let head = spot + vec3(0.0, 1.6, 0.0);
                    let Some(p) = first.world_to_screen(head, canvas) else {
                        continue;
                    };
                    let miss = p.distance(target);
                    if best.is_none_or(|(b, _)| miss < b) && hunt::clear_line(&self.world, first.position, head) {
                        best = Some((miss, spot));
                    }
                }
            }
            let Some((_, at)) = best else {
                return;
            };
            let yaw = (first.position.x - at.x).atan2(first.position.z - at.z);
            self.draw_other(canvas, camera, at, yaw);
        }
        for nothing in &self.haunt.nothings {
            let pose = Pose { yaw: nothing.yaw, pitch: 0.0, swing: 0.0, lift: 0.0 };
            for (mesh, place) in self.art.duck.parts_sized(nothing.position, pose, 1.08) {
                canvas.draw_mesh_tinted(mesh, &place, camera, Color::rgb(0, 0, 0));
            }
            // Slits for eyes: thin, white, over everything.
            if nothing.position.distance(camera.position) < 32.0 {
                self.draw_eyes(canvas, camera, nothing.position, (nothing.yaw, 0.0), 1.08, (2.6, 0.5));
            }
            if let (Stage::Playing, Some(rect)) =
                (&self.stage, screen_box(camera, canvas, nothing.position, 0.7, 1.9))
            {
                canvas.label("nothing", rect);
            }
        }
    }

    /// A duck in your own skin, with white eyes.
    fn draw_other(&self, canvas: &mut Canvas, camera: &Camera3D, at: Vec3, yaw: f32) {
        let pose = Pose { yaw, pitch: 0.12, swing: 0.0, lift: 0.0 };
        for (mesh, place) in self.art.duck.parts(at, pose) {
            canvas.draw_mesh(mesh, &place, camera);
        }
        self.draw_eyes(canvas, camera, at, (yaw, 0.12), 1.0, (2.2, 2.2));
        if let (Stage::Playing, Some(rect)) = (&self.stage, screen_box(camera, canvas, at, 0.7, 1.8)) {
            canvas.label("other", rect);
        }
    }

    /// Two eyes on the head of a duck model at `feet` (where the skin has
    /// its eyes), drawn over everything: the fog and the dark don't hide
    /// them. `shape` is their width and height in skin pixels.
    fn draw_eyes(&self, canvas: &mut Canvas, camera: &Camera3D, feet: Vec3, pose: (f32, f32), size: f32, shape: (f32, f32)) {
        let (yaw, pitch) = pose;
        let head = Transform::at(feet + (vec3(0.0, Duck::NECK, 0.0) * size).rotate_y(yaw))
            .rotated(vec3(pitch, yaw, 0.0))
            .sized(size);
        for side in [-2.0, 2.0] {
            let middle = head.apply(vec3(side, 5.0, 4.05) * SKIN_PIXEL);
            let corner = head.apply(vec3(side + shape.0 / 2.0, 5.0 + shape.1 / 2.0, 4.05) * SKIN_PIXEL);
            let (Some(p), Some(q)) = (camera.world_to_screen(middle, canvas), camera.world_to_screen(corner, canvas)) else {
                continue;
            };
            let (w, h) = (((q.x - p.x).abs() * 2.0).max(1.0), ((q.y - p.y).abs() * 2.0).max(1.0));
            canvas.fill_rect(
                Rect::new((p.x - w / 2.0).round(), (p.y - h / 2.0).round(), w.round().max(1.0), h.round().max(1.0)),
                Color::rgb(244, 244, 255),
            );
        }
    }

    /// The moon, with its face (from the fourth night). Bigger the longer
    /// you look.
    pub fn draw_moon(&self, canvas: &mut Canvas, camera: &Camera3D, sky: Color, moon: Vec3) {
        let Some(p) = camera.world_to_screen(camera.position + moon * 100.0, canvas) else {
            return;
        };
        let pale = Color::from_hex(0xc8ccd4);
        let Some(awake) = self.moon_face() else {
            // A pale moon, with a dark bite out of it.
            canvas.fill_rect(Rect::from_center(p, 9.0, 9.0), pale);
            canvas.fill_rect(Rect::new(p.x - 4.5, p.y - 4.5, 4.0, 9.0), sky.lerp(pale, 0.15));
            return;
        };
        let stare = (self.haunt.moon_stare / STARE_SECONDS).clamp(0.0, 1.0);
        let size = (12.0 + 70.0 * stare * stare).round();
        self.draw_moon_face(canvas, p, size, awake, stare);
        canvas.label("moon", Rect::from_center(p, size, size));
    }

    fn draw_moon_face(&self, canvas: &mut Canvas, p: Vec2, size: f32, awake: bool, stare: f32) {
        let pale = Color::from_hex(0xd6d2c4);
        let dark = Color::from_hex(0x16100e);
        let u = size / 12.0;
        let at = |x: f32, y: f32, w: f32, h: f32| Rect::new(p.x + (x - 6.0) * u, p.y + (y - 6.0) * u, w * u, h * u);
        canvas.fill_rect(Rect::from_center(p, size, size), pale);
        canvas.fill_rect(at(0.0, 0.0, 12.0, 1.0), pale.lerp(dark, 0.15));
        if awake {
            // Eyes that look at you, red as it comes.
            let pupil = Color::from_hex(0x16100e).lerp(Color::from_hex(0xb01010), stare);
            canvas.fill_rect(at(2.0, 3.0, 3.0, 3.0), dark);
            canvas.fill_rect(at(7.0, 3.0, 3.0, 3.0), dark);
            canvas.fill_rect(at(3.0, 4.0, 1.0, 1.0), pupil);
            canvas.fill_rect(at(8.0, 4.0, 1.0, 1.0), pupil);
            // A smile that opens.
            let open = 1.0 + 2.0 * stare;
            canvas.fill_rect(at(2.0, 8.0, 8.0, open), dark);
            canvas.fill_rect(at(1.0, 7.0, 1.0, 1.0), dark);
            canvas.fill_rect(at(10.0, 7.0, 1.0, 1.0), dark);
            if stare > 0.4 {
                for i in 0..4 {
                    canvas.fill_rect(at(2.5 + i as f32 * 2.0, 8.0, 1.0, 1.0), pale);
                }
            }
        } else {
            // Shut, for tonight.
            canvas.fill_rect(at(2.0, 4.5, 3.0, 0.6), dark);
            canvas.fill_rect(at(7.0, 4.5, 3.0, 0.6), dark);
            canvas.fill_rect(at(3.0, 8.5, 6.0, 0.6), dark);
        }
    }

    /// Over the screen: the chat, the freeze, the words, the moon up close.
    pub fn draw_haunt_screen(&self, canvas: &mut Canvas) {
        // The chat, bottom left, like another player's.
        let mut y = HEIGHT - 76.0 - 10.0 * self.haunt.chat.len() as f32;
        for (line, age) in &self.haunt.chat {
            let alpha = ((CHAT_SECONDS - age).min(1.0) * 255.0) as u8;
            let color = if line.starts_with('<') {
                Color::rgb(236, 236, 236)
            } else {
                Color::rgb(240, 220, 90)
            };
            let w = Canvas::text_width(line, 1);
            canvas.fill_rect(Rect::new(2.0, y - 1.0, w + 4.0, 10.0), Color::rgba(0, 0, 0, alpha / 2));
            canvas.draw_text(line, vec2(4.0, y), 1, color.with_alpha(alpha));
            y += 10.0;
        }
        if !self.haunt.chat.is_empty() {
            canvas.label("chat", Rect::new(2.0, HEIGHT - 86.0, 150.0, 10.0));
        }
        if self.haunt.moon_jump > 0.0 {
            canvas.fill_rect(Rect::new(0.0, 0.0, WIDTH, HEIGHT), Color::rgb(8, 4, 4));
            self.draw_moon_face(canvas, vec2(WIDTH / 2.0, HEIGHT / 2.0), HEIGHT * 1.3, true, 1.0);
        }
        if self.haunt.freeze > 0.0 {
            // Washed out, under a title bar that says so.
            canvas.fill_rect(Rect::new(0.0, 0.0, WIDTH, HEIGHT), Color::rgba(255, 255, 255, 70));
            canvas.fill_rect(Rect::new(0.0, 0.0, WIDTH, 11.0), Color::rgb(230, 230, 230));
            canvas.draw_text("Duck Craft (Not Responding)", vec2(3.0, 2.0), 1, Color::rgb(20, 20, 20));
            canvas.label("not responding", Rect::new(0.0, 0.0, WIDTH, 11.0));
        }
        if let Some((words, left)) = &self.haunt.flash {
            // Black, torn with colour, and the words.
            let long = *left > 0.3;
            if long {
                canvas.fill_rect(Rect::new(0.0, 0.0, WIDTH, HEIGHT), Color::rgb(0, 0, 0));
            }
            let seed = (left * 60.0) as i32;
            for i in 0..6 {
                let y = random01(i, seed, 0, 5) * HEIGHT;
                let colors = [Color::rgb(255, 30, 230), Color::rgb(30, 240, 255), Color::rgb(60, 255, 74)];
                let c = colors[i as usize % 3];
                canvas.fill_rect(Rect::new(0.0, y.floor(), WIDTH, 1.0 + (i % 3) as f32), c.with_alpha(160));
            }
            let w = Canvas::text_width(words, 1);
            let at = vec2((WIDTH - w) / 2.0, HEIGHT / 2.0 - 4.0);
            canvas.fill_rect(Rect::new(at.x - 4.0, at.y - 3.0, w + 8.0, 13.0), Color::rgb(0, 0, 0));
            canvas.draw_text(words, at, 1, Color::rgb(255, 255, 255));
            canvas.label("error", Rect::new(at.x, at.y, w, 8.0));
        }
    }
}

/// The chance something with a mean wait of `mean` seconds happens in a
/// frame of `dt` seconds.
fn dt_chance(dt: f32, mean: f32) -> f32 {
    dt / mean
}

/// One step of a Nothing toward `to` (a flat direction): straight on, or
/// round something, up a block or down a few. `None`: it can't.
fn nothing_step(world: &World, at: Vec3, to: Vec2, length: f32) -> Option<Vec3> {
    if to.length() < 0.05 {
        return None;
    }
    let dir = to.normalized();
    let length = length.min(to.length());
    for turn in [0.0, 0.6, -0.6, 1.2, -1.2] {
        let (s, c) = (f32::sin(turn), f32::cos(turn));
        let d = vec2(dir.x * c - dir.y * s, dir.x * s + dir.y * c);
        let next = vec2(at.x + d.x * length, at.z + d.y * length);
        let (cx, cz) = (next.x.floor() as i32, next.y.floor() as i32);
        let feet = at.y.floor() as i32;
        for dy in [0, 1, -1, -2, -3] {
            let y = feet + dy;
            let free = !world.get(cx, y, cz).solid() && !world.get(cx, y + 1, cz).solid();
            if free && world.get(cx, y - 1, cz).solid() && World::inside(cx, y, cz) {
                return Some(vec3(next.x, y as f32, next.y));
            }
        }
    }
    None
}
