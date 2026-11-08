//! Things to build and use: torches, doors, beds and chests.
//!
//! - a **torch** stands on anything solid and lights like a lamp; the
//!   Hollows keep back from it. It's drawn like a plant (`Plant::Torch`).
//! - a **door** is two blocks; right click opens it (an empty frame you
//!   walk through) and shuts it (solid: the Hollows can't pass, though on
//!   later nights they learn to break them, see `night.rs`).
//! - a **bed**: right click at night, with nothing near, to sleep until
//!   morning; you wake there after fainting, too.
//! - a **chest** holds 27 stacks; right click opens it beside your bag. It
//!   won't break while there's anything in it.

use super::*;
use inventory::Screen;

/// How many stacks a chest holds.
pub const CHEST_SLOTS: usize = 27;
/// How long the screen stays dark while you sleep.
pub const SLEEP_SECONDS: f32 = 2.5;
/// No Hollow may be this near, for you to sleep.
pub const SAFE_TO_SLEEP: f32 = 12.0;

impl DuckCraft {
    /// Right click on a door, a bed or a chest uses it. Returns whether it
    /// did (so nothing gets built on it).
    pub fn use_block(&mut self, hit: RayHit, ctx: &mut Context) -> bool {
        let [x, y, z] = hit.cell;
        match hit.block {
            door if door.is_door() => {
                if self.swing_door(x, y, z) {
                    ctx.play(Sound {
                        name: "creak",
                        wave: Wave::Triangle,
                        freq: 140.0,
                        slide_to: 90.0,
                        ms: 260.0,
                        volume: 0.12,
                        decay: 200.0,
                        ..Default::default()
                    });
                } else {
                    self.say("Something's in the way");
                }
                true
            }
            Block::Bed => {
                self.try_sleep(hit.cell, ctx);
                true
            }
            Block::Chest => {
                self.open_chest(hit.cell, ctx);
                true
            }
            _ => false,
        }
    }

    /// Opens a shut door or shuts an open one, both halves. It won't shut
    /// on the duck, a duckling or a Hollow. Returns whether it swung.
    pub fn swing_door(&mut self, x: i32, y: i32, z: i32) -> bool {
        let low = match self.world.get(x, y, z) {
            Block::DoorLow | Block::DoorLowOpen => y,
            _ => y - 1,
        };
        // Only a whole door swings (both halves there).
        if !(self.world.get(x, low, z).is_door() && self.world.get(x, low + 1, z).is_door()) {
            return false;
        }
        let shutting = !self.world.get(x, low, z).solid();
        if shutting {
            for cell_y in [low, low + 1] {
                if self.something_in(x, cell_y, z) {
                    return false;
                }
            }
        }
        for cell_y in [low, low + 1] {
            let half = self.world.get(x, cell_y, z);
            if half.is_door() {
                self.world.set(x, cell_y, z, half.door_swung());
            }
        }
        self.door_damage.remove(&[x, low, z]); // a fresh start for the Hollows
        self.update_chunks();
        true
    }

    /// Torches knocked down come back to the bag (in survival).
    pub fn pick_up_loose(&mut self) {
        let loose = std::mem::take(&mut self.world.loose_torches);
        if loose > 0 && self.mode == Mode::Survival {
            self.give(Item::Torch, loose);
        }
    }

    /// Whether the duck, a duckling or a Hollow is in the cell.
    pub fn something_in(&self, x: i32, y: i32, z: i32) -> bool {
        let cell = vec3(x as f32, y as f32, z as f32);
        boxes_overlap(self.position, PLAYER, cell)
            || self
                .ducklings
                .iter()
                .any(|d| boxes_overlap(d.position, ducklings::DUCKLING, cell))
            || self
                .hollows
                .iter()
                .any(|h| h.fading.is_none() && boxes_overlap(h.position, h.body(), cell))
    }

    /// Breaking a door's half takes the other half too, and gives back
    /// the whole door (not in creative).
    pub fn break_door(&mut self, x: i32, y: i32, z: i32) {
        let low = match self.world.get(x, y, z) {
            Block::DoorLow | Block::DoorLowOpen => y,
            _ => y - 1,
        };
        for cell_y in [low, low + 1] {
            if self.world.get(x, cell_y, z).is_door() {
                self.world.set(x, cell_y, z, Block::Air);
            }
        }
        if self.mode == Mode::Survival {
            self.give(Item::Door, 1);
        }
    }

