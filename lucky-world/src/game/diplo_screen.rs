//! Diplomacy and spies on the map screen (docs/DIPLOMACY.md, section 12):
//! the diplomacy window (P) and the spy window (I).
//!
//! | | |
//! |---|---|
//! | Diplomacy (P) | Tab: next page · Up / Down: choose · Nations: Enter: propose to the chosen nation · Treaties: B: break ours · Propose: Left / Right: the treaty, Enter: propose · Inbox: Y / N · Talks: Left / Right: our demand, Enter: send it |
//! | Spies (I) | Tab: next page · Agents: T: train one, Left / Right: the counter-intel share · Missions: Left / Right: the target, Up / Down: the mission, Enter: send |

use duckforge::prelude::*;

use super::Lucky;
use crate::diplo::spies::{self, Kind as Mission};
use crate::diplo::{talks, treaties, Diplomacy, Inbox, Kind, Proposal, State};
use crate::ui::{self, GOLD, MUTED, SOFT};

use super::windows::WIN;
const GOOD: Color = Color::rgb(107, 212, 138);
const WARN: Color = Color::rgb(245, 197, 66);
const BAD: Color = Color::rgb(240, 106, 106);
const DEMANDS: [&str; 4] = ["white peace", "reparations", "provinces", "the war's goal"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Nations,
    Treaties,
    Propose,
    Inbox,
    Talks,
}

impl Page {
    pub const ALL: [Page; 5] = [Page::Nations, Page::Treaties, Page::Propose, Page::Inbox, Page::Talks];
    fn name(self) -> &'static str {
        match self {
            Page::Nations => "NATIONS",
            Page::Treaties => "TREATIES",
            Page::Propose => "PROPOSE",
            Page::Inbox => "INBOX",
            Page::Talks => "TALKS",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpyPage {
    Agents,
    Missions,
    Reports,
}

impl SpyPage {
    pub const ALL: [SpyPage; 3] = [SpyPage::Agents, SpyPage::Missions, SpyPage::Reports];
    fn name(self) -> &'static str {
        match self {
            SpyPage::Agents => "AGENTS",
            SpyPage::Missions => "MISSIONS",
            SpyPage::Reports => "REPORTS",
        }
    }
}

/// What a treaty means, in a sentence.
pub(super) fn treaty_words(kind: Kind) -> &'static str {
    match kind {
        Kind::Trade => "No tariffs between you, and each prefers the other's goods (10 years).",
        Kind::NonAggression => "Neither may attack the other (10 years). Attacking breaks it.",
        Kind::Defensive => "If either is attacked, the other is called to arms (10 years).",
        Kind::Alliance => "Defensive, called to each other's wars, military access (15 years).",
        Kind::Access => "Armies may pass and retreat through each other's land (5 years).",
        Kind::Guarantee => "You guarantee their independence: attacked, they call on you (10 years).",
        Kind::Arms => "You buy 50 tanks from them, at the world price + 20%.",
        Kind::Loan => "You borrow 3% of their GDP, repaid monthly over 8 years with interest.",
        Kind::Sanctions => "",
    }
}

#[allow(dead_code)]
fn tab_rect(k: usize) -> Rect {
    Rect::new(WIN.x + 24.0 + k as f32 * 168.0, WIN.y + 16.0, 160.0, 40.0)
}

/// What the diplomacy and spy windows remember.
#[derive(Debug, Default)]
pub struct DiploUi {
    pub open: Option<Page>,
    pub spies: Option<SpyPage>,
    pub row: usize,
    /// The nation the Propose page and the Missions page aim at.
    pub nation: Option<usize>,
    pub kind: usize,
    pub demand: usize,
    pub mission: usize,
    /// The last answer, shown for a while.
    pub message: Option<String>,
    pub inbox_seen: usize,
}

impl DiploUi {
    pub fn wants_escape(&self) -> bool {
        self.open.is_some() || self.spies.is_some()
    }
}

fn plain(canvas: &mut Canvas, font: &Font, words: &str, at: Vec2, size: f32, color: Color) {
    ui::plain(canvas, font, words, at, size, color, Align::Left);
}

fn fit(canvas: &mut Canvas, font: &Font, words: &str, at: Vec2, size: f32, color: Color, max: f32) {
    let mut s = TextStyle { size, color, ..TextStyle::default() };
    let width = font.measure(words, &s).x;
    if width > max && width > 0.0 {
        s.size = size * max / width;
    }
    canvas.draw_font_text(font, words, at, &s);
}

fn rel_color(r: f64) -> Color {
    if r >= 20.0 {
        GOOD
    } else if r <= -20.0 {
        BAD
    } else {
        WARN
    }
}

/// The nations, best relations first (from `v`'s view).
fn order(d: &Diplomacy, v: usize) -> Vec<usize> {
    let mut list: Vec<usize> = (0..d.n).filter(|&j| j != v).collect();
    list.sort_by(|&a, &b| d.relations[v][b].total_cmp(&d.relations[v][a]).then(a.cmp(&b)));
    list
}

