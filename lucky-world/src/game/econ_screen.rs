//! The economy on the map screen: the game clock, the economy window (budget,
//! production chains, market, trade, stockpile, charts) and the map's
//! economy overlays.
//!
//! | | |
//! |---|---|
//! | Clock | Space: pause · 1-5: speed (1, 3, 7, 15 or 30 days a second) |
//! | Window | O: open or close · Tab: next page · Up / Down: choose a row |
//! | Budget | Left / Right: lower or raise · A: hand taxes back to the treasury, and the rate to the bank |
//! | Chains | B: build more of the chosen good's maker · X: close some · R: a road where goods are stuck · P: a port level |
//! | Market | T: free trade → no exports → no imports |
//! | Trade | X: embargo the chosen nation (or lift it) |
//! | Map | V: resources overlay → industry overlay → off |
//!
//! The simulation only moves in `update` (whole days, never more than a
//! month a frame), so drawing never changes the economy.

use duckforge::prelude::*;

use super::Lucky;
use crate::econ::data::Tier;
use crate::econ::player::Lever;
use crate::econ::{EconData, Economy, NationEcon, TradePolicy};
use crate::ui::{self, GOLD, MUTED, SOFT};

/// Days per second at speeds 1 to 5.
pub const SPEEDS: [f32; 6] = [1.0, 3.0, 7.0, 15.0, 30.0, 90.0];
/// Spectator mode's own top speed (key 6): three months a second.
pub const SPECTATOR_SPEED: usize = 5;
/// Where the economy window sits.
const WIN: Rect = Rect { x: 110.0, y: 64.0, w: 1060.0, h: 600.0 };
/// The economy button on the top bar.
pub const ECON_BUTTON: Rect = Rect { x: 1086.0, y: 30.0, w: 180.0, h: 22.0 };
const GOOD: Color = Color::rgb(107, 212, 138);
const WARN: Color = Color::rgb(245, 197, 66);
const BAD: Color = Color::rgb(240, 106, 106);

/// The game clock: one tick is one day.
#[derive(Debug, Clone)]
pub struct Clock {
    pub paused: bool,
    /// 0..5, into `SPEEDS`.
    pub speed: usize,
    carry: f32,
    last_month: u32,
    last_news: usize,
}

impl Default for Clock {
    fn default() -> Clock {
        Clock { paused: false, speed: 1, carry: 0.0, last_month: 0, last_news: 0 }
    }
}

/// The economy window's pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Budget,
    Chains,
    Market,
    Trade,
    Stockpile,
    Charts,
}

impl Tab {
    pub const ALL: [Tab; 6] = [Tab::Budget, Tab::Chains, Tab::Market, Tab::Trade, Tab::Stockpile, Tab::Charts];

    fn name(self) -> &'static str {
        match self {
            Tab::Budget => "BUDGET",
            Tab::Chains => "CHAINS",
            Tab::Market => "MARKET",
            Tab::Trade => "TRADE",
            Tab::Stockpile => "STOCKPILE",
            Tab::Charts => "CHARTS",
        }
    }

    fn rows(self, ed: &EconData, e: &Economy) -> usize {
        match self {
            Tab::Budget => Lever::ALL.len(),
            Tab::Chains | Tab::Market | Tab::Charts => ed.goods.len(),
            Tab::Trade => e.nations.len().saturating_sub(1),
            Tab::Stockpile => 1,
        }
    }

    fn tab_rect(k: usize) -> Rect {
        Rect::new(WIN.x + 24.0 + k as f32 * 168.0, WIN.y + 16.0, 160.0, 40.0)
    }
}

/// The map's economy overlays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Overlay {
    #[default]
    Off,
    /// Each province by its richest deposit.
    Resources,
    /// Each province by what its buildings make (W$ a day).
    Industry,
    /// Who holds each province (occupied land in the holder's colour,
    /// battles in red), with every army shown.
    War,
    /// Each bloc of alliances and defensive pacts in its own colour.
    Alliances,
    /// Relations with the player (watching: the chosen nation), green to red.
    Relations,
}

/// What the economy screens remember between frames.
#[derive(Debug, Default)]
pub struct EconUi {
    pub open: Option<Tab>,
    pub row: usize,
    pub overlay: Overlay,
    overlay_month: u32,
}

impl EconUi {
    /// The overlay is rebuilt on the next frame.
    pub fn force_overlay(&mut self) {
        self.overlay_month = u32::MAX;
    }
}

/// A colour for each raw good (the resources overlay and the chains).
fn good_color(ed: &EconData, g: usize) -> Color {
    let palette = [
        0xE8C547, 0xB0533C, 0x3A3A3A, 0x7A4FC0, 0x3F8F4A, 0xC27B4B, 0x8FA3B8, 0xE0A040, 0xFFF07A, 0xCFD8E6, 0x9A6FD0, 0x7CC46A, 0xE07AB0,
        0xC84B3A, 0x6B7B5A, 0x9BB8E8,
    ];
    let _ = ed;
    Color::from_hex(palette[g % palette.len()])
}

/// W$ with a sensible unit.
pub fn money(w: f64) -> String {
    let a = w.abs();
    let sign = if w < 0.0 { "-" } else { "" };
    if a >= 1e12 {
        format!("{sign}${:.2}T", a / 1e12)
    } else if a >= 1e9 {
        format!("{sign}${:.1}B", a / 1e9)
    } else if a >= 1e6 {
        format!("{sign}${:.1}M", a / 1e6)
    } else if a >= 1e3 {
        format!("{sign}${:.0}k", a / 1e3)
    } else {
        format!("{sign}${a:.0}")
    }
}

/// People, rounded.
pub fn people(p: f64) -> String {
    if p >= 1e9 {
        format!("{:.2}B", p / 1e9)
    } else if p >= 1e6 {
        format!("{:.1}M", p / 1e6)
    } else {
        format!("{:.0}k", p / 1e3)
    }
}

fn amount(x: f64) -> String {
    let a = x.abs();
    if a >= 1e9 {
        format!("{:.1}B", x / 1e9)
    } else if a >= 1e6 {
        format!("{:.1}M", x / 1e6)
    } else if a >= 1e4 {
        format!("{:.0}k", x / 1e3)
    } else if a >= 10.0 {
        format!("{x:.0}")
    } else {
        format!("{x:.1}")
    }
}

fn pct(x: f64) -> String {
    format!("{:.1}%", x * 100.0)
}

fn plain(canvas: &mut Canvas, font: &Font, words: &str, at: Vec2, size: f32, color: Color) {
    ui::plain(canvas, font, words, at, size, color, Align::Left);
}

fn right(canvas: &mut Canvas, font: &Font, words: &str, at: Vec2, size: f32, color: Color) {
    ui::plain(canvas, font, words, at, size, color, Align::Right);
}

/// How well a good is going at home: 1 fine, 0 stopped.
fn supply_ratio(n: &NationEcon, g: usize) -> f64 {
    if n.wanted[g] > 1e-9 { (n.got[g] / n.wanted[g]).clamp(0.0, 1.0) } else { 1.0 }
}

fn status_color(r: f64) -> Color {
    if r > 0.97 {
        GOOD
    } else if r > 0.6 {
        WARN
    } else {
        BAD
    }
}

