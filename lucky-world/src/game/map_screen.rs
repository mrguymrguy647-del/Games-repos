//! The world map screen: look round the 3D map, click provinces and nations.
//!
//! | | |
//! |---|---|
//! | Pan | drag with the left button, or WASD / arrow keys |
//! | Rotate | drag with the right button, or Q / E |
//! | Zoom | mouse wheel, or R / F (T / G tilts) |
//! | Select | click a province; click a name or flag for that nation's card |
//! | Keys | Tab: next nation · H: home · M: terrain / political · V: economy overlays · C: card · Esc: back |
//! | Economy | Space: pause · 1-5: speed · O: the economy window (see `econ_screen`) |

use duckforge::prelude::*;

use super::econ_screen::{EconUi, ECON_BUTTON};
use super::{Lucky, INSPECT_BACK};
use crate::country;
use crate::map_view::Mode;
use crate::ui::{self, Button, ButtonKind, GOLD, MUTED, SOFT};

const CARD_BUTTON: Button = Button::new(66.0, 356.0, "COUNTRY CARD", "C", ButtonKind::Gold);
/// The minimap frame's top-left corner; the map sits inside it.
const MINIMAP_FRAME: Vec2 = Vec2 { x: 976.0, y: 532.0 };
const MINIMAP_AT: Vec2 = Vec2 { x: 985.0, y: 545.0 };
const PANEL_AT: Vec2 = Vec2 { x: 14.0, y: 70.0 };

/// A nation's name as laid out this frame: (nation, name rect, name size, flag foot and scale).
type Label = (usize, Rect, f32, Option<(Vec2, f32)>);

/// What the map screen remembers between frames.
#[derive(Default)]
pub struct MapUi {
    pub selected: Option<usize>,
    pub hover: Option<usize>,
    /// The nation whose card is open.
    pub card: Option<usize>,
    pub ambient: Option<LoopId>,
    press_at: Option<Vec2>,
    last_mouse: Option<Vec2>,
    dragging: bool,
    right_last: Option<Vec2>,
    on_minimap: bool,
    next_nation: usize,
    /// Names and flags as laid out this frame: (nation, name rect, name size, flag foot).
    labels: Vec<Label>,
    camera: Option<Camera3D>,
    last_tick: f32,
    /// The economy window and overlays.
    pub econ: EconUi,
}

impl MapUi {
    /// Esc has something to close here (so it doesn't quit the game).
    pub fn wants_escape(&self) -> bool {
        self.card.is_some() || self.selected.is_some() || self.econ.open.is_some()
    }
}

impl Lucky {
    pub(super) fn enter_map(&mut self, ctx: &mut Context) {
        ctx.play_with(&self.assets.open_map, 0.9, 0.0);
        if self.map_ui.ambient.is_none() {
            self.map_ui.ambient = Some(ctx.play_loop(&self.assets.world_ambience, 0.55));
        }
        self.inspect = None;
        self.go(super::Screen::Map);
    }

    fn open_card(&mut self, ctx: &mut Context, nation: usize) {
        self.map_ui.card = Some(nation);
        ctx.play_with(&self.assets.card_sound, 0.8, 0.0);
    }

    fn select_province(&mut self, ctx: &mut Context, province: Option<usize>) {
        if self.map_ui.selected != province {
            self.map_ui.selected = province;
            let (Some(view), Some(world)) = (self.map_view.as_mut(), self.world.as_ref()) else { return };
            view.select(&self.atlas, world, province);
            if province.is_some() {
                ctx.play_with(&self.assets.select, 0.8, 0.0);
            }
        }
    }

    /// Moves the camera over a nation's capital and selects it.
    fn visit(&mut self, ctx: &mut Context, nation: usize) {
        let Some(world) = self.world.as_ref() else { return };
        let capital = world.capital[nation];
        let map = self.atlas.map(world.map);
        let (x, y) = map.provinces[capital].middle;
        if let Some(view) = self.map_view.as_mut() {
            let p = view.world(x, y, 0.0);
            view.orbit.target = vec2(p.x, p.z);
        }
        self.select_province(ctx, Some(capital));
    }