impl Lucky {
    /// Whose diplomacy the windows show: the player's, or (watching) the chosen brain's.
    fn diplo_viewer(&self) -> usize {
        if self.spectating { self.map_ui.brain.unwrap_or(0) } else { 0 }
    }

    pub(super) fn diplo_rows(&self, page: Page) -> usize {
        let (Some(d), Some(w)) = (self.diplo.as_ref(), self.wars.as_ref()) else { return 0 };
        let v = self.diplo_viewer();
        match page {
            Page::Nations | Page::Propose => d.n - 1,
            Page::Treaties => d.live().filter(|t| t.involves(v)).count(),
            Page::Inbox => d.inbox.len(),
            Page::Talks => w.wars.iter().filter(|x| x.active() && x.side_of(v).is_some() && !x.civil).count(),
        }
    }

    /// Opens a window (P: diplomacy, I: spies) from the map.
    pub(super) fn open_diplo(&mut self, ctx: &mut Context, spies: bool) {
        if spies {
            self.map_ui.diplo.spies = Some(SpyPage::Agents);
        } else {
            self.map_ui.diplo.open = Some(Page::Nations);
        }
        self.map_ui.diplo.row = 0;
        // The chosen province's nation is the one we look at.
        if let Some(o) = self.map_ui.selected.and_then(|p| self.world.as_ref().and_then(|w| w.owner[p])) {
            if o != self.diplo_viewer() {
                self.map_ui.diplo.nation = Some(o);
            }
        }
        crate::ui::sfx(ctx, &self.assets.card_sound, 0.8, 0.0);
    }

    /// Keys in the diplomacy window.
    pub(super) fn update_diplo(&mut self, ctx: &mut Context) {
        let Some(page) = self.map_ui.diplo.open else { return };
        // Tabs, the close button, rows, treaty buttons and the bottom bar (windows.rs).
        let ws = self.diplo_widgets();
        let (act, outside) = self.window_clicks(ctx, &ws);
        match act {
            Some(super::gui::Act::Cmd(cmd)) => {
                self.run(ctx, cmd);
                return;
            }
            Some(super::gui::Act::Row(i)) => self.map_ui.diplo.row = i,
            _ => {}
        }
        let wheel = ctx.input.scroll();
        let key_p = self.settings.key(crate::settings::Act::Diplomacy);
        if crate::game::gui::pressed(ctx, Key::Escape) || crate::game::gui::pressed(ctx, key_p) || outside {
            self.map_ui.diplo.open = None;
            crate::ui::sfx(ctx, &self.assets.click, 0.7, 0.0);
            return;
        }
        let mut next = None;
        if crate::game::gui::pressed(ctx, Key::Tab) {
            let k = Page::ALL.iter().position(|&p| p == page).unwrap_or(0);
            next = Some(Page::ALL[(k + 1) % Page::ALL.len()]);
        }
        if let Some(p) = next {
            self.map_ui.diplo.open = Some(p);
            self.map_ui.diplo.row = 0;
            crate::ui::sfx(ctx, &self.assets.click, 0.7, 0.0);
            return;
        }
        let rows = self.diplo_rows(page);
        let ui = &mut self.map_ui.diplo;
        if page != Page::Propose {
            if crate::game::gui::pressed(ctx, Key::Down) && rows > 0 {
                ui.row = (ui.row + 1) % rows;
            }
            if crate::game::gui::pressed(ctx, Key::Up) && rows > 0 {
                ui.row = (ui.row + rows - 1) % rows;
            }
            if wheel != 0.0 && rows > 0 {
                ui.row = (ui.row as i32 - wheel.signum() as i32).clamp(0, rows as i32 - 1) as usize;
            }
            ui.row = ui.row.min(rows.saturating_sub(1));
        }
        let v = self.diplo_viewer();
        let (Some(d), Some(e), Some(w)) = (self.diplo.as_mut(), self.economy.as_mut(), self.wars.as_mut()) else { return };
        let ed = &self.econ_data;
        let b = self.brains.as_ref();
        let ui = &mut self.map_ui.diplo;
        let list = order(d, v);
        if page == Page::Nations {
            ui.nation = list.get(ui.row).copied();
            if crate::game::gui::pressed(ctx, Key::Enter) && !self.spectating {
                ui.open = Some(Page::Propose);
                crate::ui::sfx(ctx, &self.assets.click, 0.7, 0.0);
            }
            return;
        }
        if self.spectating {
            return;
        }
        let mut said = None;
        let mut sound: Option<&'static str> = None;
        match page {
            Page::Propose => {
                if crate::game::gui::pressed(ctx, Key::Right) {
                    ui.kind = (ui.kind + 1) % Kind::PROPOSE.len();
                }
                if crate::game::gui::pressed(ctx, Key::Left) {
                    ui.kind = (ui.kind + Kind::PROPOSE.len() - 1) % Kind::PROPOSE.len();
                }
                if crate::game::gui::pressed(ctx, Key::Down) || crate::game::gui::pressed(ctx, Key::Up) {
                    let k = ui.nation.and_then(|n| list.iter().position(|&x| x == n)).unwrap_or(0);
                    let k = if crate::game::gui::pressed(ctx, Key::Down) { (k + 1) % list.len() } else { (k + list.len() - 1) % list.len() };
                    ui.nation = list.get(k).copied();
                }
                if crate::game::gui::pressed(ctx, Key::Enter) {
                    if let Some(to) = ui.nation {
                        let kind = Kind::PROPOSE[ui.kind];
                        let (good, amount) = match kind {
                            Kind::Arms => (Some(ed.tanks), 50.0),
                            Kind::Loan => (None, d.data.treaties.loan_size * e.nations[to].gdp),
                            _ => (None, 0.0),
                        };
                        let p = Proposal { from: v, to, kind, day: d.day, good, amount, target: None };
                        let (yes, words) = treaties::propose(d, e, ed, w, b, p);
                        sound = Some(if yes { "treaty" } else { "denied" });
                        said = Some(format!("{}: {words}", d.names[to]));
                    }
                }
            }
            Page::Treaties => {
                if crate::game::gui::pressed(ctx, Key::B) {
                    let ours: Vec<usize> = d.live().filter(|t| t.involves(v)).map(|t| t.id).collect();
                    if let Some(&id) = ours.get(ui.row) {
                        treaties::break_treaty(d, e, id, v, "the player ended it");
                        sound = Some("denied");
                        said = Some(format!("You break the {}: your word is worth less now", d.treaties[id].kind.name().to_lowercase()));
                    }
                }
            }
            Page::Inbox => {
                let yes = crate::game::gui::pressed(ctx, Key::Y);
                if yes || crate::game::gui::pressed(ctx, Key::N) {
                    sound = Some(if yes { "treaty" } else { "click" });
                    said = Some(treaties::answer_inbox(d, e, ed, w, b, ui.row, yes));
                }
            }
            Page::Talks => {
                if crate::game::gui::pressed(ctx, Key::Right) {
                    ui.demand = (ui.demand + 1).min(3);
                }
                if crate::game::gui::pressed(ctx, Key::Left) {
                    ui.demand = ui.demand.saturating_sub(1);
                }
                if crate::game::gui::pressed(ctx, Key::Enter) {
                    let wars: Vec<usize> = (0..w.wars.len()).filter(|&k| w.wars[k].active() && w.wars[k].side_of(v).is_some() && !w.wars[k].civil).collect();
                    if let Some(&k) = wars.get(ui.row) {
                        said = Some(talks::player_offer(d, e, ed, w, k, ui.demand));
                    }
                }
            }
            Page::Nations => {}
        }
        if let Some(s) = said {
            self.map_ui.diplo.message = Some(s);
            self.sound_named(ctx, sound.unwrap_or("select"));
        }
    }