impl Lucky {
    /// The clock: whole days, at the chosen speed, unless paused.
    pub(super) fn update_clock(&mut self, ctx: &mut Context) {
        let Some(e) = self.economy.as_mut() else { return };
        let ed = &self.econ_data;
        let keys = [Key::Digit1, Key::Digit2, Key::Digit3, Key::Digit4, Key::Digit5, Key::Digit6];
        let top = if self.spectating { SPECTATOR_SPEED } else { SPECTATOR_SPEED - 1 };
        for (k, key) in keys.into_iter().enumerate() {
            if k <= top && ctx.input.was_pressed(key) {
                self.clock.speed = k;
                self.clock.paused = false;
                ctx.play_with(&self.assets.click, 0.6, 0.0);
            }
        }
        if !self.clock.paused {
            self.clock.carry += ctx.dt() * SPEEDS[self.clock.speed];
            let days = (self.clock.carry.floor() as u32).min(30);
            self.clock.carry -= days as f32;
            self.clock.carry = self.clock.carry.min(1.0);
            for _ in 0..days {
                e.tick(ed);
                // The NPCs think after each day, through the same levers.
                if let Some(b) = self.brains.as_mut() {
                    b.day(e, ed);
                }
                // Then the war: battles hour by hour, supply, air, sea, the ladder.
                if let Some(w) = self.wars.as_mut() {
                    w.day(e, ed, self.brains.as_mut());
                    // Then diplomacy: treaties, calls to arms, talks, spies.
                    if let Some(d) = self.diplo.as_mut() {
                        d.day(e, ed, w, self.brains.as_mut());
                    }
                }
            }
        }
        // A soft chime each month, and news worth hearing about.
        let month = e.day / 30;
        if month != self.clock.last_month {
            self.clock.last_month = month;
            ctx.play_with(&self.assets.month, 0.35, 0.0);
            if self.map_ui.econ.overlay != Overlay::Off {
                self.map_ui.econ.overlay_month = u32::MAX;
            }
        }
        if e.news.len() < self.clock.last_news {
            self.clock.last_news = 0;
        }
        let watching = self.spectating;
        let fresh: Vec<String> = e.news[self.clock.last_news..]
            .iter()
            .filter(|item| !watching && item.nation.is_none_or(|i| i == 0))
            .map(|item| match item.nation {
                Some(_) => item.text.clone(),
                None => format!("World: {}", item.text),
            })
            .collect();
        self.clock.last_news = e.news.len();
        if let Some(words) = fresh.last() {
            self.toast = Some((words.clone(), 4.0));
            ctx.play_with(&self.assets.news, 0.6, 0.0);
        }
        self.after_war_days(ctx);
        self.after_diplo_days(ctx);
        self.refresh_overlay();
        self.refresh_war_scene();
    }

    /// Rebuilds the map overlay when it changes (and each month for industry).
    fn refresh_overlay(&mut self) {
        let (Some(e), Some(view), Some(world)) = (self.economy.as_ref(), self.map_view.as_mut(), self.world.as_ref()) else { return };
        let ui = &mut self.map_ui.econ;
        // The war overlay follows the war day by day; the others monthly.
        let month = if matches!(ui.overlay, Overlay::War | Overlay::Alliances | Overlay::Relations) { e.day } else { e.day / 30 };
        if ui.overlay_month == month {
            return;
        }
        ui.overlay_month = month;
        let ed = &self.econ_data;
        let colors: Option<Vec<Option<Color>>> = match ui.overlay {
            Overlay::Off => None,
            Overlay::Resources => Some(
                e.provinces
                    .iter()
                    .map(|p| {
                        let best = (0..ed.goods.len()).filter(|&g| ed.goods[g].tier == Tier::Raw).max_by(|&a, &b| p.deposits[a].total_cmp(&p.deposits[b]))?;
                        let d = p.deposits[best];
                        if d < 0.2 {
                            return None;
                        }
                        Some(Color::rgb(40, 40, 48).lerp(good_color(ed, best), (0.45 + 0.3 * d as f32).min(1.0)))
                    })
                    .collect(),
            ),
            Overlay::Industry => {
                let mut value = vec![0.0f64; e.provinces.len()];
                for n in &e.nations {
                    for b in &n.buildings {
                        value[b.province] += b.output * n.price[ed.buildings[b.kind].output] * n.exchange;
                    }
                }
                let top = value.iter().cloned().fold(1.0, f64::max).ln();
                Some(
                    value
                        .iter()
                        .map(|&v| {
                            if v <= 1.0 {
                                return None;
                            }
                            let k = (v.ln() / top).clamp(0.0, 1.0) as f32;
                            let cold = Color::rgb(40, 60, 140);
                            let warm = Color::rgb(250, 210, 70);
                            Some(cold.lerp(warm, k * k))
                        })
                        .collect(),
                )
            }
            Overlay::War => self.wars.as_ref().map(|w| {
                let fighting: std::collections::HashSet<usize> = w.battles.iter().filter(|b| !b.over).map(|b| b.province).collect();
                (0..w.control.len())
                    .map(|q| {
                        if fighting.contains(&q) {
                            return Some(Color::rgb(220, 40, 40));
                        }
                        let f = w.control[q]?;
                        let c = if f < w.nations { world.color[f] } else { Color::rgb(30, 28, 32) };
                        Some(if w.owner[q] == Some(f) { c.lerp(Color::rgb(40, 40, 48), 0.35) } else { c })
                    })
                    .collect()
            }),
            Overlay::Alliances => self.diplo.as_ref().map(|d| {
                let bloc = alliance_blocs(d);
                world
                    .owner
                    .iter()
                    .map(|o| {
                        let o = (*o)?;
                        let lead = bloc[o];
                        let alone = (0..d.n).filter(|&j| bloc[j] == lead).count() == 1;
                        Some(if alone { Color::rgb(70, 70, 80) } else { world.color[lead].lerp(Color::WHITE, 0.15) })
                    })
                    .collect()
            }),
            Overlay::Relations => self.diplo.as_ref().map(|d| {
                let v = if self.spectating { self.map_ui.brain.unwrap_or(0) } else { 0 };
                world
                    .owner
                    .iter()
                    .map(|o| {
                        let o = (*o)?;
                        if o == v {
                            return Some(Color::rgb(245, 197, 66));
                        }
                        let r = (d.relations[v][o] / 100.0).clamp(-1.0, 1.0) as f32;
                        let mid = Color::rgb(200, 200, 190);
                        Some(if r >= 0.0 { mid.lerp(Color::rgb(60, 190, 90), r) } else { mid.lerp(Color::rgb(220, 50, 50), -r) })
                    })
                    .collect()
            }),
        };
        view.set_overlay(&self.atlas, world, colors.as_deref());
    }

    /// V: resources, industry, off.
    pub(super) fn cycle_overlay(&mut self, ctx: &mut Context) {
        let ui = &mut self.map_ui.econ;
        ui.overlay = match ui.overlay {
            Overlay::Off => Overlay::Resources,
            Overlay::Resources => Overlay::Industry,
            Overlay::Industry if self.wars.is_some() => Overlay::War,
            Overlay::War if self.diplo.is_some() => Overlay::Alliances,
            Overlay::Alliances => Overlay::Relations,
            Overlay::Industry | Overlay::War | Overlay::Relations => Overlay::Off,
        };
        ui.overlay_month = u32::MAX;
        self.map_ui.war.all_armies = ui.overlay == Overlay::War;
        self.map_ui.war.refresh();
        ctx.play_with(&self.assets.click, 0.8, 0.0);
        self.refresh_overlay();
    }

