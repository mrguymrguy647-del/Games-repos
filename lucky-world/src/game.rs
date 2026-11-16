//! LUCKY WORLD: pick a world, spin 16 wheels in 4 rounds (a summary card
//! after each), see your country's card, meet your rivals, then go onto the
//! 3D world map (in `map_screen`).

mod brain_panel;
mod econ_screen;
mod map_screen;
mod war_screen;

use duckforge::prelude::*;

use crate::ai::{AiData, Brains};
use crate::war::{WarData, Wars};
use crate::country::{self, Country};
use crate::econ::{EconData, Economy};
use crate::data::Data;
use crate::fate::{self, Landed};
use crate::ui::{self, Assets, Button, ButtonKind, Particle, GOLD, MUTED, SOFT};
use crate::wheel::{self, Event, WheelView};
use crate::map_view::{MapView, Models};
use crate::mapdata::Atlas;
use crate::world::World;
use econ_screen::{Clock, SPECTATOR_SPEED};
use map_screen::MapUi;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Title,
    Wheel,
    /// The summary card after round 1 to 4.
    Round(usize),
    Final,
    Rivals,
    /// The 3D world map.
    Map,
}

const TITLE_SPIN: Button = Button::new(230.0, 572.0, "SPIN THE WHEELS", "SPACE", ButtonKind::Gold);
const TITLE_SKIP: Button = Button::new(510.0, 572.0, "SKIP SPIN", "S: roll all 16 at once", ButtonKind::Blue);
const TITLE_WATCH: Button = Button::new(790.0, 572.0, "WATCH THE BRAINS", "W: a world of NPCs only", ButtonKind::Blue);
const WHEEL_GO: Button = Button::new(895.0, 560.0, "SPIN", "SPACE", ButtonKind::Gold);
const WHEEL_SKIP: Button = Button::new(895.0, 630.0, "SKIP SPIN", "S: roll the rest at once", ButtonKind::Blue);
const CARD_NEXT: Button = Button::new(510.0, 528.0, "NEXT ROUND", "SPACE", ButtonKind::Gold);
const CARD_SKIP: Button = Button::new(1000.0, 640.0, "SKIP SPIN", "S", ButtonKind::Blue);
const FINAL_GO: Button = Button::new(510.0, 622.0, "MEET YOUR RIVALS", "SPACE", ButtonKind::Gold);
const RIVALS_GO: Button = Button::new(990.0, 22.0, "BEGIN", "SPACE", ButtonKind::Gold);
const INSPECT_BACK: Button = Button::new(510.0, 622.0, "BACK", "SPACE or ESC", ButtonKind::Blue);

/// Results this rare (or rarer) get the RARE! badge and their own sound.
const RARE: f64 = 0.08;

pub struct Lucky {
    pub data: Data,
    assets: Assets,
    pub map: usize,
    pub screen: Screen,
    screen_time: f32,
    time: f32,
    pub landed: Vec<Landed>,
    pub view: Option<WheelView>,
    /// Counting down to the next wheel spinning by itself.
    autospin: f32,
    landed_time: f32,
    pub player: Option<Country>,
    pub rivals: Vec<Country>,
    inspect: Option<usize>,
    particles: Vec<Particle>,
    /// Randomness for effects only, so sparkles never change a result.
    fx: Rng,
    mouse: Option<Vec2>,
    camera: Camera3D,
    /// A canvas the size of the screen, for working out where 3D things are.
    projector: Canvas,
    /// The Continent wheel turning slowly behind the title.
    title_wheel: Option<WheelView>,
    revealed: usize,
    toast: Option<(String, f32)>,
    pub skipped: bool,
    /// Picture quality, from `lucky_world.tune.json` (or `--set supersample=1`).
    supersample: u32,
    bloom: f32,
    /// The maps, provinces and placement settings.
    pub atlas: Atlas,
    models: Models,
    /// Who owns what, once the rivals are met.
    pub world: Option<World>,
    pub map_view: Option<MapView>,
    pub map_ui: MapUi,
    /// The page of the rival reveal (24 nations to a page).
    rival_page: usize,
    /// The economy's rules (data/economy/), and the world's economy once
    /// everyone is on the map.
    pub econ_data: EconData,
    pub economy: Option<Economy>,
    pub clock: Clock,
    /// The NPC brains' weights (data/ai/), and the brains of every nation
    /// the player doesn't run (all of them in spectator mode).
    pub ai_data: AiData,
    pub brains: Option<Brains>,
    /// The war's rules (data/war/war.toml), and every army, war and battle.
    pub war_data: WarData,
    pub wars: Option<Wars>,
    /// Spectator mode: nobody is the player; click a nation to read its mind.
    pub spectating: bool,
    /// `--spectate`: go straight to spectator mode on the first frame.
    pub spectate_at_start: bool,
    /// `--spectate-war`: and open with a declared war between the strongest neighbours.
    pub start_war: bool,
}

impl Lucky {
    pub fn new(data: Data, atlas: Atlas, assets: Assets, models: Models, econ_data: EconData, ai_data: AiData, war_data: WarData) -> Lucky {
        let mut game = Lucky {
            data,
            assets,
            map: 0,
            screen: Screen::Title,
            screen_time: 0.0,
            time: 0.0,
            landed: Vec::new(),
            view: None,
            autospin: -1.0,
            landed_time: 0.0,
            player: None,
            rivals: Vec::new(),
            inspect: None,
            particles: Vec::new(),
            fx: Rng::new(1),
            mouse: None,
            camera: camera_at(0.0, 0.0),
            projector: Canvas::new(ui::W as usize, ui::H as usize),
            title_wheel: None,
            revealed: 0,
            toast: None,
            skipped: false,
            supersample: 2,
            bloom: 0.0,
            atlas,
            models,
            world: None,
            map_view: None,
            map_ui: MapUi::default(),
            rival_page: 0,
            econ_data,
            economy: None,
            clock: Clock::default(),
            ai_data,
            brains: None,
            war_data,
            wars: None,
            spectating: false,
            spectate_at_start: false,
            start_war: false,
        };
        game.make_title_wheel();
        game
    }

    fn make_title_wheel(&mut self) {
        let odds = fate::odds(&self.data, self.map, 0, &[], &[]);
        self.title_wheel = Some(WheelView::new(odds, self.data.slices, &self.assets.font));
    }

    fn go(&mut self, screen: Screen) {
        self.screen = screen;
        self.screen_time = 0.0;
        self.revealed = 0;
    }