    /// Keys in the spy window.
    pub(super) fn update_spies(&mut self, ctx: &mut Context) {
        let Some(page) = self.map_ui.diplo.spies else { return };
        let ws = self.spy_widgets();
        let (act, outside) = self.window_clicks(ctx, &ws);
        let mut slide = None;
        match act {
            Some(super::gui::Act::Cmd(cmd)) => {
                self.run(ctx, cmd);
                return;
            }
            Some(super::gui::Act::Row(i)) => self.map_ui.diplo.mission = i,
            Some(super::gui::Act::Slide(_, t)) => slide = Some(t),
            _ => {}
        }
        if let Some(d) = self.diplo.as_ref() {
            // Reports looked at are no longer "new".
            if page == SpyPage::Reports {
                self.map_ui.spy_seen_day = d.day + 1;
            }
        }
        let key_i = self.settings.key(crate::settings::Act::Spies);
        if crate::game::gui::pressed(ctx, Key::Escape) || crate::game::gui::pressed(ctx, key_i) || outside {
            self.map_ui.diplo.spies = None;
            crate::ui::sfx(ctx, &self.assets.click, 0.7, 0.0);
            return;
        }
        let mut next = None;
        if crate::game::gui::pressed(ctx, Key::Tab) {
            let k = SpyPage::ALL.iter().position(|&p| p == page).unwrap_or(0);
            next = Some(SpyPage::ALL[(k + 1) % SpyPage::ALL.len()]);
        }
        if let Some(p) = next {
            self.map_ui.diplo.spies = Some(p);
            self.map_ui.diplo.row = 0;
            crate::ui::sfx(ctx, &self.assets.click, 0.7, 0.0);
            return;
        }
        if self.spectating {
            return;
        }
        let v = self.diplo_viewer();
        let (Some(d), Some(e), Some(w)) = (self.diplo.as_mut(), self.economy.as_mut(), self.wars.as_mut()) else { return };
        let ui = &mut self.map_ui.diplo;
        let others: Vec<usize> = (0..d.n).filter(|&j| j != v).collect();
        let mut said = None;
        let mut sound: Option<&'static str> = None;
        match page {
            SpyPage::Agents => {
                if crate::game::gui::pressed(ctx, Key::T) {
                    said = Some(spies::train(d, e, v).unwrap_or_else(|x| x));
                }
                let s = &mut d.spies.services[v];
                if let Some(t) = slide {
                    s.ci_share = ((t as f64) * 20.0).round() / 20.0;
                }
                if crate::game::gui::pressed(ctx, Key::Right) {
                    s.ci_share = (s.ci_share + 0.1).min(1.0);
                }
                if crate::game::gui::pressed(ctx, Key::Left) {
                    s.ci_share = (s.ci_share - 0.1).max(0.0);
                }
            }
            SpyPage::Missions => {
                let k = ui.nation.and_then(|n| others.iter().position(|&x| x == n)).unwrap_or(0);
                if crate::game::gui::pressed(ctx, Key::Right) {
                    ui.nation = others.get((k + 1) % others.len()).copied();
                }
                if crate::game::gui::pressed(ctx, Key::Left) {
                    ui.nation = others.get((k + others.len() - 1) % others.len()).copied();
                }
                if ui.nation.is_none() {
                    ui.nation = others.first().copied();
                }
                if crate::game::gui::pressed(ctx, Key::Down) {
                    ui.mission = (ui.mission + 1) % Mission::ALL.len();
                }
                if crate::game::gui::pressed(ctx, Key::Up) {
                    ui.mission = (ui.mission + Mission::ALL.len() - 1) % Mission::ALL.len();
                }
                if crate::game::gui::pressed(ctx, Key::Enter) {
                    if let Some(t) = ui.nation {
                        let agents = d.spies.idle(v).min(2);
                        let r = spies::launch(d, e, w, v, t, Mission::ALL[ui.mission], agents);
                        sound = Some(if r.is_ok() { "spy" } else { "denied" });
                        said = Some(r.unwrap_or_else(|x| x));
                    }
                }
            }
            SpyPage::Reports => {}
        }
        if let Some(s) = said {
            self.map_ui.diplo.message = Some(s);
            self.sound_named(ctx, sound.unwrap_or("select"));
        }
    }

