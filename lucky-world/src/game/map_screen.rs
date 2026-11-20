//! The world map screen: the 3D map under the HUD (`hud.rs`).
//!
//! Everything can be done with the mouse; keys are shortcuts (shown in the
//! tooltips, changeable in Settings):
//!
//! | | |
//! |---|---|
//! | Pan | drag with the left button; the screen's edges (in fullscreen); WASD / arrows |
//! | Turn and tilt | drag with the right or middle button; Q / E, T / G |
//! | Zoom | the mouse wheel, towards the pointer; the + and − beside the minimap; R / F |
//! | Jump | click or drag the minimap; double-click a nation to centre on it |
//! | Select | click a province; click a nation's name for its country card |
//! | Menu | right-click a nation or province |
//! | Esc | closes what is open; with nothing open, the pause menu |

use duckforge::prelude::*;

use super::cmd::Cmd;
use super::econ_screen::EconUi;
use super::gui::{self, Act, Space};
use super::hud::{Menu, BAR_H, MINIMAP_AT};
use super::{Lucky, INSPECT_BACK};
use crate::country;
use crate::map_view::Mode;
use crate::settings::Act as KeyAct;
use crate::ui;

/// A nation's name as laid out this frame: (nation, its chip, text size, flag foot and scale).
type Label = (usize, Rect, f32, Option<(Vec2, f32)>);

/// The country card's frame (clicking outside it closes it).
const CARD: Rect = Rect { x: 30.0, y: 14.0, w: 1220.0, h: 690.0 };

/// What the map screen remembers between frames.
#[derive(Default)]
pub struct MapUi {
    pub selected: Option<usize>,
    pub hover: Option<usize>,
    /// The nation whose card is open.
    pub card: Option<usize>,
    /// Spectator mode: the nation whose brain panel is open.
    pub brain: Option<usize>,
    pub ambient: Option<LoopId>,
    /// Where the left button went down on the map (None: on the HUD).
    press_at: Option<Vec2>,
    last_mouse: Option<Vec2>,
    pub dragging: bool,
    /// Turning with the right or middle button: where it went down, the
    /// last place, and how far it has moved (a still right-click is a menu).
    turn_from: Option<Vec2>,
    turn_last: Option<Vec2>,
    turn_moved: f32,
    on_minimap: bool,
    next_nation: usize,
    /// Names and flags as laid out this frame.
    labels: Vec<Label>,
    pub(super) camera: Option<Camera3D>,
    /// Where the camera is gliding to (a visit, a battle); any pan stops it.
    pub glide: Option<Vec2>,
    /// The zoom the + and − buttons are easing towards.
    pub zoom_goal: Option<f32>,
    last_tick: f32,
    /// The right-click menu.
    pub menu: Option<Menu>,
    /// The selected province's details card is open.
    pub details: bool,
    /// Where the news strip's event happened (a ground point), for "Show".
    pub news_at: Option<Vec2>,
    /// The mouse is over the HUD (the map under it ignores it).
    pub over_ui: bool,
    /// News-log entries and spy reports already looked at (the badges count the rest).
    pub log_seen: usize,
    pub spy_seen_day: u32,
    /// The economy window and overlays.
    pub econ: EconUi,
    /// The war room, battle markers and reports.
    pub war: super::war_screen::WarUi,
    /// The diplomacy and spy windows.
    pub diplo: super::diplo_screen::DiploUi,
}

impl MapUi {
    /// Esc has something to close here (before it opens the pause menu).
    pub fn wants_escape(&self) -> bool {
        self.card.is_some() || self.selected.is_some() || self.menu.is_some() || self.details || self.econ.open.is_some() || self.brain.is_some() || self.war.wants_escape() || self.diplo.wants_escape()
    }

    /// A window covers the map.
    pub fn window_open(&self) -> bool {
        self.econ.open.is_some() || self.war.open.is_some() || self.diplo.wants_escape()
    }
}

impl Lucky {
    pub(super) fn enter_map(&mut self, ctx: &mut Context) {
        crate::ui::sfx(ctx, &self.assets.open_map, 0.9, 0.0);
        if self.map_ui.ambient.is_none() {
            self.map_ui.ambient = Some(ctx.play_loop(&self.assets.world_ambience, 0.55 * ui::effects_volume()));
        }
        self.inspect = None;
        // The rival reveal's confetti stays behind.
        self.particles.clear();
        // The game starts over the player's own nation, near enough to see it whole.
        if !self.spectating {
            self.frame_nation(0);
        }
        self.map_ui.log_seen = self.notes.log.len();
        self.go(super::Screen::Map);
    }