    /// Back to the title with nothing spun (F5; for trying the screens).
    fn restart(&mut self) {
        self.landed.clear();
        self.view = None;
        self.player = None;
        self.rivals.clear();
        self.world = None;
        self.map_view = None;
        self.economy = None;
        self.brains = None;
        self.wars = None;
        self.spectating = false;
        self.clock = Clock::default();
        self.map_ui = MapUi::default();
        self.rival_page = 0;
        self.inspect = None;
        self.particles.clear();
        self.skipped = false;
        self.go(Screen::Title);
    }

    fn enter_wheel(&mut self, index: usize, autospin: bool) {
        let odds = fate::odds(&self.data, self.map, index, &self.landed, &[]);
        self.view = Some(WheelView::new(odds, self.data.slices, &self.assets.font));
        self.autospin = if autospin { 1.1 } else { -1.0 };
        self.landed_time = 0.0;
        self.go(Screen::Wheel);
    }

    /// Skip Spin: every wheel not yet landed is rolled at once, with the
    /// same odds and links, and the country card comes up.
    fn skip(&mut self, ctx: &mut Context) {
        // A wheel caught mid-spin keeps the result it was heading for.
        if let Some(view) = &self.view {
            if view.spinning() && self.landed.len() == view.odds.wheel {
                let option = view.slices[view.target_slice.expect("spinning")].option;
                let l = fate::land(&self.data, &view.odds, option, &mut ctx.rng);
                self.landed.push(l);
            }
        }
        let player = country::roll_country(&self.data, self.map, self.landed.clone(), &[], true, &mut ctx.rng);
        self.landed = player.landed.clone();
        self.player = Some(player);
        self.view = None;
        self.skipped = true;
        ctx.play_with(&self.assets.whoosh, 0.9, 0.0);
        self.finish(ctx);
    }

    /// Spectator mode: a world of NPC brains only, straight onto the map at
    /// top speed. (Nation 0 is rolled like a player's, but a brain runs it.)
    fn spectate(&mut self, ctx: &mut Context) {
        let first = country::roll_country(&self.data, self.map, Vec::new(), &[], true, &mut ctx.rng);
        self.landed = first.landed.clone();
        self.player = Some(first);
        self.spectating = true;
        self.build_world(ctx);
        if self.start_war {
            self.open_war();
        }
        self.clock.speed = SPECTATOR_SPEED;
        ctx.play_with(&self.assets.whoosh, 0.9, 0.0);
        self.enter_map(ctx);
    }

    /// `--spectate-war`: the two strongest neighbours go to war on day one.
    fn open_war(&mut self) {
        let (Some(w), Some(e)) = (self.wars.as_mut(), self.economy.as_mut()) else { return };
        let Some((a, b)) = crate::war::report::rivals(w) else { return };
        let mine = w.held(a);
        let border: Vec<usize> = w.held(b).into_iter().filter(|&q| w.geo.neighbours[q].iter().any(|r| mine.contains(r)) && !w.capital.contains(&q)).take(3).collect();
        let goal = if border.is_empty() { crate::war::Goal::Reparations } else { crate::war::Goal::Provinces(border) };
        crate::war::escalation::declare(w, e, &self.econ_data, a, b, 3, goal, "the spectator's war".to_string());
    }

    fn finish(&mut self, ctx: &mut Context) {
        if self.player.is_none() {
            let player = country::roll_country(&self.data, self.map, self.landed.clone(), &[], true, &mut ctx.rng);
            self.player = Some(player);
        }
        ctx.play_with(&self.assets.fanfare, 0.9, 0.0);
        ui::confetti(&mut self.particles, &mut self.fx, 110);
        self.go(Screen::Final);
    }

    fn mouse_clicked(&self, ctx: &Context, button: &Button) -> bool {
        ctx.input.was_mouse_pressed(MouseButton::Left) && button.hovered(self.mouse)
    }

    fn confirm(&self, ctx: &Context, button: &Button) -> bool {
        ctx.input.was_pressed(Key::Space) || ctx.input.was_pressed(Key::Enter) || self.mouse_clicked(ctx, button)
    }

    fn update_title(&mut self, ctx: &mut Context) {
        ctx.idle_ok();
        let maps = self.data.maps.len();
        let before = self.map;
        if ctx.input.was_pressed(Key::Left) {
            self.map = (self.map + maps - 1) % maps;
        }
        if ctx.input.was_pressed(Key::Right) {
            self.map = (self.map + 1) % maps;
        }
        for (i, key) in [Key::Digit1, Key::Digit2, Key::Digit3].into_iter().enumerate() {
            if ctx.input.was_pressed(key) && i < maps {
                self.map = i;
            }
        }
        if ctx.input.was_mouse_pressed(MouseButton::Left) {
            for i in 0..maps {
                if self.mouse.is_some_and(|m| map_card_rect(i).contains(m)) {
                    self.map = i;
                }
            }
        }
        if self.map != before {
            ctx.play_with(&self.assets.tick, 0.8, 0.0);
            self.make_title_wheel();
        }
        if let Some(w) = &mut self.title_wheel {
            w.angle += 0.15 * ctx.dt();
        }
        if ctx.input.was_pressed(Key::W) || self.mouse_clicked(ctx, &TITLE_WATCH) {
            self.spectate(ctx);
        } else if ctx.input.was_pressed(Key::S) || self.mouse_clicked(ctx, &TITLE_SKIP) {
            self.skip(ctx);
        } else if self.confirm(ctx, &TITLE_SPIN) {
            self.enter_wheel(0, true);
        }
    }