    /// The player's inbox, and new answers, as toasts.
    pub(super) fn after_diplo_days(&mut self, ctx: &mut Context) {
        let Some(d) = self.diplo.as_ref() else { return };
        if self.spectating {
            return;
        }
        if d.inbox.len() > self.map_ui.diplo.inbox_seen {
            let words = match d.inbox.last() {
                Some(Inbox::Proposal(p)) => format!("{} proposes a {}: answer in Diplomacy, Inbox", d.names[p.from], p.kind.name().to_lowercase()),
                Some(Inbox::Call { ally, .. }) => format!("{} calls you to war: answer in Diplomacy, Inbox (14 days)", d.names[*ally]),
                None => String::new(),
            };
            self.map_ui.diplo.inbox_seen = d.inbox.len();
            self.notify(ctx, words, super::shell::IMPORTANT);
            return;
        }
        self.map_ui.diplo.inbox_seen = d.inbox.len();
    }

    /// For the playthrough log.
    pub(super) fn report_diplo(&self, ctx: &mut Context) {
        let Some(d) = self.diplo.as_ref() else { return };
        let v = self.diplo_viewer();
        let ui = &self.map_ui.diplo;
        ctx.report("diplo_page", ui.open.map_or("closed", |p| p.name()));
        ctx.report("spy_page", ui.spies.map_or("closed", |p| p.name()));
        ctx.report("diplo_row", ui.row);
        ctx.report("treaties_ours", d.live().filter(|t| t.involves(v)).count());
        ctx.report("treaties_world", d.live().count());
        ctx.report("inbox", d.inbox.len());
        ctx.report("reputation", d.reputation[v].round());
        ctx.report("agents", d.spies.services[v].agents);
        ctx.report("agents_training", d.spies.services[v].training.len());
        ctx.report("missions_running", d.spies.missions.iter().filter(|m| m.by == v).count());
        ctx.report("spy_reports", d.spies.reports.iter().filter(|r| r.by == v).count());
        ctx.report("diplo_message", ui.message.clone().unwrap_or_default());
        ctx.report("propose_kind", Kind::PROPOSE[ui.kind].key());
        ctx.report("mission_kind", Mission::ALL[ui.mission].key());
        ctx.report("answers_to_player", d.answers.iter().filter(|a| a.from == v).count());
    }

    fn draw_frame(&self, canvas: &mut Canvas, label: &str) {
        canvas.blit_sprite(&self.assets.econ_window, vec2(WIN.x, WIN.y), Blit::default());
        canvas.label(label, WIN);
    }

