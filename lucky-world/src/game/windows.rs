//! What can be clicked in the economy, war, diplomacy and spy windows and
//! the news log: page tabs, a close button, rows, sliders, and a bar of
//! buttons along the bottom, one for every action the window has. Each
//! button presses the action's shortcut key (`gui::Act::Key`), so the
//! window's own code handles a click and a key press the same way.

use duckforge::prelude::*;

use super::cmd::Cmd;
use super::diplo_screen::{Page as DiploPage, SpyPage};
use super::econ_screen::Tab;
use super::gui::{self, Act, Kind, Widget};
use super::war_screen::Page as WarPage;
use super::Lucky;
use crate::diplo::{spies::Kind as Mission, Kind as Treaty};
use crate::econ::player::Lever;

/// Where every window sits.
pub const WIN: Rect = Rect { x: 110.0, y: 64.0, w: 1060.0, h: 600.0 };
/// The height of a lever's row on the budget page.
pub const LEVER_ROW: f32 = 27.0;

/// A page tab's place along the top of a window.
pub fn tab_rect(k: usize) -> Rect {
    Rect::new(WIN.x + 22.0 + k as f32 * 142.0, WIN.y + 14.0, 138.0, 40.0)
}

/// The close button, top right (Esc).
pub(super) fn close_x() -> Widget {
    Widget::new(Kind::Icon, Rect::new(WIN.right() - 38.0, WIN.y + 12.0, 26.0, 26.0), "", Act::Key(Key::Escape)).icon("close").tip("Close")
}

/// One button of a window's bottom bar.
struct Item {
    label: String,
    act: Act,
    tip: &'static str,
    on: bool,
    off: bool,
    danger: bool,
}

fn item(label: impl Into<String>, key: Key, tip: &'static str) -> Item {
    Item { label: label.into(), act: Act::Key(key), tip, on: false, off: false, danger: false }
}

impl Item {
    fn off(mut self, off: bool) -> Item {
        self.off = off;
        self
    }

    fn on(mut self, on: bool) -> Item {
        self.on = on;
        self
    }

    fn danger(mut self) -> Item {
        self.danger = true;
        self
    }

    fn cmd(mut self, cmd: Cmd) -> Item {
        self.act = Act::Cmd(cmd);
        self
    }
}