    fn update_wheel(&mut self, ctx: &mut Context) {
        let dt = ctx.dt();
        if ctx.input.was_pressed(Key::S) || self.mouse_clicked(ctx, &WHEEL_SKIP) {
            self.skip(ctx);
            return;
        }
        let confirm = self.confirm(ctx, &WHEEL_GO);
        let view = self.view.as_mut().expect("the wheel screen has a wheel");
        let index = view.odds.wheel;
        if view.spinning() {
            if confirm {
                view.hurry();
            }
        } else if !view.landed {
            ctx.idle_ok();
            if self.autospin > 0.0 {
                self.autospin -= dt;
            }
            if confirm || (self.autospin <= 0.0 && self.autospin > -0.5) {
                self.autospin = -1.0;
                view.start_spin(&mut ctx.rng);
                ctx.play_with(&self.assets.whoosh, 0.7, 0.0);
            }
        } else {
            ctx.idle_ok();
            self.landed_time += dt;
            if confirm && self.landed_time > 0.25 {
                let round = self.data.wheels[index].round;
                let next = index + 1;
                if next >= self.data.wheels.len() || self.data.wheels[next].round != round {
                    ctx.play_with(&self.assets.card_sound, 1.0, 0.0);
                    self.go(Screen::Round(round));
                } else {
                    self.enter_wheel(next, true);
                }
                return;
            }
        }

        let view = self.view.as_mut().expect("still here");
        for event in view.update(dt) {
            match event {
                Event::Tick { speed } => {
                    // Quieter while it whirls, crisp as it slows; high and low pegs alternate.
                    let volume = (1.0 - speed / 45.0).clamp(0.22, 1.0);
                    let wav = if view.ticks.is_multiple_of(2) { &self.assets.tick } else { &self.assets.tick_low };
                    ctx.play_with(wav, volume, 0.35);
                }
                Event::Landed => {
                    let target = view.target_slice.expect("landed");
                    ctx.check(
                        "the pointer rests on the slice the spin chose",
                        view.slice_under() == target,
                    );
                    let option = view.slices[target].option;
                    ctx.check("the result is an option on the wheel", view.odds.chance(option) > 0.0);
                    let l = fate::land(&self.data, &view.odds, option, &mut ctx.rng);
                    self.landed.push(l);
                    self.landed_time = 0.0;
                    ctx.play_with(&self.assets.land, 1.0, 0.0);
                    let rare = view.odds.chance(option) < RARE;
                    if rare {
                        ctx.play_with(&self.assets.rare_sound, 0.9, 0.0);
                    }
                    if let Some((tip, _)) = self.camera.project(wheel::CENTER + vec3(1.0, 0.0, -0.1), &self.projector) {
                        ui::burst(&mut self.particles, &mut self.fx, tip, if rare { 70 } else { 34 }, if rare { 520.0 } else { 360.0 });
                    }
                    if rare {
                        ui::burst(&mut self.particles, &mut self.fx, vec2(1025.0, 500.0), 40, 300.0);
                    }
                }
            }
        }
    }

    fn update_round(&mut self, ctx: &mut Context, round: usize) {
        ctx.idle_ok();
        if ctx.input.was_pressed(Key::S) || self.mouse_clicked(ctx, &CARD_SKIP) {
            self.skip(ctx);
            return;
        }
        if self.screen_time > 0.35 && self.confirm(ctx, &CARD_NEXT) {
            if round >= 4 {
                self.finish(ctx);
            } else {
                let next = self.landed.len();
                self.enter_wheel(next, true);
            }
        }
    }

    fn update_final(&mut self, ctx: &mut Context) {
        ctx.idle_ok();
        // Rows of the card turn up one by one, each with a tick.
        let rows = self.data.wheels.len();
        let due = (((self.screen_time - 0.5) / 0.09).max(0.0) as usize).min(rows);
        if due > self.revealed {
            self.revealed = due;
            ctx.play_with(&self.assets.tick, 0.6, 0.0);
            if due == rows {
                ctx.play_with(&self.assets.land, 0.8, 0.0);
            }
        }
        if self.screen_time > 0.3 && self.confirm(ctx, &FINAL_GO) {
            if self.revealed < rows {
                // First press shows the whole card.
                self.screen_time = 10.0;
                return;
            }
            self.build_world(ctx);
            ctx.play_with(&self.assets.whoosh, 0.8, 0.0);
            self.go(Screen::Rivals);
        }
    }

    /// The rivals spin, everyone is placed on the map, and extra nations
    /// fill the empty land.
    fn build_world(&mut self, ctx: &mut Context) {
        let player = self.player.clone().expect("the final card has a country");
        let rivals = country::roll_rivals(&self.data, &player, &mut ctx.rng);
        let world = World::build(&self.data, &self.atlas, player, rivals, &mut ctx.rng);
        // Placement can redraw a border the land couldn't give (and says so).
        self.player = Some(world.nations[0].clone());
        self.landed = world.nations[0].landed.clone();
        self.rivals = world.nations[1..].to_vec();
        self.map_view = Some(MapView::new(&self.atlas, &world, &self.models));
        // The economy starts from the wheels (seeded, so a replay is the same).
        let seed = ctx.rng.next_u32();
        let mut economy = Economy::new(&self.data, &self.econ_data, &self.atlas, &world, seed);
        // Every nation but the player's thinks for itself (all of them when watching).
        let all = self.spectating;
        self.brains = Some(Brains::new(self.ai_data.clone(), &self.data, &self.atlas, &world, &mut economy, seed, |i| all || i != 0));
        // Armies, navies and air forces from the wheels; the NPCs' generals and war brains.
        let mut wars = Wars::new(self.war_data.clone(), &self.data, &self.atlas, &world, &mut economy, &self.econ_data, seed);
        wars.set_intel(&self.ai_data);
        wars.player = if all { None } else { Some(0) };
        self.wars = Some(wars);
        self.map_ui.war = Default::default();
        self.economy = Some(economy);
        self.clock = Clock::default();
        self.world = Some(world);
        self.rival_page = 0;
    }

    fn update_rivals(&mut self, ctx: &mut Context) {
        ctx.idle_ok();
        let n = self.rivals.len();
        let pages = n.div_ceil(rivals_per_page(n)).max(1);
        let on_page = (n - self.rival_page * rivals_per_page(n)).min(rivals_per_page(n));
        let due = (((self.screen_time - 0.2) / rival_delay(n)).max(0.0) as usize + 1).min(on_page);
        if due > self.revealed {
            self.revealed = due;
            ctx.play_with(&self.assets.card_sound, if n > 9 { 0.35 } else { 0.7 }, 0.0);
        }
        if self.inspect.is_some() {
            if ctx.input.was_pressed(Key::Escape)
                || ctx.input.was_pressed(Key::Space)
                || ctx.input.was_pressed(Key::Enter)
                || self.mouse_clicked(ctx, &INSPECT_BACK)
            {
                self.inspect = None;
            }
            return;
        }
        if pages > 1 {
            let before = self.rival_page;
            if ctx.input.was_pressed(Key::Right) {
                self.rival_page = (self.rival_page + 1) % pages;
            }
            if ctx.input.was_pressed(Key::Left) {
                self.rival_page = (self.rival_page + pages - 1) % pages;
            }
            if self.rival_page != before {
                ctx.play_with(&self.assets.click, 0.8, 0.0);
                self.screen_time = 10.0;
            }
        }
        let first = self.rival_page * rivals_per_page(n);
        let digits = [
            Key::Digit1,
            Key::Digit2,
            Key::Digit3,
            Key::Digit4,
            Key::Digit5,
            Key::Digit6,
            Key::Digit7,
            Key::Digit8,
            Key::Digit9,
        ];
        for (i, key) in digits.into_iter().enumerate() {
            if first + i < n && ctx.input.was_pressed(key) {
                self.inspect = Some(first + i);
            }
        }
        if ctx.input.was_mouse_pressed(MouseButton::Left) {
            for i in first..(first + on_page) {
                if self.mouse.is_some_and(|m| rival_rect(i - first, n).contains(m)) {
                    self.inspect = Some(i);
                }
            }
        }
        if self.inspect.is_some() {
            ctx.play_with(&self.assets.card_sound, 0.8, 0.0);
        } else if self.confirm(ctx, &RIVALS_GO) {
            self.enter_map(ctx);
        }
    }