    /// The diplomacy window.
    pub(super) fn draw_diplo(&self, canvas: &mut Canvas) {
        let (Some(page), Some(d), Some(w), Some(e)) = (self.map_ui.diplo.open, self.diplo.as_ref(), self.wars.as_ref(), self.economy.as_ref()) else { return };
        let a = &self.assets;
        self.draw_frame(canvas, "diplomacy window");
        let v = self.diplo_viewer();
        let (x, y) = (WIN.x + 26.0, WIN.y + 76.0);
        let r = ui::plain(canvas, &a.font, &format!("{} · reputation {:.0}", d.names[v], d.reputation[v]), vec2(WIN.right() - 50.0, WIN.y + 28.0), 13.0, SOFT, Align::Right);
        self.tip(r, "reputation");
        let row = self.map_ui.diplo.row;
        let list = order(d, v);
        match page {
            Page::Nations => {
                plain(canvas, &a.font, "NATIONS (best relations first): our view · theirs", vec2(x, y), 12.0, GOLD);
                let first = row.saturating_sub(17);
                for (k, &j) in list.iter().enumerate().skip(first).take(18) {
                    let ry = y + 20.0 + (k - first) as f32 * 26.0;
                    if k == row {
                        canvas.fill_rect(Rect::new(x - 6.0, ry - 3.0, 470.0, 24.0), Color::rgba(120, 170, 255, 55));
                    }
                    if let Some(world) = self.world.as_ref() {
                        canvas.fill_rect(Rect::new(x, ry + 3.0, 10.0, 12.0), world.color[j]);
                    }
                    fit(canvas, &a.font, &d.names[j], vec2(x + 16.0, ry), 14.0, Color::WHITE, 210.0);
                    let (r1, r2) = (d.relations[v][j], d.relations[j][v]);
                    let bar = Rect::new(x + 240.0, ry + 5.0, 120.0, 10.0);
                    canvas.fill_rect(bar, Color::rgba(255, 255, 255, 25));
                    let len = (r1 as f32 / 100.0) * 60.0;
                    let mid = bar.x + 60.0;
                    canvas.fill_rect(if len >= 0.0 { Rect::new(mid, bar.y, len, bar.h) } else { Rect::new(mid + len, bar.y, -len, bar.h) }, rel_color(r1));
                    plain(canvas, &a.font, &format!("{:+.0} · {:+.0}", r1, r2), vec2(x + 370.0, ry), 13.0, rel_color(r1));
                    self.tip_words(Rect::new(x + 236.0, ry - 3.0, 200.0, 24.0), format!("Relations: our view of {} ({:+.0}) and theirs of us ({:+.0}), -100 to 100. Built from the reasons on the right: trade, borders, ideology, treaties, reputation and old wars.", d.names[j], r1, r2));
                    self.tip_words(Rect::new(x + 436.0, ry - 3.0, 40.0, 24.0), "Treaties with them: T trade, N non-aggression, D defensive pact, A alliance, M military access, G guarantee, L loan.".to_string());
                    let pacts: Vec<&str> = d.between(v, j).map(|t| match t.kind {
                        Kind::Trade => "T",
                        Kind::NonAggression => "N",
                        Kind::Defensive => "D",
                        Kind::Alliance => "A",
                        Kind::Access => "M",
                        Kind::Guarantee => "G",
                        Kind::Loan => "L",
                        _ => "",
                    }).collect();
                    plain(canvas, &a.font, &pacts.join(""), vec2(x + 440.0, ry), 12.0, GOLD);
                }
                if let Some(&j) = list.get(row) {
                    let dx = WIN.x + 520.0;
                    fit(canvas, &a.serif, &d.names[j], vec2(dx, y), 20.0, Color::WHITE, 500.0);
                    plain(canvas, &a.font, &format!("Our view {:+.0} · theirs {:+.0} · their reputation {:.0} · {}", d.relations[v][j], d.relations[j][v], d.reputation[j], d.ideology[j].replace('_', " ")), vec2(dx, y + 30.0), 13.0, SOFT);
                    plain(canvas, &a.font, "WHY (our view)", vec2(dx, y + 54.0), 12.0, GOLD);
                    for (k, (words, val)) in d.reasons[v][j].iter().take(9).enumerate() {
                        let ry = y + 72.0 + k as f32 * 20.0;
                        plain(canvas, &a.font, &format!("{:+.0}", val), vec2(dx, ry), 14.0, rel_color(*val * 3.0));
                        fit(canvas, &a.font, words, vec2(dx + 50.0, ry), 14.0, Color::WHITE, 450.0);
                    }
                    plain(canvas, &a.font, "THEIR TREATIES WITH US", vec2(dx, y + 262.0), 12.0, GOLD);
                    let ours: Vec<String> = d.between(v, j).map(|t| format!("{} (to Year {})", t.kind.name(), t.end / 360 + 1)).collect();
                    let text = if ours.is_empty() { "none".to_string() } else { ours.join(" · ") };
                    fit(canvas, &a.font, &text, vec2(dx, y + 280.0), 13.0, Color::WHITE, 500.0);
                    let at_war = w.at_war(v, j).is_some();
                    plain(canvas, &a.font, if at_war { "AT WAR WITH US" } else { "PROPOSE A TREATY TO THEM: click one" }, vec2(dx, WIN.bottom() - 152.0), 12.0, if at_war { BAD } else { GOLD });
                }
            }
            Page::Treaties => {
                plain(canvas, &a.font, "OUR TREATIES (breaking one costs reputation with everyone)", vec2(x, y), 12.0, GOLD);
                let ours: Vec<&crate::diplo::Treaty> = d.live().filter(|t| t.involves(v)).collect();
                for (k, t) in ours.iter().enumerate().take(12) {
                    let ry = y + 20.0 + k as f32 * 24.0;
                    if k == row {
                        canvas.fill_rect(Rect::new(x - 6.0, ry - 3.0, 1000.0, 22.0), Color::rgba(120, 170, 255, 55));
                    }
                    fit(canvas, &a.font, &format!("{}: {} · since Year {} · to Year {}", t.kind.name(), t.words(&d.names), t.start / 360 + 1, t.end / 360 + 1), vec2(x, ry), 14.0, Color::WHITE, 990.0);
                }
                if ours.is_empty() {
                    plain(canvas, &a.font, "None yet: choose a nation on the Nations page, then Propose.", vec2(x, y + 22.0), 14.0, MUTED);
                }
                let ny = y + 330.0;
                plain(canvas, &a.font, "THE WORLD'S LATEST", vec2(x, ny), 12.0, GOLD);
                let mut latest: Vec<String> = Vec::new();
                for t in d.treaties.iter().rev().take(200) {
                    let line = match &t.state {
                        State::Broken { by, day, how } => format!("Year {}: {} broke its {} ({how})", day / 360 + 1, d.names[*by], t.kind.name().to_lowercase()),
                        State::Active if t.start + 360 > d.day => format!("Year {}: {}: {}", t.start / 360 + 1, t.kind.name(), t.words(&d.names)),
                        _ => continue,
                    };
                    latest.push(line);
                    if latest.len() >= 8 {
                        break;
                    }
                }
                for (k, l) in latest.iter().enumerate() {
                    fit(canvas, &a.font, l, vec2(x, ny + 18.0 + k as f32 * 19.0), 13.0, if l.contains("broke") { BAD } else { SOFT }, 1000.0);
                }
            }
            Page::Propose => {
                let kind = Kind::PROPOSE[self.map_ui.diplo.kind];
                let to = self.map_ui.diplo.nation.or_else(|| list.first().copied());
                plain(canvas, &a.font, "PROPOSE A TREATY: click one to send it", vec2(x, y), 12.0, GOLD);
                if let Some(to) = to {
                    fit(canvas, &a.serif, &format!("To {}", d.names[to]), vec2(x, y + 26.0), 22.0, Color::WHITE, 600.0);
                    plain(canvas, &a.font, &format!("relations {:+.0} (theirs {:+.0}) · their reputation {:.0}", d.relations[v][to], d.relations[to][v], d.reputation[to]), vec2(x, y + 58.0), 13.0, SOFT);
                }
                let what = treaty_words(kind);
                plain(canvas, &a.font, what, vec2(x, y + 190.0), 14.0, Color::WHITE);
                if let Some(m) = &self.map_ui.diplo.message {
                    fit(canvas, &a.font, m, vec2(x, y + 224.0), 17.0, if m.contains("Yes") || m.contains("Signed") { GOOD } else { BAD }, 1000.0);
                }
                plain(canvas, &a.font, "THEIR LATEST ANSWERS TO US", vec2(x, y + 270.0), 12.0, GOLD);
                for (k, ans) in d.answers.iter().rev().filter(|x| x.from == v).take(10).enumerate() {
                    let line = format!("Year {} · {} · {}: {} — {}", ans.day / 360 + 1, d.names[ans.to], ans.kind.name(), if ans.yes { "Yes" } else { "No" }, ans.reason);
                    fit(canvas, &a.font, &line, vec2(x, y + 290.0 + k as f32 * 19.0), 13.0, if ans.yes { GOOD } else { SOFT }, 1000.0);
                }
            }
            Page::Inbox => {
                plain(canvas, &a.font, "INBOX", vec2(x, y), 12.0, GOLD);
                if d.inbox.is_empty() {
                    plain(canvas, &a.font, "Nothing waiting.", vec2(x, y + 24.0), 14.0, MUTED);
                }
                for (k, item) in d.inbox.iter().enumerate().take(14) {
                    let ry = y + 22.0 + k as f32 * 28.0;
                    if k == row {
                        canvas.fill_rect(Rect::new(x - 6.0, ry - 3.0, 1000.0, 26.0), Color::rgba(120, 170, 255, 55));
                    }
                    let words = match item {
                        Inbox::Proposal(p) => match p.kind {
                            Kind::Arms => format!("{} wants to buy {:.0} {} from you", d.names[p.from], p.amount, p.good.map_or("weapons", |g| if g == self.econ_data.tanks { "tanks" } else { "units" })),
                            Kind::Loan => format!("{} asks you for a loan of {}", d.names[p.from], crate::diplo::money(p.amount)),
                            Kind::Sanctions => format!("{} asks you to join sanctions on {}", d.names[p.from], p.target.map_or(String::new(), |t| d.names[t].clone())),
                            Kind::Guarantee => format!("{} offers to guarantee your independence", d.names[p.from]),
                            k => format!("{} proposes a {}", d.names[p.from], k.name().to_lowercase()),
                        },
                        Inbox::Call { ally, war, side, deadline, .. } => {
                            let enemy = w.wars.get(*war).map_or(String::new(), |x| d.names.get(x.sides[1 - side][0]).cloned().unwrap_or_default());
                            format!("{} calls you to war against {} (by Year {} day {}; unanswered, you honour it)", d.names[*ally], enemy, deadline / 360 + 1, deadline % 360)
                        }
                    };
                    fit(canvas, &a.font, &words, vec2(x, ry), 15.0, Color::WHITE, 990.0);
                }
                if let Some(m) = &self.map_ui.diplo.message {
                    fit(canvas, &a.font, m, vec2(x, WIN.bottom() - 60.0), 15.0, GOLD, 1000.0);
                }
            }
            Page::Talks => {
                plain(canvas, &a.font, "PEACE TALKS", vec2(x, y), 12.0, GOLD);
                let wars: Vec<usize> = (0..w.wars.len()).filter(|&k| w.wars[k].active() && w.wars[k].side_of(v).is_some() && !w.wars[k].civil).collect();
                for (k, &war) in wars.iter().enumerate().take(6) {
                    let ry = y + 22.0 + k as f32 * 26.0;
                    if k == row {
                        canvas.fill_rect(Rect::new(x - 6.0, ry - 3.0, 1000.0, 24.0), Color::rgba(120, 170, 255, 55));
                    }
                    let wr = &w.wars[war];
                    let s = wr.side_of(v).unwrap_or(0);
                    let score = crate::war::peace::score(w, e, war) * if s == 0 { 1.0 } else { -1.0 };
                    fit(canvas, &a.font, &format!("{} vs {} · war score {:+.0}", d.names[wr.sides[0][0].min(d.n - 1)], d.names[wr.sides[1][0].min(d.n - 1)], score), vec2(x, ry), 15.0, Color::WHITE, 700.0);
                }
                if wars.is_empty() {
                    plain(canvas, &a.font, "We are at peace. The world's talks:", vec2(x, y + 22.0), 14.0, MUTED);
                } else {
                    plain(canvas, &a.font, &format!("Our demand: < {} >", DEMANDS[self.map_ui.diplo.demand]), vec2(x + 720.0, y + 22.0), 15.0, GOLD);
                }
                if let Some(m) = &self.map_ui.diplo.message {
                    fit(canvas, &a.font, m, vec2(x, y + 190.0), 15.0, GOLD, 1000.0);
                }
                plain(canvas, &a.font, "ROUND BY ROUND (newest first)", vec2(x, y + 220.0), 12.0, GOLD);
                let mine = |r: &&talks::Round| wars.contains(&r.war) || wars.is_empty();
                for (k, r) in d.talks.rounds.iter().rev().filter(mine).take(12).enumerate() {
                    let line = format!("Year {} · {} → {}: {} — {}", r.day / 360 + 1, d.names[r.from.min(d.n - 1)], d.names[r.to.min(d.n - 1)], r.offer, r.answer);
                    fit(canvas, &a.font, &line, vec2(x, y + 240.0 + k as f32 * 19.0), 13.0, if r.agreed { GOOD } else { SOFT }, 1000.0);
                }
            }
        }
        self.pop_ui(canvas);
        self.draw_widgets(canvas, &self.diplo_widgets(), self.window_live());
        self.push_ui(canvas);
    }