    /// The economy window is open: its keys and clicks (the camera rests).
    pub(super) fn update_econ_window(&mut self, ctx: &mut Context) {
        let Some(mut tab) = self.map_ui.econ.open else { return };
        let (Some(e), ed) = (self.economy.as_mut(), &self.econ_data) else { return };
        let mouse = self.mouse;
        let click = ctx.input.was_mouse_pressed(MouseButton::Left).then_some(mouse).flatten();
        if ctx.input.was_pressed(Key::Escape) || ctx.input.was_pressed(Key::O) || click.is_some_and(|m| !WIN.contains(m) && !ECON_BUTTON.contains(m)) {
            self.map_ui.econ.open = None;
            ctx.play_with(&self.assets.click, 0.7, 0.0);
            return;
        }
        // Pages.
        let mut new_tab = None;
        if ctx.input.was_pressed(Key::Tab) {
            let k = Tab::ALL.iter().position(|&t| t == tab).unwrap_or(0);
            new_tab = Some(Tab::ALL[(k + 1) % Tab::ALL.len()]);
        }
        if let Some(m) = click {
            for (k, t) in Tab::ALL.iter().enumerate() {
                if Tab::tab_rect(k).contains(m) {
                    new_tab = Some(*t);
                }
            }
        }
        if let Some(t) = new_tab {
            if t != tab {
                tab = t;
                self.map_ui.econ.open = Some(t);
                self.map_ui.econ.row = 0;
                ctx.play_with(&self.assets.click, 0.8, 0.0);
            }
        }
        let rows = tab.rows(ed, e).max(1);
        let ui = &mut self.map_ui.econ;
        if ctx.input.was_pressed(Key::Down) {
            ui.row = (ui.row + 1) % rows;
        }
        if ctx.input.was_pressed(Key::Up) {
            ui.row = (ui.row + rows - 1) % rows;
        }
        // Clicking a row chooses it.
        if let Some(m) = click {
            if let Some(r) = row_at(tab, m, rows) {
                ui.row = r;
            }
        }
        ui.row = ui.row.min(rows - 1);
        let row = ui.row;
        let mut said: Option<Result<String, String>> = None;
        match tab {
            Tab::Budget => {
                let lever = Lever::ALL[row];
                let mut steps = 0;
                if ctx.input.was_pressed(Key::Right) {
                    steps += 1;
                }
                if ctx.input.was_pressed(Key::Left) {
                    steps -= 1;
                }
                if let Some(m) = click {
                    for (k, _) in Lever::ALL.iter().enumerate() {
                        let (minus, plus) = lever_buttons(k);
                        if minus.contains(m) {
                            said = Some(Ok(e.pull_with(Some(ed), 0, Lever::ALL[k], -1)));
                        }
                        if plus.contains(m) {
                            said = Some(Ok(e.pull_with(Some(ed), 0, Lever::ALL[k], 1)));
                        }
                    }
                }
                if steps != 0 {
                    said = Some(Ok(e.pull_with(Some(ed), 0, lever, steps)));
                }
                if ctx.input.was_pressed(Key::A) {
                    e.free_rate(0);
                    said = Some(Ok(e.free_taxes(0) + "; the bank sets the rate"));
                }
            }
            Tab::Chains => {
                let g = chain_order(ed)[row];
                if let Some(kind) = ed.buildings.iter().position(|b| b.output == g) {
                    if ctx.input.was_pressed(Key::B) || click.is_some_and(|m| BUILD_BUTTON.contains(m)) {
                        said = Some(e.player_build(ed, 0, kind));
                    }
                    if ctx.input.was_pressed(Key::X) || click.is_some_and(|m| CLOSE_BUTTON.contains(m)) {
                        said = Some(e.player_close(ed, 0, kind));
                    }
                }
                // Roads where goods are stuck, a bigger port (the brains have the same keys).
                if ctx.input.was_pressed(Key::R) {
                    said = Some(match e.most_choked(0) {
                        Some(q) => e.player_road(ed, 0, q),
                        None => Err("Goods move freely everywhere: no road needed".to_string()),
                    });
                }
                if ctx.input.was_pressed(Key::P) {
                    said = Some(match e.smallest_port(0) {
                        Some(q) => e.player_port(ed, 0, q),
                        None => Err("No coast with room for a bigger port".to_string()),
                    });
                }
            }
            Tab::Market => {
                if ctx.input.was_pressed(Key::T) || click.is_some_and(|m| POLICY_BUTTON.contains(m)) {
                    if ed.goods[row].trade {
                        said = Some(Ok(e.cycle_trade_policy(ed, 0, row)));
                    } else {
                        said = Some(Err(format!("{} can't be traded", ed.goods[row].name)));
                    }
                }
            }
            Tab::Trade => {
                if ctx.input.was_pressed(Key::X) || click.is_some_and(|m| EMBARGO_BUTTON.contains(m)) {
                    let other = partners(e)[row];
                    said = Some(Ok(e.toggle_embargo(0, other)));
                }
            }
            Tab::Stockpile | Tab::Charts => {}
        }
        match said {
            Some(Ok(words)) => {
                ctx.play_with(&self.assets.coin, 0.7, 0.0);
                self.toast = Some((words, 2.5));
            }
            Some(Err(words)) => {
                ctx.play_with(&self.assets.click, 0.5, 0.0);
                self.toast = Some((words, 2.5));
            }
            None => {}
        }
    }

    pub(super) fn report_econ(&self, ctx: &mut Context) {
        let Some(e) = &self.economy else { return };
        let n = &e.nations[0];
        ctx.report("date", e.date(&self.econ_data));
        ctx.report("day", e.day);
        ctx.report("paused", self.clock.paused);
        ctx.report("speed", self.clock.speed + 1);
        ctx.report("econ_tab", self.map_ui.econ.open.map_or("none", |t| t.name()));
        ctx.report("overlay", format!("{:?}", self.map_ui.econ.overlay));
        ctx.report("gdp", (n.gdp / 1e9 * 10.0).round() / 10.0);
        ctx.report("income_tax", (n.income_tax * 1000.0).round() / 1000.0);
        ctx.report("construction", n.construction.len());
        ctx.report("toast", self.toast.as_ref().map_or(String::new(), |t| t.0.clone()));
        ctx.check("the player's economy has sane numbers", n.gdp.is_finite() && n.gdp > 0.0 && n.stock.iter().all(|s| *s >= -1e-6));
    }

    /// Date, speed and the headline numbers, on the top bar.
    pub(super) fn draw_econ_topbar(&self, canvas: &mut Canvas) {
        let (Some(e), a) = (self.economy.as_ref(), &self.assets) else { return };
        let ed = &self.econ_data;
        let n = &e.nations[0];
        let speed = if self.clock.paused { "PAUSED".to_string() } else { format!("{}  x{}", ">".repeat(self.clock.speed + 1), SPEEDS[self.clock.speed]) };
        ui::plain(canvas, &a.font, &speed, vec2(1076.0, 32.0), 13.0, if self.clock.paused { WARN } else { GOLD }, Align::Right);
        ui::plain(canvas, &a.font, &e.date(ed), vec2(990.0, 30.0), 16.0, Color::WHITE, Align::Right);
        let facts = if self.spectating {
            let (now, start): (f64, f64) = e.nations.iter().fold((0.0, 0.0), |(a, b), n| (a + n.gdp, b + n.gdp_start));
            let years = e.day as f64 / ed.days_per_year as f64;
            let jobless = e.nations.iter().filter(|n| n.unemployment(ed) > 0.15).count();
            // A yearly rate from a few weeks says nothing (one good month
            // compounds to +20%): it waits for a whole year.
            let growth = if years >= 1.0 { format!("{} a year", pct((now / start.max(1.0)).powf(1.0 / years) - 1.0)) } else { "growth after a year".to_string() };
            format!("World GDP {}  ·  {growth}  ·  {} over 15% jobless", money(now), jobless)
        } else {
            format!(
                "GDP {}  ·  Inflation {}  ·  Jobless {}  ·  Treasury {}",
                money(n.gdp),
                pct(n.inflation),
                pct(n.unemployment(ed)),
                money(n.treasury * n.exchange)
            )
        };
        ui::plain(canvas, &a.font, &facts, vec2(1266.0, 8.0), 13.0, Color::WHITE, Align::Right);
        let hot = self.mouse.is_some_and(|m| ECON_BUTTON.contains(m));
        canvas.fill_rect(ECON_BUTTON, if hot { Color::rgba(245, 197, 66, 90) } else { Color::rgba(245, 197, 66, 40) });
        canvas.blit_sprite(&a.icon_coin, vec2(ECON_BUTTON.x + 2.0, ECON_BUTTON.y - 1.0), Blit::default());
        ui::plain(canvas, &a.font, "O: ECONOMY", vec2(ECON_BUTTON.x + 32.0, ECON_BUTTON.y + 3.0), 13.0, GOLD, Align::Left);
        canvas.label("economy button", ECON_BUTTON);
        if self.wars.is_some() {
            let b = super::war_screen::WAR_BUTTON;
            let hot = self.mouse.is_some_and(|m| b.contains(m));
            let at_war = self.wars.as_ref().is_some_and(|w| w.level_of(self.war_viewer_pub()) >= 1);
            canvas.fill_rect(b, if hot { Color::rgba(240, 106, 106, 120) } else if at_war { Color::rgba(240, 70, 70, 90) } else { Color::rgba(240, 106, 106, 45) });
            canvas.blit_sprite(&a.icon_swords, vec2(b.x + 4.0, b.y + 1.0), Blit { scale: 0.62, ..Blit::default() });
            ui::plain(canvas, &a.font, "K: WAR ROOM", vec2(b.x + 32.0, b.y + 3.0), 13.0, Color::WHITE, Align::Left);
            canvas.label("war button", b);
        }
    }

