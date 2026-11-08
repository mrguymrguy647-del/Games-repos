//! The inventory screen (**E**): your duck, a 2 x 2 crafting grid, the
//! backpack and the hotbar, drawn as Minecraft might have looked on a
//! PlayStation, and worked with the mouse or with the keys like a console.
//!
//! - **left click** (or **X**) a slot: pick up its stack, or put down (or
//!   swap) the one you hold; on the same item, as much as fits goes on
//! - **right click** (or **C**): pick up half, or put down one
//! - **Shift + click** (or **Shift + X**): send the stack across, between
//!   the hotbar and the backpack, or from the crafting grid into the bag
//! - **arrows** (or **WASD**) move a cursor from spot to spot
//! - put things in the grid that match a recipe and the result shows on
//!   the right; click it to make it (Shift: as many as you can)
//! - in creative the panel of everything is beside the bag, in three tabs
//!   (**Q** and **R** change tab)
//! - **E** or **Escape** closes it; what's in the grid and in your hand
//!   goes back in the bag
//!
//! The window lets go of the mouse pointer while it's open.

use super::*;
use menus::Mode;

/// Some of these, one in each grid slot (in any order), make that.
pub struct Recipe {
    pub needs: &'static [Item],
    pub makes: (Item, u32),
}

const fn block(b: Block) -> Item {
    Item::Block(b)
}

pub const RECIPES: [Recipe; 12] = [
    Recipe {
        needs: &[block(Block::Log)],
        makes: (block(Block::Planks), 4),
    },
    Recipe {
        needs: &[Item::Seeds, Item::Seeds, Item::Seeds],
        makes: (Item::Bread, 1),
    },
    Recipe {
        needs: &[block(Block::Sand), block(Block::Sand), Item::Coal],
        makes: (block(Block::Glass), 2),
    },
    Recipe {
        needs: &[block(Block::Dirt), block(Block::Dirt), Item::Coal],
        makes: (block(Block::Brick), 2),
    },
    Recipe {
        needs: &[
            block(Block::Cobblestone),
            block(Block::Cobblestone),
            Item::Coal,
        ],
        makes: (block(Block::Stone), 2),
    },
    Recipe {
        needs: &[Item::Gold, block(Block::Glass)],
        makes: (block(Block::Lamp), 2),
    },
    Recipe {
        needs: &[
            block(Block::Cobblestone),
            block(Block::Cobblestone),
            block(Block::Log),
        ],
        makes: (Item::Blade, 1),
    },
    Recipe {
        needs: &[
            block(Block::Leaves),
            block(Block::Leaves),
            block(Block::Leaves),
            block(Block::Leaves),
        ],
        makes: (block(Block::Dirt), 1),
    },
    // Light, and a way in and out, and somewhere to sleep and keep things.
    Recipe {
        needs: &[Item::Coal, block(Block::Planks)],
        makes: (Item::Torch, 4),
    },
    Recipe {
        needs: &[block(Block::Planks), block(Block::Planks), block(Block::Planks)],
        makes: (Item::Door, 1),
    },
    Recipe {
        needs: &[
            block(Block::Planks),
            block(Block::Planks),
            block(Block::Leaves),
            block(Block::Leaves),
        ],
        makes: (block(Block::Bed), 1),
    },
    Recipe {
        needs: &[
            block(Block::Planks),
            block(Block::Planks),
            block(Block::Planks),
            block(Block::Planks),
        ],
        makes: (block(Block::Chest), 1),
    },
];

/// The recipe the grid's contents make, if any: the same items, one a
/// slot, in any order.
pub fn matching_recipe(grid: &[Option<Stack>; 4]) -> Option<&'static Recipe> {
    let mut have: Vec<Item> = grid.iter().flatten().map(|s| s.item).collect();
    if have.is_empty() {
        return None;
    }
    have.sort_by_key(|item| item.id());
    RECIPES.iter().find(|recipe| {
        let mut needs = recipe.needs.to_vec();
        needs.sort_by_key(|item| item.id());
        needs == have
    })
}

/// The inventory screen while it's open.
#[derive(Debug, Clone, Default)]
pub struct Screen {
    /// The crafting grid: top left, top right, bottom left, bottom right.
    pub grid: [Option<Stack>; 4],
    /// The stack on the mouse pointer (or on the key cursor).
    pub held: Option<Stack>,
    /// The panel beside it is showing: creative's panel of everything.
    pub book_open: bool,
    /// Creative: which tab of the panel of everything is showing.
    pub tab: usize,
    /// The cursor the arrow keys move from spot to spot; `None` while the
    /// mouse is in charge.
    pub cursor: Option<Spot>,
    /// Seconds since it opened, for the opening wipe.
    pub opened: f32,
    /// Open at a chest (its cell): the chest's slots take the place of
    /// the duck and the crafting grid.
    pub chest: Option<[i32; 3]>,
}

/// A place on the screen that holds a stack, or does something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    /// One of the 36 inventory slots.
    Slot(usize),
    Grid(usize),
    Result,
    /// Creative: one of everything there is (its place in `Item::all()`),
    /// in the panel beside the bag.
    Palette(usize),
    /// Creative: one of that panel's tabs.
    Tab(usize),
    /// One of an open chest's slots.
    Chest(usize),
}