    fn report(&self, ctx: &mut Context) {
        ctx.report("screen", format!("{:?}", self.screen));
        ctx.report("map", self.data.maps[self.map].id.as_str());
        ctx.report("wheels_landed", self.landed.len());
        if let Some(view) = &self.view {
            ctx.report("wheel", self.data.wheels[view.odds.wheel].id.as_str());
            ctx.report("wheel_angle", (view.angle * 100.0).round() / 100.0);
        }
        if let Some(l) = self.landed.last() {
            let w = self.landed.len() - 1;
            ctx.report("last_result", country::result_text(&self.data, self.map, w, l));
        }
        if let Some(p) = &self.player {
            ctx.report("country", p.name.as_str());
        }
        ctx.report("rivals", self.rivals.len());
        if let Some(w) = &self.world {
            ctx.report("nations", w.nations.len());
            ctx.report("main_nations", w.main_count);
        }
        self.report_econ(ctx);
    }
}

impl Game for Lucky {
    fn update(&mut self, ctx: &mut Context) {
        let dt = ctx.dt();
        self.time += dt;
        self.screen_time += dt;
        // Quality: smooth 3D edges (1 = off, 2 = good, up to 4) and a bloom
        // halo round glowing things (0 = off; costly on the CPU).
        self.supersample = ctx.tune("supersample", 2.0).clamp(1.0, 4.0) as u32;
        self.bloom = ctx.tune("bloom", 0.0).max(0.0);
        if let Some(m) = ctx.input.mouse_position() {
            self.mouse = Some(m);
        }
        if std::mem::take(&mut self.spectate_at_start) && self.screen == Screen::Title {
            self.spectate(ctx);
        }
        if ctx.input.was_pressed(Key::F5) {
            if let Some(id) = self.map_ui.ambient.take() {
                ctx.stop_loop(id);
            }
            if let Some(id) = self.map_ui.war.drums.take() {
                ctx.stop_loop(id);
            }
            self.restart();
        }
        // Esc steps back out of a card or a selection on the map; elsewhere it quits.
        let map_busy = self.screen == Screen::Map && self.map_ui.wants_escape();
        if ctx.input.was_pressed(Key::Escape) && self.inspect.is_none() && !map_busy {
            ctx.quit();
        }
        let push_in = match self.screen {
            Screen::Wheel if self.view.as_ref().is_some_and(|v| v.landed) => ui::ease_out(self.landed_time / 0.6),
            _ => 0.0,
        };
        self.camera = camera_at(self.time, push_in);

        match self.screen {
            Screen::Title => self.update_title(ctx),
            Screen::Wheel => self.update_wheel(ctx),
            Screen::Round(r) => self.update_round(ctx, r),
            Screen::Final => self.update_final(ctx),
            Screen::Rivals => self.update_rivals(ctx),
            Screen::Map => self.update_map(ctx),
        }
        ui::update_particles(&mut self.particles, dt);
        if let Some((_, t)) = &mut self.toast {
            *t -= dt;
            if *t <= 0.0 {
                self.toast = None;
            }
        }
        ctx.check("never more than 16 results", self.landed.len() <= self.data.wheels.len());
        self.report(ctx);
    }

    fn draw(&self, canvas: &mut Canvas) {
        // clear() also resets the 3D depth buffer: without it, last frame's
        // depths hide this frame's wheel (see ISSUES.md).
        canvas.clear(if self.screen == Screen::Map { Color::from_hex(0x1c3c7a) } else { ui::INK });
        if self.screen != Screen::Map {
            canvas.blit_sprite(&self.assets.backdrop, Vec2::ZERO, Blit::default());
        }
        match self.screen {
            Screen::Title => self.draw_title(canvas),
            Screen::Wheel => {
                self.draw_stage(canvas);
                self.draw_panel(canvas);
            }
            Screen::Round(r) => {
                self.draw_stage(canvas);
                self.draw_round_card(canvas, r);
            }
            Screen::Final => {
                let player = self.player.as_ref().expect("final has a country");
                self.draw_country_card(canvas, player, self.revealed, true);
                FINAL_GO.draw(canvas, &self.assets, self.mouse, self.time, self.revealed >= self.data.wheels.len());
            }
            Screen::Rivals => self.draw_rivals(canvas),
            Screen::Map => self.draw_map(canvas),
        }
        ui::draw_particles(canvas, &self.assets, &self.particles);
        if let Some((words, t)) = &self.toast {
            let a = t.min(1.0);
            canvas.fill_rect(Rect::new(240.0, 640.0, 800.0, 50.0), Color::rgba(10, 5, 30, (220.0 * a) as u8));
            ui::text(canvas, &self.assets.font, words, vec2(640.0, 651.0), 22.0, ui::alpha(GOLD, a), Align::Center, 780.0);
        }
    }
}

/// The camera: a slow sway, and a push in towards a result.
fn camera_at(time: f32, push_in: f32) -> Camera3D {
    let sway = (time * 0.23).sin() * 0.18;
    let bob = (time * 0.31).sin() * 0.04;
    let target = vec3(1.18 - 0.12 * push_in, 1.47, 0.0);
    let pos = vec3(1.22 + sway, 2.0 + bob, -3.35 + 0.35 * push_in);
    Camera3D::looking_at(pos, target)
}

fn map_card_rect(i: usize) -> Rect {
    Rect::new(60.0 + i as f32 * 400.0, 300.0, 360.0, 190.0)
}