    /// The overlay's key, bottom left.
    pub(super) fn draw_overlay_legend(&self, canvas: &mut Canvas) {
        let a = &self.assets;
        let ed = &self.econ_data;
        let ui = &self.map_ui.econ;
        if ui.overlay == Overlay::Off {
            return;
        }
        let r = Rect::new(14.0, 520.0, 230.0, 168.0);
        canvas.fill_rect(r, Color::rgba(12, 6, 30, 210));
        canvas.label("overlay legend", r);
        match ui.overlay {
            Overlay::Resources => {
                plain(canvas, &a.font, "RESOURCES (V)", vec2(r.x + 12.0, r.y + 8.0), 12.0, GOLD);
                let raws: Vec<usize> = (0..ed.goods.len()).filter(|&g| ed.goods[g].tier == Tier::Raw).collect();
                for (k, &g) in raws.iter().enumerate() {
                    let y = r.y + 30.0 + k as f32 * 21.0;
                    canvas.fill_rect(Rect::new(r.x + 12.0, y + 2.0, 16.0, 14.0), good_color(ed, g));
                    plain(canvas, &a.font, &ed.goods[g].name, vec2(r.x + 36.0, y), 14.0, Color::WHITE);
                }
            }
            Overlay::Industry => {
                plain(canvas, &a.font, "INDUSTRY (V)", vec2(r.x + 12.0, r.y + 8.0), 12.0, GOLD);
                plain(canvas, &a.font, "What each province's", vec2(r.x + 12.0, r.y + 34.0), 14.0, Color::WHITE);
                plain(canvas, &a.font, "buildings make a day", vec2(r.x + 12.0, r.y + 52.0), 14.0, Color::WHITE);
                for k in 0..20 {
                    let c = Color::rgb(40, 60, 140).lerp(Color::rgb(250, 210, 70), (k as f32 / 19.0).powi(2));
                    canvas.fill_rect(Rect::new(r.x + 12.0 + k as f32 * 10.0, r.y + 90.0, 10.0, 18.0), c);
                }
                plain(canvas, &a.font, "little", vec2(r.x + 12.0, r.y + 114.0), 13.0, SOFT);
                right(canvas, &a.font, "most", vec2(r.x + 212.0, r.y + 114.0), 13.0, SOFT);
                plain(canvas, &a.font, "Grey: none", vec2(r.x + 12.0, r.y + 138.0), 13.0, SOFT);
            }
            Overlay::War => {
                plain(canvas, &a.font, "WAR (V)", vec2(r.x + 12.0, r.y + 8.0), 12.0, GOLD);
                plain(canvas, &a.font, "Each province in the colour", vec2(r.x + 12.0, r.y + 30.0), 13.0, Color::WHITE);
                plain(canvas, &a.font, "of whoever holds it", vec2(r.x + 12.0, r.y + 47.0), 13.0, Color::WHITE);
                plain(canvas, &a.font, "Bright: occupied land", vec2(r.x + 12.0, r.y + 72.0), 13.0, SOFT);
                canvas.fill_rect(Rect::new(r.x + 12.0, r.y + 96.0, 16.0, 14.0), Color::rgb(220, 40, 40));
                plain(canvas, &a.font, "A battle", vec2(r.x + 36.0, r.y + 94.0), 13.0, Color::WHITE);
                plain(canvas, &a.font, "Every army is shown", vec2(r.x + 12.0, r.y + 118.0), 13.0, SOFT);
                plain(canvas, &a.font, "(1 figure = 1,000 soldiers)", vec2(r.x + 12.0, r.y + 138.0), 13.0, SOFT);
            }
            Overlay::Alliances => {
                plain(canvas, &a.font, "ALLIANCES (V)", vec2(r.x + 12.0, r.y + 8.0), 12.0, GOLD);
                plain(canvas, &a.font, "Each bloc of alliances and", vec2(r.x + 12.0, r.y + 30.0), 13.0, Color::WHITE);
                plain(canvas, &a.font, "defensive pacts in the colour", vec2(r.x + 12.0, r.y + 47.0), 13.0, Color::WHITE);
                plain(canvas, &a.font, "of its strongest member", vec2(r.x + 12.0, r.y + 64.0), 13.0, Color::WHITE);
                canvas.fill_rect(Rect::new(r.x + 12.0, r.y + 92.0, 16.0, 14.0), Color::rgb(70, 70, 80));
                plain(canvas, &a.font, "No allies", vec2(r.x + 36.0, r.y + 90.0), 13.0, SOFT);
            }
            Overlay::Relations => {
                plain(canvas, &a.font, "RELATIONS (V)", vec2(r.x + 12.0, r.y + 8.0), 12.0, GOLD);
                plain(canvas, &a.font, if self.spectating { "With the chosen nation" } else { "With you" }, vec2(r.x + 12.0, r.y + 30.0), 13.0, Color::WHITE);
                for k in 0..20 {
                    let t = k as f32 / 19.0 * 2.0 - 1.0;
                    let mid = Color::rgb(200, 200, 190);
                    let c = if t >= 0.0 { mid.lerp(Color::rgb(60, 190, 90), t) } else { mid.lerp(Color::rgb(220, 50, 50), -t) };
                    canvas.fill_rect(Rect::new(r.x + 12.0 + k as f32 * 10.0, r.y + 60.0, 10.0, 18.0), c);
                }
                plain(canvas, &a.font, "-100", vec2(r.x + 12.0, r.y + 84.0), 13.0, SOFT);
                right(canvas, &a.font, "+100", vec2(r.x + 212.0, r.y + 84.0), 13.0, SOFT);
                plain(canvas, &a.font, "Gold: the nation itself", vec2(r.x + 12.0, r.y + 112.0), 13.0, SOFT);
            }
            Overlay::Off => {}
        }
    }

    /// The economy window.
    pub(super) fn draw_econ_window(&self, canvas: &mut Canvas) {
        let (Some(tab), Some(e)) = (self.map_ui.econ.open, self.economy.as_ref()) else { return };
        let a = &self.assets;
        ui::dim(canvas, 0.45);
        canvas.blit_sprite(&a.econ_window, vec2(WIN.x, WIN.y), Blit::default());
        canvas.label("economy window", WIN);
        let icons = [&a.icon_coin, &a.icon_factory, &a.icon_scales, &a.icon_ship, &a.icon_crate, &a.icon_chart];
        for (k, t) in Tab::ALL.iter().enumerate() {
            let r = Tab::tab_rect(k);
            let on = *t == tab;
            let hot = self.mouse.is_some_and(|m| r.contains(m));
            canvas.fill_rect(r, if on { Color::rgba(245, 197, 66, 70) } else if hot { Color::rgba(255, 255, 255, 30) } else { Color::rgba(255, 255, 255, 12) });
            canvas.blit_sprite(icons[k], vec2(r.x + 10.0, r.y + 8.0), Blit::default());
            plain(canvas, &a.font, t.name(), vec2(r.x + 42.0, r.y + 11.0), 15.0, if on { GOLD } else { Color::WHITE });
            canvas.label(&format!("tab {}", t.name()), r);
        }
        let n = &e.nations[0];
        let row = self.map_ui.econ.row;
        match tab {
            Tab::Budget => self.draw_budget(canvas, e, n, row),
            Tab::Chains => self.draw_chains(canvas, e, n, row),
            Tab::Market => self.draw_market(canvas, e, n, row),
            Tab::Trade => self.draw_trade(canvas, e, n, row),
            Tab::Stockpile => self.draw_stockpile(canvas, e, n),
            Tab::Charts => self.draw_charts(canvas, e, n, row),
        }
        let help = match tab {
            Tab::Budget => "Up/Down: choose  ·  Left/Right or -/+: change  ·  A: hand back to the autopilot  ·  Tab: next page  ·  O or Esc: close",
            Tab::Chains => "Up/Down: choose a good  ·  B: build more of its maker  ·  X: close some  ·  R: a road where goods are stuck  ·  P: a port  ·  Tab: next page",
            Tab::Market => "Up/Down: choose a good  ·  T: free → no exports → no imports  ·  Tab: next page",
            Tab::Trade => "Up/Down: choose a nation  ·  X: embargo (or lift it)  ·  Tab: next page",
            Tab::Stockpile => "Weapons made at home or bought go into the stockpile  ·  Tab: next page",
            Tab::Charts => "Up/Down: choose the good in the price chart  ·  Tab: next page",
        };
        plain(canvas, &a.font, help, vec2(WIN.x + 26.0, WIN.bottom() - 30.0), 13.0, SOFT);
    }

