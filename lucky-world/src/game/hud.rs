//! The map screen's HUD: the top bar, the left panel's three cards, the
//! right rail, the map-view buttons, the news strip, the minimap's buttons,
//! the right-click menu and the province details card.
//!
//! Everything here is a list of widgets (`gui`), each in a space anchored
//! to its own corner of the screen, so the UI scale never pushes one panel
//! over another (`hud_rects` and its test check that).

use duckforge::prelude::*;

use super::cmd::{Cmd, View};
use super::diplo_screen::{Page as DiploPage, SpyPage};
use super::econ_screen::{money, people, Overlay, Tab, SPEEDS, SPECTATOR_SPEED};
use super::gui::{self, Act, Kind, Space, Widget};
use super::war_screen::Page as WarPage;
use super::Lucky;
use crate::country;
use crate::diplo::Inbox;
use crate::map_view::Mode;
use crate::settings::Act as KeyAct;
use crate::ui;

/// The top bar's height, the left panel's place and width.
pub const BAR_H: f32 = 48.0;
const LEFT_X: f32 = 8.0;
const LEFT_W: f32 = 240.0;
const NATION_CARD: Rect = Rect { x: LEFT_X, y: 56.0, w: LEFT_W, h: 174.0 };
const PROVINCE_H: f32 = 168.0;
const STAT_X: f32 = 226.0;
const STAT_W: f32 = 100.0;
/// The right rail: round buttons down the right edge.
const RAIL_X: f32 = ui::W - 8.0 - 52.0;
const RAIL_Y: f32 = 58.0;
const RAIL_STEP: f32 = 62.0;
/// The minimap's frame (bottom right) and the map inside it.
/// The card after one of the player's battles: left of the rail, at its top.
pub const REPORT_CARD: Rect = Rect { x: RAIL_X - 12.0 - 316.0, y: RAIL_Y, w: 316.0, h: 96.0 };
pub const MINIMAP_FRAME: Vec2 = Vec2 { x: 976.0, y: 532.0 };
pub const MINIMAP_AT: Vec2 = Vec2 { x: 985.0, y: 545.0 };
const VIEW_W: f32 = 74.0;
const VIEW_Y: f32 = ui::H - 8.0 - 30.0;

/// Something waiting for the player (the "Needs you" card).
#[derive(Debug, Clone)]
pub struct Need {
    pub words: String,
    pub icon: &'static str,
    pub cmd: Cmd,
    /// What happens if it is left alone (the tooltip).
    pub why: String,
}

/// The right-click menu: where it opened and what it is about.
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub at: Vec2,
    pub nation: Option<usize>,
    pub province: Option<usize>,
}

impl Lucky {
    fn s(&self) -> f32 {
        self.settings.ui_scale
    }

    /// The left panel's cards, top to bottom: (your nation, selected province, needs you).
    fn left_cards(&self) -> (Rect, Option<Rect>, Option<Rect>) {
        self.left_cards_for(self.map_ui.selected.is_some(), self.needs().len())
    }

    fn left_cards_for(&self, selected: bool, n: usize) -> (Rect, Option<Rect>, Option<Rect>) {
        let mut y = NATION_CARD.bottom() + 8.0;
        let province = selected.then(|| {
            let r = Rect::new(LEFT_X, y, LEFT_W, PROVINCE_H);
            y = r.bottom() + 8.0;
            r
        });
        let needs = (n > 0).then(|| Rect::new(LEFT_X, y, LEFT_W, 30.0 + n as f32 * 28.0));
        (NATION_CARD, province, needs)
    }

    /// The news strip's place on the screen: between the view buttons and
    /// the minimap's buttons, whatever the UI scale.
    fn strip(&self) -> Rect {
        let s = self.s();
        let left = (LEFT_X + 5.0 * (VIEW_W + 4.0)) * s + 10.0;
        let right = ui::W - (ui::W - (MINIMAP_FRAME.x - 36.0)) * s - 10.0;
        Rect::new(left, ui::H - 38.0 * s, (right - left).max(80.0), 30.0 * s)
    }

    /// Every panel of the HUD as a screen rectangle, by name (for the
    /// layout test and for telling whether the mouse is over the HUD).
    pub fn hud_rects(&self) -> Vec<(&'static str, Rect)> {
        self.hud_layout(false)
    }

    /// The HUD's panels; `fullest`: as full as it gets (a province chosen,
    /// four things waiting, a battle's card showing), for the layout test.
    pub fn hud_layout(&self, fullest: bool) -> Vec<(&'static str, Rect)> {
        let s = self.s();
        let (a, b, c) = if fullest { self.left_cards_for(true, 4) } else { self.left_cards() };
        let mut out = vec![("top bar", Rect::new(0.0, 0.0, ui::W, BAR_H * s))];
        if !self.spectating {
            out.push(("your nation", Space::TopLeft.to_screen(s, a)));
            if let Some(r) = b {
                out.push(("selected province", Space::TopLeft.to_screen(s, r)));
            }
            if let Some(r) = c {
                out.push(("needs you", Space::TopLeft.to_screen(s, r)));
            }
        }
        out.push(("rail", Space::TopRight.to_screen(s, Rect::new(RAIL_X - 6.0, RAIL_Y, 64.0, 6.0 * RAIL_STEP))));
        out.push(("view buttons", Space::BottomLeft.to_screen(s, Rect::new(LEFT_X, VIEW_Y, 5.0 * (VIEW_W + 4.0) - 4.0, 30.0))));
        out.push(("news strip", self.strip()));
        out.push(("minimap", Space::BottomRight.to_screen(s, Rect::new(MINIMAP_FRAME.x - 36.0, MINIMAP_FRAME.y, 292.0 + 36.0, 176.0))));
        if fullest || self.report_card().is_some() {
            out.push(("battle card", Space::TopRight.to_screen(s, REPORT_CARD)));
        }
        out
    }