    pub(super) fn open_card(&mut self, ctx: &mut Context, nation: usize) {
        self.map_ui.card = Some(nation);
        crate::ui::sfx(ctx, &self.assets.card_sound, 0.8, 0.0);
    }

    pub(super) fn select_province(&mut self, ctx: &mut Context, province: Option<usize>) {
        if self.map_ui.selected != province {
            self.map_ui.selected = province;
            if province.is_none() {
                self.map_ui.details = false;
            }
            let (Some(view), Some(world)) = (self.map_view.as_mut(), self.world.as_ref()) else { return };
            view.select(&self.atlas, world, province);
            if province.is_some() {
                crate::ui::sfx(ctx, &self.assets.select, 0.8, 0.0);
            }
        }
    }

    /// Moves the camera over a nation's capital and selects it.
    fn visit(&mut self, ctx: &mut Context, nation: usize) {
        let Some(world) = self.world.as_ref() else { return };
        let capital = world.capital[nation];
        self.centre_on(nation);
        self.select_province(ctx, Some(capital));
    }

    /// A screen point where a click lands on one of this nation's
    /// provinces (and not on a name label or the HUD): for the bot, which
    /// plays with the mouse as a person would.
    pub fn clickable_province(&self, nation: usize) -> Option<(usize, Vec2)> {
        let (view, world) = (self.map_view.as_ref()?, self.world.as_ref()?);
        let camera = view.orbit.camera();
        let map = self.atlas.map(world.map);
        let s = self.settings.ui_scale;
        let mut hud = self.hud_widgets();
        hud.extend(self.details_widgets());
        let mut best: Option<(usize, Vec2, f32)> = None;
        for p in (0..world.owner.len()).filter(|&p| world.owner[p] == Some(nation)) {
            let Some((at, _)) = camera.project(view.province_point(&self.atlas, world, p, 0.0), &self.projector) else { continue };
            if !(260.0..1100.0).contains(&at.x) || !(80.0..600.0).contains(&at.y) || gui::over(&hud, s, Some(at)) {
                continue;
            }
            if self.map_ui.labels.iter().any(|(_, rect, ..)| rect.contains(at) || rect.center().distance(at) < 60.0) {
                continue;
            }
            let ray = camera.ray_through(at, &self.projector);
            if view.pick(map, ray.origin, ray.direction) != Some(p) {
                continue;
            }
            let d = at.distance(vec2(ui::W / 2.0, ui::H / 2.0));
            if best.is_none_or(|b| d < b.2) {
                best = Some((p, at, d));
            }
        }
        best.map(|(p, at, _)| (p, at))
    }

    /// The ground point under a screen point, with the camera as it is now.
    fn ground_under(&self, p: Vec2) -> Option<Vec2> {
        let view = self.map_view.as_ref()?;
        let ray = view.orbit.camera().ray_through(p, &self.projector);
        view.ground(ray.origin, ray.direction)
    }

    /// Keeps the camera over the map: how near and far it may go, and the
    /// middle of the screen never leaves the land's own rectangle, so at
    /// most a margin of open sea shows at the map's edges.
    fn clamp_camera(&mut self) {
        let Some(view) = self.map_view.as_mut() else { return };
        view.clamp_target();
        // The map's corners on the ground (from the minimap's corners).
        let mm = view.minimap();
        let (w, h) = (mm.width() as f32, mm.height() as f32);
        let a = view.minimap_to_ground(vec2(0.0, 0.0), Vec2::ZERO);
        let b = view.minimap_to_ground(vec2(w, h), Vec2::ZERO);
        let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
        let (z0, z1) = (a.y.min(b.y), a.y.max(b.y));
        // Zoomed right out, the whole map just fits: no further.
        let far = ((x1 - x0).max((z1 - z0) * 1.6) * 0.75).clamp(14.0, 48.0);
        view.orbit.distance = view.orbit.distance.clamp(4.5, far);
        // The nearer the camera, the nearer the edge it may look; from far
        // away it stays towards the middle, so the map fills the screen.
        let reach = (view.orbit.distance * 0.55).min((x1 - x0) / 2.0).min((z1 - z0) / 2.0);
        let margin = 2.0;
        let clamp = |v: f32, lo: f32, hi: f32| if lo > hi { (lo + hi) / 2.0 } else { v.clamp(lo, hi) };
        view.orbit.target.x = clamp(view.orbit.target.x, x0 + reach - margin, x1 - reach + margin);
        view.orbit.target.y = clamp(view.orbit.target.y, z0 + reach - margin, z1 - reach + margin);
    }