    pub(super) fn update_map(&mut self, ctx: &mut Context) {
        ctx.idle_ok();
        let dt = ctx.dt();
        let mouse = self.mouse;

        // The days go by (a card or the economy window doesn't stop them).
        self.update_clock(ctx);

        // A country card is open: only closing it.
        if self.map_ui.card.is_some() {
            if ctx.input.was_pressed(Key::Escape)
                || ctx.input.was_pressed(Key::Space)
                || ctx.input.was_pressed(Key::Enter)
                || ctx.input.was_pressed(Key::C)
                || self.mouse_clicked(ctx, &INSPECT_BACK)
            {
                self.map_ui.card = None;
                ctx.play_with(&self.assets.click, 0.7, 0.0);
            }
            self.report_map(ctx);
            return;
        }

        // The clock's pause, and the economy window.
        if ctx.input.was_pressed(Key::Space) {
            self.clock.paused = !self.clock.paused;
            ctx.play_with(&self.assets.click, 0.7, 0.0);
        }
        if self.map_ui.econ.open.is_some() {
            self.update_econ_window(ctx);
            self.report_map(ctx);
            return;
        }
        let econ_click = ctx.input.was_mouse_pressed(MouseButton::Left) && mouse.is_some_and(|m| ECON_BUTTON.contains(m));
        if self.economy.is_some() && (ctx.input.was_pressed(Key::O) || econ_click) {
            self.map_ui.econ.open = Some(super::econ_screen::Tab::Budget);
            self.map_ui.econ.row = 0;
            ctx.play_with(&self.assets.card_sound, 0.8, 0.0);
            self.report_map(ctx);
            return;
        }
        if ctx.input.was_pressed(Key::V) {
            self.cycle_overlay(ctx);
        }

        // Keys.
        let nations = self.world.as_ref().map_or(0, |w| w.nations.len());
        if ctx.input.was_pressed(Key::Escape) {
            self.select_province(ctx, None);
        }
        if ctx.input.was_pressed(Key::M) {
            if let Some(view) = self.map_view.as_mut() {
                view.mode = match view.mode {
                    Mode::Political => Mode::Terrain,
                    Mode::Terrain => Mode::Political,
                };
            }
            ctx.play_with(&self.assets.click, 0.8, 0.0);
        }
        if ctx.input.was_pressed(Key::H) {
            self.visit(ctx, 0);
        }
        if ctx.input.was_pressed(Key::Tab) && nations > 0 {
            self.map_ui.next_nation = (self.map_ui.next_nation + 1) % nations;
            let n = self.map_ui.next_nation;
            self.visit(ctx, n);
        }
        if ctx.input.was_pressed(Key::C) || self.mouse_clicked(ctx, &CARD_BUTTON) && self.map_ui.selected.is_some() {
            let owner = self.map_ui.selected.and_then(|p| self.world.as_ref().and_then(|w| w.owner[p])).unwrap_or(0);
            self.open_card(ctx, owner);
            self.report_map(ctx);
            return;
        }

        let Some(view) = self.map_view.as_mut() else { return };
        let orbit = &mut view.orbit;
        let (sy, cy) = orbit.yaw.sin_cos();
        let forward = vec2(sy, cy);
        let right = vec2(cy, -sy);
        let pan = orbit.distance * 0.9 * dt;
        let ax = ctx.input.axis(Key::A, Key::D) + ctx.input.axis(Key::Left, Key::Right);
        let az = ctx.input.axis(Key::S, Key::W) + ctx.input.axis(Key::Down, Key::Up);
        orbit.target = orbit.target + right * (ax * pan) + forward * (az * pan);
        orbit.yaw += (ctx.input.axis(Key::Q, Key::E)) * 1.3 * dt;
        orbit.pitch += ctx.input.axis(Key::T, Key::G) * 0.8 * dt;
        let zoom = ctx.input.axis(Key::R, Key::F);
        orbit.distance *= (zoom * 1.4 * dt).exp();
        let scroll = ctx.input.scroll();
        if scroll != 0.0 {
            orbit.distance *= 0.87f32.powf(scroll);
            if self.time - self.map_ui.last_tick > 0.08 {
                ctx.play_with(&self.assets.pan_tick, 0.5, 0.0);
                self.map_ui.last_tick = self.time;
            }
        }

        // The mouse: drag to pan (the ground under it stays under it),
        // right-drag to turn, click to select.
        let camera = view.orbit.camera();
        let ground_at = |p: Vec2| {
            let ray = camera.ray_through(p, &self.projector);
            view.ground(ray.origin, ray.direction)
        };
        let minimap_rect = Rect::new(MINIMAP_AT.x, MINIMAP_AT.y, view.minimap().width() as f32, view.minimap().height() as f32);
        if ctx.input.was_mouse_pressed(MouseButton::Left) {
            self.map_ui.press_at = mouse;
            self.map_ui.dragging = false;
            self.map_ui.on_minimap = mouse.is_some_and(|m| minimap_rect.contains(m));
        }
        if ctx.input.is_mouse_down(MouseButton::Left) {
            if let (Some(m), Some(from)) = (mouse, self.map_ui.press_at) {
                if self.map_ui.on_minimap {
                    view.orbit.target = view.minimap_to_ground(m, MINIMAP_AT);
                } else if self.map_ui.dragging || m.distance(from) > 5.0 {
                    self.map_ui.dragging = true;
                    if let (Some(last), true) = (self.map_ui.last_mouse, true) {
                        if let (Some(a), Some(b)) = (ground_at(last), ground_at(m)) {
                            view.orbit.target += a - b;
                        }
                    }
                }
            }
        }
        let mut clicked = None;
        if ctx.input.was_mouse_released(MouseButton::Left) {
            if !self.map_ui.dragging && !self.map_ui.on_minimap {
                clicked = mouse;
            }
            self.map_ui.press_at = None;
            self.map_ui.dragging = false;
            self.map_ui.on_minimap = false;
        }
        if ctx.input.is_mouse_down(MouseButton::Right) {
            if let (Some(m), Some(last)) = (mouse, self.map_ui.right_last) {
                view.orbit.yaw += (m.x - last.x) * 0.008;
                view.orbit.pitch += (m.y - last.y) * 0.005;
            }
            self.map_ui.right_last = mouse;
        } else {
            self.map_ui.right_last = None;
        }
        self.map_ui.last_mouse = mouse;
        view.clamp_target();

        // What's under the mouse.
        let camera = view.orbit.camera();
        let map = self.atlas.map(self.world.as_ref().map_or(0, |w| w.map));
        self.map_ui.hover = mouse
            .filter(|m| !minimap_rect.contains(*m))
            .and_then(|m| {
                let ray = camera.ray_through(m, &self.projector);
                view.pick(map, ray.origin, ray.direction)
            });
        self.map_ui.camera = Some(camera);
        self.layout_labels();

        if let Some(at) = clicked {
            let on_label = self
                .map_ui
                .labels
                .iter()
                .find(|(_, rect, _, flag)| {
                    rect.contains(at) || flag.is_some_and(|(foot, k)| Rect::new(foot.x - 2.0, foot.y - 46.0 * k, 44.0 * k, 32.0 * k).contains(at))
                })
                .map(|(n, ..)| *n);
            let on_panel = self.map_ui.selected.is_some() && Rect::new(PANEL_AT.x, PANEL_AT.y, 360.0, 350.0).contains(at);
            if let Some(n) = on_label {
                self.open_card(ctx, n);
            } else if !on_panel {
                let hit = self.map_ui.hover;
                self.select_province(ctx, hit);
            }
        }
        self.report_map(ctx);
    }