    /// Building with a torch or a door (the things that aren't one block).
    /// Returns whether `item` was one of them (so the usual building
    /// doesn't happen).
    pub fn place_special(&mut self, item: Item, hit: RayHit, pressed: bool, ctx: &mut Context) -> bool {
        if !matches!(item, Item::Torch | Item::Door) {
            return false;
        }
        let [x, y, z] = if hit.plant.is_some() {
            hit.cell
        } else {
            [0, 1, 2].map(|i| hit.cell[i] + hit.normal[i])
        };
        let free = |game: &DuckCraft, y: i32| {
            World::inside(x, y, z)
                && game.world.get(x, y, z) == Block::Air
                && game.world.plant(x, y, z).is_none_or(|p| p != Plant::Torch)
        };
        let ground = self.world.get(x, y - 1, z).solid();
        let fits = match item {
            Item::Torch => free(self, y) && ground,
            _ => free(self, y) && free(self, y + 1) && ground && !self.something_in(x, y, z)
                && !self.something_in(x, y + 1, z),
        };
        if !fits {
            if pressed {
                self.say(match item {
                    Item::Torch => "A torch needs something under it",
                    _ => "A door needs two spaces and a floor",
                });
            }
            return true;
        }
        match item {
            Item::Torch => {
                self.world.remove_plant(x, y, z);
                self.world.put_torch(x, y, z);
            }
            _ => {
                self.world.remove_plant(x, y, z);
                self.world.remove_plant(x, y + 1, z);
                self.world.set(x, y, z, Block::DoorLow);
                self.world.set(x, y + 1, z, Block::DoorHigh);
            }
        }
        if self.mode == Mode::Survival {
            self.inventory.take_one_from(self.slot);
        }
        self.placed += 1;
        self.place_wait = PLACE_REPEAT;
        self.update_chunks();
        ctx.play(Sound {
            name: "place",
            wave: Wave::Square,
            freq: 170.0,
            slide_to: 110.0,
            ms: 70.0,
            volume: 0.2,
            decay: 50.0,
            ..Default::default()
        });
        true
    }

    // ---- Beds ----

    /// Night, with nothing near: sleep till morning, and wake here from now
    /// on. In daylight it only says you'll wake here.
    pub fn try_sleep(&mut self, bed: [i32; 3], ctx: &mut Context) {
        self.bed = Some(bed);
        if self.time_of_day() < menus::DUSK - 0.02 {
            self.say("You'll wake here now. It isn't dark yet");
            return;
        }
        let at = vec3(bed[0] as f32 + 0.5, bed[1] as f32, bed[2] as f32 + 0.5);
        let near = self
            .hollows
            .iter()
            .any(|h| h.fading.is_none() && h.position.distance(at) < SAFE_TO_SLEEP);
        if near {
            self.say("You can't sleep. Something is close");
            return;
        }
        self.sleeping = SLEEP_SECONDS;
        self.digging = None;
        self.eating_seconds = 0.0;
        ctx.play(Sound {
            name: "sleep",
            wave: Wave::Sine,
            freq: 220.0,
            slide_to: 110.0,
            ms: 900.0,
            volume: 0.15,
            decay: 800.0,
            ..Default::default()
        });
    }

    /// While asleep: the dark deepens; then it's morning, and whatever was
    /// out there has gone.
    pub fn update_sleep(&mut self, ctx: &mut Context) {
        let before = self.sleeping;
        self.sleeping = (self.sleeping - ctx.dt()).max(0.0);
        if before > 0.0 && self.sleeping == 0.0 {
            let to_morning = (1.0 - self.time_of_day() + menus::DAY_START).rem_euclid(1.0);
            self.time += to_morning * DAY_SECONDS;
            for hollow in &mut self.hollows {
                hollow.fading.get_or_insert(0.0);
            }
            self.watcher = None;
            self.say("You slept. Nothing remembers it");
        }
    }

    /// Where the duck wakes: by its bed, if it still has one, or at the
    /// island's start.
    pub fn wake_point(&self) -> Vec3 {
        match self.bed {
            Some([x, y, z]) if self.world.get(x, y, z) == Block::Bed => {
                vec3(x as f32 + 0.5, y as f32 + 1.0, z as f32 + 0.5)
            }
            _ => self.world.spawn,
        }
    }

    /// The dark while asleep.
    pub fn draw_sleep(&self, canvas: &mut Canvas) {
        if self.sleeping <= 0.0 {
            return;
        }
        let passed = SLEEP_SECONDS - self.sleeping;
        let dark = (passed / 0.8).min(1.0) * (self.sleeping / 0.5).min(1.0);
        canvas.fill_rect(
            Rect::new(0.0, 0.0, WIDTH, HEIGHT),
            Color::rgba(0, 0, 0, (dark * 255.0) as u8),
        );
        if dark > 0.6 {
            canvas.draw_text_centered("...", HEIGHT / 2.0 - 4.0, 1, inventory::DIM);
        }
    }

    // ---- Chests ----

    /// Opens the chest at `cell` beside the bag.
    pub fn open_chest(&mut self, cell: [i32; 3], ctx: &mut Context) {
        self.world
            .chests
            .entry(cell)
            .or_insert_with(|| vec![None; CHEST_SLOTS]);
        self.screen = Some(Screen {
            chest: Some(cell),
            // What waits in the crafting grid stays waiting (unseen).
            grid: std::mem::take(&mut self.kept_grid),
            ..Screen::default()
        });
        self.digging = None;
        ctx.lock_pointer(false);
        ctx.play(Sound {
            name: "chest",
            wave: Wave::Noise,
            freq: 300.0,
            slide_to: 120.0,
            ms: 160.0,
            volume: 0.15,
            decay: 140.0,
            ..Default::default()
        });
    }

    /// Whether the chest at `cell` holds anything.
    pub fn chest_has_things(&self, cell: [i32; 3]) -> bool {
        self.world
            .chests
            .get(&cell)
            .is_some_and(|slots| slots.iter().any(|s| s.is_some()))
    }
}