    pub(super) fn update_map(&mut self, ctx: &mut Context) {
        ctx.idle_ok();
        let dt = ctx.dt();
        let mouse = self.mouse;
        let s = self.settings.ui_scale;
        let left = ctx.input.was_mouse_pressed(MouseButton::Left);

        // The news log covers everything: only closing it.
        if self.notes.log_open {
            let ws = self.log_widgets();
            let outside = left && !gui::over(&ws, s, mouse);
            let _ = self.clicked(ctx, &ws);
            if gui::pressed(ctx, Key::Escape) || gui::pressed(ctx, self.settings.key(KeyAct::Log)) || outside {
                self.notes.log_open = false;
                crate::ui::sfx(ctx, &self.assets.click, 0.6, 0.0);
            }
            self.notes.log_scroll = (self.notes.log_scroll as f32 - ctx.input.scroll() * 3.0).clamp(0.0, self.notes.log.len().saturating_sub(10) as f32) as usize;
            self.report_map(ctx);
            return;
        }
        // The days go by (a card or a window doesn't stop them).
        self.update_clock(ctx);
        if self.screen != super::Screen::Map {
            // A result came in (victory or defeat): its screen is up.
            return;
        }

        // A country card is open: only closing it (its button, Esc, or a click outside it).
        if self.map_ui.card.is_some() {
            let outside = left && self.ui_mouse().is_some_and(|m| !CARD.contains(m));
            if gui::pressed(ctx, Key::Escape)
                || gui::pressed(ctx, Key::Space)
                || gui::pressed(ctx, Key::Enter)
                || gui::pressed(ctx, self.settings.key(KeyAct::Card))
                || self.mouse_clicked(ctx, &INSPECT_BACK)
                || outside
            {
                self.map_ui.card = None;
                crate::ui::sfx(ctx, &self.assets.click, 0.7, 0.0);
            }
            self.report_map(ctx);
            return;
        }

        // The clock's pause works under any window.
        if gui::pressed(ctx, self.settings.key(KeyAct::Pause)) {
            self.clock.paused = !self.clock.paused;
            crate::ui::sfx(ctx, &self.assets.click, 0.7, 0.0);
        }
        // A battle report opened in full: only closing it.
        if self.map_ui.war.full.is_some() {
            if gui::pressed(ctx, Key::Escape) || gui::pressed(ctx, Key::Enter) || left {
                self.map_ui.war.full = None;
                crate::ui::sfx(ctx, &self.assets.click, 0.7, 0.0);
            }
            self.report_map(ctx);
            return;
        }
        if self.map_ui.econ.open.is_some() {
            self.update_econ_window(ctx);
            self.report_map(ctx);
            return;
        }
        if self.map_ui.war.open.is_some() {
            self.update_war_room(ctx);
            self.report_map(ctx);
            return;
        }
        if self.map_ui.diplo.open.is_some() {
            self.update_diplo(ctx);
            self.report_map(ctx);
            return;
        }
        if self.map_ui.diplo.spies.is_some() {
            self.update_spies(ctx);
            self.report_map(ctx);
            return;
        }

        // The right-click menu: a line, or anything else closes it.
        if self.map_ui.menu.is_some() {
            let ws = self.menu_widgets();
            let any = left || ctx.input.was_mouse_pressed(MouseButton::Right);
            match self.clicked(ctx, &ws) {
                Some(Act::Cmd(cmd)) => {
                    self.map_ui.menu = None;
                    self.run(ctx, cmd);
                }
                Some(_) => {}
                None if any || gui::pressed(ctx, Key::Escape) => self.map_ui.menu = None,
                None => {}
            }
            self.report_map(ctx);
            return;
        }

        // The HUD: its buttons either run a command or press their key.
        let mut hud = self.hud_widgets();
        hud.extend(self.details_widgets());
        self.map_ui.over_ui = gui::over(&hud, s, mouse);
        if let Some(Act::Cmd(cmd)) = self.clicked(ctx, &hud) {
            self.run(ctx, cmd);
            self.report_map(ctx);
            return;
        }
        let over_ui = self.map_ui.over_ui;

        // The windows' keys.
        let (k_diplo, k_spies) = (self.settings.key(KeyAct::Diplomacy), self.settings.key(KeyAct::Spies));
        if self.diplo.is_some() && (gui::pressed(ctx, k_diplo) || gui::pressed(ctx, k_spies)) {
            let spies = gui::pressed(ctx, k_spies);
            self.close_windows();
            self.open_diplo(ctx, spies);
            self.report_map(ctx);
            return;
        }
        // The last battle's card fades; Enter (or a click on it) opens it.
        if let Some((k, left_s)) = self.map_ui.war.card {
            let left_s = left_s - dt;
            self.map_ui.war.card = if left_s > 0.0 { Some((k, left_s)) } else { None };
            if gui::pressed(ctx, Key::Enter) {
                self.run(ctx, Cmd::Report(k));
            }
        }
        if self.wars.is_some() && gui::pressed(ctx, self.settings.key(KeyAct::WarRoom)) {
            self.run(ctx, Cmd::War(super::war_screen::Page::Wars));
            self.report_map(ctx);
            return;
        }
        if gui::pressed(ctx, self.settings.key(KeyAct::Battle)) {
            if let Some(b) = self.biggest_battle() {
                self.watch_battle(ctx, b);
            }
        }
        if self.economy.is_some() && gui::pressed(ctx, self.settings.key(KeyAct::Economy)) {
            self.run(ctx, Cmd::Econ(super::econ_screen::Tab::Budget, 0));
            self.report_map(ctx);
            return;
        }
        if gui::pressed(ctx, self.settings.key(KeyAct::Overlay)) {
            self.cycle_overlay(ctx);
        }
        // Quick save and load, help, the news log.
        if !self.spectating && gui::pressed(ctx, self.settings.key(KeyAct::QuickSave)) {
            let path = crate::save::path_for("Quick save");
            self.save_game(ctx, "Quick save", &path);
        }
        if gui::pressed(ctx, self.settings.key(KeyAct::QuickLoad)) {
            let path = crate::save::path_for("Quick save");
            if path.exists() {
                self.load_from(ctx, &path);
            } else {
                self.notify(ctx, "There's no quick save yet (F6 makes one)".to_string(), super::shell::IMPORTANT);
            }
            return;
        }
        if gui::pressed(ctx, self.settings.key(KeyAct::Help)) {
            self.open_pause(ctx);
            self.open_sub(super::Screen::Help);
            return;
        }
        if gui::pressed(ctx, self.settings.key(KeyAct::Log)) {
            self.notes.log_open = true;
            self.notes.log_scroll = 0;
            self.map_ui.log_seen = self.notes.log.len();
            crate::ui::sfx(ctx, &self.assets.card_sound, 0.6, 0.0);
            return;
        }

        // Esc: closes what is open on the map; with nothing open, the pause menu.
        let nations = self.world.as_ref().map_or(0, |w| w.nations.len());
        if gui::pressed(ctx, Key::Escape) {
            let had = self.map_ui.selected.is_some() || self.map_ui.brain.is_some() || self.map_ui.war.watching.is_some() || self.map_ui.details;
            if self.map_ui.details {
                self.map_ui.details = false;
            } else {
                self.select_province(ctx, None);
                self.map_ui.brain = None;
                self.map_ui.war.watching = None;
            }
            if !had {
                self.open_pause(ctx);
                self.report_map(ctx);
                return;
            }
        }
        if gui::pressed(ctx, self.settings.key(KeyAct::MapMode)) {
            let next = if self.map_view.as_ref().is_some_and(|v| v.mode == Mode::Political) { super::cmd::View::Terrain } else { super::cmd::View::Nations };
            self.run(ctx, Cmd::View(next));
        }
        if gui::pressed(ctx, self.settings.key(KeyAct::Home)) {
            if self.spectating {
                self.visit(ctx, 0);
            } else {
                self.centre_on(0);
                crate::ui::sfx(ctx, &self.assets.select, 0.6, 0.0);
            }
        }
        if gui::pressed(ctx, self.settings.key(KeyAct::NextNation)) && nations > 0 {
            self.map_ui.next_nation = (self.map_ui.next_nation + 1) % nations;
            let n = self.map_ui.next_nation;
            self.visit(ctx, n);
            if self.spectating {
                self.map_ui.brain = Some(n);
            }
        }
        if gui::pressed(ctx, self.settings.key(KeyAct::Card)) {
            let owner = self.map_ui.selected.and_then(|p| self.world.as_ref().and_then(|w| w.owner[p])).unwrap_or(0);
            self.open_card(ctx, owner);
            self.report_map(ctx);
            return;
        }

        // ---- The camera.
        let minimap = self.minimap_rect();
        let mm_mouse = mouse.map(|m| Space::BottomRight.to_local(s, m));
        let edge = self.settings.edge_scroll && self.settings.fullscreen;
        let settings = &self.settings;
        let k = |a: KeyAct| settings.key(a);
        let mut ax = ctx.input.axis(k(KeyAct::PanLeft), k(KeyAct::PanRight)) + ctx.input.axis(Key::Left, Key::Right);
        let mut az = ctx.input.axis(k(KeyAct::PanDown), k(KeyAct::PanUp)) + ctx.input.axis(Key::Down, Key::Up);
        // The screen's edges push the map along (fullscreen only: in a
        // window the pointer rests on the edge on its way out).
        if let (true, Some(m)) = (edge, ctx.input.mouse_position()) {
            let e = 3.0;
            ax += f32::from(u8::from(m.x >= ui::W - e)) - f32::from(u8::from(m.x <= e));
            az += f32::from(u8::from(m.y <= e)) - f32::from(u8::from(m.y >= ui::H - e));
        }
        let turn = ctx.input.axis(k(KeyAct::TurnLeft), k(KeyAct::TurnRight));
        let tilt = ctx.input.axis(k(KeyAct::TiltUp), k(KeyAct::TiltDown));
        let zoom_keys = ctx.input.axis(k(KeyAct::ZoomIn), k(KeyAct::ZoomOut));
        let scroll = if over_ui { 0.0 } else { ctx.input.scroll() };
        // The wheel zooms towards the pointer: the ground under it stays under it.
        let anchor = if scroll != 0.0 { mouse.and_then(|m| self.ground_under(m).map(|g| (m, g))) } else { None };
        {
            let Some(view) = self.map_view.as_mut() else { return };
            let orbit = &mut view.orbit;
            let (sy, cy) = orbit.yaw.sin_cos();
            let forward = vec2(sy, cy);
            let right = vec2(cy, -sy);
            let pan = orbit.distance * 0.9 * dt;
            orbit.target = orbit.target + right * (ax.clamp(-1.0, 1.0) * pan) + forward * (az.clamp(-1.0, 1.0) * pan);
            if ax != 0.0 || az != 0.0 {
                self.map_ui.glide = None;
            }
            orbit.yaw += turn * 1.3 * dt;
            orbit.pitch += tilt * 0.8 * dt;
            orbit.distance *= (zoom_keys * 1.4 * dt).exp();
            if zoom_keys != 0.0 {
                self.map_ui.zoom_goal = None;
            }
            if scroll != 0.0 {
                orbit.distance *= 0.87f32.powf(scroll);
                self.map_ui.zoom_goal = None;
                if self.time - self.map_ui.last_tick > 0.08 {
                    crate::ui::sfx(ctx, &self.assets.pan_tick, 0.5, 0.0);
                    self.map_ui.last_tick = self.time;
                }
            }
            // The + and − buttons ease the zoom to its goal.
            if let Some(goal) = self.map_ui.zoom_goal {
                let t = 1.0 - (-9.0 * dt).exp();
                orbit.distance += (goal - orbit.distance) * t;
                if (orbit.distance - goal).abs() < 0.05 {
                    self.map_ui.zoom_goal = None;
                }
            }
            // A glide eases the camera to its goal.
            if let Some(goal) = self.map_ui.glide {
                let t = 1.0 - (-7.0 * dt).exp();
                orbit.target = orbit.target + (goal - orbit.target) * t;
                if orbit.target.distance(goal) < 0.01 {
                    orbit.target = goal;
                    self.map_ui.glide = None;
                }
            }
        }
        self.clamp_camera();
        if let Some((m, before)) = anchor {
            if let Some(after) = self.ground_under(m) {
                if let Some(view) = self.map_view.as_mut() {
                    view.orbit.target += before - after;
                }
                self.clamp_camera();
            }
        }

        // The mouse on the map: drag to pan (the ground under the pointer
        // stays under it), right- or middle-drag to turn and tilt, click to
        // select, right-click for the menu.
        if left {
            self.map_ui.on_minimap = mm_mouse.is_some_and(|m| minimap.contains(m));
            self.map_ui.press_at = if over_ui && !self.map_ui.on_minimap { None } else { mouse };
            self.map_ui.dragging = false;
        }
        if ctx.input.is_mouse_down(MouseButton::Left) {
            if let (Some(m), Some(from)) = (mouse, self.map_ui.press_at) {
                if self.map_ui.on_minimap {
                    // Click or drag the minimap: the camera goes there.
                    if let (Some(view), Some(mm)) = (self.map_view.as_mut(), mm_mouse) {
                        let at = vec2(mm.x.clamp(minimap.left(), minimap.right()), mm.y.clamp(minimap.top(), minimap.bottom()));
                        view.orbit.target = view.minimap_to_ground(at, MINIMAP_AT);
                        self.map_ui.glide = None;
                    }
                } else if self.map_ui.dragging || m.distance(from) > 5.0 {
                    self.map_ui.dragging = true;
                    self.map_ui.glide = None;
                    if let Some(last) = self.map_ui.last_mouse {
                        if let (Some(a), Some(b)) = (self.ground_under(last), self.ground_under(m)) {
                            if let Some(view) = self.map_view.as_mut() {
                                view.orbit.target += a - b;
                            }
                        }
                    }
                }
            }
        }
        let mut clicked = None;
        if ctx.input.was_mouse_released(MouseButton::Left) {
            if !self.map_ui.dragging && !self.map_ui.on_minimap && self.map_ui.press_at.is_some() {
                clicked = mouse;
            }
            self.map_ui.press_at = None;
            self.map_ui.dragging = false;
            self.map_ui.on_minimap = false;
        }
        let turning = ctx.input.is_mouse_down(MouseButton::Right) || ctx.input.is_mouse_down(MouseButton::Middle);
        let mut menu_at = None;
        if ctx.input.was_mouse_pressed(MouseButton::Right) || ctx.input.was_mouse_pressed(MouseButton::Middle) {
            self.map_ui.turn_from = if over_ui { None } else { mouse };
            self.map_ui.turn_last = mouse;
            self.map_ui.turn_moved = 0.0;
        }
        if turning && self.map_ui.turn_from.is_some() {
            if let (Some(m), Some(last), Some(view)) = (mouse, self.map_ui.turn_last, self.map_view.as_mut()) {
                self.map_ui.turn_moved += m.distance(last);
                if self.map_ui.turn_moved > 4.0 {
                    view.orbit.yaw += (m.x - last.x) * 0.008;
                    view.orbit.pitch += (m.y - last.y) * 0.005;
                }
            }
            self.map_ui.turn_last = mouse;
        }
        if ctx.input.was_mouse_released(MouseButton::Right) {
            // A right-click that didn't drag: the menu.
            if self.map_ui.turn_from.is_some() && self.map_ui.turn_moved <= 4.0 {
                menu_at = mouse;
            }
            self.map_ui.turn_from = None;
        }
        if !turning {
            self.map_ui.turn_from = None;
        }
        self.map_ui.last_mouse = mouse;
        self.clamp_camera();

        // What's under the mouse.
        let Some(view) = self.map_view.as_ref() else { return };
        let camera = view.orbit.camera();
        let map = self.atlas.map(self.world.as_ref().map_or(0, |w| w.map));
        self.map_ui.hover = mouse.filter(|_| !over_ui).and_then(|m| {
            let ray = camera.ray_through(m, &self.projector);
            view.pick(map, ray.origin, ray.direction)
        });
        self.map_ui.camera = Some(camera);
        self.layout_labels();
        self.layout_battles();
        let label_at = |g: &Lucky, at: Vec2| g.map_ui.labels.iter().find(|(_, rect, _, flag)| rect.contains(at) || flag.is_some_and(|(foot, k)| Rect::new(foot.x - 2.0, foot.y - 46.0 * k, 44.0 * k, 32.0 * k).contains(at))).map(|(n, ..)| *n);

        // A right-click: the menu for the nation or province under it.
        if let Some(at) = menu_at {
            let province = self.map_ui.hover;
            let nation = label_at(self, at).or_else(|| province.and_then(|p| self.world.as_ref().and_then(|w| w.owner[p])));
            if nation.is_some() || province.is_some() {
                if let Some(p) = province {
                    self.select_province(ctx, Some(p));
                }
                self.map_ui.menu = Some(Menu { at, nation, province });
                crate::ui::sfx(ctx, &self.assets.click, 0.7, 0.0);
            }
        }
        // A battle marker: watch that battle.
        if let Some(b) = clicked.and_then(|at| self.battle_at(at)) {
            self.watch_battle(ctx, b);
            clicked = None;
        }
        if let Some(at) = clicked {
            let twice = self.pointer.double(at, self.time);
            let on_label = label_at(self, at);
            let on_panel = self.map_ui.war.watching.is_some() && super::war_screen::BATTLE_PANEL.contains(at) || self.map_ui.brain.is_some() && super::brain_panel::BRAIN_PANEL.contains(at);
            let hit = self.map_ui.hover;
            let owner = hit.and_then(|p| self.world.as_ref().and_then(|w| w.owner[p]));
            if let Some(n) = on_label {
                if twice {
                    self.map_ui.card = None;
                    self.centre_on(n);
                } else if self.spectating {
                    // Watching, a name opens its brain; playing, its card.
                    self.map_ui.brain = Some(n);
                    crate::ui::sfx(ctx, &self.assets.card_sound, 0.8, 0.0);
                } else {
                    self.open_card(ctx, n);
                }
            } else if !on_panel {
                self.select_province(ctx, hit);
                if self.spectating {
                    if let Some(o) = owner {
                        self.map_ui.brain = Some(o);
                    }
                }
                // A double click on a nation's land centres the map on it.
                if let (true, Some(o)) = (twice, owner) {
                    self.centre_on(o);
                }
            }
        }
        self.report_map(ctx);
    }