    /// The minimap's picture on the screen, in the bottom-right space's own coordinates.
    pub(super) fn minimap_rect(&self) -> Rect {
        let (w, h) = self.map_view.as_ref().map_or((272.0, 150.0), |v| (v.minimap().width() as f32, v.minimap().height() as f32));
        Rect::new(MINIMAP_AT.x, MINIMAP_AT.y, w, h)
    }

    // ---------------------------------------------------------------
    // What needs the player
    // ---------------------------------------------------------------

    /// What is waiting for the player, most urgent first, at most four.
    pub fn needs(&self) -> Vec<Need> {
        let mut out: Vec<Need> = Vec::new();
        if self.spectating {
            return out;
        }
        let (Some(e), Some(w), Some(d)) = (self.economy.as_ref(), self.wars.as_ref(), self.diplo.as_ref()) else { return out };
        let ed = &self.econ_data;
        // An escalation waiting for our answer.
        for (k, war) in w.wars.iter().enumerate() {
            if let (true, Some((by, level, deadline)), Some(side)) = (war.active(), war.pending, crate::war::player::side(w, k)) {
                if by != side {
                    let who = &w.names[war.sides[by][0]];
                    out.push(Need {
                        words: format!("{who} escalates: answer in {} days", deadline.saturating_sub(w.day)),
                        icon: "war",
                        cmd: Cmd::War(WarPage::Wars),
                        why: format!("They went up to a {}. Match it or back down in the war room; with no answer your generals fight on.", crate::war::LEVELS[level.min(4)].to_lowercase()),
                    });
                }
            }
        }
        // Peace offers.
        for o in &w.offers {
            let who = &w.names[w.wars[o.0].sides[o.1][0]];
            out.push(Need { words: format!("{who} offers peace"), icon: "diplomacy", cmd: Cmd::War(WarPage::Wars), why: format!("Their terms: {}. Accept or refuse in the war room.", o.2.words(w)) });
        }
        // Calls to arms and proposals.
        for item in &d.inbox {
            match item {
                Inbox::Call { ally, deadline, .. } => out.push(Need {
                    words: format!("{} calls you to war", d.names[*ally]),
                    icon: "war",
                    cmd: Cmd::Diplo(DiploPage::Inbox, None),
                    why: format!("Answer within {} days: refusing breaks the treaty and costs your reputation.", deadline.saturating_sub(d.day)),
                }),
                Inbox::Proposal(p) => out.push(Need {
                    words: format!("{} offers a {}", d.names[p.from], p.kind.name().to_lowercase()),
                    icon: "diplomacy",
                    cmd: Cmd::Diplo(DiploPage::Inbox, None),
                    why: "A treaty offer is waiting in your inbox: accept or decline.".to_string(),
                }),
            }
        }
        // Generals' requests (when supply isn't on Auto).
        let open = w.forces[0].requests.iter().filter(|r| r.answered.is_none()).count();
        if open > 0 && !w.forces[0].auto {
            out.push(Need {
                words: format!("{open} supply request{} from your generals", if open == 1 { "" } else { "s" }),
                icon: "army",
                cmd: Cmd::War(WarPage::Supply),
                why: "Your generals ask for weapons and fuel: approve or deny (or put supply back on Auto).".to_string(),
            });
        }
        // Shortages: the worst two goods (weapons only matter in a war).
        let n = &e.nations[0];
        let fighting = w.wars.iter().any(|x| x.end.is_none() && x.sides.iter().any(|s| s.contains(&0)));
        let mut short: Vec<(f64, usize)> = (0..ed.goods.len()).filter(|&g| n.wanted[g] > 1e-6 && (fighting || !ed.goods[g].military)).map(|g| (super::econ_screen::supply_ratio(n, g), g)).filter(|x| x.0 < 0.6).collect();
        short.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (ratio, g) in short.into_iter().take(2) {
            out.push(Need {
                words: format!("Short of {}: {:.0}% of what's needed", ed.goods[g].name.to_lowercase(), ratio * 100.0),
                icon: "warning",
                cmd: Cmd::Econ(Tab::Market, g),
                why: "Buildings that need it slow down and people go without. Build more of it, import it, or trade for it.".to_string(),
            });
        }
        if n.stability < 25.0 {
            out.push(Need { words: format!("Stability is {:.0}: unrest is near", n.stability), icon: "stability", cmd: Cmd::Econ(Tab::People, 0), why: "Under 20 for long, a civil war can start. See what drags it down.".to_string() });
        }
        // Spy reports not yet read.
        let fresh = d.spies.reports.iter().filter(|r| r.by == 0 && r.day >= self.map_ui.spy_seen_day).count();
        if fresh > 0 {
            out.push(Need { words: format!("{fresh} new spy report{}", if fresh == 1 { "" } else { "s" }), icon: "spies", cmd: Cmd::Spies(SpyPage::Reports, None), why: "Your agents have reported back.".to_string() });
        }
        out.truncate(4);
        out
    }

    /// How many things wait behind each rail button: (economy, war room, battles, diplomacy, spies, news).
    fn badges(&self) -> [usize; 6] {
        let (Some(e), Some(w), Some(d)) = (self.economy.as_ref(), self.wars.as_ref(), self.diplo.as_ref()) else { return [0; 6] };
        if self.spectating {
            return [0, 0, w.battles.iter().filter(|b| !b.over).count(), 0, 0, 0];
        }
        let n = &e.nations[0];
        let short = (0..self.econ_data.goods.len()).filter(|&g| n.wanted[g] > 1e-6 && super::econ_screen::supply_ratio(n, g) < 0.6).count();
        let pending = w.wars.iter().enumerate().filter(|(k, x)| x.active() && x.pending.is_some_and(|p| crate::war::player::side(w, *k).is_some_and(|s| s != p.0))).count();
        let requests = if w.forces[0].auto { 0 } else { w.forces[0].requests.iter().filter(|r| r.answered.is_none()).count() };
        let battles = w.battles.iter().filter(|b| !b.over && b.sides.iter().flatten().any(|&f| w.nation_of(f) == 0)).count();
        let spies = d.spies.reports.iter().filter(|r| r.by == 0 && r.day >= self.map_ui.spy_seen_day).count();
        let news = self.notes.log.len().saturating_sub(self.map_ui.log_seen);
        [short, pending + w.offers.len() + requests, battles, d.inbox.len(), spies, news]
    }