    /// Lays out the nations' names (biggest first, none on top of another)
    /// and their capitals' flags.
    fn layout_labels(&mut self) {
        let (Some(view), Some(world), Some(camera)) = (self.map_view.as_ref(), self.world.as_ref(), self.map_ui.camera.as_ref()) else {
            return;
        };
        let map = self.atlas.map(world.map);
        let mut order: Vec<(usize, Vec3, usize)> = view
            .label_points
            .iter()
            .map(|&(n, p)| (n, p, world.provinces_of(n).iter().map(|&q| map.provinces[q].cells.len()).sum()))
            .collect();
        order.sort_by_key(|&(n, _, cells)| (n != 0, std::cmp::Reverse(cells)));
        let mut placed: Vec<Label> = Vec::new();
        for (n, point, cells) in order {
            let Some((at, depth)) = camera.project(point, &self.projector) else { continue };
            let size = ((cells as f32).sqrt() * 34.0 / depth.max(1.0)).clamp(10.0, 30.0);
            let name = &world.nations[n].name;
            let style = ui::style(size, Color::WHITE);
            let measured = self.assets.serif.measure(name, &style);
            let rect = Rect::new(at.x - measured.x / 2.0, at.y - measured.y / 2.0, measured.x, measured.y);
            let on_screen = rect.x > 4.0 && rect.right() < ui::W - 4.0 && rect.y > 60.0 && rect.bottom() < ui::H - 28.0;
            let clash = placed.iter().any(|(_, r, ..)| r.expand(4.0).overlaps(&rect));
            let flag = camera
                .project(view.capital_tops[n], &self.projector)
                .map(|(foot, d)| (foot, (14.0 / d.max(1.0)).clamp(0.45, 1.1)));
            let rect = if on_screen && (!clash || n == 0) { rect } else { Rect::new(-1000.0, -1000.0, 0.0, 0.0) };
            placed.push((n, rect, size, flag));
        }
        self.map_ui.labels = placed;
    }