    /// Lays out the nations' names (biggest first, none on top of another
    /// or under the HUD) and their capitals' flags.
    fn layout_labels(&mut self) {
        let (Some(view), Some(world), Some(camera)) = (self.map_view.as_ref(), self.world.as_ref(), self.map_ui.camera.as_ref()) else {
            return;
        };
        let map = self.atlas.map(world.map);
        let hud: Vec<Rect> = self.hud_rects().into_iter().map(|(_, r)| r).collect();
        let mut order: Vec<(usize, Vec3, usize)> = view.label_points.iter().map(|&(n, p)| (n, p, world.provinces_of(n).iter().map(|&q| map.provinces[q].cells.len()).sum())).collect();
        let me = if self.spectating { usize::MAX } else { 0 };
        order.sort_by_key(|&(n, _, cells)| (n != me, std::cmp::Reverse(cells)));
        let mut placed: Vec<Label> = Vec::new();
        for (n, point, cells) in order {
            let Some((at, depth)) = camera.project(point, &self.projector) else { continue };
            let size = ((cells as f32).sqrt() * 30.0 / depth.max(1.0)).clamp(11.0, 22.0);
            let name = &world.nations[n].name;
            let style = TextStyle { size, ..TextStyle::default() };
            let measured = self.assets.serif.measure(name, &style);
            // The chip: the colour square, the name, a little air.
            let (pad, chip) = (5.0, size * 0.62);
            let w = measured.x + chip + pad * 3.0;
            let h = size + pad * 1.2;
            let rect = Rect::new(at.x - w / 2.0, at.y - h / 2.0, w, h);
            let on_screen = rect.x > 4.0 && rect.right() < ui::W - 4.0 && rect.y > BAR_H && rect.bottom() < ui::H - 4.0;
            let clash = placed.iter().any(|(_, r, ..)| r.expand(3.0).overlaps(&rect)) || hud.iter().any(|r| r.overlaps(&rect));
            let flag = camera.project(view.capital_tops[n], &self.projector).map(|(foot, d)| (foot, (14.0 / d.max(1.0)).clamp(0.45, 1.1)));
            let rect = if on_screen && !clash { rect } else { Rect::new(-1000.0, -1000.0, 0.0, 0.0) };
            placed.push((n, rect, size, flag));
        }
        self.map_ui.labels = placed;
    }