    // ---------------------------------------------------------------
    // The widgets
    // ---------------------------------------------------------------

    /// Everything clickable on the map's HUD.
    pub(super) fn hud_widgets(&self) -> Vec<Widget> {
        let mut ws: Vec<Widget> = Vec::new();
        let s = self.s();
        let (Some(world), Some(e)) = (self.world.as_ref(), self.economy.as_ref()) else { return ws };
        let ed = &self.econ_data;
        let key = |a: KeyAct| self.settings.key(a);
        // The panels themselves swallow clicks meant for the map under them.
        for (_, r) in self.hud_rects() {
            ws.push(Widget::ghost(r).space(Space::Screen));
        }
        // The last battle's card: a click reads the report in full.
        if let Some(k) = self.report_card() {
            ws.push(Widget::new(Kind::Ghost, REPORT_CARD, "", Act::Cmd(Cmd::Report(k))).space(Space::TopRight).tip("Click: the full battle report"));
        }

        // ---- The top bar: the nation, five numbers, the clock, the menu.
        if self.spectating {
            ws.push(Widget::ghost(Rect::new(6.0, 4.0, 420.0, 40.0)).space(Space::TopLeft));
        } else {
            let n = &e.nations[0];
            ws.push(Widget::new(Kind::Ghost, Rect::new(6.0, 4.0, 214.0, 40.0), "", Act::Cmd(Cmd::Card(0))).space(Space::TopLeft).tip("Your country card: all your wheels"));
            let stats: [(&str, &str, String, Cmd, &str, bool); 5] = [
                ("treasury", "Treasury", money(n.treasury * n.exchange), Cmd::Econ(Tab::Budget, 0), "treasury", n.treasury < 0.0),
                ("gdp", "GDP", money(n.gdp), Cmd::Econ(Tab::Charts, 0), "gdp", false),
                ("inflation", "Inflation", format!("{:.1}%", n.inflation * 100.0), Cmd::Econ(Tab::Budget, 5), "inflation", n.inflation > 0.15),
                ("jobless", "Jobless", format!("{:.1}%", n.unemployment(ed) * 100.0), Cmd::Econ(Tab::People, 0), "jobless", n.unemployment(ed) > 0.15),
                ("stability", "Stability", format!("{:.0}", n.stability), Cmd::Econ(Tab::People, 1), "stability", n.stability < 25.0),
            ];
            for (k, (icon, label, value, cmd, tip, bad)) in stats.into_iter().enumerate() {
                let opens = ["the budget", "the charts", "the central bank's rate", "people and jobs", "what moves stability"][k];
                let mut w = Widget::new(Kind::Stat, Rect::new(STAT_X + k as f32 * STAT_W, 2.0, STAT_W - 2.0, BAR_H - 4.0), value, Act::Cmd(cmd))
                    .space(Space::TopLeft)
                    .sub(label)
                    .icon(icon)
                    .tip(format!("{} Click: {opens}.", self.tip_text.get(tip).cloned().unwrap_or_default()));
                w.danger = bad;
                ws.push(w);
            }
        }
        let speeds = if self.spectating { SPECTATOR_SPEED + 1 } else { SPECTATOR_SPEED };
        let menu = Rect::new(ui::W - 8.0 - 76.0, 8.0, 76.0, 32.0);
        ws.push(Widget::cmd(menu, "Menu", Cmd::Menu).space(Space::TopRight).icon("menu").tip("The pause menu: save, load, settings, help"));
        let x0 = menu.x - 8.0 - speeds as f32 * 26.0;
        let digits = [Key::Digit1, Key::Digit2, Key::Digit3, Key::Digit4, Key::Digit5, Key::Digit6];
        for k in 0..speeds {
            ws.push(
                Widget::new(Kind::Icon, Rect::new(x0 + k as f32 * 26.0, 10.0, 24.0, 28.0), format!("{}", k + 1), Act::Key(digits[k]))
                    .space(Space::TopRight)
                    .on(self.clock.speed == k && !self.clock.paused)
                    .tip(format!("Speed {}: {} day{} a second", k + 1, SPEEDS[k], if SPEEDS[k] == 1.0 { "" } else { "s" })),
            );
        }
        ws.push(
            Widget::new(Kind::Icon, Rect::new(x0 - 34.0, 10.0, 28.0, 28.0), "", Act::Key(key(KeyAct::Pause)))
                .space(Space::TopRight)
                .icon(if self.clock.paused { "play" } else { "pause" })
                .on(self.clock.paused)
                .tip(if self.clock.paused { "The clock is stopped: click to go on" } else { "Stop the clock" }),
        );
        ws.push(Widget::ghost(Rect::new(x0 - 34.0 - 118.0, 4.0, 112.0, 40.0)).space(Space::TopRight).tip(self.tip_text.get("date").cloned().unwrap_or_default()));

        // ---- The left panel.
        let (nation, province, needs) = self.left_cards();
        if !self.spectating {
            let y = nation.bottom() - 40.0;
            ws.push(Widget::cmd(Rect::new(nation.x + 8.0, y, 110.0, 30.0), "Country card", Cmd::Card(0)).space(Space::TopLeft).tip("Your nation's sixteen... all your wheels, on one card").icon("card"));
            ws.push(Widget::new(Kind::Button, Rect::new(nation.x + 122.0, y, 110.0, 30.0), "Go home", Act::Key(key(KeyAct::Home))).space(Space::TopLeft).icon("home").tip("The camera goes back to your nation"));
            if let (Some(r), Some(p)) = (province, self.map_ui.selected) {
                let mine = world.owner[p] == Some(0);
                let y = r.bottom() - 40.0;
                ws.push(
                    Widget::cmd(Rect::new(r.x + 8.0, y, 110.0, 30.0), "Build here", Cmd::BuildHere)
                        .space(Space::TopLeft)
                        .on(mine)
                        .off(!mine)
                        .icon("build")
                        .tip(if mine { "Choose what to build in this province" } else { "You can only build in your own provinces" }),
                );
                ws.push(Widget::cmd(Rect::new(r.x + 122.0, y, 110.0, 30.0), "Details", Cmd::Details).space(Space::TopLeft).on(self.map_ui.details).tip("Coast, rivers, roads, deposits and neighbours"));
            }
            if let Some(r) = needs {
                for (k, need) in self.needs().into_iter().enumerate() {
                    ws.push(Widget::new(Kind::Row, Rect::new(r.x + 4.0, r.y + 26.0 + k as f32 * 28.0, r.w - 8.0, 26.0), need.words, Act::Cmd(Cmd::Need(k))).space(Space::TopLeft).icon(need.icon).tip(need.why));
                }
            }
        }

        // ---- The right rail.
        let badges = self.badges();
        let battle_live = self.biggest_battle().is_some();
        let log_open = self.notes.log_open;
        let rail: [(&str, &str, Act, &str, bool); 6] = [
            ("economy", "Economy", Act::Key(key(KeyAct::Economy)), "Budget, production chains, market, trade, people", self.map_ui.econ.open.is_some()),
            ("war", "War room", Act::Key(key(KeyAct::WarRoom)), "Wars, armies, supply, battle reports", self.map_ui.war.open.is_some()),
            ("battles", "Battles", if battle_live { Act::Key(key(KeyAct::Battle)) } else { Act::Cmd(Cmd::War(WarPage::Reports)) }, if battle_live { "Jump to the biggest battle going on" } else { "No battle now: the battle reports" }, self.map_ui.war.watching.is_some()),
            ("diplomacy", "Diplomacy", Act::Key(key(KeyAct::Diplomacy)), "Relations, treaties, proposals, peace talks", self.map_ui.diplo.open.is_some()),
            ("spies", "Spies", Act::Key(key(KeyAct::Spies)), "Agents, missions, reports", self.map_ui.diplo.spies.is_some()),
            ("news", "News log", Act::Key(key(KeyAct::Log)), "Everything that has happened", log_open),
        ];
        for (k, (icon, label, act, tip, on)) in rail.into_iter().enumerate() {
            ws.push(Widget::new(Kind::Round, Rect::new(RAIL_X, RAIL_Y + k as f32 * RAIL_STEP, 52.0, 58.0), label, act).space(Space::TopRight).icon(icon).tip(tip).on(on).badge(badges[k]));
        }

        // ---- Bottom left: the map's views.
        let now = self.view_now();
        for (k, v) in View::ALL.into_iter().enumerate() {
            ws.push(Widget::cmd(Rect::new(LEFT_X + k as f32 * (VIEW_W + 4.0), VIEW_Y, VIEW_W, 30.0), v.name(), Cmd::View(v)).space(Space::BottomLeft).on(now == Some(v)).tip(v.tip()));
        }

        // ---- Bottom centre: the news strip's Show button.
        let strip = self.strip();
        if self.map_ui.news_at.is_some() {
            ws.push(Widget::cmd(Rect::new(strip.right() - 58.0 * s, strip.y + 3.0 * s, 54.0 * s, strip.h - 6.0 * s), "Show", Cmd::ShowNews).space(Space::Screen).icon("show").tip("The camera goes to where it happened"));
        }
        ws.push(Widget::new(Kind::Ghost, Rect::new(strip.x, strip.y, strip.w - 62.0 * s, strip.h), "", Act::Key(key(KeyAct::Log))).space(Space::Screen).tip("The latest news. Click: the whole news log"));

        // ---- Bottom right: the minimap's buttons.
        let bx = MINIMAP_FRAME.x - 34.0;
        ws.push(Widget::new(Kind::Icon, Rect::new(bx, MINIMAP_FRAME.y + 10.0, 28.0, 28.0), "", Act::Cmd(Cmd::Zoom(-1))).space(Space::BottomRight).icon("plus").tip("Zoom in (mouse wheel)"));
        ws.push(Widget::new(Kind::Icon, Rect::new(bx, MINIMAP_FRAME.y + 42.0, 28.0, 28.0), "", Act::Cmd(Cmd::Zoom(1))).space(Space::BottomRight).icon("minus").tip("Zoom out (mouse wheel)"));
        ws.push(Widget::new(Kind::Icon, Rect::new(bx, MINIMAP_FRAME.y + 74.0, 28.0, 28.0), "", Act::Key(key(KeyAct::Home))).space(Space::BottomRight).icon("home").tip("Centre on my nation"));
        ws
    }