impl Lucky {
    /// Page tabs along the top of a window: (name, icon, what it opens).
    fn tabs(&self, pages: Vec<(&str, &'static str, Cmd)>, on: usize) -> Vec<Widget> {
        let mut ws = vec![Widget::ghost(WIN)];
        for (k, (name, icon, cmd)) in pages.into_iter().enumerate() {
            ws.push(Widget::new(Kind::Tab, tab_rect(k), name, Act::Cmd(cmd)).icon(icon).on(k == on).tip("Tab: the next page"));
        }
        ws.push(close_x());
        ws
    }

    /// A button in the tab row, left of the X: closes the window and shows
    /// this overlay on the map.
    fn overlay_button(&self, label: &str, overlay: crate::game::econ_screen::Overlay, tip: &str) -> Widget {
        let on = self.map_ui.econ.overlay == overlay;
        Widget::new(Kind::Button, Rect::new(WIN.right() - 62.0 - 200.0, WIN.y + 18.0, 196.0, 32.0), label, Act::Cmd(Cmd::Overlay(overlay))).icon("show").tip(tip).on(on)
    }

    /// A window's bottom bar: its buttons, left to right.
    fn bar(&self, items: Vec<Item>) -> Vec<Widget> {
        let mut x = WIN.x + 24.0;
        let y = WIN.bottom() - 46.0;
        let style = TextStyle { size: 15.0, ..TextStyle::default() };
        items
            .into_iter()
            .map(|it| {
                let w = (self.assets.font.measure(&it.label, &style).x + 26.0).max(64.0);
                let mut wd = Widget::new(Kind::Button, Rect::new(x, y, w, 32.0), it.label, it.act).tip(it.tip).on(it.on).off(it.off);
                wd.danger = it.danger;
                x += w + 6.0;
                wd
            })
            .collect()
    }

    /// The clicks in a window this frame: the widget's action, and whether
    /// a click fell outside the window (which closes it).
    pub(super) fn window_clicks(&mut self, ctx: &Context, ws: &[Widget]) -> (Option<Act>, bool) {
        let s = self.settings.ui_scale;
        let outside = ctx.input.was_mouse_pressed(MouseButton::Left) && !self.pointer.dragging() && !gui::over(ws, s, self.mouse);
        (self.clicked(ctx, ws), outside)
    }

    // ---------------------------------------------------------------
    // The economy window
    // ---------------------------------------------------------------

    pub(super) fn econ_widgets(&self) -> Vec<Widget> {
        let (Some(tab), Some(e)) = (self.map_ui.econ.open, self.economy.as_ref()) else { return Vec::new() };
        let ed = &self.econ_data;
        let pages = vec![
            ("Budget", "treasury", Cmd::Econ(Tab::Budget, 0)),
            ("Chains", "economy", Cmd::Econ(Tab::Chains, 0)),
            ("Market", "trade", Cmd::Econ(Tab::Market, 0)),
            ("Trade", "diplomacy", Cmd::Econ(Tab::Trade, 0)),
            ("Stockpile", "army", Cmd::Econ(Tab::Stockpile, 0)),
            ("Charts", "gdp", Cmd::Econ(Tab::Charts, 0)),
            ("People", "people", Cmd::Econ(Tab::People, 0)),
        ];
        let mut ws = self.tabs(pages, Tab::ALL.iter().position(|t| *t == tab).unwrap_or(0));
        let row = self.map_ui.econ.row;
        match tab {
            Tab::Budget => {
                for (k, lever) in Lever::ALL.iter().enumerate() {
                    let ry = WIN.y + 102.0 + k as f32 * LEVER_ROW;
                    let (_, lo, hi) = lever.step();
                    let v = e.lever_with(0, *lever, Some(ed));
                    ws.push(Widget::new(Kind::Ghost, Rect::new(WIN.x + 20.0, ry, 200.0, LEVER_ROW - 4.0), "", Act::Row(k)));
                    ws.push(
                        Widget::new(Kind::Slider(((v - lo) / (hi - lo).max(1e-9)) as f32), Rect::new(WIN.x + 222.0, ry, 118.0, LEVER_ROW - 4.0), "", Act::Slide(k as u8, 0.0))
                            .tip(format!("Drag to set it: from {:.0}% to {:.0}%", lo * 100.0, hi * 100.0)),
                    );
                }
                ws.extend(self.bar(vec![
                    item("Hand back to the autopilot", Key::A, "Taxes, welfare and the interest rate go back to your ministers"),
                    item("Lower", Key::Left, "The chosen lever, one step down"),
                    item("Raise", Key::Right, "The chosen lever, one step up"),
                ]));
            }
            Tab::Chains => {
                let order = super::econ_screen::chain_order(ed);
                let pos = super::econ_screen::chain_positions(ed);
                for (k, &g) in order.iter().enumerate() {
                    ws.push(Widget::new(Kind::Ghost, Rect::new(pos[g].x, pos[g].y, 150.0, 32.0), "", Act::Row(k)).tip(format!("{}: click to choose it", ed.goods[g].name)));
                }
                let chosen = order[row.min(order.len() - 1)];
                let maker = ed.buildings.iter().find(|b| b.output == chosen).map_or("its maker".to_string(), |b| b.name.to_lowercase());
                let here = self.map_ui.econ.build_at.and_then(|p| self.world.as_ref().map(|w| self.atlas.map(w.map).provinces[p].name.clone()));
                let build = match &here {
                    Some(name) => format!("Build a {maker} in {name}"),
                    None => format!("Build a {maker}"),
                };
                ws.extend(self.bar(vec![
                    item(build, Key::B, "Builds more of the chosen good's maker (it costs steel, timber and money)").on(true),
                    item("Close some", Key::X, "Closes part of the chosen good's makers"),
                    item("Road where goods are stuck", Key::R, "A road in the province whose goods move worst"),
                    item("Bigger port", Key::P, "A bigger port in your smallest harbour"),
                ]));
            }
            Tab::Market => {
                for g in 0..ed.goods.len() {
                    ws.push(Widget::new(Kind::Ghost, Rect::new(WIN.x + 20.0, WIN.y + 97.0 + g as f32 * 26.5, 1010.0, 26.0), "", Act::Row(g)));
                }
                let name = ed.goods[row.min(ed.goods.len() - 1)].name.to_lowercase();
                ws.extend(self.bar(vec![item(format!("Trade policy for {name}"), Key::T, "Free trade, then no exports, then no imports")]));
            }
            Tab::Trade => {
                ws.extend(self.bar(vec![item("Embargo this nation (or lift it)", Key::X, "No trade at all with the chosen nation: it hurts you both, and they remember").danger()]));
            }
            Tab::Charts => {
                ws.extend(self.bar(vec![item("Previous good", Key::Up, "The price chart's good"), item("Next good", Key::Down, "The price chart's good")]));
            }
            Tab::Stockpile | Tab::People => {}
        }
        ws
    }

    // ---------------------------------------------------------------
    // The war room
    // ---------------------------------------------------------------

    pub(super) fn war_widgets(&self) -> Vec<Widget> {
        let (Some(page), Some(w)) = (self.map_ui.war.open, self.wars.as_ref()) else { return Vec::new() };
        let pages = vec![("Wars", "war", Cmd::War(WarPage::Wars)), ("Army", "army", Cmd::War(WarPage::Army)), ("Supply", "trade", Cmd::War(WarPage::Supply)), ("Reports", "news", Cmd::War(WarPage::Reports))];
        let mut ws = self.tabs(pages, WarPage::ALL.iter().position(|p| *p == page).unwrap_or(0));
        ws.push(self.overlay_button("Armies on the map", crate::game::econ_screen::Overlay::War, "Close this and show every army and who holds each province"));
        let row = self.map_ui.war.row;
        let (x, y) = (WIN.x + 26.0, WIN.y + 76.0);
        let watching = self.spectating;
        match page {
            WarPage::Wars => {
                let list = crate::war::player::wars_in_view(w);
                let first = row.saturating_sub(13);
                for n in first..list.len().min(first + 14) {
                    ws.push(Widget::new(Kind::Ghost, Rect::new(x - 6.0, y + 16.0 + (n - first) as f32 * 30.0, 500.0, 27.0), "", Act::Row(n)));
                }
                let k = list.get(row).copied();
                let mine = k.is_some_and(|k| crate::war::player::side(w, k).is_some() && w.wars[k].active());
                let pending = k.is_some_and(|k| w.wars[k].pending.is_some_and(|p| crate::war::player::side(w, k).is_some_and(|s| s != p.0)));
                let offer = k.is_some_and(|k| w.offers.iter().any(|o| o.0 == k));
                let target = self.map_ui.selected.and_then(|p| w.owner[p]).filter(|&o| o != 0);
                let level = crate::war::LEVELS[self.map_ui.war.declare_level.clamp(1, 4)];
                let declare = match target {
                    Some(t) => format!("Declare on {}", w.names[t]),
                    None => "Declare war".to_string(),
                };
                ws.extend(self.bar(vec![
                    item("Escalate", Key::E, "One rung up the ladder; they must match it or back down").off(!mine || watching),
                    item("Match", Key::M, "Match their escalation").off(!pending || watching),
                    item("Back down", Key::B, "Give way: they get what they went to war for").off(!pending || watching),
                    item("Offer peace", Key::P, "Offer peace on what the war score allows").off(!mine || watching),
                    item("Accept their offer", Key::Y, "Sign the peace they offer").off(!offer || watching).on(offer),
                    item("Refuse", Key::N, "Turn their offer down").off(!offer || watching),
                    item(format!("Start as: {level}"), Key::L, "How a war you declare begins: skirmish, operation, declared war, total war").off(watching),
                    item(declare, Key::D, "Declares war on the nation whose province is selected on the map (or right-click a nation on the map)").off(target.is_none() || watching).danger().cmd(match target {
                        Some(t) => Cmd::AskWar(t),
                        None => Cmd::Nothing,
                    }),
                ]));
            }
            WarPage::Army => {
                for fr in 0..crate::war::FRONTS.len() {
                    ws.push(Widget::new(Kind::Ghost, Rect::new(x - 6.0, y + 41.0 + fr as f32 * 26.0, 1010.0, 24.0), "", Act::Row(fr)));
                }
                ws.extend(self.bar(vec![
                    item("Attack", Key::A, "The chosen front attacks its nearest enemy province").off(watching),
                    item("Hold", Key::H, "Hold the line").off(watching),
                    item("Dig in", Key::G, "Dig in: stronger in defence, slow to move").off(watching),
                    item("Retreat", Key::R, "Fall back towards the capital").off(watching),
                    item("Reinforce the selected province", Key::F, "Sends the front's divisions to your province selected on the map").off(watching || self.map_ui.selected.is_none()),
                    item("The general decides", Key::U, "Hand the front back to its general").off(watching),
                ]));
            }
            WarPage::Supply => {
                for fr in 0..crate::war::FRONTS.len() {
                    ws.push(Widget::new(Kind::Ghost, Rect::new(x - 6.0, y + 17.0 + fr as f32 * 26.0, 480.0, 24.0), "", Act::Row(fr)));
                }
                let list = super::war_screen::request_list(w, if watching { self.map_ui.brain.unwrap_or(0) } else { 0 });
                let mut ry = WIN.y + 120.0;
                for n in 0..list.len() {
                    if ry > WIN.bottom() - 60.0 {
                        break;
                    }
                    ws.push(Widget::new(Kind::Ghost, Rect::new(x + 504.0, ry - 3.0, 500.0, 40.0), "", Act::Row(crate::war::FRONTS.len() + n)));
                    ry += 46.0;
                }
                let on_front = row < crate::war::FRONTS.len();
                ws.extend(self.bar(vec![
                    item("Lower priority", Key::Left, "The chosen front gets its supplies later").off(!on_front || watching),
                    item("Raise priority", Key::Right, "The chosen front gets its supplies first").off(!on_front || watching),
                    item("Approve the request", Key::Y, "Send what the general asks for").off(on_front || watching),
                    item("Deny it", Key::N, "Refuse the request").off(on_front || watching),
                    item(if w.forces[0].auto { "Auto is on: answer requests myself" } else { "Auto is off: let the staff answer" }, Key::T, "On Auto, requests are answered by each front's priority").off(watching).on(w.forces[0].auto),
                ]));
            }
            WarPage::Reports => {
                let n = self.report_list().len();
                let first = row.saturating_sub(16);
                for k in first..n.min(first + 17) {
                    ws.push(Widget::new(Kind::Ghost, Rect::new(x - 6.0, y + 17.0 + (k - first) as f32 * 26.0, 420.0, 24.0), "", Act::Row(k)));
                }
                ws.extend(self.bar(vec![item("Read the whole report", Key::Enter, "The battle, on paper").off(n == 0)]));
            }
        }
        ws
    }

    // ---------------------------------------------------------------
    // Diplomacy and spies
    // ---------------------------------------------------------------

    pub(super) fn diplo_widgets(&self) -> Vec<Widget> {
        let (Some(page), Some(d)) = (self.map_ui.diplo.open, self.diplo.as_ref()) else { return Vec::new() };
        let pages = vec![
            ("Nations", "people", Cmd::Diplo(DiploPage::Nations, None)),
            ("Treaties", "diplomacy", Cmd::Diplo(DiploPage::Treaties, None)),
            ("Propose", "card", Cmd::Diplo(DiploPage::Propose, None)),
            ("Inbox", "news", Cmd::Diplo(DiploPage::Inbox, None)),
            ("Talks", "war", Cmd::Diplo(DiploPage::Talks, None)),
        ];
        let mut ws = self.tabs(pages, DiploPage::ALL.iter().position(|p| *p == page).unwrap_or(0));
        ws.push(self.overlay_button("Relations on the map", crate::game::econ_screen::Overlay::Relations, "Close this and colour every nation by how it feels about you"));
        let row = self.map_ui.diplo.row;
        let (x, y) = (WIN.x + 26.0, WIN.y + 76.0);
        let watching = self.spectating;
        let rows = self.diplo_rows(page);
        // A treaty button for each kind: one click proposes it to the chosen nation.
        let treaty_buttons = |ws: &mut Vec<Widget>, at: Vec2, chosen: Option<usize>| {
            for (k, kd) in Treaty::PROPOSE.iter().enumerate() {
                let r = Rect::new(at.x + (k % 4) as f32 * 124.0, at.y + (k / 4) as f32 * 36.0, 120.0, 30.0);
                ws.push(Widget::cmd(r, kd.name(), Cmd::ProposeKind(k)).on(chosen == Some(k)).off(watching).tip(super::diplo_screen::treaty_words(*kd)));
            }
        };
        match page {
            DiploPage::Nations => {
                let first = row.saturating_sub(17);
                for k in first..rows.min(first + 18) {
                    ws.push(Widget::new(Kind::Ghost, Rect::new(x - 6.0, y + 17.0 + (k - first) as f32 * 26.0, 470.0, 24.0), "", Act::Row(k)));
                }
                treaty_buttons(&mut ws, vec2(WIN.x + 520.0, WIN.bottom() - 130.0), None);
            }
            DiploPage::Treaties => {
                for k in 0..rows.min(18) {
                    ws.push(Widget::new(Kind::Ghost, Rect::new(x - 6.0, y + 17.0 + k as f32 * 24.0, 1000.0, 22.0), "", Act::Row(k)));
                }
                ws.extend(self.bar(vec![item("Break this treaty", Key::B, "Ends the chosen treaty: your reputation falls with everyone").off(rows == 0 || watching).danger()]));
            }
            DiploPage::Propose => {
                treaty_buttons(&mut ws, vec2(x, y + 92.0), Some(self.map_ui.diplo.kind));
                ws.extend(self.bar(vec![
                    item("Previous nation", Key::Up, "Who the proposal goes to").off(watching),
                    item("Next nation", Key::Down, "Who the proposal goes to").off(watching),
                    item("Propose it", Key::Enter, "Send the chosen treaty; they answer with their reason").off(watching).on(true),
                ]));
            }
            DiploPage::Inbox => {
                for k in 0..rows.min(14) {
                    ws.push(Widget::new(Kind::Ghost, Rect::new(x - 6.0, y + 19.0 + k as f32 * 28.0, 1000.0, 26.0), "", Act::Row(k)));
                }
                ws.extend(self.bar(vec![item("Accept", Key::Y, "Sign it (or join your ally's war)").off(rows == 0 || watching).on(rows > 0), item("Decline", Key::N, "Turn it down (refusing an ally's call breaks the treaty)").off(rows == 0 || watching)]));
            }
            DiploPage::Talks => {
                for k in 0..rows.min(14) {
                    ws.push(Widget::new(Kind::Ghost, Rect::new(x - 6.0, y + 19.0 + k as f32 * 26.0, 1000.0, 24.0), "", Act::Row(k)));
                }
                ws.extend(self.bar(vec![
                    item("Ask for less", Key::Left, "A softer demand").off(rows == 0 || watching),
                    item("Ask for more", Key::Right, "A harder demand").off(rows == 0 || watching),
                    item("Send the offer", Key::Enter, "They answer within the week").off(rows == 0 || watching).on(rows > 0),
                ]));
            }
        }
        let _ = d;
        ws
    }

    pub(super) fn spy_widgets(&self) -> Vec<Widget> {
        let (Some(page), Some(d)) = (self.map_ui.diplo.spies, self.diplo.as_ref()) else { return Vec::new() };
        let pages = vec![("Agents", "spies", Cmd::Spies(SpyPage::Agents, None)), ("Missions", "show", Cmd::Spies(SpyPage::Missions, None)), ("Reports", "news", Cmd::Spies(SpyPage::Reports, None))];
        let mut ws = self.tabs(pages, SpyPage::ALL.iter().position(|p| *p == page).unwrap_or(0));
        let (x, y) = (WIN.x + 26.0, WIN.y + 76.0);
        let watching = self.spectating;
        let v = if watching { self.map_ui.brain.unwrap_or(0) } else { 0 };
        match page {
            SpyPage::Agents => {
                let share = d.spies.services[v].ci_share as f32;
                ws.push(Widget::new(Kind::Slider(share), Rect::new(x + 520.0, y + 50.0, 220.0, 24.0), "", Act::Slide(0, 0.0)).off(watching).tip("The share of your agents hunting foreign spies at home: drag to set it"));
                ws.extend(self.bar(vec![
                    item("Train an agent", Key::T, "A new agent: it takes time and money").off(watching).on(true),
                    item("Fewer at home", Key::Left, "Less counter-intelligence, more agents free for missions").off(watching),
                    item("More at home", Key::Right, "More counter-intelligence").off(watching),
                ]));
            }
            SpyPage::Missions => {
                for k in 0..Mission::ALL.len() {
                    ws.push(Widget::new(Kind::Ghost, Rect::new(x - 6.0, y + 79.0 + k as f32 * 28.0, 1000.0, 26.0), "", Act::Row(k)));
                }
                ws.extend(self.bar(vec![
                    item("Previous target", Key::Left, "The nation the mission is against").off(watching),
                    item("Next target", Key::Right, "The nation the mission is against").off(watching),
                    item("Send the agents", Key::Enter, "Start the chosen mission; the odds shown are your service's own estimate").off(watching).on(true),
                ]));
            }
            SpyPage::Reports => {}
        }
        ws
    }

    // ---------------------------------------------------------------
    // The news log
    // ---------------------------------------------------------------

    pub(super) fn log_widgets(&self) -> Vec<Widget> {
        vec![Widget::ghost(WIN), close_x()]
    }
}

impl crate::econ::Economy {
    /// The player's "Build here": like `player_build`, in the province they chose.
    pub fn player_build_at(&mut self, ed: &crate::econ::EconData, nation: usize, kind: usize, province: usize) -> Result<String, String> {
        let bd = &ed.buildings[kind];
        if !self.nations[nation].provinces.contains(&province) {
            return Err("That province isn't yours".to_string());
        }
        if bd.output == ed.jets && self.nations[nation].hdi < 0.6 {
            return Err("Jets need an HDI of at least 0.6".to_string());
        }
        let p = &self.provinces[province];
        if bd.coastal && !p.coastal {
            return Err(format!("A {} needs a coast", bd.name));
        }
        let (mut levels, _) = self.build_block(ed, nation, kind);
        if bd.deposit {
            let d = p.deposits[bd.output];
            if d < 0.2 {
                return Err(format!("There is no {} to dig here", ed.goods[bd.output].name.to_lowercase()));
            }
            let n = &self.nations[nation];
            let have: f64 = n.buildings.iter().filter(|b| b.kind == kind && b.province == province).map(|b| b.levels).sum::<f64>()
                + n.construction.iter().filter(|c| c.kind == kind && c.province == province).map(|c| c.levels).sum::<f64>();
            let room = d.min(3.0) * p.hexes * bd.cap_per_hex - have;
            if room < 1.0 {
                return Err(format!("This province has no room for another {}", bd.name));
            }
            levels = levels.min(room);
        }
        let n = &mut self.nations[nation];
        for &(g, per) in &bd.build_goods {
            if per > 0.0 {
                levels = levels.min(n.stock[g] / per);
            }
        }
        if levels < 0.5 {
            let short: Vec<&str> = bd.build_goods.iter().filter(|&&(g, per)| n.stock[g] < per * 0.5).map(|&(g, _)| ed.goods[g].name.as_str()).collect();
            return Err(format!("Not enough {} in store to build", short.join(" or ")));
        }
        for &(g, per) in &bd.build_goods {
            n.stock[g] -= per * levels;
            n.ledger.used[g] += per * levels;
        }
        let cost = bd.cost * n.mods.get("build_cost") / n.exchange * levels;
        n.treasury -= cost;
        if n.treasury < 0.0 {
            let gap = -n.treasury;
            n.debt += gap;
            n.savings -= gap;
            n.treasury = 0.0;
        }
        n.budget_month.construction += cost;
        n.projects.push((cost / bd.build_days.max(1) as f64, bd.build_days.max(1)));
        n.construction.push(crate::econ::Construction { kind, province, levels, days_left: bd.build_days.max(1), by_state: true, by_player: true });
        Ok(format!("Building {:.0} levels of {} (ready in {} days)", levels, bd.name, bd.build_days))
    }
}
