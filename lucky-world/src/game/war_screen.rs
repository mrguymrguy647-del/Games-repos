//! War on the map screen: armies, fleets and jets on the 3D map (one
//! soldier figure for about 1,000 soldiers, one tank for about 100 tanks),
//! red front lines, attack arrows, battle markers you can click to zoom in,
//! the war room (K) and a battle report after each of your battles.
//!
//! | | |
//! |---|---|
//! | Map | J: jump to the biggest battle · click crossed swords: watch that battle |
//! | War room | K: open or close · Tab: next page · Up / Down: choose a row |
//! | Wars | E: escalate · M: match their escalation · B: back down · P: offer peace · Y / N: answer their offer · D: declare on the selected nation · L: the level to declare at |
//! | Army | A: attack · H: hold · G: dig in · R: retreat · F: reinforce the selected province · U: back to the general |
//! | Supply | Left / Right: a front's priority · Y / N: approve or deny a request · T: Auto on or off |
//! | Reports | Up / Down: choose a battle · Enter: the full report |

use duckforge::prelude::*;

use super::Lucky;
use crate::econ::{EconData, Economy};
use crate::map_view::{Piece, WarPiece, WarScene};
use crate::ui::{self, GOLD, MUTED, SOFT};
use crate::war::report::{battle_text, big};
use crate::war::{self, land, peace, player, supply, Order, Wars, FRONTS, LEVELS};

const WIN: Rect = Rect { x: 110.0, y: 64.0, w: 1060.0, h: 600.0 };
/// The war room button on the top bar (under the economy button).
pub const WAR_BUTTON: Rect = Rect { x: 1086.0, y: 62.0, w: 180.0, h: 22.0 };
const PAPER: Vec2 = Vec2 { x: 360.0, y: 140.0 };
const INK: Color = Color::rgb(46, 34, 22);
const GOOD: Color = Color::rgb(107, 212, 138);
const WARN: Color = Color::rgb(245, 197, 66);
const BAD: Color = Color::rgb(240, 106, 106);
/// Attack arrows on the map.
const ARROW: Color = Color::rgb(255, 92, 40);
const PRIORITY: [&str; 4] = ["hold back", "low", "normal", "high"];

/// The war room's pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Wars,
    Army,
    Supply,
    Reports,
}

impl Page {
    pub const ALL: [Page; 4] = [Page::Wars, Page::Army, Page::Supply, Page::Reports];

    fn name(self) -> &'static str {
        match self {
            Page::Wars => "WARS",
            Page::Army => "ARMY",
            Page::Supply => "SUPPLY",
            Page::Reports => "REPORTS",
        }
    }

    fn tab_rect(k: usize) -> Rect {
        Rect::new(WIN.x + 24.0 + k as f32 * 168.0, WIN.y + 16.0, 160.0, 40.0)
    }
}

/// What the war screens remember between frames.
#[derive(Debug)]
pub struct WarUi {
    pub open: Option<Page>,
    pub row: usize,
    /// The level the player declares at (1 skirmish to 4 total war).
    pub declare_level: usize,
    /// Battle reports already seen (index into `Wars::reports`).
    reports_seen: usize,
    /// The newest report of the player's, shown as a card for a while.
    pub card: Option<(usize, f32)>,
    /// A report opened in full.
    pub full: Option<usize>,
    /// The battle being watched (its panel is open).
    pub watching: Option<usize>,
    /// Battle markers as laid out this frame: (battle, screen point, size).
    markers: Vec<(usize, Vec2, f32)>,
    news_seen: usize,
    battles_seen: usize,
    offers_seen: u32,
    scene_day: u32,
    scene_time: f32,
    scene_dirty: bool,
    borders_dirty: bool,
    borders_time: f32,
    pub drums: Option<LoopId>,
    pub drums_quiet: f32,
    last_shot: f32,
    /// Show every army, even in peace (the war overlay).
    pub all_armies: bool,
}

impl Default for WarUi {
    fn default() -> WarUi {
        WarUi {
            open: None,
            row: 0,
            declare_level: 2,
            reports_seen: 0,
            card: None,
            full: None,
            watching: None,
            markers: Vec::new(),
            news_seen: 0,
            battles_seen: 0,
            offers_seen: 0,
            scene_day: u32::MAX,
            scene_time: -10.0,
            scene_dirty: true,
            borders_dirty: false,
            borders_time: -10.0,
            drums: None,
            drums_quiet: 0.0,
            last_shot: -10.0,
            all_armies: false,
        }
    }
}

impl WarUi {
    pub fn wants_escape(&self) -> bool {
        self.open.is_some() || self.full.is_some() || self.watching.is_some()
    }

    pub fn refresh(&mut self) {
        self.scene_dirty = true;
    }
}

/// A faction's colour on the map (rebels are black).
fn faction_color(w: &Wars, world: &crate::world::World, f: usize) -> Color {
    if f < w.nations { world.color[f] } else { Color::rgb(28, 26, 30) }
}

fn fit(canvas: &mut Canvas, font: &Font, words: &str, at: Vec2, size: f32, color: Color, max: f32) {
    let mut s = TextStyle { size, color, ..TextStyle::default() };
    let width = font.measure(words, &s).x;
    if width > max && width > 0.0 {
        s.size = size * max / width;
    }
    canvas.draw_font_text(font, words, at, &s);
}

fn plain(canvas: &mut Canvas, font: &Font, words: &str, at: Vec2, size: f32, color: Color) {
    ui::plain(canvas, font, words, at, size, color, Align::Left);
}

fn right(canvas: &mut Canvas, font: &Font, words: &str, at: Vec2, size: f32, color: Color) {
    ui::plain(canvas, font, words, at, size, color, Align::Right);
}

fn pct(x: f64) -> String {
    format!("{:.0}%", x * 100.0)
}

impl Lucky {
    /// Whose war the screens show: the player's nation, or (watching) the
    /// nation whose brain is open.
    fn war_viewer(&self) -> usize {
        if self.spectating { self.map_ui.brain.unwrap_or(0) } else { 0 }
    }

    pub(super) fn war_viewer_pub(&self) -> usize {
        self.war_viewer()
    }

    /// A day (or many) of war went by: news, sounds, borders, reports.
    /// A loaded game's battles, reports, news and offers are old news.
    pub(super) fn war_seen_all(&mut self) {
        let Some(w) = self.wars.as_ref() else { return };
        let ui = &mut self.map_ui.war;
        ui.news_seen = w.news.len();
        ui.battles_seen = w.battles.len();
        ui.reports_seen = w.reports.len();
        ui.offers_seen = w.day;
        ui.scene_dirty = true;
        ui.borders_dirty = true;
    }