    fn draw_budget(&self, canvas: &mut Canvas, e: &Economy, n: &NationEcon, row: usize) {
        let a = &self.assets;
        let ed = &self.econ_data;
        let (x, y) = (WIN.x + 26.0, WIN.y + 84.0);
        plain(canvas, &a.font, "POLICY", vec2(x, y), 12.0, GOLD);
        for (k, lever) in Lever::ALL.iter().enumerate() {
            let ry = y + 22.0 + k as f32 * LEVER_ROW;
            let r = Rect::new(x - 6.0, ry - 4.0, 470.0, LEVER_ROW - 4.0);
            if k == row {
                canvas.fill_rect(r, Color::rgba(245, 197, 66, 40));
            }
            plain(canvas, &a.font, lever.name(), vec2(x, ry + 4.0), 16.0, Color::WHITE);
            let value = e.lever_with(0, *lever, Some(ed));
            let auto = match lever {
                Lever::InterestRate => n.manual_rate.is_none(),
                Lever::Tariff | Lever::Research => false,
                Lever::Arms => n.arms.is_none(),
                Lever::PublicWorks => n.works.is_none(),
                _ => !n.manual_taxes,
            };
            right(canvas, &a.font, &pct(value), vec2(x + 360.0, ry + 4.0), 17.0, GOLD);
            if auto {
                plain(canvas, &a.font, "auto", vec2(x + 252.0, ry + 7.0), 12.0, SOFT);
            }
            let (minus, plus) = lever_buttons(k);
            for (b, s) in [(minus, "-"), (plus, "+")] {
                let hot = self.mouse.is_some_and(|m| b.contains(m));
                canvas.fill_rect(b, if hot { Color::rgba(245, 197, 66, 120) } else { Color::rgba(255, 255, 255, 30) });
                ui::plain(canvas, &a.font, s, vec2(b.center().x, b.y + 2.0), 18.0, Color::WHITE, Align::Center);
                canvas.label(&format!("{} {}", lever.name(), if s == "-" { "down" } else { "up" }), b);
            }
        }
        // The state's money.
        let y2 = y + 34.0 + Lever::ALL.len() as f32 * LEVER_ROW;
        let gdp_year = (n.gdp_local * ed.days_per_year as f64).max(1.0);
        let lines = [
            ("Currency", format!("{} ({}): 1 = {:.4} W$", n.currency, n.symbol, n.exchange)),
            ("Debt", format!("{} ({} of GDP)", money(n.debt * n.exchange), pct(n.debt / gdp_year))),
            ("Credit rating", format!("{}  ·  bonds pay {}", ed.ratings[n.rating].name, pct(n.bond_rate))),
            ("Treasury", money(n.treasury * n.exchange)),
            ("Money printed last month", money(n.printed_last_month * n.exchange)),
            ("Model", ed.models[n.model].name.clone()),
        ];
        for (k, (label, value)) in lines.iter().enumerate() {
            let ly = y2 + k as f32 * 24.0;
            plain(canvas, &a.font, label, vec2(x, ly), 14.0, MUTED);
            plain(canvas, &a.font, value, vec2(x + 190.0, ly), 14.0, Color::WHITE);
        }
        // Last month's budget, as bars (W$ a year).
        let b = &n.budget_last_month;
        let yr = |v: f64| v * n.exchange * 12.0;
        let income = [
            ("Income tax", b.income_tax),
            ("Corporate tax", b.corporate_tax),
            ("Sales tax", b.sales_tax),
            ("Tariffs", b.tariffs),
            ("Tribute in", b.tribute_in),
        ];
        let spending = [
            ("Army", b.army),
            ("Welfare", b.welfare),
            ("Government", b.government),
            ("Interest", b.interest),
            ("Infrastructure", b.infrastructure),
            ("Building", b.construction),
            ("Weapons", b.procurement),
            ("Research", b.research),
            ("Lost to corruption", b.corruption),
            ("Tribute out", b.tribute_out),
        ];
        let top = income.iter().chain(spending.iter()).map(|(_, v)| yr(*v)).fold(1.0, f64::max);
        let bx = WIN.x + 540.0;
        plain(canvas, &a.font, "LAST MONTH, AS A YEAR", vec2(bx, y), 12.0, GOLD);
        let mut ly = y + 22.0;
        for (group, items, color) in [("REVENUE", &income[..], GOOD), ("SPENDING", &spending[..], BAD)] {
            plain(canvas, &a.font, group, vec2(bx, ly), 11.0, SOFT);
            ly += 16.0;
            for (label, v) in items {
                let w = (yr(*v) / top * 250.0) as f32;
                plain(canvas, &a.font, label, vec2(bx, ly), 13.0, Color::WHITE);
                canvas.fill_rect(Rect::new(bx + 140.0, ly + 3.0, w.max(1.0), 12.0), color);
                plain(canvas, &a.font, &money(yr(*v)), vec2(bx + 146.0 + w, ly), 12.0, MUTED);
                ly += 19.0;
            }
            ly += 6.0;
        }
        let net = yr(b.revenue() - b.spending());
        let words = format!("Balance: {} a year ({} of GDP)", money(net), pct(net / (n.gdp).max(1.0)));
        plain(canvas, &a.font, &words, vec2(bx, ly + 4.0), 15.0, if net >= 0.0 { GOOD } else { BAD });
    }