    /// The view the map is in now, if it is one of the five buttons'.
    pub(super) fn view_now(&self) -> Option<View> {
        let terrain = self.map_view.as_ref().is_some_and(|v| v.mode == Mode::Terrain);
        match self.map_ui.econ.overlay {
            Overlay::Off if terrain => Some(View::Terrain),
            Overlay::Off => Some(View::Nations),
            Overlay::Resources => Some(View::Resources),
            Overlay::Industry => Some(View::Industry),
            Overlay::Alliances => Some(View::Alliances),
            _ => None,
        }
    }

    pub(super) fn set_view(&mut self, v: View) {
        if let Some(view) = self.map_view.as_mut() {
            view.mode = if v == View::Terrain { Mode::Terrain } else { Mode::Political };
        }
        let ui = &mut self.map_ui.econ;
        ui.overlay = match v {
            View::Nations | View::Terrain => Overlay::Off,
            View::Resources => Overlay::Resources,
            View::Industry => Overlay::Industry,
            View::Alliances => Overlay::Alliances,
        };
        ui.force_overlay();
        self.map_ui.war.all_armies = false;
        self.map_ui.war.refresh();
        self.refresh_overlay();
    }

    /// The right-click menu's lines.
    pub(super) fn menu_widgets(&self) -> Vec<Widget> {
        let Some(m) = self.map_ui.menu.as_ref() else { return Vec::new() };
        let Some(world) = self.world.as_ref() else { return Vec::new() };
        let mut items: Vec<(&str, &'static str, Cmd, bool, &str)> = Vec::new();
        match m.nation {
            Some(0) if !self.spectating => {
                items.push(("Country card", "card", Cmd::Card(0), false, "Your wheels"));
                items.push(("Economy", "economy", Cmd::Econ(Tab::Budget, 0), false, "Your budget"));
                if m.province.is_some_and(|p| world.owner[p] == Some(0)) {
                    items.push(("Build here", "build", Cmd::BuildHere, false, "Choose what to build in this province"));
                }
                items.push(("Centre the map here", "show", Cmd::Centre(0), false, ""));
            }
            Some(n) => {
                items.push(("Country card", "card", Cmd::Card(n), false, "Their wheels"));
                items.push(("Diplomacy", "diplomacy", Cmd::Diplo(DiploPage::Nations, Some(n)), false, "How they see you, and why"));
                if !self.spectating {
                    let at_war = self.wars.as_ref().is_some_and(|w| w.at_war(0, n).is_some());
                    items.push(("Propose a treaty", "diplomacy", Cmd::Diplo(DiploPage::Propose, Some(n)), false, "Trade, non-aggression, alliance and more"));
                    items.push(("Trade", "trade", Cmd::TradeWith(n), false, "What you buy and sell with them; embargo"));
                    items.push(("Send spies", "spies", Cmd::Spies(SpyPage::Missions, Some(n)), false, "Choose a mission against them"));
                    if at_war {
                        items.push(("War room", "war", Cmd::War(WarPage::Wars), false, "You are at war with them"));
                    } else {
                        items.push(("Declare war", "war", Cmd::AskWar(n), true, "Asks you to confirm first"));
                    }
                }
                items.push(("Centre the map here", "show", Cmd::Centre(n), false, ""));
            }
            None => items.push(("Unclaimed land", "province", Cmd::CloseMenu, false, "Nobody owns this province")),
        }
        let (w, row) = (184.0, 27.0);
        let h = 30.0 + items.len() as f32 * row + 6.0;
        let at = vec2(m.at.x.min(ui::W - w - 6.0), m.at.y.min(ui::H - h - 6.0));
        let mut ws = vec![Widget::ghost(Rect::new(at.x, at.y, w, h)).space(Space::Screen)];
        for (k, (label, icon, cmd, danger, tip)) in items.into_iter().enumerate() {
            let mut wd = Widget::new(Kind::Menu, Rect::new(at.x + 3.0, at.y + 30.0 + k as f32 * row, w - 6.0, row - 1.0), label, Act::Cmd(cmd)).space(Space::Screen).icon(icon).tip(tip);
            wd.danger = danger;
            ws.push(wd);
        }
        ws
    }

    /// The right-click menu's own row in the diplomacy list.
    pub(super) fn diplo_row_to(&mut self, nation: usize) {
        if let Some(d) = self.diplo.as_ref() {
            let v = if self.spectating { self.map_ui.brain.unwrap_or(0) } else { 0 };
            let mut list: Vec<usize> = (0..d.n).filter(|&j| j != v).collect();
            list.sort_by(|&a, &b| d.relations[v][b].total_cmp(&d.relations[v][a]).then(a.cmp(&b)));
            self.map_ui.diplo.row = list.iter().position(|&j| j == nation).unwrap_or(0);
        }
    }

    pub(super) fn trade_row_of(&self, nation: usize) -> usize {
        self.economy.as_ref().map_or(0, |e| super::econ_screen::partners(e).iter().position(|&j| j == nation).unwrap_or(0))
    }

    /// The camera glides to a nation's capital.
    pub(super) fn centre_on(&mut self, nation: usize) {
        let (Some(world), Some(view)) = (self.world.as_ref(), self.map_view.as_ref()) else { return };
        let Some(&capital) = world.capital.get(nation) else { return };
        let (x, y) = self.atlas.map(world.map).provinces[capital].middle;
        let p = view.world(x, y, 0.0);
        self.map_ui.glide = Some(vec2(p.x, p.z));
    }

    /// The battle whose card shows now (none while a window or report is open).
    pub(super) fn report_card(&self) -> Option<usize> {
        let (k, _) = self.map_ui.war.card?;
        let open = self.map_ui.war.full.is_some() || self.map_ui.window_open() || self.map_ui.war.watching.is_some();
        (!open && self.wars.as_ref().is_some_and(|w| k < w.reports.len())).then_some(k)
    }

    /// The camera at the start: over the player's nation, near enough to
    /// see all of it and a little of its neighbours.
    pub(super) fn frame_nation(&mut self, nation: usize) {
        let (Some(world), Some(view)) = (self.world.as_ref(), self.map_view.as_mut()) else { return };
        let map = self.atlas.map(world.map);
        let points: Vec<Vec2> = world
            .provinces_of(nation)
            .into_iter()
            .map(|p| {
                let (x, y) = map.provinces[p].middle;
                let q = view.world(x, y, 0.0);
                vec2(q.x, q.z)
            })
            .collect();
        if points.is_empty() {
            return;
        }
        let n = points.len() as f32;
        let mid = points.iter().fold(Vec2::ZERO, |a, p| a + *p) * (1.0 / n);
        let reach = points.iter().map(|p| p.distance(mid)).fold(0.0, f32::max);
        view.orbit.target = mid;
        view.orbit.distance = (reach * 2.4 + 9.0).clamp(9.0, 30.0);
        view.orbit.yaw = 0.0;
        view.orbit.pitch = 1.05;
        self.map_ui.glide = None;
    }

    /// War on a nation, from the right-click menu (after the question).
    pub(super) fn declare_war(&mut self, ctx: &mut Context, nation: usize) {
        let level = self.map_ui.war.declare_level;
        let (Some(w), Some(e)) = (self.wars.as_mut(), self.economy.as_mut()) else { return };
        match crate::war::player::declare(w, e, &self.econ_data, nation, level) {
            Some(_) => {
                self.map_ui.war.refresh();
                self.sound_named(ctx, "declare");
            }
            None => {
                let name = w.names[nation].clone();
                crate::ui::sfx(ctx, &self.assets.denied, 0.7, 0.0);
                self.notify(ctx, format!("You can't declare war on {name} now (a truce, or already at war)"), super::shell::IMPORTANT);
            }
        }
    }

    // ---------------------------------------------------------------
    // Drawing
    // ---------------------------------------------------------------

    /// The HUD: panels, their words, then every widget. `live`: nothing is
    /// open on top of it, so buttons light up under the mouse.
    pub(super) fn draw_hud(&self, canvas: &mut Canvas, live: bool) {
        let (Some(world), Some(e)) = (self.world.as_ref(), self.economy.as_ref()) else { return };
        let a = &self.assets;
        let ed = &self.econ_data;
        let s = self.s();

        // ---- The top bar.
        Space::TopLeft.push(canvas, s);
        canvas.fill_rect(Rect::new(0.0, 0.0, ui::W / s + 2.0, BAR_H), gui::NAVY);
        canvas.fill_rect(Rect::new(0.0, BAR_H - 1.0, ui::W / s + 2.0, 1.0), gui::BRONZE);
        if self.spectating {
            ui::plain(canvas, &a.bold, "SPECTATING", vec2(12.0, 6.0), 11.0, gui::GOLD, Align::Left);
            let words = format!("{} NPC brains, {} difficulty · click a nation to read its mind", world.nations.len(), self.brains.as_ref().map_or("", |b| b.difficulty_name.as_str()));
            gui::fit(canvas, &a.font, &words, vec2(12.0, 21.0), 16.0, gui::IVORY, Align::Left, 440.0);
        } else {
            let player = &world.nations[0];
            country::draw_flag(canvas, Rect::new(10.0, 9.0, 44.0, 30.0), &player.flag);
            canvas.draw_rect(Rect::new(10.0, 9.0, 44.0, 30.0), gui::BRONZE);
            gui::fit(canvas, &a.serif, &player.name, vec2(62.0, 5.0), 18.0, gui::IVORY, Align::Left, 156.0);
            if let Some(w) = self.wars.as_ref() {
                let sc = crate::victory::scores(&self.victory.victory, e, ed, w);
                let rank = 1 + (1..sc.len()).filter(|&j| sc[j] > sc[0]).count();
                let r = ui::plain(canvas, &a.font, &format!("Score {:.0} · rank {} of {}", sc[0], rank, sc.len()), vec2(62.0, 28.0), 12.0, gui::LABEL, Align::Left);
                if live {
                    self.tip_screen(r, self.victory_progress());
                }
            }
        }
        canvas.pop_camera();
        Space::TopRight.push(canvas, s);
        let speeds = if self.spectating { SPECTATOR_SPEED + 1 } else { SPECTATOR_SPEED };
        let date_right = ui::W - 8.0 - 76.0 - 8.0 - speeds as f32 * 26.0 - 34.0 - 10.0;
        ui::plain(canvas, &a.bold, &e.date(ed), vec2(date_right, 7.0), 16.0, gui::IVORY, Align::Right);
        let pace = if self.clock.paused { "paused".to_string() } else { format!("{} day{} a second", SPEEDS[self.clock.speed], if SPEEDS[self.clock.speed] == 1.0 { "" } else { "s" }) };
        ui::plain(canvas, &a.font, &pace, vec2(date_right, 28.0), 11.0, if self.clock.paused { gui::GOLD } else { gui::LABEL }, Align::Right);
        canvas.pop_camera();

        // ---- The left panel's cards.
        if !self.spectating {
            let (nation, province, needs) = self.left_cards();
            Space::TopLeft.push(canvas, s);
            let n = &e.nations[0];
            let player = &world.nations[0];
            gui::panel(canvas, a, nation);
            canvas.label("your nation", nation);
            gui::title(canvas, a, "Your nation", vec2(nation.x + 12.0, nation.y + 9.0));
            let at_war = self.wars.as_ref().is_some_and(|w| w.level_of(0) >= 1);
            let soldiers = self.wars.as_ref().map_or(n.army, |w| w.strength(0).0);
            let rows = [
                ("Government", player.label(&self.data, "ideology")),
                ("Leader", player.label(&self.data, "leader")),
                ("Provinces", format!("{} of {}", world.provinces_of(0).len(), world.owner.len())),
                ("People", people(n.population)),
                ("Army", format!("{} · {}", people(soldiers), if at_war { "at war" } else { "at peace" })),
            ];
            for (k, (label, value)) in rows.iter().enumerate() {
                let y = nation.y + 30.0 + k as f32 * 19.0;
                ui::plain(canvas, &a.font, label, vec2(nation.x + 12.0, y), 13.0, gui::LABEL, Align::Left);
                gui::fit(canvas, &a.font, value, vec2(nation.right() - 12.0, y), 13.0, if k == 4 && at_war { gui::RED } else { gui::IVORY }, Align::Right, 140.0);
            }
            if let (Some(r), Some(p)) = (province, self.map_ui.selected) {
                gui::panel(canvas, a, r);
                canvas.label("selected province", r);
                self.draw_province_card(canvas, r, p);
            }
            if let Some(r) = needs {
                gui::panel(canvas, a, r);
                canvas.label("needs you", r);
                gui::title(canvas, a, "Needs you", vec2(r.x + 12.0, r.y + 9.0));
            }
            canvas.pop_camera();
        }

        // ---- The news strip.
        let strip = self.strip();
        gui::panel(canvas, a, strip);
        canvas.label("news strip", strip);
        let (words, fresh) = match (&self.toast, self.notes.log.last()) {
            (Some((w, _)), _) => (w.clone(), true),
            (None, Some((date, w, _))) => (format!("{date}: {w}"), false),
            (None, None) => ("No news yet. The world is quiet.".to_string(), false),
        };
        gui::icon(canvas, a, "news", vec2(strip.x + 8.0 * s, strip.y + 7.0 * s), 16.0 * s, if fresh { gui::GOLD } else { gui::LABEL });
        let room = strip.w - 34.0 * s - if self.map_ui.news_at.is_some() { 62.0 * s } else { 6.0 * s };
        gui::fit(canvas, &a.font, &words, vec2(strip.x + 30.0 * s, strip.y + 7.0 * s), 14.0 * s, if fresh { gui::IVORY } else { gui::LABEL }, Align::Left, room);
        let waiting = self.notes.queue_len();
        if waiting > 0 {
            ui::plain(canvas, &a.font, &format!("+{waiting}"), vec2(strip.x + 30.0 * s + room, strip.y + 8.0 * s), 12.0 * s, gui::GOLD, Align::Right);
        }

        // ---- The minimap.
        self.draw_minimap(canvas);

        // ---- Every button.
        self.draw_widgets(canvas, &self.hud_widgets(), live);
    }

    fn draw_province_card(&self, canvas: &mut Canvas, r: Rect, p: usize) {
        let (Some(world), a) = (self.world.as_ref(), &self.assets) else { return };
        let map = self.atlas.map(world.map);
        let prov = &map.provinces[p];
        gui::title(canvas, a, "Selected province", vec2(r.x + 12.0, r.y + 9.0));
        gui::fit(canvas, &a.serif, &prov.name, vec2(r.x + 12.0, r.y + 26.0), 19.0, gui::IVORY, Align::Left, r.w - 24.0);
        let owner = world.owner[p];
        let who = owner.map_or("Unclaimed land".to_string(), |o| if o == 0 && !self.spectating { "Yours".to_string() } else { world.nations[o].name.clone() });
        if let Some(o) = owner {
            canvas.fill_rect(Rect::new(r.x + 12.0, r.y + 54.0, 10.0, 10.0), world.color[o]);
        }
        gui::fit(canvas, &a.font, &who, vec2(r.x + if owner.is_some() { 28.0 } else { 12.0 }, r.y + 51.0), 13.0, if owner == Some(0) { gui::GOLD } else { gui::IVORY }, Align::Left, r.w - 44.0);
        let continent = &self.data.maps[world.map].continents[prov.continent].label;
        let terrain = &self.atlas.terrains[prov.terrain].name;
        let mut rows: Vec<(&str, String)> = vec![("Land", format!("{terrain}{} · {continent}", if prov.coastal { ", coast" } else { "" }))];
        if let (Some(e), Some(o)) = (self.economy.as_ref(), owner) {
            let ed = &self.econ_data;
            rows.push(("People", people(e.nations[o].population * e.provinces[p].people)));
            let mut built: Vec<(f64, usize)> = Vec::new();
            for b in e.nations[o].buildings.iter().filter(|b| b.province == p) {
                match built.iter_mut().find(|(_, k)| *k == b.kind) {
                    Some(x) => x.0 += b.levels,
                    None => built.push((b.levels, b.kind)),
                }
            }
            built.sort_by(|a, b| b.0.total_cmp(&a.0));
            let names: Vec<String> = built.iter().take(2).map(|(l, k)| format!("{} {:.0}", ed.buildings[*k].name, l)).collect();
            let more = built.len().saturating_sub(2);
            rows.push(("Buildings", if names.is_empty() { "none".to_string() } else if more > 0 { format!("{} +{more}", names.join(", ")) } else { names.join(", ") }));
        }
        for (k, (label, value)) in rows.iter().enumerate() {
            let y = r.y + 72.0 + k as f32 * 18.0;
            ui::plain(canvas, &a.font, label, vec2(r.x + 12.0, y), 13.0, gui::LABEL, Align::Left);
            gui::fit(canvas, &a.font, value, vec2(r.right() - 12.0, y), 13.0, gui::IVORY, Align::Right, r.w - 90.0);
        }
    }

    /// The province details card (beside the left panel) and its close button.
    pub(super) fn details_widgets(&self) -> Vec<Widget> {
        if !self.map_ui.details || self.map_ui.selected.is_none() {
            return Vec::new();
        }
        let r = self.details_rect();
        vec![
            Widget::ghost(r).space(Space::TopLeft),
            Widget::new(Kind::Icon, Rect::new(r.right() - 30.0, r.y + 6.0, 24.0, 24.0), "", Act::Cmd(Cmd::Details)).space(Space::TopLeft).icon("close").tip("Close"),
        ]
    }

    fn details_rect(&self) -> Rect {
        Rect::new(LEFT_X + LEFT_W + 8.0, NATION_CARD.bottom() + 8.0, 300.0, 316.0)
    }

    pub(super) fn draw_details(&self, canvas: &mut Canvas, live: bool) {
        let (Some(p), true, Some(world)) = (self.map_ui.selected, self.map_ui.details, self.world.as_ref()) else { return };
        let a = &self.assets;
        let map = self.atlas.map(world.map);
        let prov = &map.provinces[p];
        let r = self.details_rect();
        Space::TopLeft.push(canvas, self.s());
        gui::panel(canvas, a, r);
        canvas.label("province details", r);
        gui::title(canvas, a, "Province details", vec2(r.x + 12.0, r.y + 10.0));
        gui::fit(canvas, &a.serif, &prov.name, vec2(r.x + 12.0, r.y + 28.0), 20.0, gui::IVORY, Align::Left, r.w - 50.0);
        let mut facts: Vec<String> = Vec::new();
        facts.push(if prov.coastal { format!("Coastal ({} sea edges)", prov.sea_edges) } else { "Inland: no coast".to_string() });
        if prov.river {
            let names: Vec<&str> = map.rivers.iter().filter(|rv| rv.cells.iter().any(|&(c, rr)| map.province_of[map.index(c, rr)] == Some(p))).map(|rv| rv.name.as_str()).collect();
            facts.push(format!("River: {}", names.join(", ")));
        }
        if prov.lake {
            facts.push("On a lake shore".to_string());
        }
        for l in &map.landmarks {
            if map.province_of[map.index(l.at.0, l.at.1)] == Some(p) {
                facts.push(format!("Landmark: {}", l.name));
            }
        }
        facts.push(format!("{} hexes", prov.cells.len()));
        if let Some(e) = self.economy.as_ref() {
            let ed = &self.econ_data;
            let pe = &e.provinces[p];
            if world.owner[p].is_some() {
                facts.push(format!("Roads {:.1} · rail {:.1} · port {:.1}", pe.road, pe.rail, pe.port));
                facts.push(format!("Goods moving: {:.0}% of what's wanted", pe.transport * 100.0));
            }
            let mut rich: Vec<usize> = (0..ed.goods.len()).filter(|&g| pe.deposits[g] >= 0.2).collect();
            rich.sort_by(|&x, &y| pe.deposits[y].total_cmp(&pe.deposits[x]));
            if !rich.is_empty() {
                let names: Vec<String> = rich.iter().take(3).map(|&g| format!("{} {:.1}", ed.goods[g].name, pe.deposits[g])).collect();
                facts.push(format!("Rich in {}", names.join(", ")));
            }
            if let Some(o) = pe.occupied_by {
                facts.push(format!("Occupied by {}", world.nations.get(o).map_or("rebels", |c| c.name.as_str())));
            }
        }
        let mut y = r.y + 58.0;
        for words in &facts {
            canvas.fill_circle(vec2(r.x + 17.0, y + 8.0), 2.5, gui::BRONZE);
            let rr = gui::fit(canvas, &a.font, words, vec2(r.x + 28.0, y), 14.0, gui::IVORY, Align::Left, r.w - 40.0);
            if live {
                if words.starts_with("Roads") {
                    self.tip_screen(rr, self.tip_text.get("roads").cloned().unwrap_or_default());
                } else if words.starts_with("Rich in") {
                    self.tip_screen(rr, self.tip_text.get("deposits").cloned().unwrap_or_default());
                }
            }
            y += 21.0;
        }
        let names: Vec<&str> = prov.neighbours.iter().map(|&q| map.provinces[q].name.as_str()).collect();
        ui::plain(canvas, &a.bold, &format!("NEIGHBOURS ({})", names.len()), vec2(r.x + 12.0, y + 6.0), 11.0, gui::LABEL, Align::Left);
        let style = TextStyle { size: 13.0, color: gui::LABEL, wrap: Some(r.w - 24.0), ..TextStyle::default() };
        canvas.draw_font_text(&a.font, &if names.is_empty() { "none (an island)".to_string() } else { names.join(", ") }, vec2(r.x + 12.0, y + 22.0), &style);
        canvas.pop_camera();
        self.draw_widgets(canvas, &self.details_widgets(), live);
    }

    /// The right-click menu.
    pub(super) fn draw_menu_popup(&self, canvas: &mut Canvas) {
        let Some(m) = self.map_ui.menu.as_ref() else { return };
        let ws = self.menu_widgets();
        let Some(frame) = ws.first() else { return };
        let a = &self.assets;
        gui::panel(canvas, a, frame.rect);
        canvas.label("right-click menu", frame.rect);
        let name = match (m.nation, self.world.as_ref()) {
            (Some(n), Some(world)) => {
                canvas.fill_rect(Rect::new(frame.rect.x + 10.0, frame.rect.y + 10.0, 10.0, 10.0), world.color[n]);
                world.nations[n].name.clone()
            }
            _ => "Unclaimed".to_string(),
        };
        gui::fit(canvas, &a.serif, &name, vec2(frame.rect.x + 26.0, frame.rect.y + 6.0), 15.0, gui::GOLD, Align::Left, frame.rect.w - 34.0);
        canvas.fill_rect(Rect::new(frame.rect.x + 6.0, frame.rect.y + 27.0, frame.rect.w - 12.0, 1.0), Color::rgba(138, 109, 47, 150));
        self.draw_widgets(canvas, &ws, true);
    }

    /// The minimap, the camera's view on it, and the capital's star.
    fn draw_minimap(&self, canvas: &mut Canvas) {
        let (Some(view), Some(world)) = (self.map_view.as_ref(), self.world.as_ref()) else { return };
        let a = &self.assets;
        let map = self.atlas.map(world.map);
        let camera = self.map_ui.camera.unwrap_or_else(|| view.orbit.camera());
        Space::BottomRight.push(canvas, self.s());
        canvas.blit_sprite(&a.minimap_frame, MINIMAP_FRAME, Blit::default());
        canvas.blit_sprite(view.minimap(), MINIMAP_AT, Blit::default());
        let clip = self.minimap_rect();
        let corners = [vec2(0.0, BAR_H), vec2(ui::W, BAR_H), vec2(ui::W, ui::H), vec2(0.0, ui::H)];
        let ground: Vec<Option<Vec2>> = corners
            .iter()
            .map(|&c| {
                let ray = camera.ray_through(c, &self.projector);
                view.ground(ray.origin, ray.direction)
            })
            .collect();
        let clamp = |p: Vec2| vec2(p.x.clamp(clip.left(), clip.right()), p.y.clamp(clip.top(), clip.bottom()));
        for k in 0..4 {
            if let (Some(p), Some(q)) = (ground[k], ground[(k + 1) % 4]) {
                let (p, q) = (clamp(view.to_minimap(p, MINIMAP_AT)), clamp(view.to_minimap(q, MINIMAP_AT)));
                canvas.draw_line(p, q, gui::IVORY);
                canvas.draw_line(p + vec2(0.0, 1.0), q + vec2(0.0, 1.0), Color::rgba(0, 0, 0, 160));
            }
        }
        if let (Some(cap), false) = (world.capital.first(), self.spectating) {
            let (x, y) = map.provinces[*cap].middle;
            canvas.blit_sprite(&a.icon_capital, MINIMAP_AT + vec2(x * 4.0 - 7.0, y * 4.0 - 7.0), Blit { scale: 0.5, ..Blit::default() });
        }
        canvas.label("minimap", clip);
        canvas.pop_camera();
    }
}