    pub(super) fn after_war_days(&mut self, ctx: &mut Context) {
        let Some(w) = self.wars.as_mut() else { return };
        let ui = &mut self.map_ui.war;
        if w.day != ui.scene_day {
            ui.scene_dirty = true;
        }
        player::live_offers(w);
        // Provinces changed hands: the map's borders follow (at most twice a second).
        if w.borders_changed {
            w.borders_changed = false;
            if let Some(world) = self.world.as_mut() {
                world.owner = w.owner.clone();
                world.capital = w.capital.clone();
            }
            ui.borders_dirty = true;
        }
        if ui.borders_dirty && self.time - ui.borders_time > 0.5 {
            ui.borders_dirty = false;
            ui.borders_time = self.time;
            if let (Some(view), Some(world)) = (self.map_view.as_mut(), self.world.as_ref()) {
                view.rebuild_borders(&self.atlas, world, &self.models);
                self.map_ui.econ.force_overlay();
            }
        }
        let viewer = if self.spectating { None } else { Some(0usize) };
        // News of the player's wars (watching: the wars that matter).
        if w.news.len() < ui.news_seen {
            ui.news_seen = 0;
        }
        let fresh: Vec<String> = w.news[ui.news_seen..]
            .iter()
            .filter(|n| match viewer {
                Some(v) => n.factions.iter().any(|&f| w.nation_of(f) == v),
                None => n.text.contains("Declared war") || n.text.contains("Total war") || n.text.contains("Peace between"),
            })
            .map(|n| n.text.clone())
            .collect();
        ui.news_seen = w.news.len();
        // A declaration is a horn, a peace a bell; the player's own war news is critical.
        let mut notes: Vec<(String, u8, Option<&'static str>)> = Vec::new();
        for words in fresh.iter().rev().take(3).rev() {
            // A war starting: "<level>: X against Y (why)", or a civil war.
            let war = crate::war::LEVELS.iter().skip(1).any(|l| words.starts_with(&format!("{l}:"))) || words.contains("Civil war");
            let peace = words.starts_with("Peace");
            let level = if viewer.is_some() && (war || peace) { super::shell::CRITICAL } else { super::shell::IMPORTANT };
            notes.push((words.clone(), level, if war { Some("declare") } else if peace { Some("peace") } else { None }));
        }
        // A new peace offer for the player: say so (it waits in the war room).
        if let Some(o) = w.offers.iter().filter(|o| o.3 >= ui.offers_seen).max_by_key(|o| o.3) {
            if viewer.is_some() {
                let from = w.names[w.wars[o.0].sides[o.1][0]].clone();
                notes.push((format!("{from} offers peace: {}. K: the war room to answer", o.2.words(w)), super::shell::CRITICAL, None));
            }
            ui.offers_seen = o.3 + 1;
        }
        // Old battles were cleared away (their reports stay): start counting again.
        if w.battles.len() < ui.battles_seen {
            ui.battles_seen = 0;
            ui.watching = None;
        }
        // A battle begins: gunfire (the player's, or watching, any).
        let started = w.battles.len().saturating_sub(ui.battles_seen);
        let mine_started = w.battles[ui.battles_seen.min(w.battles.len())..].iter().any(|b| viewer.is_none_or(|v| b.sides.iter().flatten().any(|&f| w.nation_of(f) == v)));
        ui.battles_seen = w.battles.len();
        if started > 0 && mine_started && self.time - ui.last_shot > 0.6 {
            ui.last_shot = self.time;
            crate::ui::sfx(ctx, &self.assets.gunfire, 0.55, 0.0);
        }
        // A battle ends: an explosion, and (the player's) its report.
        let mut ended_mine = None;
        for k in ui.reports_seen.min(w.reports.len())..w.reports.len() {
            let r = &w.reports[k];
            let involved = viewer.is_none_or(|v| r.attacker.factions.iter().chain(r.defender.factions.iter()).any(|&f| w.nation_of(f) == v));
            if involved {
                ended_mine = Some(k);
            }
        }
        ui.reports_seen = w.reports.len();
        if let Some(k) = ended_mine {
            crate::ui::sfx(ctx, &self.assets.explosion, 0.5, 0.0);
            if viewer.is_some() {
                ui.card = Some((k, 9.0));
            }
        }
        // The music follows war and peace (shell.rs, `want_music`).
        // A watched battle that ended closes.
        if let Some(b) = ui.watching {
            if b >= w.battles.len() {
                ui.watching = None;
            }
        }
        for (words, level, sound) in notes {
            if let Some(s) = sound {
                self.sound_named(ctx, s);
            }
            self.notify(ctx, words, level);
        }
    }

    /// Rebuilds the war layer on the 3D map when the war moved (at most 5 times a second).
    pub(super) fn refresh_war_scene(&mut self) {
        let ui = &self.map_ui.war;
        if !ui.scene_dirty || self.time - ui.scene_time < 0.2 {
            return;
        }
        let (Some(w), Some(e), Some(world)) = (self.wars.as_ref(), self.economy.as_ref(), self.world.as_ref()) else { return };
        let scene = war_scene(w, e, &self.econ_data, world, &self.atlas, ui.all_armies);
        if let Some(view) = self.map_view.as_mut() {
            view.set_war(&self.atlas, world, &self.models, &scene);
        }
        let ui = &mut self.map_ui.war;
        ui.scene_dirty = false;
        ui.scene_time = self.time;
        ui.scene_day = w.day;
    }

    /// Lays out the battle markers (crossed swords over each battle).
    pub(super) fn layout_battles(&mut self) {
        let (Some(w), Some(view), Some(world), Some(camera)) = (self.wars.as_ref(), self.map_view.as_ref(), self.world.as_ref(), self.map_ui.camera.as_ref()) else {
            return;
        };
        let mut markers = Vec::new();
        for (k, b) in w.battles.iter().enumerate().rev().take(400) {
            if b.over {
                continue;
            }
            let p = view.province_point(&self.atlas, world, b.province, 0.75);
            let Some((at, depth)) = camera.project(p, &self.projector) else { continue };
            if at.x < -40.0 || at.x > ui::W + 40.0 || at.y < 50.0 || at.y > ui::H + 40.0 {
                continue;
            }
            let men = b.report.attacker.soldiers_start + b.report.defender.soldiers_start;
            let size = ((men / 20_000.0).sqrt() as f32 * 26.0 / depth.max(1.0) * 10.0).clamp(16.0, 46.0);
            markers.push((k, at, size));
        }
        self.map_ui.war.markers = markers;
    }

    /// The battle marker under a click, if any.
    pub(super) fn battle_at(&self, at: Vec2) -> Option<usize> {
        self.map_ui.war.markers.iter().find(|(_, p, s)| p.distance(at) < s * 0.6 + 4.0).map(|m| m.0)
    }

    /// Watches a battle: the camera goes there and its panel opens.
    pub(super) fn watch_battle(&mut self, ctx: &mut Context, b: usize) {
        let (Some(w), Some(world)) = (self.wars.as_ref(), self.world.as_ref()) else { return };
        let p = w.battles[b].province;
        if let Some(view) = self.map_view.as_mut() {
            let at = view.province_point(&self.atlas, world, p, 0.0);
            self.map_ui.glide = Some(vec2(at.x, at.z));
            let _ = &view;
            view.orbit.distance = view.orbit.distance.min(9.0);
        }
        self.map_ui.war.watching = Some(b);
        crate::ui::sfx(ctx, &self.assets.gunfire, 0.5, 0.0);
    }

    /// J: the biggest battle going on.
    pub(super) fn biggest_battle(&self) -> Option<usize> {
        let w = self.wars.as_ref()?;
        let mine = |b: &war::Battle| self.spectating || b.sides.iter().flatten().any(|&f| w.nation_of(f) == 0);
        let best = |only_mine: bool| {
            w.battles
                .iter()
                .enumerate()
                .filter(|(_, b)| !b.over && (!only_mine || mine(b)))
                .max_by(|a, b| (a.1.report.attacker.soldiers_start + a.1.report.defender.soldiers_start).total_cmp(&(b.1.report.attacker.soldiers_start + b.1.report.defender.soldiers_start)))
                .map(|x| x.0)
        };
        best(true).or_else(|| best(false))
    }

    /// The war room's keys (K closes it).
    pub(super) fn update_war_room(&mut self, ctx: &mut Context) {
        let Some(page) = self.map_ui.war.open else { return };
        let mouse = self.ui_mouse();
        if ctx.input.was_pressed(Key::Escape) || ctx.input.was_pressed(Key::K) {
            self.map_ui.war.open = None;
            crate::ui::sfx(ctx, &self.assets.click, 0.7, 0.0);
            return;
        }
        let mut next = None;
        if ctx.input.was_pressed(Key::Tab) {
            let k = Page::ALL.iter().position(|&p| p == page).unwrap_or(0);
            next = Some(Page::ALL[(k + 1) % Page::ALL.len()]);
        }
        if ctx.input.was_mouse_pressed(MouseButton::Left) {
            if let Some(m) = mouse {
                for (k, p) in Page::ALL.iter().enumerate() {
                    if Page::tab_rect(k).contains(m) {
                        next = Some(*p);
                    }
                }
            }
        }
        if let Some(p) = next {
            self.map_ui.war.open = Some(p);
            self.map_ui.war.row = 0;
            crate::ui::sfx(ctx, &self.assets.click, 0.7, 0.0);
            return;
        }
        let rows = self.war_rows(page);
        let ui = &mut self.map_ui.war;
        if ctx.input.was_pressed(Key::Down) && rows > 0 {
            ui.row = (ui.row + 1) % rows;
            crate::ui::sfx(ctx, &self.assets.pan_tick, 0.5, 0.0);
        }
        if ctx.input.was_pressed(Key::Up) && rows > 0 {
            ui.row = (ui.row + rows - 1) % rows;
            crate::ui::sfx(ctx, &self.assets.pan_tick, 0.5, 0.0);
        }
        ui.row = ui.row.min(rows.saturating_sub(1));
        let row = ui.row;
        if page == Page::Reports {
            if ctx.input.was_pressed(Key::Enter) {
                if let Some(&k) = self.report_list().get(row) {
                    self.map_ui.war.full = Some(k);
                    crate::ui::sfx(ctx, &self.assets.card_sound, 0.7, 0.0);
                }
            }
            return;
        }
        // Watching, the war room only shows.
        if self.spectating {
            return;
        }
        let (Some(w), Some(e)) = (self.wars.as_mut(), self.economy.as_mut()) else { return };
        let ed = &self.econ_data;
        let mut did = false;
        match page {
            Page::Wars => {
                let list = player::wars_in_view(w);
                if let Some(&k) = list.get(row) {
                    if ctx.input.was_pressed(Key::E) {
                        did = player::escalate(w, k);
                    }
                    if ctx.input.was_pressed(Key::M) {
                        player::answer_escalation(w, e, ed, k, true);
                        did = true;
                    }
                    if ctx.input.was_pressed(Key::B) {
                        player::answer_escalation(w, e, ed, k, false);
                        did = true;
                    }
                    if ctx.input.was_pressed(Key::P) {
                        player::offer_peace(w, e, ed, k);
                        did = true;
                    }
                    let offer = w.offers.iter().position(|o| o.0 == k);
                    if let Some(o) = offer {
                        if ctx.input.was_pressed(Key::Y) {
                            player::answer_offer(w, e, ed, o, true);
                            did = true;
                        } else if ctx.input.was_pressed(Key::N) {
                            player::answer_offer(w, e, ed, o, false);
                            did = true;
                        }
                    }
                }
                if ctx.input.was_pressed(Key::L) {
                    self.map_ui.war.declare_level = self.map_ui.war.declare_level % 4 + 1;
                    did = true;
                }
                if ctx.input.was_pressed(Key::D) {
                    let target = self.map_ui.selected.and_then(|p| w.owner[p]).filter(|&o| o != 0);
                    if let Some(t) = target {
                        did = player::declare(w, e, ed, t, self.map_ui.war.declare_level).is_some();
                    }
                }
            }
            Page::Army => {
                let front = row.min(FRONTS.len() - 1);
                let order = if ctx.input.was_pressed(Key::A) {
                    player::front_target(w, front).map(Order::Attack)
                } else if ctx.input.was_pressed(Key::H) {
                    Some(Order::Hold)
                } else if ctx.input.was_pressed(Key::G) {
                    Some(Order::DigIn)
                } else if ctx.input.was_pressed(Key::R) {
                    Some(Order::Retreat)
                } else if ctx.input.was_pressed(Key::U) {
                    Some(Order::Auto)
                } else if ctx.input.was_pressed(Key::F) {
                    self.map_ui.selected.filter(|&p| w.owner[p] == Some(0)).map(Order::Reinforce)
                } else {
                    None
                };
                if let Some(o) = order {
                    did = player::order_front(w, front, o) > 0;
                }
            }
            Page::Supply => {
                let f = 0;
                if row < FRONTS.len() {
                    let p = &mut w.forces[f].priority[row];
                    if ctx.input.was_pressed(Key::Right) && *p < 3 {
                        *p += 1;
                        did = true;
                    }
                    if ctx.input.was_pressed(Key::Left) && *p > 0 {
                        *p -= 1;
                        did = true;
                    }
                } else if let Some(&k) = request_list(w, f).get(row - FRONTS.len()) {
                    if ctx.input.was_pressed(Key::Y) {
                        supply::answer(w, f, k, true);
                        did = true;
                    } else if ctx.input.was_pressed(Key::N) {
                        supply::answer(w, f, k, false);
                        did = true;
                    }
                }
                if ctx.input.was_pressed(Key::T) {
                    w.forces[f].auto = !w.forces[f].auto;
                    did = true;
                }
            }
            Page::Reports => {}
        }
        if did {
            self.map_ui.war.refresh();
            crate::ui::sfx(ctx, &self.assets.select, 0.7, 0.0);
        }
    }

    fn war_rows(&self, page: Page) -> usize {
        let Some(w) = self.wars.as_ref() else { return 0 };
        match page {
            Page::Wars => player::wars_in_view(w).len(),
            Page::Army => FRONTS.len(),
            Page::Supply => FRONTS.len() + request_list(w, self.war_viewer()).len(),
            Page::Reports => self.report_list().len(),
        }
    }

    /// The viewer's battle reports, newest first (watching: everyone's).
    fn report_list(&self) -> Vec<usize> {
        let Some(w) = self.wars.as_ref() else { return Vec::new() };
        let v = self.war_viewer();
        (0..w.reports.len())
            .rev()
            .filter(|&k| {
                let r = &w.reports[k];
                self.spectating || r.attacker.factions.iter().chain(r.defender.factions.iter()).any(|&f| w.nation_of(f) == v)
            })
            .take(60)
            .collect()
    }

    /// Reports for the playthrough log.
    pub(super) fn report_war(&self, ctx: &mut Context) {
        let Some(w) = self.wars.as_ref() else { return };
        let active: Vec<&war::War> = w.wars.iter().filter(|x| x.active()).collect();
        ctx.report("wars_active", active.len());
        ctx.report("wars_total", w.wars.len());
        ctx.report("war_level_max", active.iter().map(|x| x.level).max().unwrap_or(0));
        ctx.report("battles_now", w.battles.iter().filter(|b| !b.over).count());
        ctx.report("battle_reports", w.reports.len());
        ctx.report("player_war_level", w.level_of(0));
        let (soldiers, tanks) = w.strength(0);
        ctx.report("player_soldiers", soldiers.round());
        ctx.report("player_tanks", tanks.round());
        ctx.report("player_divisions", w.divisions.iter().filter(|d| d.alive && d.faction == 0).count());
        ctx.report("war_room", self.map_ui.war.open.map_or("closed", |p| p.name()));
        ctx.report("war_row", self.map_ui.war.row);
        ctx.report("declare_level", self.map_ui.war.declare_level);
        ctx.report("watching_battle", self.map_ui.war.watching.map_or(-1, |b| b as i64));
        ctx.report("report_open", self.map_ui.war.full.is_some());
        ctx.report("report_card", self.map_ui.war.card.is_some());
        ctx.report("supply_auto", w.forces[0].auto);
        ctx.report("supply_requests_open", w.forces[0].requests.iter().filter(|r| r.answered.is_none()).count());
        ctx.report("peace_offers", w.offers.len());
        ctx.report("dead_total", w.forces.iter().map(|f| f.dead).sum::<f64>().round());
        ctx.report("occupied_provinces", (0..w.control.len()).filter(|&q| w.control[q].is_some() && w.control[q] != w.owner[q]).count());
        ctx.report("divisions_order_not_auto", w.divisions.iter().filter(|d| d.alive && d.faction == 0 && d.order != Order::Auto).count());
        ctx.report("priority_0", w.forces[0].priority[0] as i64);
        if let Some(x) = w.wars.iter().filter(|x| !x.active()).max_by_key(|x| x.end) {
            let names = |s: usize| x.sides[s].iter().map(|&f| w.names[f].clone()).collect::<Vec<_>>().join(" + ");
            ctx.report("last_war_ended", format!("{} vs {}: {}", names(0), names(1), x.result.clone().unwrap_or_default()));
        }
    }

    // ---------------------------------------------------------------- drawing

    /// Battle markers on the map, and a watched battle's panel.
    pub(super) fn draw_war_map(&self, canvas: &mut Canvas) {
        let Some(w) = self.wars.as_ref() else { return };
        let a = &self.assets;
        for &(k, at, size) in &self.map_ui.war.markers {
            let pulse = 1.0 + 0.12 * (self.time * 5.0 + k as f32).sin();
            let s = size * pulse;
            canvas.fill_circle(at, s * 0.62, Color::rgba(160, 20, 20, 150));
            canvas.blit_sprite(&a.icon_swords, at - vec2(s / 2.0, s / 2.0), Blit { scale: s / 32.0, ..Blit::default() });
            canvas.label(&format!("battle {}", w.battles[k].report.province_name), Rect::new(at.x - s / 2.0, at.y - s / 2.0, s, s));
        }
        let window = self.map_ui.econ.open.is_some() || self.map_ui.war.open.is_some() || self.map_ui.diplo.wants_escape();
        if let Some(b) = self.map_ui.war.watching.filter(|_| !window) {
            self.draw_battle_panel(canvas, w, b);
        }
    }

    fn draw_battle_panel(&self, canvas: &mut Canvas, w: &Wars, b: usize) {
        let a = &self.assets;
        let bt = &w.battles[b];
        let r = &bt.report;
        let rect = Rect::new(900.0, 70.0, 366.0, 446.0);
        canvas.fill_rect(rect, Color::rgba(14, 8, 28, 228));
        canvas.draw_rect(rect, Color::rgba(245, 197, 66, 160));
        canvas.label("battle panel", rect);
        let (x, mut y) = (rect.x + 16.0, rect.y + 12.0);
        canvas.blit_sprite(&a.icon_swords, vec2(x, y), Blit::default());
        plain(canvas, &a.font, if bt.over { "BATTLE (OVER)" } else { "BATTLE (LIVE)" }, vec2(x + 40.0, y), 11.0, if bt.over { SOFT } else { BAD });
        fit(canvas, &a.serif, &format!("Battle of {}", r.province_name), vec2(x + 40.0, y + 13.0), 20.0, Color::WHITE, 300.0);
        y += 46.0;
        let level = LEVELS[w.wars[bt.war].level];
        let weather = r.weather.last().cloned().unwrap_or_default();
        fit(canvas, &a.font, &format!("{} · {} · {} · {} hours · {level}", r.terrain, weather, if r.day_start > 0 { format!("day {}", r.day_start) } else { String::new() }, r.hours), vec2(x, y), 13.0, SOFT, 334.0);
        y += 24.0;
        let now = |side: usize| -> (f64, f64, f64) {
            let mut s = (0.0, 0.0, 0.0);
            let mut n = 0.0;
            for d in w.divisions.iter().filter(|d| d.alive && d.battle == Some(b)) {
                let att = bt.attackers.contains(&d.id);
                if (side == 0) == att {
                    s.0 += d.soldiers;
                    s.1 += d.tanks - d.tanks_broken;
                    s.2 += d.morale * d.soldiers;
                    n += d.soldiers;
                }
            }
            (s.0, s.1, if n > 0.0 { s.2 / n } else { 0.0 })
        };
        for (side, sr, name) in [(0, &r.attacker, "ATTACKERS"), (1, &r.defender, "DEFENDERS")] {
            let who = sr.factions.iter().map(|&f| w.names[f].clone()).collect::<Vec<_>>().join(", ");
            let color = sr.factions.first().and_then(|&f| self.world.as_ref().map(|wd| faction_color(w, wd, f))).unwrap_or(Color::WHITE);
            canvas.fill_rect(Rect::new(x, y + 2.0, 10.0, 10.0), color);
            plain(canvas, &a.font, name, vec2(x + 16.0, y), 11.0, GOLD);
            fit(canvas, &a.font, &who, vec2(x + 100.0, y - 1.0), 13.0, Color::WHITE, 234.0);
            y += 18.0;
            let (men, tanks, morale) = now(side);
            let first = if bt.over {
                let lost = sr.killed + sr.wounded + sr.captured + sr.surrendered + sr.deserted;
                format!("{} soldiers at the start · {} lost · {} tanks at the start", big(sr.soldiers_start), big(lost), big(sr.tanks_start))
            } else {
                format!("{} soldiers now ({} at the start) · {} tanks", big(men), big(sr.soldiers_start), big(tanks))
            };
            let second = if bt.over {
                format!("{} divisions · {}", sr.divisions.len(), crate::war::report::some(&sr.generals, 1))
            } else {
                format!("morale {} · {} divisions · {}", pct(morale), sr.divisions.len(), crate::war::report::some(&sr.generals, 1))
            };
            let lines = [
                first,
                second,
                format!("rounds {} ({} misfires, {} jams) · shells {} ({} duds)", big(sr.rounds_fired), big(sr.misfires), big(sr.jams), big(sr.shells_fired), big(sr.duds)),
                format!("killed {} · wounded {} · captured {}", big(sr.killed), big(sr.wounded), big(sr.captured)),
                format!("surrendered {} · deserted {} · tanks lost {}", big(sr.surrendered), big(sr.deserted), big(sr.tanks_lost + sr.tanks_abandoned)),
                format!("ammo used {} units · fuel {} t", big(sr.ammo_used), big(sr.fuel_used)),
            ];
            for l in lines {
                fit(canvas, &a.font, &l, vec2(x, y), 13.0, Color::rgba(235, 230, 245, 255), 334.0);
                y += 19.0;
            }
            y += 10.0;
        }
        // The balance of the fight, as a bar (over: who won).
        let (ma, _, _) = now(0);
        let (md, _, _) = now(1);
        if bt.over {
            let words = if r.winner == 0 { "The attackers took the province." } else { "The defenders held." };
            plain(canvas, &a.font, words, vec2(x, y + 2.0), 15.0, GOLD);
            plain(canvas, &a.font, "J: the biggest battle · Esc: close", vec2(x, rect.bottom() - 22.0), 12.0, SOFT);
            return;
        }
        let share = if ma + md > 0.0 { (ma / (ma + md)) as f32 } else { 0.5 };
        let bar = Rect::new(x, y + 4.0, 334.0, 12.0);
        canvas.fill_rect(bar, Color::rgba(90, 120, 220, 220));
        canvas.fill_rect(Rect::new(bar.x, bar.y, bar.w * share, bar.h), Color::rgba(220, 70, 60, 230));
        plain(canvas, &a.font, "attackers · soldiers left · defenders", vec2(x, y + 20.0), 11.0, SOFT);
        plain(canvas, &a.font, "J: the biggest battle · Esc: close", vec2(x, rect.bottom() - 22.0), 12.0, SOFT);
    }

    /// The card after one of the player's battles, and a report opened in full.
    pub(super) fn draw_battle_reports(&self, canvas: &mut Canvas) {
        let Some(w) = self.wars.as_ref() else { return };
        let a = &self.assets;
        if let Some((k, left)) = self.map_ui.war.card {
            if self.map_ui.war.full.is_none() && self.map_ui.war.open.is_none() && k < w.reports.len() {
                let r = &w.reports[k];
                let rect = Rect::new(ui::W - 330.0, 432.0, 316.0, 92.0);
                let _ = left;
                canvas.fill_rect(rect, Color::rgba(236, 224, 196, 240));
                canvas.draw_rect(rect, Color::rgba(120, 80, 40, 255));
                canvas.label("battle report card", rect);
                let we = r.attacker.factions.iter().any(|&f| w.nation_of(f) == 0);
                let won = (r.winner == 0) == we;
                canvas.blit_sprite(&a.icon_swords, vec2(rect.x + 10.0, rect.y + 10.0), Blit::default());
                fit(canvas, &a.serif, &format!("Battle of {}", r.province_name), vec2(rect.x + 48.0, rect.y + 8.0), 17.0, INK, 256.0);
                plain(canvas, &a.font, if won { "VICTORY" } else { "DEFEAT" }, vec2(rect.x + 48.0, rect.y + 30.0), 12.0, if won { Color::rgb(30, 120, 50) } else { Color::rgb(160, 30, 30) });
                let (us, them) = if we { (&r.attacker, &r.defender) } else { (&r.defender, &r.attacker) };
                fit(
                    canvas,
                    &a.font,
                    &format!("We lost {} dead, {} captured · they lost {} dead, {} captured", big(us.killed), big(us.captured + us.surrendered), big(them.killed), big(them.captured + them.surrendered)),
                    vec2(rect.x + 12.0, rect.y + 50.0),
                    12.0,
                    INK,
                    292.0,
                );
                plain(canvas, &a.font, "Enter: the full report", vec2(rect.x + 12.0, rect.y + 70.0), 11.0, Color::rgb(110, 80, 50));
            }
        }
        if let Some(k) = self.map_ui.war.full {
            if k < w.reports.len() {
                ui::dim(canvas, 0.55);
                self.draw_report_paper(canvas, w, k, PAPER);
                plain(canvas, &a.font, "Enter or Esc: close", vec2(PAPER.x + 400.0, PAPER.y + 438.0), 12.0, SOFT);
            }
        }
    }

    fn draw_report_paper(&self, canvas: &mut Canvas, w: &Wars, k: usize, at: Vec2) {
        let a = &self.assets;
        canvas.blit_sprite(&a.report_paper, at, Blit::default());
        canvas.label("battle report", Rect::new(at.x, at.y, 560.0, 430.0));
        let text = battle_text(w, &w.reports[k]);
        let mut y = at.y + 22.0;
        for (n, line) in text.lines().enumerate() {
            let (size, color, font) = if n == 0 { (19.0, INK, &a.serif) } else if line.starts_with("ATTACKERS") || line.starts_with("DEFENDERS") { (13.0, Color::rgb(120, 30, 20), &a.font) } else { (11.5, INK, &a.font) };
            fit(canvas, font, line.trim_start(), vec2(at.x + 30.0 + if line.starts_with("  ") { 10.0 } else { 0.0 }, y), size, color, 500.0);
            y += if n == 0 { 26.0 } else { 12.6 };
        }
    }

    /// The war room window.
    pub(super) fn draw_war_room(&self, canvas: &mut Canvas) {
        let (Some(page), Some(w), Some(e)) = (self.map_ui.war.open, self.wars.as_ref(), self.economy.as_ref()) else { return };
        let a = &self.assets;
        ui::dim(canvas, 0.45);
        canvas.blit_sprite(&a.econ_window, vec2(WIN.x, WIN.y), Blit::default());
        canvas.label("war room", WIN);
        let icons = [&a.icon_swords, &a.icon_army, &a.icon_crate, &a.icon_chart];
        for (k, p) in Page::ALL.iter().enumerate() {
            let r = Page::tab_rect(k);
            let on = *p == page;
            let hot = self.ui_mouse().is_some_and(|m| r.contains(m));
            canvas.fill_rect(r, if on { Color::rgba(240, 106, 106, 80) } else if hot { Color::rgba(255, 255, 255, 30) } else { Color::rgba(255, 255, 255, 12) });
            canvas.blit_sprite(icons[k], vec2(r.x + 10.0, r.y + 8.0), Blit { scale: if k == 0 { 0.75 } else { 1.0 }, ..Blit::default() });
            plain(canvas, &a.font, p.name(), vec2(r.x + 42.0, r.y + 11.0), 15.0, if on { GOLD } else { Color::WHITE });
            canvas.label(&format!("tab {}", p.name()), r);
        }
        let v = self.war_viewer();
        right(canvas, &a.font, &format!("{} · {}", w.names[v], if self.spectating { "watching" } else { "your war room" }), vec2(WIN.right() - 28.0, WIN.y + 28.0), 13.0, SOFT);
        let row = self.map_ui.war.row;
        match page {
            Page::Wars => self.draw_wars_page(canvas, w, e, row),
            Page::Army => self.draw_army_page(canvas, w, v, row),
            Page::Supply => self.draw_supply_page(canvas, w, e, v, row),
            Page::Reports => self.draw_reports_page(canvas, w, row),
        }
        let help = match (page, self.spectating) {
            (Page::Reports, _) => "Up/Down: choose a battle  ·  Enter: the full report  ·  Tab: next page  ·  K or Esc: close",
            (_, true) => "Watching: the war room only shows  ·  Up/Down: choose  ·  Tab: next page  ·  K or Esc: close",
            (Page::Wars, _) => "E: escalate · M: match · B: back down · P: offer peace · Y/N: their offer · D: declare on the selected nation · L: level · Tab · K: close",
            (Page::Army, _) => "Up/Down: a front  ·  A: attack · H: hold · G: dig in · R: retreat · F: reinforce the selected province · U: back to the general",
            (Page::Supply, _) => "Up/Down: choose  ·  Left/Right: a front's priority  ·  Y/N: approve or deny a request  ·  T: Auto on/off  ·  Tab: next page",
        };
        plain(canvas, &a.font, help, vec2(WIN.x + 26.0, WIN.bottom() - 30.0), 13.0, SOFT);
    }

    fn draw_wars_page(&self, canvas: &mut Canvas, w: &Wars, e: &Economy, row: usize) {
        let a = &self.assets;
        let ed: &EconData = &self.econ_data;
        let (x, y) = (WIN.x + 26.0, WIN.y + 76.0);
        let list = player::wars_in_view(w);
        plain(canvas, &a.font, "WARS (yours first, then the world's, then the latest ended)", vec2(x, y), 12.0, GOLD);
        let per_row = 30.0;
        let shown = 14usize;
        let first = row.saturating_sub(shown - 1);
        for (n, &k) in list.iter().enumerate().skip(first).take(shown) {
            let wr = &w.wars[k];
            let ry = y + 20.0 + (n - first) as f32 * per_row;
            let r = Rect::new(x - 6.0, ry - 4.0, 500.0, per_row - 3.0);
            if n == row {
                canvas.fill_rect(r, Color::rgba(240, 106, 106, 55));
            }
            let mine = player::side(w, k).is_some() || self.spectating && wr.side_of(self.war_viewer()).is_some();
            let color = if !wr.active() { MUTED } else { [Color::rgb(140, 160, 255), WARN, Color::rgb(255, 150, 70), BAD, Color::rgb(255, 60, 60)][wr.level] };
            canvas.fill_rect(Rect::new(x, ry + 3.0, 8.0, 14.0), color);
            let names = |s: usize| wr.sides[s].iter().map(|&f| w.names[f].clone()).collect::<Vec<_>>().join(" + ");
            fit(canvas, &a.font, &format!("{} vs {}", names(0), names(1)), vec2(x + 16.0, ry), 15.0, if mine { GOLD } else { Color::WHITE }, 300.0);
            let status = if wr.active() { LEVELS[wr.level].to_string() } else { "ended".to_string() };
            right(canvas, &a.font, &status, vec2(x + 490.0, ry + 1.0), 13.0, color);
        }
        if list.is_empty() {
            plain(canvas, &a.font, "No wars. Select a nation's province on the map, then D to declare.", vec2(x, y + 24.0), 14.0, MUTED);
        }
        // The chosen war.
        let dx = WIN.x + 560.0;
        let mut dy = y;
        if let Some(&k) = list.get(row) {
            let wr = &w.wars[k];
            let names = |s: usize| wr.sides[s].iter().map(|&f| w.names[f].clone()).collect::<Vec<_>>().join(" + ");
            fit(canvas, &a.serif, &format!("{} vs {}", names(0), names(1)), vec2(dx, dy), 20.0, Color::WHITE, 470.0);
            dy += 30.0;
            let goal = self.world.as_ref().map_or(String::new(), |wd| wr.goal.words(wd, &self.atlas));
            fit(canvas, &a.font, &format!("Why: {}  ·  goal: {goal}", wr.cause), vec2(dx, dy), 13.0, SOFT, 470.0);
            dy += 20.0;
            let year = |d: u32| d as f64 / ed.days_per_year as f64 + 1.0;
            fit(canvas, &a.font, &format!("Since year {:.1} ({} days) · {} battles{}", year(wr.start), wr.end.unwrap_or(w.day) - wr.start, wr.battles, if wr.civil { " · civil war" } else { "" }), vec2(dx, dy), 13.0, SOFT, 470.0);
            dy += 24.0;
            // The ladder.
            self.tip(Rect::new(dx, dy, 470.0, 34.0), "level");
            for l in 0..5 {
                let r = Rect::new(dx + l as f32 * 94.0, dy, 90.0, 34.0);
                let on = l == wr.level && wr.active();
                canvas.fill_rect(r, if on { Color::rgba(240, 106, 106, 140) } else if l <= wr.max_level { Color::rgba(255, 255, 255, 34) } else { Color::rgba(255, 255, 255, 10) });
                fit(canvas, &a.font, LEVELS[l], vec2(r.x + 5.0, r.y + 4.0), 12.0, if on { Color::WHITE } else { SOFT }, 82.0);
                plain(canvas, &a.font, &format!("{l}"), vec2(r.x + 5.0, r.y + 18.0), 11.0, MUTED);
            }
            dy += 44.0;
            if let Some((by, level, deadline)) = wr.pending {
                let who = w.names[wr.sides[by][0]].clone();
                let days = deadline.saturating_sub(w.day);
                let ours = player::side(w, k).is_some_and(|s| s != by);
                let words = format!("{who} escalated to {}: {} {days} days to match or back down", LEVELS[level], if ours { "you have" } else { "the other side has" });
                fit(canvas, &a.font, &words, vec2(dx, dy), 14.0, if ours { BAD } else { WARN }, 470.0);
                dy += 22.0;
            }
            // The score and losses.
            let score = peace::score(w, e, k);
            let mine = player::side(w, k).unwrap_or(0);
            let s = if mine == 0 { score } else { -score };
            plain(canvas, &a.font, &format!("WAR SCORE ({} view): {:+}", w.names[wr.sides[mine][0]], s.round() as i64), vec2(dx, dy), 12.0, GOLD);
            self.tip(Rect::new(dx, dy, 470.0, 30.0), "war_score");
            let bar = Rect::new(dx, dy + 18.0, 470.0, 10.0);
            canvas.fill_rect(bar, Color::rgba(255, 255, 255, 25));
            let mid = bar.x + bar.w / 2.0;
            let len = (s as f32 / 100.0) * bar.w / 2.0;
            canvas.fill_rect(if len >= 0.0 { Rect::new(mid, bar.y, len, bar.h) } else { Rect::new(mid + len, bar.y, -len, bar.h) }, if s >= 0.0 { GOOD } else { BAD });
            dy += 38.0;
            let l = &wr.losses;
            let cols = [dx + 140.0, dx + 300.0];
            plain(canvas, &a.font, "LOSSES", vec2(dx, dy), 12.0, GOLD);
            self.tip_words(Rect::new(dx, dy, 470.0, 170.0), "Losses on each side of this war: the dead, wounded, captured and deserted, and the tanks, jets and ships destroyed or captured. Your own are counted; the enemy's are your service's count.".to_string());
            for (c, name) in cols.iter().zip([names(0), names(1)]) {
                fit(canvas, &a.font, &name, vec2(*c, dy), 12.0, Color::WHITE, 150.0);
            }
            dy += 18.0;
            let rows: [(&str, fn(&war::Losses) -> f64); 8] = [
                ("dead", |x| x.killed),
                ("wounded", |x| x.wounded),
                ("captured", |x| x.captured),
                ("deserted", |x| x.deserted),
                ("tanks", |x| x.tanks),
                ("jets", |x| x.jets),
                ("warships", |x| x.warships),
                ("ammo (units)", |x| x.ammo),
            ];
            for (name, get) in rows {
                plain(canvas, &a.font, name, vec2(dx, dy), 13.0, SOFT);
                for (s, c) in cols.iter().enumerate() {
                    plain(canvas, &a.font, &big(get(&l[s])), vec2(*c, dy), 13.0, Color::WHITE);
                }
                dy += 17.0;
            }
            dy += 8.0;
            if let Some(res) = &wr.result {
                fit(canvas, &a.font, &format!("Result: {res}"), vec2(dx, dy), 14.0, GOOD, 470.0);
            } else if player::side(w, k).is_some() {
                if let Some((t, yes)) = player::peace_on_offer(w, e, k) {
                    fit(canvas, &a.font, &format!("P: offer peace ({})", t.words(w)), vec2(dx, dy), 13.0, Color::WHITE, 470.0);
                    plain(canvas, &a.font, if yes { "They would sign it today." } else { "They would refuse it today." }, vec2(dx, dy + 18.0), 13.0, if yes { GOOD } else { BAD });
                }
                if let Some(o) = w.offers.iter().find(|o| o.0 == k) {
                    fit(canvas, &a.font, &format!("THEIR OFFER: {}  ·  Y: sign · N: refuse", o.2.words(w)), vec2(dx, dy + 40.0), 13.0, WARN, 470.0);
                }
            }
        }
        // Declaring.
        let target = self.map_ui.selected.and_then(|p| w.owner[p]).filter(|&o| o != 0 || self.spectating);
        let words = match target {
            Some(t) if !self.spectating => format!("D: declare on {} as a {} (L: change)", w.names[t], LEVELS[self.map_ui.war.declare_level].to_lowercase()),
            _ => format!("To declare: select a province of theirs on the map, then D (level: {})", LEVELS[self.map_ui.war.declare_level].to_lowercase()),
        };
        fit(canvas, &a.font, &words, vec2(dx, WIN.bottom() - 58.0), 13.0, WARN, 470.0);
    }

    fn draw_army_page(&self, canvas: &mut Canvas, w: &Wars, v: usize, row: usize) {
        let a = &self.assets;
        let (x, mut y) = (WIN.x + 26.0, WIN.y + 76.0);
        let f = &w.forces[v];
        let e = self.economy.as_ref();
        let stock = |g: usize| e.map_or(0.0, |e| e.nations[v].stockpile[g]);
        let ed = &self.econ_data;
        let (soldiers, tanks) = w.strength(v);
        let held_by_them: f64 = (0..w.prisoners.len()).map(|c| w.prisoners[c][v]).sum();
        let held: f64 = w.prisoners[v].iter().sum();
        let facts = format!(
            "{} divisions · {} soldiers · {} tanks · {} jets ({} pilots) · {} warships · {} transports · {} in hospital · {} prisoners held, {} of ours held",
            w.divisions.iter().filter(|d| d.alive && d.faction == v).count(),
            big(soldiers),
            big(tanks),
            big(stock(ed.jets)),
            big(f.pilots),
            big(stock(ed.warships)),
            big(stock(ed.transports)),
            big(f.hospital),
            big(held),
            big(held_by_them)
        );
        fit(canvas, &a.font, &facts, vec2(x, y), 14.0, Color::WHITE, 1000.0);
        y += 26.0;
        let cols = [0.0, 150.0, 240.0, 350.0, 440.0, 530.0, 620.0, 720.0];
        let heads = ["FRONT", "DIVISIONS", "SOLDIERS", "TANKS", "EQUIPPED", "MORALE", "IN BATTLE", "ORDERS"];
        for (c, h) in cols.iter().zip(heads) {
            plain(canvas, &a.font, h, vec2(x + c, y), 11.0, GOLD);
        }
        y += 18.0;
        for (fr, name) in FRONTS.iter().enumerate() {
            let ds: Vec<&war::Division> = w.divisions.iter().filter(|d| d.alive && d.faction == v && w.front_of(v, d.province) == fr).collect();
            let r = Rect::new(x - 6.0, y - 3.0, 1010.0, 24.0);
            if fr == row {
                canvas.fill_rect(r, Color::rgba(240, 106, 106, 55));
            }
            let men: f64 = ds.iter().map(|d| d.soldiers).sum();
            let avg = |g: &dyn Fn(&war::Division) -> f64| if men > 0.0 { ds.iter().map(|d| g(d) * d.soldiers).sum::<f64>() / men } else { 0.0 };
            let eq = avg(&|d| land::equipped(w, d));
            let mo = avg(&|d| d.morale);
            let orders: Vec<String> = {
                let mut o: Vec<String> = ds.iter().map(|d| order_name(w, d.order)).collect();
                o.sort();
                o.dedup();
                o
            };
            let vals = [
                name.to_string(),
                ds.len().to_string(),
                big(men),
                big(ds.iter().map(|d| d.tanks).sum()),
                pct(eq),
                pct(mo),
                ds.iter().filter(|d| d.battle.is_some()).count().to_string(),
                orders.join(", "),
            ];
            for (k, (c, val)) in cols.iter().zip(vals.iter()).enumerate() {
                let color = match k {
                    4 => if eq > 0.8 { GOOD } else if eq > 0.5 { WARN } else { BAD },
                    5 => if mo > 0.6 { GOOD } else if mo > 0.35 { WARN } else { BAD },
                    _ => Color::WHITE,
                };
                fit(canvas, &a.font, val, vec2(x + c, y), 14.0, color, if k == 7 { 280.0 } else { 120.0 });
            }
            y += 24.0;
        }
        y += 10.0;
        plain(canvas, &a.font, &format!("{} FRONT: ITS DIVISIONS", FRONTS[row.min(4)].to_uppercase()), vec2(x, y), 11.0, GOLD);
        y += 18.0;
        let dcols = [0.0, 190.0, 380.0, 470.0, 545.0, 615.0, 685.0, 760.0, 850.0];
        for (c, h) in dcols.iter().zip(["DIVISION", "GENERAL", "SOLDIERS", "TRAINED", "EXPER.", "MORALE", "EQUIP.", "TANKS", "WHERE / DOING"]) {
            plain(canvas, &a.font, h, vec2(x + c, y), 11.0, MUTED);
        }
        y += 16.0;
        let map = self.world.as_ref().map(|wd| self.atlas.map(wd.map));
        let mut shown = 0;
        for d in w.divisions.iter().filter(|d| d.alive && d.faction == v && w.front_of(v, d.province) == row.min(4)) {
            if y > WIN.bottom() - 54.0 {
                break;
            }
            let g = &w.generals[d.general];
            let doing = if d.battle.is_some() {
                "fighting".to_string()
            } else if let Some((to, _, sea)) = d.moving {
                format!("{} {}", if sea { "sailing to" } else { "marching to" }, map.map_or(String::new(), |m| m.provinces[to].name.clone()))
            } else {
                order_name(w, d.order)
            };
            let vals = [
                d.name.clone(),
                format!("{} ({:.2}, {})", g.name, g.skill, g.style.name()),
                big(d.soldiers),
                pct(d.training),
                pct(d.experience),
                pct(d.morale),
                pct(land::equipped(w, d)),
                big(d.tanks),
                format!("{} · {doing}", map.map_or(String::new(), |m| m.provinces[d.province].name.clone())),
            ];
            for (k, (c, val)) in dcols.iter().zip(vals.iter()).enumerate() {
                fit(canvas, &a.font, val, vec2(x + c, y), 12.5, Color::WHITE, if k == 8 { 160.0 } else if k < 2 { 180.0 } else { 80.0 });
            }
            y += 18.0;
            shown += 1;
        }
        if shown == 0 {
            plain(canvas, &a.font, "No divisions on this front.", vec2(x, y), 13.0, MUTED);
        }
    }

    fn draw_supply_page(&self, canvas: &mut Canvas, w: &Wars, e: &Economy, v: usize, row: usize) {
        let a = &self.assets;
        let ed = &self.econ_data;
        let (x, mut y) = (WIN.x + 26.0, WIN.y + 76.0);
        let f = &w.forces[v];
        let line = supply::lines(w, e, v);
        plain(canvas, &a.font, "FRONTS: who gets supplies first", vec2(x, y), 12.0, GOLD);
        right(canvas, &a.font, if f.auto || v != 0 { "AUTO: the generals' requests are answered by priority" } else { "AUTO OFF: you answer every request" }, vec2(x + 1000.0, y), 12.0, if f.auto { SOFT } else { WARN });
        y += 20.0;
        for (fr, name) in FRONTS.iter().enumerate() {
            let r = Rect::new(x - 6.0, y - 3.0, 480.0, 24.0);
            if fr == row {
                canvas.fill_rect(r, Color::rgba(240, 106, 106, 55));
            }
            let ds: Vec<&war::Division> = w.divisions.iter().filter(|d| d.alive && d.faction == v && w.front_of(v, d.province) == fr).collect();
            let arrive: Vec<f64> = ds.iter().map(|d| line[d.province].map_or(0.0, |l| l.0)).collect();
            let avg = if arrive.is_empty() { 1.0 } else { arrive.iter().sum::<f64>() / arrive.len() as f64 };
            let cut = ds.iter().filter(|d| line[d.province].is_none()).count();
            plain(canvas, &a.font, name, vec2(x, y), 14.0, Color::WHITE);
            let p = f.priority[fr] as usize;
            plain(canvas, &a.font, &format!("< {} >", PRIORITY[p.min(3)]), vec2(x + 110.0, y), 14.0, [BAD, WARN, Color::WHITE, GOOD][p.min(3)]);
            let words = if ds.is_empty() { "no divisions".to_string() } else if cut > 0 { format!("{} arrives · {cut} cut off!", pct(avg)) } else { format!("{} of what's sent arrives", pct(avg)) };
            plain(canvas, &a.font, &words, vec2(x + 240.0, y), 13.0, if cut > 0 { BAD } else if avg > 0.85 { GOOD } else { WARN });
            y += 24.0;
        }
        // The stockpile.
        y += 12.0;
        plain(canvas, &a.font, "THE STOCKPILE (made at home or bought; every shell fired comes out of it)", vec2(x, y), 12.0, GOLD);
        y += 20.0;
        let n = &e.nations[v];
        let goods = [
            ("ammo", n.stockpile[ed.ammo], f.flows.used[ed.ammo]),
            ("rifles", n.stockpile[ed.rifles], f.flows.lost[ed.rifles]),
            ("tanks", n.stockpile[ed.tanks], f.flows.lost[ed.tanks]),
            ("jets", n.stockpile[ed.jets], f.flows.lost[ed.jets]),
            ("warships", n.stockpile[ed.warships], f.flows.lost[ed.warships]),
            ("transports", n.stockpile[ed.transports], f.flows.lost[ed.transports]),
            ("fuel (stock)", n.stock[ed.fuel], 0.0),
            ("food (stock)", n.stock[ed.food], 0.0),
        ];
        for (k, (name, have, gone)) in goods.iter().enumerate() {
            let cx = x + (k % 2) as f32 * 240.0;
            let cy = y + (k / 2) as f32 * 20.0;
            plain(canvas, &a.font, name, vec2(cx, cy), 13.0, SOFT);
            plain(canvas, &a.font, &big(*have), vec2(cx + 100.0, cy), 13.0, Color::WHITE);
            if *gone > 0.0 {
                plain(canvas, &a.font, &format!("-{}", big(*gone)), vec2(cx + 170.0, cy), 12.0, BAD);
            }
        }
        plain(canvas, &a.font, "red: used (ammo) or lost (weapons) in all wars so far", vec2(x, y + 84.0), 11.0, MUTED);
        // The generals' requests.
        let rx = x + 510.0;
        let mut ry = WIN.y + 100.0;
        plain(canvas, &a.font, "THE GENERALS ASK", vec2(rx, ry), 12.0, GOLD);
        ry += 20.0;
        let list = request_list(w, v);
        if list.is_empty() {
            plain(canvas, &a.font, "No requests: the fronts have what they need (or there is no war).", vec2(rx, ry), 13.0, MUTED);
        }
        for (n, &k) in list.iter().enumerate() {
            if ry > WIN.bottom() - 60.0 {
                break;
            }
            let r = &f.requests[k];
            let rr = Rect::new(rx - 6.0, ry - 3.0, 500.0, 40.0);
            if row == FRONTS.len() + n {
                canvas.fill_rect(rr, Color::rgba(240, 106, 106, 55));
            }
            fit(canvas, &a.font, &format!("“{}”", r.words()), vec2(rx, ry), 14.0, Color::WHITE, 490.0);
            let (status, color) = match r.answered {
                None => ("waiting for you  ·  Y: approve · N: deny".to_string(), WARN),
                Some(true) => ("approved: it goes out as supplies allow".to_string(), GOOD),
                Some(false) => ("denied".to_string(), BAD),
            };
            plain(canvas, &a.font, &format!("day {} · {status}", r.day), vec2(rx, ry + 18.0), 12.0, color);
            ry += 42.0;
        }
    }

    fn draw_reports_page(&self, canvas: &mut Canvas, w: &Wars, row: usize) {
        let a = &self.assets;
        let (x, y) = (WIN.x + 26.0, WIN.y + 76.0);
        let list = self.report_list();
        plain(canvas, &a.font, "BATTLE REPORTS (newest first)", vec2(x, y), 12.0, GOLD);
        if list.is_empty() {
            plain(canvas, &a.font, "No battles yet.", vec2(x, y + 24.0), 14.0, MUTED);
            return;
        }
        let shown = 17usize;
        let first = row.saturating_sub(shown - 1);
        for (n, &k) in list.iter().enumerate().skip(first).take(shown) {
            let r = &w.reports[k];
            let ry = y + 20.0 + (n - first) as f32 * 26.0;
            if n == row {
                canvas.fill_rect(Rect::new(x - 6.0, ry - 3.0, 420.0, 24.0), Color::rgba(240, 106, 106, 55));
            }
            let dead = r.attacker.killed + r.defender.killed;
            fit(canvas, &a.font, &format!("{} · day {} · {} dead · {}", r.province_name, r.day_end, big(dead), if r.winner == 0 { "taken" } else { "held" }), vec2(x, ry), 14.0, Color::WHITE, 410.0);
        }
        if let Some(&k) = list.get(row) {
            self.draw_report_paper(canvas, w, k, vec2(WIN.x + 470.0, WIN.y + 76.0));
        }
    }
}

/// A player's order in words.
fn order_name(w: &Wars, o: Order) -> String {
    match o {
        Order::Auto => "the general decides".to_string(),
        Order::Attack(q) => format!("attack {}", w.geo.names[q]),
        Order::Hold => "hold".to_string(),
        Order::DigIn => "dig in".to_string(),
        Order::Retreat => "retreat".to_string(),
        Order::Reinforce(q) => format!("reinforce {}", w.geo.names[q]),
    }
}

/// A faction's requests, waiting ones first, then the latest answered.
fn request_list(w: &Wars, f: usize) -> Vec<usize> {
    let r = &w.forces[f].requests;
    let mut out: Vec<usize> = (0..r.len()).rev().filter(|&k| r[k].answered.is_none()).collect();
    out.extend((0..r.len()).rev().filter(|&k| r[k].answered.is_some()).take(8));
    out
}

/// What the war layer shows, from the war's state.
pub fn war_scene(w: &Wars, e: &Economy, ed: &EconData, world: &crate::world::World, atlas: &crate::mapdata::Atlas, all: bool) -> WarScene {
    let map = atlas.map(world.map);
    let mut scene = WarScene::default();
    let fighting = |f: usize| w.wars.iter().any(|x| x.active() && x.level >= 1 && x.side_of(f).is_some());
    // Armies: per province and faction, soldiers and tanks.
    let mut groups: std::collections::BTreeMap<(usize, usize), (f64, f64, Option<usize>)> = std::collections::BTreeMap::new();
    for d in w.divisions.iter().filter(|d| d.alive) {
        if !all && !fighting(d.faction) {
            continue;
        }
        if let Some((to, _, true)) = d.moving {
            // At sea: a transport halfway.
            let (a, b) = (map.provinces[d.province].middle, map.provinces[to].middle);
            scene.pieces.push(WarPiece { piece: Piece::Transport, cell: None, at: ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0), lift: 0.0, color: faction_color(w, world, d.faction), turn: (b.0 - a.0).atan2(b.1 - a.1) });
            continue;
        }
        let g = groups.entry((d.province, d.faction)).or_insert((0.0, 0.0, None));
        g.0 += d.soldiers;
        g.1 += d.tanks - d.tanks_broken;
        if let Some((to, _, false)) = d.moving {
            g.2 = Some(to);
        }
    }
    let mut slot: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
    for (&(p, f), &(men, tanks, to)) in &groups {
        let k = *slot.entry(p).and_modify(|x| *x += 1).or_insert(0);
        let prov = &map.provinces[p];
        let cell = map.index(prov.at.0, prov.at.1);
        let (cx, cy) = crate::hex::center(prov.at.0, prov.at.1);
        let angle = k as f32 * 2.1 + 0.4;
        let base = (cx + 0.75 * angle.cos(), cy + 0.75 * angle.sin());
        let turn = match to {
            Some(t) => {
                let m = map.provinces[t].middle;
                (m.0 - base.0).atan2(base.1 - m.1)
            }
            None => angle,
        };
        let color = faction_color(w, world, f);
        let figures = ((men / 1000.0).round() as usize).clamp(1, 16);
        for n in 0..figures {
            let (gx, gy) = ((n % 4) as f32 - 1.5, (n / 4) as f32 - 1.5);
            scene.pieces.push(WarPiece { piece: Piece::Soldier, cell: Some(cell), at: (base.0 + gx * 0.13, base.1 + gy * 0.13), lift: 0.0, color, turn });
        }
        let armour = ((tanks / 100.0).round() as usize).min(6);
        for n in 0..armour {
            scene.pieces.push(WarPiece { piece: Piece::Tank, cell: Some(cell), at: (base.0 + (n as f32 - 2.5) * 0.2, base.1 + 0.42), lift: 0.0, color, turn });
        }
        if let Some(t) = to {
            scene.arrows.push((p, t, ARROW));
        }
    }
    // Battles: arrows from where the attackers came.
    for b in w.battles.iter().filter(|b| !b.over) {
        for &from in &b.from {
            if from != b.province && !scene.arrows.iter().any(|x| x.0 == from && x.1 == b.province) {
                scene.arrows.push((from, b.province, ARROW));
            }
        }
    }
    // Jets over the capitals of nations at war (one for about 40 jets).
    for f in 0..w.nations {
        if w.level_of(f) < 2 {
            continue;
        }
        let jets = e.nations[f].stockpile[ed.jets];
        let n = ((jets / 40.0).round() as usize).min(5);
        let prov = &map.provinces[w.capital[f]];
        let (cx, cy) = crate::hex::center(prov.at.0, prov.at.1);
        let cell = map.index(prov.at.0, prov.at.1);
        for k in 0..n {
            let a = k as f32 * 1.25;
            scene.pieces.push(WarPiece { piece: Piece::Jet, cell: Some(cell), at: (cx + 0.9 * a.cos(), cy + 0.9 * a.sin()), lift: 0.9 + 0.1 * k as f32, color: world.color[f], turn: a + 1.57 });
        }
    }
    // Fleets at sea (one warship for about 8).
    for (z, fleets) in w.zone_fleets.iter().enumerate() {
        let (zx, zy) = w.geo.zone_middle[z];
        for (k, &(f, ships)) in fleets.iter().enumerate() {
            if ships < 1.0 || (!all && !fighting(f)) {
                continue;
            }
            let n = ((ships / 8.0).round() as usize).clamp(1, 5);
            for s in 0..n {
                scene.pieces.push(WarPiece { piece: Piece::Warship, cell: None, at: (zx + (s as f32 - 2.0) * 0.45, zy + k as f32 * 0.7), lift: 0.0, color: faction_color(w, world, f), turn: 1.57 });
            }
        }
    }
    // Front lines and occupation.
    for p in 0..w.control.len() {
        let Some(a) = w.control[p] else { continue };
        for &q in &w.geo.neighbours[p] {
            if q <= p {
                continue;
            }
            if let Some(b) = w.control[q] {
                if a != b && w.at_war(a, b).is_some_and(|k| w.wars[k].level >= 1) {
                    scene.fronts.push((p, q));
                }
            }
        }
        if w.owner[p].is_some() && w.owner[p] != Some(a) {
            scene.occupied.push((p, faction_color(w, world, a)));
        }
    }
    scene
}