    fn draw_chains(&self, canvas: &mut Canvas, _e: &Economy, n: &NationEcon, row: usize) {
        let a = &self.assets;
        let ed = &self.econ_data;
        let order = chain_order(ed);
        let chosen = order[row];
        let pos = chain_positions(ed);
        let (x0, y0) = (WIN.x + 26.0, WIN.y + 84.0);
        for (k, title) in ["RAW", "POWER", "REFINED", "FINAL"].iter().enumerate() {
            plain(canvas, &a.font, title, vec2(x0 + k as f32 * 172.0, y0), 12.0, GOLD);
        }
        // Lines: each maker's inputs to its output.
        for bd in &ed.buildings {
            let to = pos[bd.output];
            for &(i, _) in &bd.inputs {
                let from = pos[i];
                let hot = bd.output == chosen || i == chosen;
                let c = if hot { Color::rgba(245, 197, 66, 230) } else { Color::rgba(127, 168, 255, 70) };
                canvas.draw_line(from + vec2(150.0, 16.0), to + vec2(0.0, 16.0), c);
            }
        }
        for &g in &order {
            let p = pos[g];
            let r = Rect::new(p.x, p.y, 150.0, 32.0);
            let ratio = supply_ratio(n, g);
            let on = g == chosen;
            canvas.fill_rect(r, if on { Color::rgba(245, 197, 66, 90) } else { Color::rgba(20, 12, 45, 230) });
            canvas.fill_rect(Rect::new(r.x, r.y, 5.0, r.h), status_color(ratio));
            plain(canvas, &a.font, &ed.goods[g].name, vec2(r.x + 11.0, r.y + 2.0), 13.0, Color::WHITE);
            let made = n.ledger.produced[g];
            plain(canvas, &a.font, &format!("{}/day", amount(made)), vec2(r.x + 11.0, r.y + 17.0), 11.0, MUTED);
            canvas.label(&format!("chain {}", ed.goods[g].id), r);
        }
        // The chosen good.
        let px = WIN.x + 720.0;
        let g = chosen;
        plain(canvas, &a.font, &ed.goods[g].name.to_uppercase(), vec2(px, y0), 16.0, GOLD);
        let ratio = supply_ratio(n, g);
        let lines = [
            ("Made", format!("{}/day", amount(n.ledger.produced[g]))),
            ("Used and eaten", format!("{}/day", amount(n.ledger.used[g] + n.ledger.consumed[g]))),
            ("In store", format!("{} ({:.0} days)", amount(n.stock[g]), n.stock[g] / n.use_rate[g].max(1e-9))),
            ("Wanted, got", format!("{:.0}%", ratio * 100.0)),
            ("Bought, sold abroad", format!("{} / {}", amount(n.ledger.imported[g]), amount(n.ledger.exported[g]))),
        ];
        for (k, (label, value)) in lines.iter().enumerate() {
            let ly = y0 + 28.0 + k as f32 * 21.0;
            plain(canvas, &a.font, label, vec2(px, ly), 13.0, MUTED);
            plain(canvas, &a.font, value, vec2(px + 150.0, ly), 13.0, Color::WHITE);
        }
        let mut ly = y0 + 146.0;
        if let Some(kind) = ed.buildings.iter().position(|b| b.output == g) {
            let bd = &ed.buildings[kind];
            let mine: Vec<&crate::econ::Building> = n.buildings.iter().filter(|b| b.kind == kind).collect();
            let levels: f64 = mine.iter().map(|b| b.levels).sum();
            let hired: f64 = mine.iter().map(|b| b.hired).sum();
            let inputs = if levels > 0.0 { mine.iter().map(|b| b.input_factor * b.levels).sum::<f64>() / levels } else { 1.0 };
            let profit = if levels > 0.0 { mine.iter().map(|b| b.profit * b.levels).sum::<f64>() / levels } else { 0.0 };
            let building: f64 = n.construction.iter().filter(|c| c.kind == kind).map(|c| c.levels).sum();
            plain(canvas, &a.font, &bd.name.to_uppercase(), vec2(px, ly), 12.0, GOLD);
            ly += 20.0;
            let recipe = if bd.inputs.is_empty() {
                "from the land".to_string()
            } else {
                bd.inputs.iter().map(|&(i, per)| format!("{per} {}", ed.goods[i].name)).collect::<Vec<_>>().join(" + ")
            };
            for (label, value) in [
                ("Needs", recipe),
                ("Levels", format!("{:.0} (+{:.0} being built)", levels, building)),
                ("Workers", amount(hired)),
                ("Inputs on hand", format!("{:.0}%", inputs * 100.0)),
                ("Profit a level", format!("{}/day", money(profit * n.exchange))),
            ] {
                plain(canvas, &a.font, label, vec2(px, ly), 13.0, MUTED);
                ui::text(canvas, &a.font, &value, vec2(px + 120.0, ly), 13.0, Color::WHITE, Align::Left, 190.0);
                ly += 21.0;
            }
            let (block, cost) = self.economy.as_ref().map_or((0.0, 0.0), |e| e.build_block(ed, 0, kind));
            plain(canvas, &a.font, &format!("B: build {:.0} levels for {}", block, money(cost * n.exchange)), vec2(px, ly + 8.0), 13.0, SOFT);
            for (b, label) in [(BUILD_BUTTON, "BUILD (B)"), (CLOSE_BUTTON, "CLOSE (X)")] {
                let hot = self.mouse.is_some_and(|m| b.contains(m));
                canvas.fill_rect(b, if hot { Color::rgba(245, 197, 66, 140) } else { Color::rgba(245, 197, 66, 60) });
                ui::plain(canvas, &a.font, label, vec2(b.center().x, b.y + 8.0), 14.0, Color::WHITE, Align::Center);
                canvas.label(label, b);
            }
        }
        // What's being built.
        let mut cy = WIN.bottom() - 120.0;
        plain(canvas, &a.font, "BEING BUILT", vec2(x0, cy), 12.0, GOLD);
        cy += 18.0;
        let mut jobs: Vec<String> = n
            .construction
            .iter()
            .map(|c| format!("{} +{:.0} ({} days){}", ed.buildings[c.kind].name, c.levels, c.days_left, if c.by_player { " (yours)" } else { "" }))
            .collect();
        jobs.truncate(4);
        if jobs.is_empty() {
            jobs.push("Nothing".to_string());
        }
        ui::text(canvas, &a.font, &jobs.join("  ·  "), vec2(x0, cy), 13.0, Color::WHITE, Align::Left, 660.0);
    }

    fn draw_market(&self, canvas: &mut Canvas, e: &Economy, n: &NationEcon, row: usize) {
        let a = &self.assets;
        let ed = &self.econ_data;
        let (x, y) = (WIN.x + 26.0, WIN.y + 80.0);
        let cols = [
            ("GOOD", 0.0),
            ("HOME (W$)", 260.0),
            ("WORLD", 360.0),
            ("STOCK (DAYS)", 470.0),
            ("MADE/DAY", 570.0),
            ("USED/DAY", 670.0),
            ("IN / OUT", 790.0),
            ("POLICY", 860.0),
        ];
        for (title, cx) in cols {
            plain(canvas, &a.font, title, vec2(x + cx, y), 11.0, GOLD);
        }
        for g in 0..ed.goods.len() {
            let ry = y + 20.0 + g as f32 * 26.5;
            if g == row {
                canvas.fill_rect(Rect::new(x - 6.0, ry - 3.0, 1010.0, 25.0), Color::rgba(245, 197, 66, 40));
            }
            let home = n.price[g] * n.exchange;
            let world = e.world_price[g];
            let ratio = supply_ratio(n, g);
            canvas.fill_rect(Rect::new(x, ry + 5.0, 10.0, 10.0), status_color(ratio));
            plain(canvas, &a.font, &ed.goods[g].name, vec2(x + 18.0, ry), 14.0, Color::WHITE);
            let rel = home / world;
            right(canvas, &a.font, &format!("{:.0}", home), vec2(x + 340.0, ry), 14.0, if rel > 1.15 { BAD } else if rel < 0.87 { GOOD } else { Color::WHITE });
            right(canvas, &a.font, &format!("{:.0}", world), vec2(x + 440.0, ry), 14.0, MUTED);
            let days = n.stock[g] / n.use_rate[g].max(1e-9);
            right(canvas, &a.font, &format!("{:.0}", days.min(9999.0)), vec2(x + 550.0, ry), 14.0, if days < 10.0 { BAD } else { Color::WHITE });
            right(canvas, &a.font, &amount(n.ledger.produced[g]), vec2(x + 650.0, ry), 14.0, Color::WHITE);
            right(canvas, &a.font, &amount(n.ledger.used[g] + n.ledger.consumed[g] + n.ledger.stockpiled[g]), vec2(x + 750.0, ry), 14.0, Color::WHITE);
            let io = format!("{}/{}", amount(n.ledger.imported[g]), amount(n.ledger.exported[g]));
            right(canvas, &a.font, &io, vec2(x + 850.0, ry), 13.0, MUTED);
            let policy = if !ed.goods[g].trade {
                "not traded"
            } else {
                match n.trade_policy[g] {
                    TradePolicy::Free => "free",
                    TradePolicy::NoExports => "no exports",
                    TradePolicy::NoImports => "no imports",
                }
            };
            plain(canvas, &a.font, policy, vec2(x + 870.0, ry), 13.0, if policy == "free" { SOFT } else { WARN });
            canvas.label(&format!("market {}", ed.goods[g].id), Rect::new(x - 6.0, ry - 3.0, 1010.0, 25.0));
        }
        let hot = self.mouse.is_some_and(|m| POLICY_BUTTON.contains(m));
        canvas.fill_rect(POLICY_BUTTON, if hot { Color::rgba(245, 197, 66, 140) } else { Color::rgba(245, 197, 66, 60) });
        ui::plain(canvas, &a.font, "TRADE POLICY (T)", vec2(POLICY_BUTTON.center().x, POLICY_BUTTON.y + 8.0), 14.0, Color::WHITE, Align::Center);
    }