/// Up to 9 rivals get big cards; more get small ones, 24 to a page.
fn rivals_per_page(n: usize) -> usize {
    if n <= 9 { 9 } else { 24 }
}

fn rival_delay(n: usize) -> f32 {
    if n <= 9 { 0.14 } else { 0.05 }
}

/// Where the `i`th card on the page goes, with `n` rivals in all.
fn rival_rect(i: usize, n: usize) -> Rect {
    if n <= 9 {
        let (col, row) = (i % 3, i / 3);
        Rect::new(36.0 + col as f32 * 406.0, 112.0 + row as f32 * 196.0, 400.0, 182.0)
    } else {
        let (col, row) = (i % 4, i / 4);
        Rect::new(24.0 + col as f32 * 310.0, 104.0 + row as f32 * 98.0, 300.0, 92.0)
    }
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

impl Lucky {
    fn light_3d(&self, canvas: &mut Canvas) {
        if self.supersample > 1 {
            canvas.set_supersample(self.supersample);
        }
        canvas.set_lighting(Lighting {
            sun_direction: vec3(-0.35, -0.55, 0.75),
            sun_color: Color::from_hex(0xfff2dd),
            sun_strength: 0.75,
            ambient: 0.5,
        });
        if self.bloom > 0.0 {
            let landed = self.view.as_ref().is_some_and(|v| v.landed);
            let pulse = if landed { 1.1 + 0.5 * (self.time * 5.0).sin() } else { 0.7 };
            canvas.set_bloom(Some(Bloom {
                strength: pulse * self.bloom,
                radius: 0.025,
            }));
        }
    }

    fn draw_stage(&self, canvas: &mut Canvas) {
        self.light_3d(canvas);
        let cam = &self.camera;
        // The floor is painted into the backdrop (a 3D floor cost more than the wheel).
        canvas.draw_mesh(&self.assets.stand, &Transform::default(), cam);
        if let Some(view) = &self.view {
            view.draw(canvas, cam, &self.assets, self.time);
            // The landed slice glows (a soft light over it, pulsing).
            if view.landed {
                let t = self.landed_time;
                if let Some((at, _)) = cam.project(wheel::CENTER + vec3(0.62, 0.0, -0.06), canvas) {
                    let pulse = 0.55 + 0.25 * (self.time * 5.0).sin();
                    ui::glow(canvas, &self.assets, at, 330.0 * ui::ease_out_back(t / 0.4), pulse);
                }
            }
            if let Some((c, r)) = view.on_screen(canvas, cam) {
                canvas.label("wheel", Rect::from_center(c, 2.0 * r, 2.0 * r));
            }
        }
    }

    fn draw_title(&self, canvas: &mut Canvas) {
        let a = &self.assets;
        // The chosen world's Continent wheel turning slowly, dimmed, behind everything.
        if let Some(w) = &self.title_wheel {
            self.light_3d(canvas);
            let cam = Camera3D::looking_at(vec3(0.0, 1.55, -2.6), wheel::CENTER);
            w.draw(canvas, &cam, a, self.time);
            ui::dim(canvas, 0.62);
        }
        let drop = (1.0 - ui::ease_out_back(self.screen_time / 0.8)) * -120.0;
        canvas.blit_sprite(&a.logo, vec2(140.0, 26.0 + drop), Blit::default());
        canvas.label("logo", Rect::new(162.0, 51.0, 960.0, 150.0));
        ui::text(canvas, &a.font, "Spin sixteen wheels. Become a nation. Outplay your rivals.", vec2(640.0, 212.0), 24.0, Color::WHITE, Align::Center, 1100.0);
        ui::plain(canvas, &a.font, "CHOOSE YOUR WORLD", vec2(640.0, 262.0), 18.0, GOLD, Align::Center);
        for (i, m) in self.data.maps.iter().enumerate() {
            let r = map_card_rect(i);
            let on = i == self.map;
            if on {
                ui::glow(canvas, a, r.center(), 520.0, 0.35 + 0.1 * (self.time * 3.0).sin());
            }
            canvas.blit_sprite(if on { &a.card_map_on } else { &a.card_map }, vec2(r.x, r.y), Blit::default());
            ui::text(canvas, &a.font, &m.name, vec2(r.x + 180.0, r.y + 26.0), 26.0, if on { GOLD } else { Color::WHITE }, Align::Center, 320.0);
            let mut s = ui::style(16.0, MUTED);
            s.outline = 0.0;
            s.shadow = None;
            s.align = Align::Center;
            s.wrap = Some(310.0);
            canvas.draw_font_text(&a.font, &m.description, vec2(r.x + 180.0, r.y + 70.0), &s);
            let island = if m.continents.iter().all(|c| c.tags.iter().any(|t| t == "island")) { " · islands" } else { "" };
            let facts = format!("{} continents · {} rivals{island}", m.continents.len(), m.rivals);
            ui::plain(canvas, &a.font, &facts, vec2(r.x + 180.0, r.y + 140.0), 15.0, SOFT, Align::Center);
            ui::plain(canvas, &a.font, &format!("{}", i + 1), vec2(r.x + 26.0, r.y + 22.0), 16.0, SOFT, Align::Left);
            canvas.label(&format!("map {}", m.id), r);
        }
        TITLE_SPIN.draw(canvas, a, self.mouse, self.time, true);
        TITLE_SKIP.draw(canvas, a, self.mouse, self.time, false);
        TITLE_WATCH.draw(canvas, a, self.mouse, self.time, false);
        ui::plain(canvas, &a.font, "Left / Right or 1-3: choose a world   ·   W: watch a world of NPC brains   ·   F5: start over   ·   Esc: quit", vec2(640.0, 670.0), 15.0, SOFT, Align::Center);
    }

    fn draw_panel(&self, canvas: &mut Canvas) {
        let a = &self.assets;
        let view = self.view.as_ref().expect("wheel screen");
        let index = view.odds.wheel;
        let def = &self.data.wheels[index];
        let round = self.data.round(def.round);
        let (x0, cx) = (790.0, 1025.0);
        let slide = (1.0 - ui::ease_out(self.screen_time / 0.35)) * 30.0;
        canvas.blit_sprite(&a.panel, vec2(x0, 20.0), Blit::default());
        ui::plain(canvas, &a.font, &format!("ROUND {} OF 4  ·  {}", def.round, round.title.to_uppercase()), vec2(cx, 40.0), 20.0, GOLD, Align::Center);
        ui::plain(canvas, &a.font, &round.tagline, vec2(cx, 66.0), 15.0, MUTED, Align::Center);

        // Sixteen pips in four groups: done, now, still to come.
        let mut px = cx - (16.0 * 20.0 + 3.0 * 14.0) / 2.0 + 8.0;
        for w in 0..self.data.wheels.len() {
            if w > 0 && self.data.wheels[w].round != self.data.wheels[w - 1].round {
                px += 14.0;
            }
            let c = vec2(px, 100.0);
            if w < self.landed.len() {
                canvas.fill_circle(c, 6.0, GOLD);
            } else if w == index {
                canvas.fill_circle(c, 7.0 + (self.time * 6.0).sin(), Color::WHITE);
            } else {
                canvas.fill_circle(c, 5.0, Color::rgba(255, 255, 255, 50));
            }
            px += 20.0;
        }

        ui::plain(canvas, &a.font, &format!("WHEEL {} OF 16", index + 1), vec2(cx, 120.0 - slide), 14.0, SOFT, Align::Center);
        ui::text(canvas, &a.font, &def.title.to_uppercase(), vec2(cx, 138.0 - slide), 40.0, Color::WHITE, Align::Center, 410.0);

        // What shaped these odds.
        let because: Vec<String> = view
            .odds
            .because
            .iter()
            .map(|&w| format!("{}: {}", self.data.wheels[w].title, country::result_text(&self.data, self.map, w, &self.landed[w])))
            .collect();
        let linked = if because.is_empty() { "Odds: as they come".to_string() } else { format!("Linked to {}", because.join(" · ")) };
        let mut s = ui::style(14.0, ui::BLUE);
        s.outline = 0.0;
        s.shadow = None;
        s.align = Align::Center;
        s.wrap = Some(410.0);
        let shown: String = if linked.chars().count() > 150 { format!("{}...", linked.chars().take(147).collect::<String>()) } else { linked };
        canvas.draw_font_text(&a.font, &shown, vec2(cx, 188.0), &s);

        // The odds, exactly as the slices show them.
        let result = view.result();
        let rows: Vec<(usize, f64)> = view.odds.on_wheel().collect();
        let row_h = if rows.len() > 8 { 21.0 } else { 25.0 };
        let mut y = 232.0;
        for (option, chance) in rows {
            let o = &view.odds.options[option];
            let win = result == Some(option);
            if win {
                canvas.fill_rect(Rect::new(x0 + 22.0, y - 3.0, 426.0, row_h), ui::alpha(GOLD, 0.25));
            }
            canvas.fill_circle(vec2(x0 + 40.0, y + row_h / 2.0 - 2.0), 7.0, o.color);
            let color = if win { ui::GOLD_HI } else if result.is_some() { SOFT } else { Color::WHITE };
            ui::plain(canvas, &a.font, &o.label, vec2(x0 + 56.0, y), 17.0, color, Align::Left);
            let pct = if chance < 0.01 { format!("{:.1}%", chance * 100.0) } else { format!("{:.0}%", chance * 100.0) };
            ui::plain(canvas, &a.font, &pct, vec2(x0 + 440.0, y), 17.0, color, Align::Right);
            y += row_h;
        }

        // The result, glowing.
        if let Some(option) = result {
            let t = self.landed_time;
            let l = &self.landed[index];
            let o = &view.odds.options[option];
            let pop = ui::ease_out_back(t / 0.45);
            ui::glow(canvas, a, vec2(cx, 500.0), 420.0 * pop, 0.55 + 0.15 * (self.time * 5.0).sin());
            ui::plain(canvas, &a.font, "YOU GOT", vec2(cx, 458.0), 14.0, MUTED, Align::Center);
            let words = country::result_text(&self.data, self.map, index, l);
            ui::text(canvas, &a.font, &words, vec2(cx, 476.0), 36.0 * pop.max(0.1), ui::GOLD_HI, Align::Center, 420.0);
            let detail = match (&o.detail, def.kind) {
                (Some(d), _) => d.clone(),
                (None, crate::data::Kind::Number) if !words.contains(&o.label) => o.label.clone(),
                _ => String::new(),
            };
            ui::plain(canvas, &a.font, &detail, vec2(cx, 522.0), 15.0, MUTED, Align::Center);
            if view.odds.chance(option) < RARE {
                let k = ui::ease_out_back((t - 0.2) / 0.4);
                canvas.draw_placed(
                    &a.rare,
                    vec2(1180.0, 470.0),
                    Placed {
                        pivot: vec2(80.0, 80.0),
                        scale: vec2(0.75 * k, 0.75 * k),
                        turn: (self.time * 2.0).sin() * 6.0,
                        ..Placed::default()
                    },
                );
                canvas.label("rare", Rect::from_center(vec2(1180.0, 470.0), 120.0, 120.0));
            }
            canvas.label("result", Rect::new(x0 + 20.0, 456.0, 430.0, 90.0));
        } else if view.spinning() {
            ui::plain(canvas, &a.font, "Spinning...  (SPACE: faster)", vec2(cx, 492.0), 17.0, MUTED, Align::Center);
        } else {
            ui::plain(canvas, &a.font, "Each slice is its real chance.", vec2(cx, 480.0), 16.0, MUTED, Align::Center);
            ui::plain(canvas, &a.font, "Ready when you are!", vec2(cx, 504.0), 16.0, MUTED, Align::Center);
        }

        let mut go = WHEEL_GO;
        if view.landed {
            let last_of_round = index + 1 >= self.data.wheels.len() || self.data.wheels[index + 1].round != def.round;
            go.label = if last_of_round { "ROUND SUMMARY" } else { "NEXT WHEEL" };
        } else if view.spinning() {
            go.label = "FASTER";
        }
        go.draw(canvas, a, self.mouse, self.time, view.landed || (!view.spinning() && self.autospin < 0.0));
        WHEEL_SKIP.draw(canvas, a, self.mouse, self.time, false);
    }

    fn draw_round_card(&self, canvas: &mut Canvas, round: usize) {
        let a = &self.assets;
        let t = self.screen_time;
        ui::dim(canvas, (t / 0.3).min(1.0) * 0.6);
        let y0 = 110.0 + (1.0 - ui::ease_out_back(t / 0.5)) * 500.0;
        let x0 = 210.0;
        ui::glow(canvas, a, vec2(640.0, y0 + 40.0), 700.0, 0.25);
        canvas.blit_sprite(&a.card_round, vec2(x0, y0), Blit::default());
        let r = self.data.round(round);
        ui::plain(canvas, &a.font, &format!("ROUND {round} COMPLETE"), vec2(640.0, y0 + 30.0), 18.0, MUTED, Align::Center);
        ui::text(canvas, &a.font, &r.title.to_uppercase(), vec2(640.0, y0 + 52.0), 46.0, GOLD, Align::Center, 700.0);
        let wheels = self.data.round_wheels(round);
        let row_h = if wheels.len() > 4 { 62.0 } else { 74.0 };
        for (k, &w) in wheels.iter().enumerate() {
            let y = y0 + 122.0 + k as f32 * row_h;
            let l = &self.landed[w];
            let options = self.data.options_on(w, self.map);
            let o = &options[l.option];
            canvas.fill_circle(vec2(x0 + 70.0, y + 22.0), 11.0, o.color);
            ui::plain(canvas, &a.font, &self.data.wheels[w].title, vec2(x0 + 96.0, y + 10.0), 20.0, MUTED, Align::Left);
            let words = country::result_text(&self.data, self.map, w, l);
            ui::text(canvas, &a.font, &words, vec2(x0 + 790.0, y), 28.0, Color::WHITE, Align::Right, 420.0);
            let detail = o.detail.clone().unwrap_or_default();
            ui::plain(canvas, &a.font, &detail, vec2(x0 + 790.0, y + 34.0), 14.0, SOFT, Align::Right);
            canvas.fill_rect(Rect::new(x0 + 60.0, y + row_h - 8.0, 740.0, 1.0), Color::rgba(255, 255, 255, 30));
        }
        let mut next = CARD_NEXT;
        next.at.y = y0 + 418.0;
        if round >= 4 {
            next.label = "SEE YOUR COUNTRY";
        }
        next.draw(canvas, a, self.mouse, self.time, true);
        if round < 4 {
            CARD_SKIP.draw(canvas, a, self.mouse, self.time, false);
        }
    }

    /// The card with all sixteen results. `rows` of them are showing.
    fn draw_country_card(&self, canvas: &mut Canvas, c: &Country, rows: usize, mine: bool) {
        let a = &self.assets;
        let (x0, y0) = (30.0, 14.0);
        ui::glow(canvas, a, vec2(640.0, 90.0), 900.0, 0.18);
        canvas.blit_sprite(&a.card_final, vec2(x0, y0), Blit::default());
        country::draw_flag(canvas, Rect::new(x0 + 46.0, y0 + 38.0, 150.0, 100.0), &c.flag);
        canvas.label("flag", Rect::new(x0 + 46.0, y0 + 38.0, 150.0, 100.0));
        let heading = if mine { "YOUR COUNTRY" } else { "RIVAL NATION" };
        ui::plain(canvas, &a.font, heading, vec2(x0 + 222.0, y0 + 36.0), 16.0, GOLD, Align::Left);
        ui::text(canvas, &a.serif, &c.name, vec2(x0 + 222.0, y0 + 56.0), 46.0, Color::WHITE, Align::Left, 940.0);
        let place = format!("{}, {}", c.label(&self.data, "continent"), self.data.maps[c.map].name);
        ui::plain(canvas, &a.font, &place, vec2(x0 + 222.0, y0 + 116.0), 18.0, MUTED, Align::Left);
        if mine && self.skipped {
            ui::plain(canvas, &a.font, "rolled with Skip Spin", vec2(x0 + 1170.0, y0 + 40.0), 14.0, SOFT, Align::Right);
        }

        let mut shown = 0;
        for round in 1..=4 {
            let col_x = x0 + 40.0 + (round - 1) as f32 * 290.0;
            let r = self.data.round(round);
            ui::plain(canvas, &a.font, &r.title.to_uppercase(), vec2(col_x, y0 + 168.0), 22.0, GOLD, Align::Left);
            canvas.fill_rect(Rect::new(col_x, y0 + 198.0, 260.0, 2.0), ui::alpha(GOLD, 0.5));
            for (k, &w) in self.data.round_wheels(round).iter().enumerate() {
                if shown >= rows {
                    break;
                }
                shown += 1;
                let y = y0 + 212.0 + k as f32 * 66.0;
                let options = self.data.options_on(w, c.map);
                let o = &options[c.landed[w].option];
                canvas.fill_circle(vec2(col_x + 7.0, y + 9.0), 6.0, o.color);
                ui::plain(canvas, &a.font, &self.data.wheels[w].title, vec2(col_x + 20.0, y), 15.0, SOFT, Align::Left);
                let words = c.result_text(&self.data, w);
                let color = if o.tags.iter().any(|t| t == "curse") {
                    ui::RED
                } else if o.tags.iter().any(|t| t == "blessing" || t == "rare") {
                    ui::GREEN
                } else {
                    Color::WHITE
                };
                ui::text(canvas, &a.font, &words, vec2(col_x + 20.0, y + 21.0), 21.0, color, Align::Left, 248.0);
            }
        }
        if rows >= self.data.wheels.len() {
            let stats = format!(
                "{} soldiers on duty   ·   {} per person   ·   {} people per km²",
                country::group(c.troops(&self.data)),
                format!("${}", country::group(c.gdp_per_person(&self.data))),
                country::group(c.value(&self.data, "population") / c.value(&self.data, "size")),
            );
            ui::plain(canvas, &a.font, &stats, vec2(640.0, y0 + 560.0), 17.0, MUTED, Align::Center);
            // Its land, once the world is made.
            if let Some((w, n)) = self.world.as_ref().and_then(|w| w.nations.iter().position(|x| x.name == c.name).map(|n| (w, n))) {
                let map = self.atlas.map(w.map);
                let provinces = w.provinces_of(n).len();
                let capital = &map.provinces[w.capital[n]].name;
                let mut land = format!("{provinces} provinces  ·  capital {capital}");
                if let Some(note) = &w.notes[n] {
                    land.push_str(&format!("  ·  {note}"));
                }
                ui::text(canvas, &a.font, &land, vec2(640.0, y0 + 584.0), 16.0, ui::BLUE, Align::Center, 1100.0);
            }
        }
    }

    fn draw_rivals(&self, canvas: &mut Canvas) {
        let a = &self.assets;
        let player = self.player.as_ref().expect("rivals come after the player");
        let n = self.rivals.len();
        let main = self.world.as_ref().map_or(n + 1, |w| w.main_count);
        ui::text(canvas, &a.font, "YOUR RIVALS", vec2(40.0, 18.0), 42.0, GOLD, Align::Left, 400.0);
        let filled = (n + 1).saturating_sub(main);
        let sub = if filled > 0 {
            format!("{} spun with you; {} more rose to fill {}", main - 1, filled, self.data.maps[self.map].name)
        } else {
            format!("{} nations spun their wheels on {}", n, self.data.maps[self.map].name)
        };
        ui::text(canvas, &a.font, &sub, vec2(42.0, 70.0), 16.0, MUTED, Align::Left, 500.0);
        country::draw_flag(canvas, Rect::new(560.0, 26.0, 60.0, 40.0), &player.flag);
        ui::plain(canvas, &a.font, "YOU", vec2(632.0, 24.0), 13.0, GOLD, Align::Left);
        ui::text(canvas, &a.serif, &player.name, vec2(632.0, 40.0), 22.0, Color::WHITE, Align::Left, 340.0);
        let mut go = RIVALS_GO;
        go.label = "BEGIN";
        go.key = "SPACE: onto the map";
        go.draw(canvas, a, self.mouse, self.time, self.revealed > 0);

        let per = rivals_per_page(n);
        let first = self.rival_page * per;
        let pages = n.div_ceil(per).max(1);
        for (k, r) in self.rivals.iter().enumerate().skip(first).take(per.min(self.revealed)) {
            let i = k - first;
            let appear = self.screen_time - 0.2 - i as f32 * rival_delay(n);
            let ease = ui::ease_out_back(appear / 0.35);
            let rect = rival_rect(i, n);
            let y = rect.y + (1.0 - ease) * 40.0;
            let hover = self.mouse.is_some_and(|m| rect.contains(m));
            let is_new = k + 1 >= main;
            if n <= 9 {
                self.draw_big_rival(canvas, r, k, rect, y, hover);
            } else {
                let card = if hover {
                    &a.card_small_on
                } else if is_new {
                    &a.card_small_new
                } else {
                    &a.card_small
                };
                canvas.blit_sprite(card, vec2(rect.x, y), Blit::default());
                country::draw_flag(canvas, Rect::new(rect.x + 14.0, y + 14.0, 54.0, 36.0), &r.flag);
                ui::text(canvas, &a.serif, &r.name, vec2(rect.x + 78.0, y + 10.0), 17.0, Color::WHITE, Align::Left, 176.0);
                let line = format!("{} · {}", r.label(&self.data, "continent"), r.label(&self.data, "ideology"));
                ui::plain(canvas, &a.font, &line, vec2(rect.x + 78.0, y + 36.0), 13.0, MUTED, Align::Left);
                let line = format!("{} · Army: {}", country::money(r.value(&self.data, "gdp")), r.label(&self.data, "army"));
                ui::plain(canvas, &a.font, &line, vec2(rect.x + 14.0, y + 62.0), 13.0, MUTED, Align::Left);
                if is_new {
                    ui::plain(canvas, &a.font, "NEW", vec2(rect.x + 288.0, y + 10.0), 11.0, ui::GREEN, Align::Right);
                } else if k < 9 {
                    ui::plain(canvas, &a.font, &format!("{}", k + 1), vec2(rect.x + 288.0, y + 10.0), 12.0, SOFT, Align::Right);
                }
                canvas.label(&format!("rival {}", k + 1), rect);
            }
        }
        if self.inspect.is_none() {
            let hint = if pages > 1 {
                format!("Page {} of {} (Left / Right)  ·  click a nation for its full card", self.rival_page + 1, pages)
            } else {
                "Click a nation (or press 1-9) for its full card".to_string()
            };
            ui::plain(canvas, &a.font, &hint, vec2(640.0, if n > 9 { 92.0 } else { 700.0 }), 13.0, SOFT, Align::Center);
        }
        if let Some(i) = self.inspect {
            ui::dim(canvas, 0.75);
            self.draw_country_card(canvas, &self.rivals[i], usize::MAX, false);
            INSPECT_BACK.draw(canvas, a, self.mouse, self.time, false);
        }
    }

    fn draw_big_rival(&self, canvas: &mut Canvas, r: &Country, k: usize, rect: Rect, y: f32, hover: bool) {
        let a = &self.assets;
        let card = if hover { &a.card_rival_you } else { &a.card_rival };
        canvas.blit_sprite(card, vec2(rect.x, y), Blit::default());
        country::draw_flag(canvas, Rect::new(rect.x + 22.0, y + 24.0, 84.0, 56.0), &r.flag);
        ui::text(canvas, &a.serif, &r.name, vec2(rect.x + 120.0, y + 20.0), 21.0, Color::WHITE, Align::Left, 238.0);
        ui::plain(canvas, &a.font, &r.label(&self.data, "continent"), vec2(rect.x + 120.0, y + 50.0), 14.0, MUTED, Align::Left);
        let line1 = format!("{} · {} leader", r.label(&self.data, "ideology"), r.label(&self.data, "leader"));
        ui::plain(canvas, &a.font, &line1, vec2(rect.x + 22.0, y + 92.0), 15.0, Color::WHITE, Align::Left);
        let line2 = format!(
            "{} people · GDP {}",
            country::people(r.value(&self.data, "population")),
            country::money(r.value(&self.data, "gdp"))
        );
        ui::plain(canvas, &a.font, &line2, vec2(rect.x + 22.0, y + 114.0), 15.0, MUTED, Align::Left);
        let army = format!("Army: {}", r.label(&self.data, "army"));
        ui::plain(canvas, &a.font, &army, vec2(rect.x + 22.0, y + 136.0), 15.0, MUTED, Align::Left);
        let fate_id = r.option_id(&self.data, "fate");
        let fate_w = self.data.wheel("fate");
        let curse = self.data.wheels[fate_w].options.iter().any(|o| o.id == fate_id && o.tags.iter().any(|t| t == "curse"));
        ui::plain(canvas, &a.font, &r.label(&self.data, "fate"), vec2(rect.x + 378.0, y + 136.0), 15.0, if curse { ui::RED } else { ui::GREEN }, Align::Right);
        ui::plain(canvas, &a.font, &format!("{}", k + 1), vec2(rect.x + 378.0, y + 18.0), 13.0, SOFT, Align::Right);
        canvas.label(&format!("rival {}", k + 1), rect);
    }
}