    fn report_map(&self, ctx: &mut Context) {
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
    }

    pub(super) fn draw_map(&self, canvas: &mut Canvas) {
        let (Some(view), Some(world)) = (self.map_view.as_ref(), self.world.as_ref()) else { return };
        let a = &self.assets;
        let camera = self.map_ui.camera.unwrap_or_else(|| view.orbit.camera());
        let map = self.atlas.map(world.map);
        view.draw(canvas, &camera);

        // Flags on the capitals, then the names.
        for &(n, _, _, flag) in &self.map_ui.labels {
            let Some((foot, k)) = flag else { continue };
            if foot.x < -50.0 || foot.x > ui::W + 50.0 || foot.y < -50.0 || foot.y > ui::H + 50.0 {
                continue;
            }
            canvas.draw_placed(&a.flag_pole, foot, Placed { pivot: vec2(4.0, 64.0), scale: vec2(k, k * 0.75), ..Placed::default() });
            let r = Rect::new(foot.x + 2.0 * k, foot.y - 46.0 * k, 40.0 * k, 26.0 * k);
            country::draw_flag(canvas, r, &world.nations[n].flag);
            canvas.draw_placed(&a.flag_cloth, vec2(r.x, r.y), Placed { pivot: Vec2::ZERO, scale: vec2(k, k), ..Placed::default() });
            canvas.label(&format!("flag {}", world.nations[n].name), r);
        }
        for &(n, rect, size, _) in &self.map_ui.labels {
            if rect.x < -500.0 {
                continue;
            }
            let color = if n == 0 { ui::GOLD_HI } else { Color::WHITE };
            let mut style = ui::style(size, color);
            style.outline = (size / 7.0).max(2.0);
            style.align = Align::Center;
            canvas.draw_font_text(&a.serif, &world.nations[n].name, vec2(rect.center().x, rect.y), &style);
            canvas.label(&format!("nation {}", world.nations[n].name), rect);
        }

        // The province under the mouse.
        if let (Some(p), Some(m)) = (self.map_ui.hover, self.mouse) {
            if self.map_ui.card.is_none() {
                let prov = &map.provinces[p];
                let owner = world.owner[p].map_or("Unclaimed".to_string(), |o| world.nations[o].name.clone());
                let words = format!("{}  ·  {}", prov.name, owner);
                let w = a.font.measure(&words, &ui::style(15.0, Color::WHITE)).x + 16.0;
                let at = vec2((m.x + 16.0).min(ui::W - w - 4.0), m.y + 18.0);
                canvas.fill_rect(Rect::new(at.x, at.y, w, 26.0), Color::rgba(12, 6, 30, 210));
                ui::plain(canvas, &a.font, &words, at + vec2(8.0, 4.0), 15.0, Color::WHITE, Align::Left);
            }
        }

        // The top bar.
        canvas.blit_sprite(&a.topbar, Vec2::ZERO, Blit::default());
        let player = &world.nations[0];
        country::draw_flag(canvas, Rect::new(14.0, 10.0, 48.0, 32.0), &player.flag);
        ui::plain(canvas, &a.font, "YOU", vec2(72.0, 7.0), 11.0, GOLD, Align::Left);
        ui::text(canvas, &a.serif, &player.name, vec2(72.0, 20.0), 20.0, Color::WHITE, Align::Left, 380.0);
        ui::text(canvas, &a.font, &self.data.maps[world.map].name.to_uppercase(), vec2(640.0, 14.0), 22.0, GOLD, Align::Center, 360.0);
        let mine = world.provinces_of(0).len();
        let facts = format!("{} nations  ·  {} provinces  ·  yours: {}", world.nations.len(), map.provinces.len(), mine);
        ui::plain(canvas, &a.font, &facts, vec2(640.0, 39.0), 12.0, SOFT, Align::Center);
        self.draw_econ_topbar(canvas);

        if let Some(p) = self.map_ui.selected {
            self.draw_province_panel(canvas, p);
        }

        // The minimap, with the camera's view on it.
        canvas.blit_sprite(&a.minimap_frame, MINIMAP_FRAME, Blit::default());
        canvas.blit_sprite(view.minimap(), MINIMAP_AT, Blit::default());
        let corners = [vec2(0.0, 58.0), vec2(ui::W, 58.0), vec2(ui::W, ui::H), vec2(0.0, ui::H)];
        let ground: Vec<Option<Vec2>> = corners
            .iter()
            .map(|&c| {
                let ray = camera.ray_through(c, &self.projector);
                view.ground(ray.origin, ray.direction)
            })
            .collect();
        let mm = view.minimap();
        let clip = Rect::new(MINIMAP_AT.x, MINIMAP_AT.y, mm.width() as f32, mm.height() as f32);
        let clamp = |p: Vec2| vec2(p.x.clamp(clip.left(), clip.right()), p.y.clamp(clip.top(), clip.bottom()));
        for k in 0..4 {
            if let (Some(p), Some(q)) = (ground[k], ground[(k + 1) % 4]) {
                let (p, q) = (clamp(view.to_minimap(p, MINIMAP_AT)), clamp(view.to_minimap(q, MINIMAP_AT)));
                canvas.draw_line(p, q, Color::WHITE);
                canvas.draw_line(p + vec2(0.0, 1.0), q + vec2(0.0, 1.0), Color::rgba(0, 0, 0, 160));
            }
        }
        if let Some(cap) = world.capital.first() {
            let (x, y) = map.provinces[*cap].middle;
            canvas.blit_sprite(&a.icon_capital, MINIMAP_AT + vec2(x * 4.0 - 7.0, y * 4.0 - 7.0), Blit { scale: 0.5, ..Blit::default() });
        }
        canvas.label("minimap", clip);
        self.draw_overlay_legend(canvas);

        ui::plain(
            canvas,
            &a.font,
            match view.mode {
                Mode::Political => "Drag: pan · Right-drag, Q/E: turn · Wheel, R/F: zoom · T/G: tilt · Click: province · Name or flag: card · Tab: next nation · H: home · M: terrain · V: overlays · Space: pause · 1-5: speed · O: economy",
                Mode::Terrain => "Drag: pan · Right-drag, Q/E: turn · Wheel, R/F: zoom · T/G: tilt · Click: province · Name or flag: card · Tab: next nation · H: home · M: nations · V: overlays · Space: pause · 1-5: speed · O: economy",
            },
            vec2(14.0, 699.0),
            12.0,
            Color::rgba(255, 255, 255, 200),
            Align::Left,
        );

        self.draw_econ_window(canvas);
        if let Some(n) = self.map_ui.card {
            ui::dim(canvas, 0.75);
            self.draw_country_card(canvas, &world.nations[n], usize::MAX, n == 0);
            INSPECT_BACK.draw(canvas, a, self.mouse, self.time, false);
        }
    }