    fn draw_trade(&self, canvas: &mut Canvas, e: &Economy, n: &NationEcon, row: usize) {
        let a = &self.assets;
        let ed = &self.econ_data;
        let (x, y) = (WIN.x + 26.0, WIN.y + 84.0);
        let exports: f64 = n.exports_to_last.iter().sum::<f64>() * 12.0;
        let imports: f64 = n.imports_from_last.iter().sum::<f64>() * 12.0;
        let facts = [
            ("Exports a year", money(exports)),
            ("Imports a year", money(imports)),
            ("Balance", money(exports - imports)),
            ("Trade capacity used", pct(if n.trade_capacity > 0.0 { n.trade_used / n.trade_capacity } else { 0.0 })),
            ("Lost to transport", pct(n.transport_share)),
            ("Tariff", pct(n.tariff)),
            ("Way to the sea", if n.landlocked { "landlocked: overland, dearer".to_string() } else { "ports: cheap by sea".to_string() }),
        ];
        plain(canvas, &a.font, "YOUR TRADE", vec2(x, y), 12.0, GOLD);
        for (k, (label, value)) in facts.iter().enumerate() {
            let ly = y + 22.0 + k as f32 * 24.0;
            plain(canvas, &a.font, label, vec2(x, ly), 14.0, MUTED);
            plain(canvas, &a.font, value, vec2(x + 180.0, ly), 14.0, Color::WHITE);
        }
        // Top exports and imports by good (today).
        let mut top: Vec<(usize, f64)> = (0..ed.goods.len()).map(|g| (g, (n.ledger.exported[g] - n.ledger.imported[g]) * e.world_price[g])).collect();
        top.sort_by(|a, b| b.1.total_cmp(&a.1));
        let ly = y + 210.0;
        plain(canvas, &a.font, "SELLS MOST", vec2(x, ly), 12.0, GOLD);
        plain(canvas, &a.font, "BUYS MOST", vec2(x + 210.0, ly), 12.0, GOLD);
        for k in 0..4 {
            let (g, v) = top[k];
            if v > 0.0 {
                plain(canvas, &a.font, &format!("{} {}", ed.goods[g].name, money(v * 360.0)), vec2(x, ly + 20.0 + k as f32 * 20.0), 13.0, GOOD);
            }
            let (g, v) = top[top.len() - 1 - k];
            if v < 0.0 {
                plain(canvas, &a.font, &format!("{} {}", ed.goods[g].name, money(-v * 360.0)), vec2(x + 210.0, ly + 20.0 + k as f32 * 20.0), 13.0, BAD);
            }
        }
        // Partners.
        let px = WIN.x + 470.0;
        plain(canvas, &a.font, "NATION", vec2(px, y), 11.0, GOLD);
        right(canvas, &a.font, "SOLD TO / YR", vec2(px + 330.0, y), 11.0, GOLD);
        right(canvas, &a.font, "BOUGHT / YR", vec2(px + 440.0, y), 11.0, GOLD);
        let list = partners(e);
        let first = row.saturating_sub(15);
        for (k, &j) in list.iter().enumerate().skip(first).take(16) {
            let ry = y + 18.0 + (k - first) as f32 * 25.0;
            if k == row {
                canvas.fill_rect(Rect::new(px - 6.0, ry - 3.0, 560.0, 24.0), Color::rgba(245, 197, 66, 40));
            }
            let blocked = n.embargo[j] || e.nations[j].embargo[0];
            let name: String = e.nations[j].name.chars().take(28).collect();
            plain(canvas, &a.font, &name, vec2(px, ry), 14.0, if blocked { BAD } else { Color::WHITE });
            right(canvas, &a.font, &money(n.exports_to_last[j] * 12.0), vec2(px + 330.0, ry), 13.0, Color::WHITE);
            right(canvas, &a.font, &money(n.imports_from_last[j] * 12.0), vec2(px + 440.0, ry), 13.0, Color::WHITE);
            if blocked {
                plain(canvas, &a.font, if n.embargo[j] { "EMBARGO" } else { "embargoes you" }, vec2(px + 452.0, ry + 1.0), 11.0, BAD);
            }
        }
        let hot = self.mouse.is_some_and(|m| EMBARGO_BUTTON.contains(m));
        canvas.fill_rect(EMBARGO_BUTTON, if hot { Color::rgba(245, 197, 66, 140) } else { Color::rgba(245, 197, 66, 60) });
        ui::plain(canvas, &a.font, "EMBARGO (X)", vec2(EMBARGO_BUTTON.center().x, EMBARGO_BUTTON.y + 8.0), 14.0, Color::WHITE, Align::Center);
    }

    fn draw_stockpile(&self, canvas: &mut Canvas, e: &Economy, n: &NationEcon) {
        let a = &self.assets;
        let ed = &self.econ_data;
        let (x, y) = (WIN.x + 26.0, WIN.y + 84.0);
        plain(canvas, &a.font, "THE NATIONAL STOCKPILE", vec2(x, y), 12.0, GOLD);
        for (k, g) in [ed.ammo, ed.tanks, ed.jets].into_iter().enumerate() {
            let cx = x + k as f32 * 330.0;
            let r = Rect::new(cx, y + 24.0, 310.0, 150.0);
            canvas.fill_rect(r, Color::rgba(20, 12, 45, 220));
            plain(canvas, &a.font, &ed.goods[g].name.to_uppercase(), vec2(cx + 16.0, y + 36.0), 14.0, GOLD);
            ui::plain(canvas, &a.font, &amount(n.stockpile[g]), vec2(cx + 16.0, y + 58.0), 38.0, Color::WHITE, Align::Left);
            let worth = n.stockpile[g] * e.world_price[g];
            plain(canvas, &a.font, &format!("worth {}", money(worth)), vec2(cx + 16.0, y + 110.0), 13.0, MUTED);
            plain(canvas, &a.font, &format!("made at home: {}/day", amount(n.ledger.produced[g])), vec2(cx + 16.0, y + 130.0), 13.0, MUTED);
            plain(canvas, &a.font, &format!("added today: {}", amount(n.ledger.stockpiled[g])), vec2(cx + 16.0, y + 150.0), 13.0, MUTED);
        }
        let share = ed.state.procurement.get(&n.army_option).copied().unwrap_or(0.0) * n.mods.get("procurement");
        let upkeep = (n.stockpile[ed.tanks] * ed.state.upkeep_tank + n.stockpile[ed.jets] * ed.state.upkeep_jet) * n.mods.get("army_upkeep") * ed.days_per_year as f64;
        let facts = [
            ("Army (active duty)", format!("{} soldiers", amount(n.army))),
            ("Army strength (wheel)", n.army_option.clone()),
            ("Spent on weapons", format!("{} of GDP a year ({})", pct(share), money(share * n.gdp))),
            ("Upkeep of tanks and jets", format!("{} a year", money(upkeep))),
            ("Army pay and training", format!("{} a month", money(n.budget_last_month.army * n.exchange))),
        ];
        for (k, (label, value)) in facts.iter().enumerate() {
            let ly = y + 200.0 + k as f32 * 26.0;
            plain(canvas, &a.font, label, vec2(x, ly), 14.0, MUTED);
            plain(canvas, &a.font, value, vec2(x + 240.0, ly), 14.0, Color::WHITE);
        }
        plain(
            canvas,
            &a.font,
            "Weapons are bought from home factories first, then abroad. War, later, will use them up.",
            vec2(x, y + 350.0),
            13.0,
            SOFT,
        );
    }