    pub(super) fn report_map(&self, ctx: &mut Context) {
        let Some(view) = &self.map_view else { return };
        ctx.report("map_mode", format!("{:?}", view.mode));
        ctx.report("camera_x", (view.orbit.target.x * 10.0).round() / 10.0);
        ctx.report("camera_z", (view.orbit.target.y * 10.0).round() / 10.0);
        ctx.report("camera_zoom", (view.orbit.distance * 10.0).round() / 10.0);
        ctx.report("camera_yaw", (view.orbit.yaw * 100.0).round() / 100.0);
        let map = self.atlas.map(self.world.as_ref().map_or(0, |w| w.map));
        let selected = self.map_ui.selected.map_or("none", |p| map.provinces[p].id.as_str());
        ctx.report("selected", selected);
        ctx.report("card", self.map_ui.card.map_or("none".to_string(), |n| self.world.as_ref().map_or(String::new(), |w| w.nations[n].name.clone())));
        ctx.report("spectating", self.spectating);
        ctx.report("menu_open", self.map_ui.menu.is_some());
        ctx.report("view", self.view_now().map_or("other", |v| v.name()));
        ctx.report("needs", self.needs().len());
        self.report_war(ctx);
        self.report_diplo(ctx);
        if let (Some(n), Some(b)) = (self.map_ui.brain, self.brains.as_ref()) {
            if let Some(brain) = b.brain(n) {
                ctx.report("brain_nation", n);
                ctx.report("brain_goal", crate::ai::goal_name(brain.goal));
                ctx.report("brain_options", brain.thinking.options.len());
            }
        }
    }