    /// The spy window.
    pub(super) fn draw_spies(&self, canvas: &mut Canvas) {
        let (Some(page), Some(d)) = (self.map_ui.diplo.spies, self.diplo.as_ref()) else { return };
        let a = &self.assets;
        self.draw_frame(canvas, "spy window");
        let v = self.diplo_viewer();
        let (x, y) = (WIN.x + 26.0, WIN.y + 76.0);
        let s = &d.spies.services[v];
        ui::plain(canvas, &a.font, &format!("{} · secret service", d.names[v]), vec2(WIN.right() - 50.0, WIN.y + 28.0), 13.0, SOFT, Align::Right);
        match page {
            SpyPage::Agents => {
                let lines = [
                    format!("Agents: {} ({} free, {} on missions, {} on counter-intelligence) · at most {}", s.agents, d.spies.idle(v), d.spies.busy(v), d.spies.ci_agents(v), s.max),
                    format!("Skill {:.0}% · in training: {}", s.skill * 100.0, s.training.len()),
                    format!("Counter-intelligence: {:.2} (base {:.2}) · share at home < {:.0}% >", d.spies.counter_intel(&d.data, v), s.ci_base, s.ci_share * 100.0),
                    format!("Training an agent: {} days and {}; keeping one: {} a year", d.data.spies.train_days, crate::diplo::money(d.data.spies.train_cost * self.economy.as_ref().map_or(0.0, |e| e.nations[v].gdp)), crate::diplo::money(d.data.spies.upkeep * self.economy.as_ref().map_or(0.0, |e| e.nations[v].gdp))),
                ];
                for (k, l) in lines.iter().enumerate() {
                    fit(canvas, &a.font, l, vec2(x, y + k as f32 * 26.0), 16.0, Color::WHITE, 1000.0);
                    let key = ["agents", "agents", "counter", "agents"][k];
                    self.tip(Rect::new(x, y + k as f32 * 26.0, 1000.0, 22.0), key);
                }
                plain(canvas, &a.font, "MISSIONS UNDER WAY", vec2(x, y + 120.0), 12.0, GOLD);
                for (k, m) in d.spies.missions.iter().filter(|m| m.by == v).enumerate().take(10) {
                    fit(canvas, &a.font, &format!("{} in {} · {} agent{} · {} days left", m.kind.name(), d.names[m.target], m.agents, if m.agents == 1 { "" } else { "s" }, m.end.saturating_sub(d.day)), vec2(x, y + 140.0 + k as f32 * 20.0), 14.0, Color::WHITE, 1000.0);
                }
                if let Some(m) = &self.map_ui.diplo.message {
                    fit(canvas, &a.font, m, vec2(x, WIN.bottom() - 60.0), 15.0, GOLD, 1000.0);
                }
            }
            SpyPage::Missions => {
                let target = self.map_ui.diplo.nation.filter(|&t| t != v).or_else(|| (0..d.n).find(|&j| j != v)).unwrap_or(0);
                fit(canvas, &a.serif, &format!("Target: < {} >", d.names[target]), vec2(x, y), 22.0, Color::WHITE, 700.0);
                let est = spies::estimate_ci(d, v, target);
                let clear = d.spies.intel_factor(d.day, v, target, &d.data) < 1.0;
                plain(canvas, &a.font, &format!("Their counter-intelligence, as our service sees it: about {:.2}{}", est, if clear { " (we have intel on them)" } else { " (no intel: it may be well off)" }), vec2(x, y + 34.0), 13.0, SOFT);
                let agents = d.spies.idle(v).min(2).max(1);
                plain(canvas, &a.font, &format!("MISSION (sending {} agent{})", agents, if agents == 1 { "" } else { "s" }), vec2(x, y + 62.0), 12.0, GOLD);
                plain(canvas, &a.font, "SUCCESS · CAUGHT (our estimate)", vec2(x + 420.0, y + 62.0), 12.0, GOLD);
                for (k, m) in Mission::ALL.iter().enumerate() {
                    let ry = y + 82.0 + k as f32 * 28.0;
                    if k == self.map_ui.diplo.mission {
                        canvas.fill_rect(Rect::new(x - 6.0, ry - 3.0, 1000.0, 26.0), Color::rgba(120, 170, 255, 55));
                    }
                    let (ps, pc) = spies::seen_chances(d, v, target, *m, agents);
                    plain(canvas, &a.font, m.name(), vec2(x, ry), 15.0, Color::WHITE);
                    plain(canvas, &a.font, &format!("{:.0}% · {:.0}%", ps * 100.0, pc * 100.0), vec2(x + 420.0, ry), 15.0, if ps > 0.5 { GOOD } else if ps > 0.25 { WARN } else { BAD });
                    self.tip_words(Rect::new(x + 416.0, ry - 3.0, 130.0, 26.0), format!("{} {}", self.tip_text.get("chance").cloned().unwrap_or_default(), self.tip_text.get("caught").cloned().unwrap_or_default()));
                    let ok = match (self.economy.as_ref(), self.wars.as_ref()) {
                        (Some(e), Some(w)) => spies::check(d, e, w, v, target, *m, agents).err().unwrap_or_default(),
                        _ => String::new(),
                    };
                    fit(canvas, &a.font, &ok, vec2(x + 560.0, ry), 13.0, MUTED, 440.0);
                }
                if let Some(m) = &self.map_ui.diplo.message {
                    fit(canvas, &a.font, m, vec2(x, WIN.bottom() - 60.0), 15.0, GOLD, 1000.0);
                }
            }
            SpyPage::Reports => {
                plain(canvas, &a.font, "OUR MISSIONS (newest first) · and agents caught in our country", vec2(x, y), 12.0, GOLD);
                let rows: Vec<String> = d
                    .spies
                    .reports
                    .iter()
                    .rev()
                    .filter(|r| r.by == v || (r.target == v && r.caught > 0))
                    .take(20)
                    .map(|r| {
                        if r.by == v {
                            format!(
                                "Year {} · {} in {}: {} ({:.0}% odds){}",
                                r.day / 360 + 1,
                                r.kind.name(),
                                d.names[r.target],
                                if r.success { r.words.clone() } else { "failed".to_string() },
                                r.p_success * 100.0,
                                if r.caught > 0 { format!(" · {} agent{} caught: an incident", r.caught, if r.caught == 1 { "" } else { "s" }) } else { String::new() }
                            )
                        } else {
                            format!("Year {} · we caught {} agent{} of {} ({})", r.day / 360 + 1, r.caught, if r.caught == 1 { "" } else { "s" }, d.names[r.by], r.kind.name().to_lowercase())
                        }
                    })
                    .collect();
                if rows.is_empty() {
                    plain(canvas, &a.font, "No missions yet.", vec2(x, y + 24.0), 14.0, MUTED);
                }
                for (k, l) in rows.iter().enumerate() {
                    fit(canvas, &a.font, l, vec2(x, y + 22.0 + k as f32 * 22.0), 14.0, if l.contains("caught") { BAD } else if l.contains("failed") { SOFT } else { Color::WHITE }, 1000.0);
                }
            }
        }
        self.pop_ui(canvas);
        self.draw_widgets(canvas, &self.spy_widgets(), self.window_live());
        self.push_ui(canvas);
    }
}