    fn draw_charts(&self, canvas: &mut Canvas, e: &Economy, n: &NationEcon, row: usize) {
        let a = &self.assets;
        let ed = &self.econ_data;
        let h = &n.history;
        let x0 = WIN.x + 26.0;
        let y0 = WIN.y + 80.0;
        let g = row.min(ed.goods.len() - 1);
        let price: Vec<f32> = h.prices[g].iter().zip(&h.day).map(|(p, _)| *p).collect();
        let world: Vec<f32> = e.world_history[g].clone();
        // Home prices in W$ need the exchange rate of the day; history keeps local prices, so show them as an index.
        let charts: [Chart; 4] = [
            ("GDP (W$ a year)", h.gdp.clone(), None, false),
            ("Inflation", h.inflation.clone(), None, true),
            ("Unemployment", h.unemployment.clone(), None, true),
            ("", price, Some(world), false),
        ];
        for (k, (title, series, other, percent)) in charts.iter().enumerate() {
            let cx = x0 + (k % 2) as f32 * 510.0;
            let cy = y0 + (k / 2) as f32 * 250.0;
            let r = Rect::new(cx, cy + 20.0, 490.0, 200.0);
            canvas.fill_rect(r, Color::rgba(20, 12, 45, 220));
            let title = if k == 3 { format!("{}: home price (local) vs world (W$), as an index", ed.goods[g].name) } else { title.to_string() };
            plain(canvas, &a.font, &title, vec2(cx, cy), 13.0, GOLD);
            canvas.label(&format!("chart {k}"), r);
            if series.len() < 2 {
                plain(canvas, &a.font, "Not enough history yet", vec2(r.x + 12.0, r.y + 90.0), 14.0, SOFT);
                continue;
            }
            let lines: Vec<(Vec<f32>, Color)> = match other {
                Some(o) if o.len() >= 2 => {
                    let norm = |v: &[f32]| -> Vec<f32> {
                        let first = v[0].abs().max(1e-9);
                        v.iter().map(|x| x / first).collect()
                    };
                    vec![(norm(series), GOLD), (norm(o), Color::rgb(127, 168, 255))]
                }
                _ => vec![(series.clone(), GOLD)],
            };
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for (v, _) in &lines {
                for &x in v {
                    lo = lo.min(x);
                    hi = hi.max(x);
                }
            }
            if *percent {
                lo = lo.min(0.0);
            }
            if hi - lo < 1e-6 {
                hi = lo + 1.0;
            }
            let pad = (hi - lo) * 0.08;
            let (lo, hi) = (lo - pad, hi + pad);
            let to = |i: usize, len: usize, v: f32| vec2(r.x + 8.0 + (r.w - 16.0) * i as f32 / (len - 1) as f32, r.bottom() - 8.0 - (r.h - 16.0) * (v - lo) / (hi - lo));
            if lo < 0.0 && hi > 0.0 {
                let z0 = to(0, 2, 0.0);
                canvas.draw_line(vec2(r.x + 4.0, z0.y), vec2(r.right() - 4.0, z0.y), Color::rgba(255, 255, 255, 50));
            }
            for (v, color) in &lines {
                // At most ~240 points: every k-th sample.
                let step = (v.len() / 240).max(1);
                let pts: Vec<(usize, f32)> = v.iter().copied().enumerate().step_by(step).collect();
                for w in pts.windows(2) {
                    canvas.draw_line(to(w[0].0, v.len(), w[0].1), to(w[1].0, v.len(), w[1].1), *color);
                }
            }
            let fmt = |x: f32| if *percent { pct(x as f64) } else if k == 0 { money(x as f64) } else { format!("{x:.2}") };
            right(canvas, &a.font, &fmt(hi), vec2(r.right() - 6.0, r.y + 4.0), 11.0, SOFT);
            right(canvas, &a.font, &fmt(lo), vec2(r.right() - 6.0, r.bottom() - 18.0), 11.0, SOFT);
            let last = lines[0].0.last().copied().unwrap_or(0.0);
            plain(canvas, &a.font, &format!("now {}", fmt(last)), vec2(r.x + 8.0, r.y + 4.0), 12.0, Color::WHITE);
        }
        let years = h.day.last().copied().unwrap_or(0) as f32 / ed.days_per_year as f32;
        plain(canvas, &a.font, &format!("Weekly, over {years:.1} years"), vec2(x0 + 700.0, WIN.bottom() - 52.0), 12.0, SOFT);
    }
}

/// A chart: title, its line, a second line to compare (as an index), and whether it's a percentage.
type Chart<'a> = (&'a str, Vec<f32>, Option<Vec<f32>>, bool);

const BUILD_BUTTON: Rect = Rect { x: 830.0, y: 560.0, w: 150.0, h: 34.0 };
const CLOSE_BUTTON: Rect = Rect { x: 994.0, y: 560.0, w: 150.0, h: 34.0 };
const POLICY_BUTTON: Rect = Rect { x: 960.0, y: 618.0, w: 190.0, h: 30.0 };
const EMBARGO_BUTTON: Rect = Rect { x: 1010.0, y: 618.0, w: 140.0, h: 30.0 };

/// The -/+ buttons of a lever row.
/// The height of a lever's row on the budget page.
const LEVER_ROW: f32 = 27.0;

fn lever_buttons(k: usize) -> (Rect, Rect) {
    let y = WIN.y + 84.0 + 22.0 + k as f32 * LEVER_ROW;
    (Rect::new(WIN.x + 400.0, y, 30.0, 28.0), Rect::new(WIN.x + 436.0, y, 30.0, 28.0))
}

/// The row under a click, on pages with rows.
fn row_at(tab: Tab, m: Vec2, rows: usize) -> Option<usize> {
    if !WIN.contains(m) {
        return None;
    }
    let (top, height, left, width) = match tab {
        Tab::Budget => (WIN.y + 102.0, LEVER_ROW, WIN.x + 20.0, 370.0),
        Tab::Market => (WIN.y + 97.0, 26.5, WIN.x + 20.0, 1010.0),
        Tab::Trade => (WIN.y + 99.0, 25.0, WIN.x + 464.0, 560.0),
        _ => return None,
    };
    if m.x < left || m.x > left + width || m.y < top {
        return None;
    }
    let r = ((m.y - top) / height) as usize;
    (r < rows).then_some(r)
}

/// The goods in the chain diagram's order: by tier, then as listed.
pub fn chain_order(ed: &EconData) -> Vec<usize> {
    let mut order: Vec<usize> = (0..ed.goods.len()).collect();
    order.sort_by_key(|&g| (column(ed, g), g));
    order
}

/// Which column of the chain a good sits in: raw, power, refined, final.
fn column(ed: &EconData, g: usize) -> usize {
    if g == ed.electricity {
        return 1;
    }
    match ed.goods[g].tier {
        Tier::Raw => 0,
        Tier::Refined => 2,
        Tier::Final => 3,
    }
}

/// Where each good's box sits in the chain diagram.
fn chain_positions(ed: &EconData) -> Vec<Vec2> {
    let order = chain_order(ed);
    let mut pos = vec![Vec2::ZERO; ed.goods.len()];
    let mut count = [0usize; 4];
    for g in order {
        let c = column(ed, g);
        pos[g] = vec2(WIN.x + 26.0 + c as f32 * 172.0, WIN.y + 104.0 + count[c] as f32 * 52.0);
        count[c] += 1;
    }
    pos
}

/// The player's trading partners, the biggest first (everyone else after).
fn partners(e: &Economy) -> Vec<usize> {
    let me = &e.nations[0];
    let mut list: Vec<usize> = (1..e.nations.len()).collect();
    list.sort_by(|&a, &b| {
        let va = me.exports_to_last[a] + me.imports_from_last[a];
        let vb = me.exports_to_last[b] + me.imports_from_last[b];
        vb.total_cmp(&va).then(a.cmp(&b))
    });
    list
}

/// Each nation's alliance bloc, named by its strongest member (by GDP order
/// of index: the lowest index wins ties): nations joined by alliances or
/// defensive pacts, directly or through others.
fn alliance_blocs(d: &crate::diplo::Diplomacy) -> Vec<usize> {
    let mut bloc: Vec<usize> = (0..d.n).collect();
    fn root(b: &mut [usize], i: usize) -> usize {
        let mut x = i;
        while b[x] != x {
            b[x] = b[b[x]];
            x = b[x];
        }
        x
    }
    for t in d.live().filter(|t| matches!(t.kind, crate::diplo::Kind::Alliance | crate::diplo::Kind::Defensive)) {
        let (ra, rb) = (root(&mut bloc, t.a), root(&mut bloc, t.b));
        if ra != rb {
            bloc[ra.max(rb)] = ra.min(rb);
        }
    }
    (0..d.n).map(|i| root(&mut bloc, i)).collect()
}