    pub(super) fn draw_map(&self, canvas: &mut Canvas) {
        let (Some(view), Some(world)) = (self.map_view.as_ref(), self.world.as_ref()) else { return };
        let a = &self.assets;
        let camera = self.map_ui.camera.unwrap_or_else(|| view.orbit.camera());
        let map = self.atlas.map(world.map);
        view.draw(canvas, &camera);
        let window = self.map_ui.window_open();
        let modal = window || self.map_ui.card.is_some() || self.map_ui.war.full.is_some() || self.notes.log_open || self.shell.pause || self.shell.confirm.is_some() || self.screen != super::Screen::Map;

        // Flags on the capitals, then each nation's name on its dark chip.
        for &(n, _, _, flag) in &self.map_ui.labels {
            let Some((foot, k)) = flag else { continue };
            if foot.x < -50.0 || foot.x > ui::W + 50.0 || foot.y < BAR_H || foot.y > ui::H + 50.0 {
                continue;
            }
            canvas.draw_placed(&a.flag_pole, foot, Placed { pivot: vec2(4.0, 64.0), scale: vec2(k, k * 0.75), ..Placed::default() });
            let r = Rect::new(foot.x + 2.0 * k, foot.y - 46.0 * k, 40.0 * k, 26.0 * k);
            country::draw_flag(canvas, r, &world.nations[n].flag);
            canvas.draw_placed(&a.flag_cloth, vec2(r.x, r.y), Placed { pivot: Vec2::ZERO, scale: vec2(k, k), ..Placed::default() });
            if r.x > 0.0 && r.right() < ui::W && r.bottom() < ui::H {
                canvas.label(&format!("flag {}", world.nations[n].name), r);
            }
        }
        let me = if self.spectating { usize::MAX } else { 0 };
        for &(n, rect, size, _) in &self.map_ui.labels {
            if rect.x < -500.0 {
                continue;
            }
            let hot = !modal && !self.map_ui.over_ui && self.mouse.is_some_and(|m| rect.contains(m));
            gui::chip(canvas, a, rect);
            if n == me || hot {
                canvas.draw_rect(rect, gui::GOLD);
            }
            let chip = size * 0.62;
            let c = Rect::new(rect.x + 5.0, rect.center().y - chip / 2.0, chip, chip);
            canvas.fill_rect(c, world.color[n]);
            canvas.draw_rect(c, Color::rgba(0, 0, 0, 120));
            let style = TextStyle { size, color: if n == me { gui::GOLD } else { gui::IVORY }, ..TextStyle::default() };
            canvas.draw_font_text(&a.serif, &world.nations[n].name, vec2(c.right() + 5.0, rect.y + (rect.h - size) / 2.0 - 1.0), &style);
            canvas.label(&format!("nation {}", world.nations[n].name), rect);
        }

        // Battles going on, and the one being watched.
        self.draw_war_map(canvas);

        // The province under the mouse.
        if let (Some(p), Some(m), false) = (self.map_ui.hover, self.mouse, modal || self.map_ui.menu.is_some()) {
            let prov = &map.provinces[p];
            let owner = world.owner[p].map_or("Unclaimed".to_string(), |o| world.nations[o].name.clone());
            let words = format!("{}  ·  {}", prov.name, owner);
            let w = a.font.measure(&words, &TextStyle { size: 14.0, ..TextStyle::default() }).x + 16.0;
            let at = vec2((m.x + 16.0).min(ui::W - w - 4.0), (m.y + 18.0).min(ui::H - 30.0));
            gui::chip(canvas, a, Rect::new(at.x, at.y, w, 24.0));
            ui::plain(canvas, &a.font, &words, at + vec2(8.0, 4.0), 14.0, gui::IVORY, Align::Left);
        }

        // Watching, a nation's brain panel.
        if let Some(n) = self.map_ui.brain.filter(|_| !window) {
            self.draw_brain_panel(canvas, n);
        }
        self.draw_overlay_legend(canvas);

        // The HUD, then whatever is open on top of it.
        self.draw_hud(canvas, !modal && self.map_ui.menu.is_none());
        self.draw_details(canvas, !modal && self.map_ui.menu.is_none());
        self.draw_menu_popup(canvas);

        // A window covers the map: the map behind it is dimmed, and it is drawn at the UI scale.
        if window {
            ui::dim(canvas, 0.6);
            self.push_ui(canvas);
        }
        self.draw_econ_window(canvas);
        self.draw_war_room(canvas);
        self.draw_diplo(canvas);
        self.draw_spies(canvas);
        if window {
            self.pop_ui(canvas);
        }
        self.draw_battle_reports(canvas);
        if let Some(n) = self.map_ui.card {
            ui::dim(canvas, 0.75);
            self.draw_country_card(canvas, &world.nations[n], usize::MAX, n == 0);
            self.button(canvas, &INSPECT_BACK, false);
        }
    }
}