    fn draw_province_panel(&self, canvas: &mut Canvas, p: usize) {
        let (Some(world), a) = (self.world.as_ref(), &self.assets) else { return };
        let map = self.atlas.map(world.map);
        let prov = &map.provinces[p];
        let (x, y) = (PANEL_AT.x, PANEL_AT.y);
        canvas.blit_sprite(&a.map_panel, PANEL_AT, Blit::default());
        canvas.label("province panel", Rect::new(x, y, 360.0, 350.0));
        ui::plain(canvas, &a.font, "PROVINCE", vec2(x + 22.0, y + 16.0), 12.0, GOLD, Align::Left);
        ui::text(canvas, &a.serif, &prov.name, vec2(x + 22.0, y + 30.0), 26.0, Color::WHITE, Align::Left, 316.0);
        let continent = &self.data.maps[world.map].continents[prov.continent].label;
        let terrain = &self.atlas.terrains[prov.terrain].name;
        ui::plain(canvas, &a.font, &format!("{terrain}  ·  {continent}"), vec2(x + 22.0, y + 66.0), 15.0, MUTED, Align::Left);

        // Its owner.
        let mut row = y + 96.0;
        match world.owner[p] {
            Some(o) => {
                country::draw_flag(canvas, Rect::new(x + 22.0, row, 42.0, 28.0), &world.nations[o].flag);
                let who = if o == 0 { "YOUR NATION" } else { "OWNED BY" };
                ui::plain(canvas, &a.font, who, vec2(x + 74.0, row - 2.0), 11.0, if o == 0 { GOLD } else { SOFT }, Align::Left);
                ui::text(canvas, &a.serif, &world.nations[o].name, vec2(x + 74.0, row + 11.0), 17.0, Color::WHITE, Align::Left, 262.0);
                if world.capital[o] == p {
                    canvas.blit_sprite(&a.icon_capital, vec2(x + 318.0, row + 54.0), Blit::default());
                    ui::plain(canvas, &a.font, "Capital", vec2(x + 312.0, row + 58.0), 14.0, GOLD, Align::Right);
                }
            }
            None => {
                ui::plain(canvas, &a.font, "Unclaimed land", vec2(x + 22.0, row + 4.0), 17.0, MUTED, Align::Left);
            }
        }
        row += 52.0;

        // What it's like.
        let mut facts: Vec<(Option<&Sprite>, String)> = Vec::new();
        facts.push((None, if prov.coastal { format!("Coastal ({} sea edges)", prov.sea_edges) } else { "Inland: no coast".to_string() }));
        if prov.river {
            let names: Vec<&str> = map
                .rivers
                .iter()
                .filter(|r| r.cells.iter().any(|&(c, rr)| map.province_of[map.index(c, rr)] == Some(p)))
                .map(|r| r.name.as_str())
                .collect();
            facts.push((Some(&a.icon_river), names.join(", ")));
        }
        if prov.lake {
            facts.push((None, "On a lake shore".to_string()));
        }
        for l in &map.landmarks {
            if map.province_of[map.index(l.at.0, l.at.1)] == Some(p) {
                facts.push((Some(&a.icon_landmark), l.name.clone()));
            }
        }
        facts.push((None, format!("{} hexes", prov.cells.len())));
        // Its economy: people, what's under the ground, what's built.
        if let Some(e) = self.economy.as_ref() {
            let ed = &self.econ_data;
            let pe = &e.provinces[p];
            if let Some(o) = world.owner[p] {
                let people = e.nations[o].population * pe.people;
                let mut built: Vec<(f64, usize)> = Vec::new();
                for b in e.nations[o].buildings.iter().filter(|b| b.province == p) {
                    match built.iter_mut().find(|(_, k)| *k == b.kind) {
                        Some(x) => x.0 += b.levels,
                        None => built.push((b.levels, b.kind)),
                    }
                }
                built.sort_by(|a, b| b.0.total_cmp(&a.0));
                let names: Vec<String> = built.iter().take(3).map(|(l, k)| format!("{} {:.0}", ed.buildings[*k].name, l)).collect();
                let jobs = if names.is_empty() { "no buildings".to_string() } else { names.join(", ") };
                facts.push((None, format!("{} people  ·  {}", super::econ_screen::people(people), jobs)));
                facts.push((None, format!("Roads {:.1}  ·  rail {:.1}  ·  port {:.1}  ·  moving {:.0}%", pe.road, pe.rail, pe.port, pe.transport * 100.0)));
            }
            let mut rich: Vec<usize> = (0..ed.goods.len()).filter(|&g| pe.deposits[g] >= 0.2).collect();
            rich.sort_by(|&a, &b| pe.deposits[b].total_cmp(&pe.deposits[a]));
            if !rich.is_empty() {
                let names: Vec<String> = rich.iter().take(3).map(|&g| format!("{} {:.1}", ed.goods[g].name, pe.deposits[g])).collect();
                facts.push((None, format!("Rich in {}", names.join(", "))));
            }
        }
        for (icon, words) in facts {
            if let Some(icon) = icon {
                canvas.blit_sprite(icon, vec2(x + 22.0, row - 1.0), Blit::default());
            } else {
                canvas.fill_circle(vec2(x + 31.0, row + 9.0), 3.0, SOFT);
            }
            ui::text(canvas, &a.font, &words, vec2(x + 48.0, row), 15.0, Color::WHITE, Align::Left, 290.0);
            row += 24.0;
        }

        // Its neighbours.
        let names: Vec<&str> = prov.neighbours.iter().map(|&q| map.provinces[q].name.as_str()).collect();
        let shown = if names.is_empty() { "none (an island)".to_string() } else { names.join(", ") };
        let mut style = ui::style(14.0, MUTED);
        style.outline = 0.0;
        style.shadow = None;
        style.wrap = Some(316.0);
        ui::plain(canvas, &a.font, &format!("NEIGHBOURS ({})", names.len()), vec2(x + 22.0, row + 4.0), 11.0, SOFT, Align::Left);
        canvas.draw_font_text(&a.font, &shown, vec2(x + 22.0, row + 20.0), &style);

        if world.owner[p].is_some() {
            CARD_BUTTON.draw(canvas, a, self.mouse, self.time, false);
        }
    }
}