/// Creative's tabs; `tab_of` says which each thing is under.
pub const TABS: [&str; 3] = ["BUILD", "NATURE", "THINGS"];

pub fn tab_of(item: Item) -> usize {
    match item {
        Item::Block(
            Block::Stone
            | Block::Cobblestone
            | Block::Planks
            | Block::Brick
            | Block::Glass
            | Block::Lamp
            | Block::Bed
            | Block::Chest,
        ) => 0,
        Item::Block(_) => 1,
        _ => 2,
    }
}

/// Where `Item::all()[i]` sits among the things in its tab.
fn place_in_tab(i: usize) -> usize {
    let all = Item::all();
    let tab = tab_of(all[i]);
    all[..i].iter().filter(|&&item| tab_of(item) == tab).count()
}

// The layout, in canvas pixels (the screen is 320 x 240): the panel in the
// middle, creative's panel of everything to its left.
const PANEL_W: f32 = 178.0;
const PANEL_H: f32 = 174.0;
const PANEL_TOP: f32 = 22.0;
const BOOK_W: f32 = 112.0;
const GAP: f32 = 4.0;
const SLOT: f32 = 18.0;

pub fn panel(book_open: bool) -> Rect {
    let width = if book_open {
        PANEL_W + BOOK_W + GAP
    } else {
        PANEL_W
    };
    let left = ((WIDTH - width) / 2.0).floor() + if book_open { BOOK_W + GAP } else { 0.0 };
    Rect::new(left, PANEL_TOP, PANEL_W, PANEL_H)
}

fn book_panel(book_open: bool) -> Rect {
    let p = panel(book_open);
    Rect::new(p.x - BOOK_W - GAP, p.y, BOOK_W, PANEL_H)
}

/// Where each spot is (its 18 x 18 box, or the button's).
pub fn spot_rect(spot: Spot, book_open: bool) -> Rect {
    let p = panel(book_open);
    let at = |x: f32, y: f32| Rect::new(p.x + x, p.y + y, SLOT, SLOT);
    match spot {
        Spot::Slot(i) if i < HOTBAR_SLOTS => at(8.0 + i as f32 * SLOT, 150.0),
        Spot::Slot(i) => {
            let i = i - HOTBAR_SLOTS;
            at(8.0 + (i % 9) as f32 * SLOT, 90.0 + (i / 9) as f32 * SLOT)
        }
        Spot::Grid(i) => at(84.0 + (i % 2) as f32 * SLOT, 28.0 + (i / 2) as f32 * SLOT),
        Spot::Chest(i) => at(8.0 + (i % 9) as f32 * SLOT, 22.0 + (i / 9) as f32 * SLOT),
        Spot::Result => at(147.0, 37.0),
        Spot::Palette(i) => {
            let b = book_panel(book_open);
            let k = place_in_tab(i);
            Rect::new(
                b.x + 2.0 + (k % 6) as f32 * SLOT,
                b.y + 19.0 + (k / 6) as f32 * SLOT,
                SLOT,
                SLOT,
            )
        }
        Spot::Tab(t) => {
            let b = book_panel(book_open);
            let w = ((b.w - 6.0) / TABS.len() as f32).floor();
            Rect::new(b.x + 3.0 + t as f32 * w, b.y + 3.0, w - 1.0, 12.0)
        }
    }
}

/// The duck's window.
fn preview_rect(book_open: bool) -> Rect {
    let p = panel(book_open);
    Rect::new(p.x + 8.0, p.y + 17.0, 70.0, 69.0)
}

/// Every spot there is right now. In creative the panel of everything is
/// beside the bag, showing one tab.
fn spots(book_open: bool, creative: bool, tab: usize, chest: bool) -> Vec<Spot> {
    if chest {
        return (0..SLOTS)
            .map(Spot::Slot)
            .chain((0..furniture::CHEST_SLOTS).map(Spot::Chest))
            .collect();
    }
    let mut all: Vec<Spot> = (0..SLOTS)
        .map(Spot::Slot)
        .chain((0..4).map(Spot::Grid))
        .chain([Spot::Result])
        .collect();
    if book_open && creative {
        all.extend((0..TABS.len()).map(Spot::Tab));
        all.extend(
            Item::all()
                .iter()
                .enumerate()
                .filter(|&(_, &item)| tab_of(item) == tab)
                .map(|(i, _)| Spot::Palette(i)),
        );
    }
    all
}

pub fn spot_at(point: Vec2, book_open: bool, creative: bool, tab: usize, chest: bool) -> Option<Spot> {
    spots(book_open, creative, tab, chest)
        .into_iter()
        .find(|&spot| spot_rect(spot, book_open).contains(point))
}

/// Where the arrow keys go from `from`, heading `dir`: the nearest spot
/// that way, keeping to the same row or column when there's one there.
pub fn step_from(
    from: Spot,
    dir: Vec2,
    book_open: bool,
    creative: bool,
    tab: usize,
    chest: bool,
) -> Option<Spot> {
    let center = |spot: Spot| {
        let r = spot_rect(spot, book_open);
        vec2(r.x + r.w / 2.0, r.y + r.h / 2.0)
    };
    let here = center(from);
    spots(book_open, creative, tab, chest)
        .into_iter()
        .filter(|&spot| spot != from)
        .filter_map(|spot| {
            let d = center(spot) - here;
            let along = d.x * dir.x + d.y * dir.y;
            let across = (d.x * dir.y - d.y * dir.x).abs();
            (along > 2.0).then_some((spot, along + across * 2.5))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(spot, _)| spot)
}

impl DuckCraft {
    /// Opens and closes the screen (E), and works it while it's open: with
    /// the mouse, or with the keys like a console (arrows or WASD move the
    /// cursor, X takes or puts, C takes half or puts one, Q and R change
    /// creative's tab).
    pub fn inventory_screen(&mut self, ctx: &mut Context) {
        let input = &ctx.input;
        if self.screen.is_none() {
            if input.was_pressed(Key::E) && !self.vitals.fainted && self.sleeping <= 0.0 {
                // In creative the panel of everything is open from the start.
                // What didn't fit in the bag last time is still in the grid.
                self.screen = Some(Screen {
                    book_open: self.mode == Mode::Creative,
                    grid: std::mem::take(&mut self.kept_grid),
                    ..Screen::default()
                });
                self.digging = None;
                ctx.lock_pointer(false);
                ctx.play(click(700.0));
            }
            return;
        }
        if input.was_pressed(Key::E) || input.was_pressed(Key::Escape) {
            self.close_inventory();
            ctx.lock_pointer(true);
            ctx.play(click(500.0));
            return;
        }
        let pressed = |keys: &[Key]| keys.iter().any(|&key| input.was_pressed(key));
        let creative = self.mode == Mode::Creative;
        let shift = input.is_down(Key::LeftShift);
        // Number keys still pick the hotbar slot, as outside.
        for (i, key) in [
            Key::Digit1,
            Key::Digit2,
            Key::Digit3,
            Key::Digit4,
            Key::Digit5,
            Key::Digit6,
            Key::Digit7,
            Key::Digit8,
            Key::Digit9,
        ]
        .into_iter()
        .enumerate()
        {
            if input.was_pressed(key) {
                self.slot = i;
                self.eating_seconds = 0.0;
            }
        }
        let mouse = input.mouse_position();
        let mouse_moved = input.mouse_delta().length() > 0.0;
        let left_click = input.was_mouse_pressed(MouseButton::Left);
        let right_click = input.was_mouse_pressed(MouseButton::Right);
        let key_left = pressed(&[Key::X, Key::Enter, Key::Space]);
        let key_right = pressed(&[Key::C, Key::Backspace]);
        let dir = [
            (&[Key::Up, Key::W], vec2(0.0, -1.0)),
            (&[Key::Down, Key::S], vec2(0.0, 1.0)),
            (&[Key::Left, Key::A], vec2(-1.0, 0.0)),
            (&[Key::Right, Key::D], vec2(1.0, 0.0)),
        ]
        .into_iter()
        .find(|(keys, _)| pressed(*keys))
        .map(|(_, dir)| dir);
        let tab_step = if pressed(&[Key::Q]) {
            TABS.len() - 1
        } else if pressed(&[Key::R, Key::Tab]) {
            1
        } else {
            0
        };

        let mut sounds = Vec::new();
        let hand = self.slot;
        let screen = self.screen.as_mut().expect("open");
        screen.opened += ctx.dt();
        if mouse_moved || left_click || right_click {
            screen.cursor = None; // the mouse takes over
        }
        let (book, tab, chest) = (screen.book_open, screen.tab, screen.chest.is_some());
        let under_mouse = mouse.and_then(|m| spot_at(m, book, creative, tab, chest));
        if let Some(dir) = dir {
            // The first press shows where the cursor is; then it moves.
            let next = match screen.cursor {
                Some(from) => step_from(from, dir, book, creative, tab, chest).unwrap_or(from),
                None => under_mouse.unwrap_or(Spot::Slot(hand)),
            };
            screen.cursor = Some(next);
            sounds.push(1250.0);
        }
        if tab_step != 0 && creative && book {
            screen.tab = (tab + tab_step) % TABS.len();
            if matches!(screen.cursor, Some(Spot::Palette(_))) {
                screen.cursor = Some(Spot::Tab(screen.tab));
            }
            sounds.push(1000.0);
        }
        let cursor = screen.cursor;
        let target = if key_left || key_right {
            cursor.or(under_mouse)
        } else if left_click || right_click {
            under_mouse
        } else {
            None
        };
        if let Some(spot) = target {
            if let Some(freq) = self.click_spot(spot, key_left || left_click, shift) {
                sounds.push(freq);
            }
        }
        for freq in sounds {
            ctx.play(click(freq));
        }
    }

    /// What a click on `spot` does (`left`, else right; `shift` held).
    /// Returns the pitch of a click to play, if anything happened.
    pub fn click_spot(&mut self, spot: Spot, left: bool, shift: bool) -> Option<f32> {
        let mut screen = self.screen.take()?;
        let sound = match spot {
            Spot::Palette(i) => {
                let item = Item::all()[i];
                if screen.held.take().is_some() {
                    Some(300.0) // what was held goes in the bin
                } else if shift {
                    self.inventory.add(item, item.max_stack());
                    Some(900.0)
                } else {
                    screen.held = Some(Stack::new(item, item.max_stack()));
                    Some(900.0)
                }
            }
            Spot::Tab(t) => {
                screen.tab = t;
                if matches!(screen.cursor, Some(Spot::Palette(_))) {
                    screen.cursor = Some(Spot::Tab(t));
                }
                Some(1000.0)
            }
            Spot::Result => self.take_result(&mut screen, shift),
            Spot::Chest(i) => {
                let cell = screen.chest.unwrap_or_default();
                let slots = self
                    .world
                    .chests
                    .entry(cell)
                    .or_insert_with(|| vec![None; furniture::CHEST_SLOTS]);
                if shift && left {
                    // Into the bag, as much as fits.
                    match slots[i].take() {
                        Some(stack) => {
                            let left_over = self.inventory.add(stack.item, stack.count);
                            slots[i] = (left_over > 0).then(|| Stack::new(stack.item, left_over));
                            Some(650.0)
                        }
                        None => None,
                    }
                } else {
                    click_slot(&mut slots[i], &mut screen.held, left)
                }
            }
            Spot::Slot(i) if shift && left && screen.chest.is_some() => {
                // From the bag into the chest.
                let cell = screen.chest.unwrap_or_default();
                match (self.inventory.slots[i].take(), self.world.chests.get_mut(&cell)) {
                    (Some(stack), Some(slots)) => {
                        self.inventory.slots[i] = add_into(slots, stack);
                        Some(650.0)
                    }
                    (stack, _) => {
                        self.inventory.slots[i] = stack;
                        None
                    }
                }
            }
            Spot::Slot(i) if shift && left => {
                // Across: hotbar to backpack, backpack to hotbar.
                if let Some(stack) = self.inventory.slots[i].take() {
                    let range = if i < HOTBAR_SLOTS {
                        HOTBAR_SLOTS..SLOTS
                    } else {
                        0..HOTBAR_SLOTS
                    };
                    let left_over = add_into(&mut self.inventory.slots[range], stack);
                    self.inventory.slots[i] = left_over;
                    Some(650.0)
                } else {
                    None
                }
            }
            Spot::Grid(i) if shift && left => {
                if let Some(stack) = screen.grid[i].take() {
                    let left_over = self.inventory.add(stack.item, stack.count);
                    if left_over > 0 {
                        screen.grid[i] = Some(Stack::new(stack.item, left_over));
                    }
                    Some(650.0)
                } else {
                    None
                }
            }
            Spot::Slot(i) => click_slot(&mut self.inventory.slots[i], &mut screen.held, left),
            Spot::Grid(i) => click_slot(&mut screen.grid[i], &mut screen.held, left),
        };
        self.screen = Some(screen);
        sound
    }

    /// Takes what the grid makes: onto the held stack, or (Shift) as many
    /// as can be made, into the bag.
    fn take_result(&mut self, screen: &mut Screen, shift: bool) -> Option<f32> {
        let recipe = matching_recipe(&screen.grid)?;
        let (item, amount) = recipe.makes;
        let mut made = 0;
        loop {
            if matching_recipe(&screen.grid).is_none_or(|r| !std::ptr::eq(r, recipe)) {
                break;
            }
            if shift {
                if !room_for(&self.inventory, item, amount) {
                    break;
                }
                self.inventory.add(item, amount);
            } else {
                match &mut screen.held {
                    None => screen.held = Some(Stack::new(item, amount)),
                    Some(held) if held.item == item && held.count + amount <= item.max_stack() => {
                        held.count += amount;
                    }
                    Some(_) => break,
                }
            }
            for slot in screen.grid.iter_mut() {
                if let Some(stack) = slot {
                    stack.count -= 1;
                    if stack.count == 0 {
                        *slot = None;
                    }
                }
            }
            made += 1;
            self.crafted += amount;
            if !shift {
                break;
            }
        }
        if made > 0 {
            self.say(format!("Made {} {}", made * amount, item.name()));
            Some(1100.0)
        } else {
            None
        }
    }

    /// Closes the screen, putting the grid and the held stack back in the
    /// bag.
    pub fn close_inventory(&mut self) {
        let Some(mut screen) = self.screen.take() else {
            return;
        };
        // The grid goes back in the bag; what doesn't fit stays in the grid
        // for next time.
        for slot in screen.grid.iter_mut() {
            if let Some(stack) = *slot {
                let left = self.inventory.add(stack.item, stack.count);
                *slot = (left > 0).then(|| Stack::new(stack.item, left));
            }
        }
        // The stack on the pointer: the bag, else the chest (if one is
        // open), else the grid.
        if let Some(held) = screen.held.take() {
            let mut left = self.inventory.add(held.item, held.count);
            if left > 0
                && let Some(slots) = screen.chest.and_then(|cell| self.world.chests.get_mut(&cell))
            {
                left = add_into(slots, Stack::new(held.item, left)).map_or(0, |s| s.count);
            }
            if left > 0 {
                let rest = add_into(&mut screen.grid, Stack::new(held.item, left));
                if let Some(rest) = rest {
                    self.say(format!(
                        "Your bag is full: {} {} were lost",
                        rest.count,
                        rest.item.name()
                    ));
                }
            }
        }
        if screen.grid.iter().any(|s| s.is_some()) {
            self.say("Your bag is full: some things wait in the crafting grid");
        }
        self.kept_grid = screen.grid;
    }

    /// The screen, drawn like Minecraft would have looked on a PlayStation:
    /// dark slate panels shaded in dithered bands, chunky bevels, sunken
    /// black slots, bone-white capitals with a blood-red rule under each
    /// title, scanlines over the world, and a wipe as it opens.
    pub fn draw_inventory(&self, canvas: &mut Canvas, camera: &Camera3D, mouse: Option<Vec2>) {
        let Some(screen) = &self.screen else {
            return;
        };
        let book = screen.book_open;
        let creative = self.mode == Mode::Creative;
        canvas.fill_rect(
            Rect::new(0.0, 0.0, WIDTH, HEIGHT),
            Color::rgba(4, 4, 10, 150),
        );
        let mut y = 0.0;
        while y < HEIGHT {
            canvas.fill_rect(Rect::new(0.0, y, WIDTH, 1.0), Color::rgba(0, 0, 0, 45));
            y += 2.0;
        }

        let p = panel(book);
        let corner = match self.mode {
            Mode::Survival => format!("DAY {}", self.day_number()),
            Mode::Creative => String::from("CREATIVE"),
        };
        let chest = screen.chest.is_some();
        ps_panel(canvas, p, Some((if chest { "CHEST" } else { "INVENTORY" }, &corner)));
        canvas.label("inventory", p);
        self.fresh_depth(canvas);
        if let Some(cell) = screen.chest {
            // The chest's 27 slots, where the duck and the grid would be.
            let slots = self.world.chests.get(&cell);
            for i in 0..furniture::CHEST_SLOTS {
                let stack = slots.and_then(|s| s.get(i).copied().flatten());
                self.draw_slot(canvas, spot_rect(Spot::Chest(i), book), stack);
            }
            canvas.label("chest", Rect::new(p.x + 8.0, p.y + 22.0, 162.0, 54.0));
        }

        if !chest {
            // The duck's window: a spotlight on black, a shadow on the floor.
            let window = preview_rect(book);
            canvas.fill_rect(window, WELL);
            for k in 0..5 {
                let inset = 6.0 + k as f32 * 6.0;
                canvas.fill_rect(
                    Rect::new(window.x + inset, window.y, window.w - inset * 2.0, window.h - 4.0 - k as f32 * 8.0),
                    Color::rgba(255, 236, 210, 7),
                );
            }
            let floor = vec2(window.x + window.w / 2.0, window.y + window.h - 8.0);
            for (w, h) in [(34.0, 5.0), (26.0, 7.0), (14.0, 9.0)] {
                canvas.fill_rect(
                    Rect::new(floor.x - w / 2.0, floor.y - h / 2.0 + 1.0, w, h),
                    Color::rgba(0, 0, 0, 60),
                );
            }
            well_edges(canvas, window);

            // Crafting: the grid, an arrow, what it makes.
            shadowed_text(canvas, "CRAFTING", vec2(p.x + 84.0, p.y + 18.0), DIM);
            for i in 0..4 {
                self.draw_slot(canvas, spot_rect(Spot::Grid(i), book), screen.grid[i]);
            }
            let arrow = vec2(p.x + 126.0, p.y + 42.0);
            canvas.fill_rect(Rect::new(arrow.x, arrow.y + 2.0, 8.0, 4.0), FAINT);
            canvas.fill_triangle(
                arrow + vec2(8.0, -2.0),
                arrow + vec2(14.0, 4.0),
                arrow + vec2(8.0, 10.0),
                FAINT,
            );
            let result = matching_recipe(&screen.grid).map(|r| Stack::new(r.makes.0, r.makes.1));
            let result_rect = spot_rect(Spot::Result, book);
            let big = Rect::new(result_rect.x - 4.0, result_rect.y - 4.0, 26.0, 26.0);
            canvas.fill_rect(big, WELL);
            well_edges(canvas, big);
            if result.is_some() {
                // Something can be made: the frame breathes.
                let glow = (0.5 + 0.5 * (self.time * 4.0).sin()) * 0.6 + 0.2;
                canvas.draw_rect(big, Color::rgba(200, 60, 50, (glow * 255.0) as u8));
            }
            self.draw_stack(canvas, result, vec2(result_rect.x + 1.0, result_rect.y + 1.0));
        }

        // The bag, a rule, the hotbar; the slot in hand framed in red.
        for i in 0..SLOTS {
            self.draw_slot(canvas, spot_rect(Spot::Slot(i), book), self.inventory.slots[i]);
        }
        canvas.fill_rect(Rect::new(p.x + 8.0, p.y + 146.0, 162.0, 1.0), Color::rgb(16, 16, 20));
        canvas.fill_rect(Rect::new(p.x + 8.0, p.y + 147.0, 162.0, 1.0), Color::rgb(58, 58, 68));
        let hand = spot_rect(Spot::Slot(self.slot), book);
        canvas.draw_rect(
            Rect::new(hand.x - 1.0, hand.y - 1.0, hand.w + 2.0, hand.h + 2.0),
            BLOOD,
        );

        if book && creative {
            self.draw_palette(canvas, screen.tab);
        }

        // The cursor: on the key cursor, or on what the mouse is over.
        let pointed = screen
            .cursor
            .or_else(|| mouse.and_then(|m| spot_at(m, book, creative, screen.tab, chest)));
        if let Some(spot) = pointed {
            selection(canvas, spot_rect(spot, book), self.time);
        }
        let look_at = match screen.cursor {
            Some(spot) => {
                let r = spot_rect(spot, book);
                Some(vec2(r.x + r.w / 2.0, r.y + r.h / 2.0))
            }
            None => mouse,
        };
        if !chest {
            self.draw_duck_preview(canvas, camera, look_at);
        }

        // The held stack floats by the cursor, or follows the mouse.
        let hold_at = match screen.cursor {
            Some(spot) => {
                let r = spot_rect(spot, book);
                Some(vec2(r.x + 7.0, r.y - 7.0))
            }
            None => mouse.map(|m| m - vec2(8.0, 8.0)),
        };
        if let (Some(at), Some(held)) = (hold_at, screen.held) {
            self.fresh_depth(canvas); // on top of everything
            self.draw_stack(canvas, Some(held), at);
        }
        if let (Some(m), true, None) = (mouse, self.draw_pointer, screen.cursor) {
            canvas.fill_triangle(
                m + vec2(-1.0, -1.0),
                m + vec2(-1.0, 12.0),
                m + vec2(9.0, 8.0),
                Color::BLACK,
            );
            canvas.fill_triangle(m, m + vec2(0.0, 10.0), m + vec2(7.0, 7.0), INK);
        }

        // Opening: a wipe from a line in the middle, like an old set warming up.
        let wipe = 0.16;
        if screen.opened < wipe {
            let half = HEIGHT * (screen.opened / wipe).clamp(0.0, 1.0) / 2.0;
            let middle = HEIGHT / 2.0;
            let night = Color::rgb(4, 4, 10);
            canvas.fill_rect(Rect::new(0.0, 0.0, WIDTH, (middle - half).max(0.0)), night);
            canvas.fill_rect(Rect::new(0.0, middle + half, WIDTH, HEIGHT - middle - half), night);
            canvas.fill_rect(Rect::new(0.0, middle - half - 1.0, WIDTH, 1.0), INK);
            canvas.fill_rect(Rect::new(0.0, middle + half, WIDTH, 1.0), INK);
        }
    }

    fn draw_slot(&self, canvas: &mut Canvas, rect: Rect, stack: Option<Stack>) {
        canvas.fill_rect(rect, WELL);
        well_edges(canvas, rect);
        self.draw_stack(canvas, stack, vec2(rect.x + 1.0, rect.y + 1.0));
    }

    /// Creative: one of everything, under three tabs, to take as many as
    /// you like.
    fn draw_palette(&self, canvas: &mut Canvas, tab: usize) {
        let b = book_panel(true);
        ps_panel(canvas, b, None);
        canvas.fill_rect(Rect::new(b.x + 2.0, b.y + 2.0, b.w - 4.0, 14.0), Color::rgb(20, 20, 25));
        for (t, name) in TABS.iter().enumerate() {
            let r = spot_rect(Spot::Tab(t), true);
            let (fill, text) = if t == tab {
                (Color::rgb(64, 64, 76), INK)
            } else {
                (Color::rgb(30, 30, 36), FAINT)
            };
            canvas.fill_rect(r, fill);
            if t == tab {
                canvas.fill_rect(Rect::new(r.x, r.y + r.h - 1.0, r.w, 1.0), BLOOD);
            }
            let w = Canvas::text_width(name, 1);
            canvas.draw_text(name, vec2(r.x + ((r.w - w) / 2.0).floor(), r.y + 3.0), 1, text);
        }
        for (i, item) in Item::all().into_iter().enumerate() {
            if tab_of(item) != tab {
                continue;
            }
            self.draw_slot(canvas, spot_rect(Spot::Palette(i), true), Some(Stack::new(item, 1)));
        }
        for (n, note) in ["X: a full stack", "Holding: bin it"].iter().enumerate() {
            let at = vec2(b.x + 6.0, b.y + b.h - 24.0 + n as f32 * 10.0);
            canvas.draw_text(note, at, 1, FAINT);
        }
    }

    /// The duck in its window, turning to look at the cursor (and, left
    /// alone, slowly looking about).
    fn draw_duck_preview(&self, canvas: &mut Canvas, camera: &Camera3D, look_at: Option<Vec2>) {
        let book = self.screen.as_ref().is_some_and(|s| s.book_open);
        let r = preview_rect(book);
        // A screen point, at `depth` ahead in view space (x right, y up).
        let depth = 5.6;
        let per_pixel = 2.0 * depth * (camera.fov / 2.0).tan() / HEIGHT;
        let to_view = |sx: f32, sy: f32| {
            vec3(
                (sx - WIDTH / 2.0) * per_pixel,
                (HEIGHT / 2.0 - sy) * per_pixel,
                depth,
            )
        };
        let feet = to_view(r.x + r.w / 2.0, r.y + r.h - 8.0);
        let eyes = vec2(r.x + r.w / 2.0, r.y + 22.0);
        let look = look_at.map_or(Vec2::ZERO, |m| m - eyes);
        let idle = (self.time * 0.6).sin() * 0.25;
        let (yaw, pitch) = preview_look(look);
        let pose = Pose {
            yaw: yaw + idle,
            pitch,
            swing: 0.0,
            lift: 0.0,
        };
        canvas.set_lighting(Lighting {
            sun_direction: vec3(0.3, 0.9, -0.4).normalized(),
            sun_color: Color::rgb(255, 240, 220),
            sun_strength: 0.8,
            ambient: 0.35,
        });
        for (mesh, place) in self.art.duck.parts(feet, pose) {
            canvas.draw_mesh_in_view(mesh, &place, camera);
        }
    }
}

/// Where the duck in the inventory's window turns its head, for a cursor
/// `look` away from its eyes (in screen pixels, y down): facing out of the
/// screen, turned a little towards the cursor, and tipped up (positive
/// pitch) for a cursor above it, down for one below.
pub fn preview_look(look: Vec2) -> (f32, f32) {
    let yaw = std::f32::consts::PI - (look.x / 60.0).clamp(-1.0, 1.0) * 0.7;
    let pitch = -(look.y / 60.0).clamp(-1.0, 1.0) * 0.5;
    (yaw, pitch)
}

/// Puts `stack` into the first slots of `slots` it fits (same item with
/// room first, then empty). Returns what didn't fit.
fn add_into(slots: &mut [Option<Stack>], mut stack: Stack) -> Option<Stack> {
    for slot in slots.iter_mut().flatten() {
        if slot.item == stack.item && slot.count < stack.item.max_stack() {
            let moved = stack.count.min(stack.item.max_stack() - slot.count);
            slot.count += moved;
            stack.count -= moved;
        }
    }
    for slot in slots.iter_mut() {
        if stack.count == 0 {
            break;
        }
        if slot.is_none() {
            *slot = Some(stack);
            stack.count = 0;
        }
    }
    (stack.count > 0).then_some(stack)
}

/// Whether `count` more of `item` would fit in the bag.
fn room_for(inventory: &Inventory, item: Item, count: u32) -> bool {
    let room: u32 = inventory
        .slots
        .iter()
        .map(|slot| match slot {
            None => item.max_stack(),
            Some(stack) if stack.item == item => item.max_stack() - stack.count,
            Some(_) => 0,
        })
        .sum();
    room >= count
}

/// A click on one slot with (or without) a stack on the pointer.
fn click_slot(slot: &mut Option<Stack>, held: &mut Option<Stack>, left: bool) -> Option<f32> {
    match (slot.as_mut(), held.as_mut(), left) {
        (None, None, _) => None,
        // Pick up all, or half.
        (Some(stack), None, true) => {
            *held = Some(*stack);
            *slot = None;
            Some(900.0)
        }
        (Some(stack), None, false) => {
            let half = stack.count.div_ceil(2);
            *held = Some(Stack::new(stack.item, half));
            stack.count -= half;
            if stack.count == 0 {
                *slot = None;
            }
            Some(900.0)
        }
        // Put down all, or one.
        (None, Some(h), true) => {
            *slot = Some(*h);
            *held = None;
            Some(700.0)
        }
        (None, Some(h), false) => {
            *slot = Some(Stack::new(h.item, 1));
            h.count -= 1;
            if h.count == 0 {
                *held = None;
            }
            Some(700.0)
        }
        // The same thing: as much as fits (or one).
        (Some(stack), Some(h), _) if stack.item == h.item => {
            let room = stack.item.max_stack() - stack.count;
            let moved = if left { h.count.min(room) } else { 1.min(room) };
            if moved == 0 {
                return None;
            }
            stack.count += moved;
            h.count -= moved;
            if h.count == 0 {
                *held = None;
            }
            Some(700.0)
        }
        // Different things: swap (left click).
        (Some(stack), Some(h), true) => {
            std::mem::swap(stack, h);
            Some(800.0)
        }
        (Some(_), Some(_), false) => None,
    }
}

// ---- The look ----

/// Bone-white type.
pub(crate) const INK: Color = Color::rgb(216, 208, 188);
pub(crate) const DIM: Color = Color::rgb(132, 126, 118);
pub(crate) const FAINT: Color = Color::rgb(84, 82, 88);
/// The panels, light at the top to dark at the bottom.
const SLATE: Color = Color::rgb(44, 44, 52);
const SLATE_DEEP: Color = Color::rgb(25, 25, 31);
pub(crate) const LIGHT_EDGE: Color = Color::rgb(92, 92, 106);
pub(crate) const DARK_EDGE: Color = Color::rgb(10, 10, 13);
/// Inside a slot.
pub(crate) const WELL: Color = Color::rgb(16, 16, 20);
pub(crate) const BLOOD: Color = Color::rgb(150, 30, 28);

/// A panel: a drop shadow, slate shaded in six bands that meet in a
/// checkerboard (the dithering of an old console that couldn't blend),
/// a two-pixel bevel and a black outline; with `title`, a dark bar at the
/// top with the title on the left, a note on the right, and a red rule.
pub(crate) fn ps_panel(canvas: &mut Canvas, r: Rect, title: Option<(&str, &str)>) {
    canvas.fill_rect(Rect::new(r.x + 3.0, r.y + 3.0, r.w, r.h), Color::rgba(0, 0, 0, 110));
    let bands = 6;
    let shade = |k: usize| SLATE.lerp(SLATE_DEEP, k as f32 / (bands - 1) as f32);
    for k in 0..bands {
        let top = r.y + (r.h * k as f32 / bands as f32).floor();
        let bottom = r.y + (r.h * (k + 1) as f32 / bands as f32).floor();
        canvas.fill_rect(Rect::new(r.x, top, r.w, bottom - top), shade(k));
        if k > 0 {
            // Two rows of the band above, every other pixel then every fourth.
            for (row, step) in [(0.0, 2.0), (1.0, 4.0)] {
                let mut x = r.x + ((k + row as usize) % 2) as f32;
                while x < r.x + r.w {
                    canvas.fill_rect(Rect::new(x, top + row, 1.0, 1.0), shade(k - 1));
                    x += step;
                }
            }
        }
    }
    canvas.fill_rect(Rect::new(r.x, r.y, r.w, 2.0), LIGHT_EDGE);
    canvas.fill_rect(Rect::new(r.x, r.y, 2.0, r.h), LIGHT_EDGE);
    canvas.fill_rect(Rect::new(r.x + 1.0, r.y + r.h - 2.0, r.w - 1.0, 2.0), DARK_EDGE);
    canvas.fill_rect(Rect::new(r.x + r.w - 2.0, r.y + 1.0, 2.0, r.h - 1.0), DARK_EDGE);
    canvas.draw_rect(r, Color::BLACK);
    if let Some((left, right)) = title {
        let bar = Rect::new(r.x + 2.0, r.y + 2.0, r.w - 4.0, 12.0);
        canvas.fill_rect(bar, Color::rgb(19, 19, 24));
        canvas.fill_rect(Rect::new(bar.x, bar.y + bar.h, bar.w, 1.0), BLOOD);
        shadowed_text(canvas, left, vec2(bar.x + 4.0, bar.y + 3.0), INK);
        let w = Canvas::text_width(right, 1);
        canvas.draw_text(right, vec2(bar.x + bar.w - w - 4.0, bar.y + 3.0), 1, DIM);
    }
}

/// A raised button or key cap.
pub(crate) fn bevel_box(canvas: &mut Canvas, r: Rect, fill: Color) {
    canvas.fill_rect(r, fill);
    canvas.fill_rect(Rect::new(r.x, r.y, r.w, 1.0), LIGHT_EDGE);
    canvas.fill_rect(Rect::new(r.x, r.y, 1.0, r.h), LIGHT_EDGE);
    canvas.fill_rect(Rect::new(r.x, r.y + r.h - 1.0, r.w, 1.0), DARK_EDGE);
    canvas.fill_rect(Rect::new(r.x + r.w - 1.0, r.y, 1.0, r.h), DARK_EDGE);
}

/// A sunken edge: dark top left, lit bottom right.
pub(crate) fn well_edges(canvas: &mut Canvas, r: Rect) {
    canvas.fill_rect(Rect::new(r.x, r.y, r.w, 1.0), Color::rgb(4, 4, 6));
    canvas.fill_rect(Rect::new(r.x, r.y, 1.0, r.h), Color::rgb(4, 4, 6));
    canvas.fill_rect(Rect::new(r.x, r.y + r.h - 1.0, r.w, 1.0), Color::rgb(66, 66, 78));
    canvas.fill_rect(Rect::new(r.x + r.w - 1.0, r.y, 1.0, r.h), Color::rgb(66, 66, 78));
}

/// The cursor: four corner brackets just outside `r`, pulsing between bone
/// and old gold, over a faint light.
pub(crate) fn selection(canvas: &mut Canvas, r: Rect, time: f32) {
    canvas.fill_rect(
        Rect::new(r.x + 1.0, r.y + 1.0, r.w - 2.0, r.h - 2.0),
        Color::rgba(255, 240, 210, 40),
    );
    let pulse = 0.5 + 0.5 * (time * 6.0).sin();
    let color = INK.lerp(Color::rgb(230, 180, 70), pulse);
    let (x0, y0, x1, y1) = (r.x - 2.0, r.y - 2.0, r.x + r.w + 1.0, r.y + r.h + 1.0);
    for (x, y, dx, dy) in [(x0, y0, 1.0, 1.0), (x1, y0, -1.0, 1.0), (x0, y1, 1.0, -1.0), (x1, y1, -1.0, -1.0)] {
        let across = if dx > 0.0 { x } else { x - 3.0 };
        let down = if dy > 0.0 { y } else { y - 3.0 };
        canvas.fill_rect(Rect::new(across, y, 4.0, 1.0), color);
        canvas.fill_rect(Rect::new(x, down, 1.0, 4.0), color);
    }
}

/// Text with a one-pixel shadow, down and right.
pub(crate) fn shadowed_text(canvas: &mut Canvas, text: &str, at: Vec2, color: Color) {
    canvas.draw_text(text, at + vec2(1.0, 1.0), 1, Color::rgba(0, 0, 0, 200));
    canvas.draw_text(text, at, 1, color);
}

pub fn click(freq: f32) -> Sound {
    Sound {
        name: "click",
        wave: Wave::Triangle,
        freq,
        ms: 35.0,
        volume: 0.12,
        ..Default::default()
    }
}
