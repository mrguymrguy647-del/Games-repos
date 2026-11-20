//! Commands: what a click does when no key does it already (most buttons
//! simply press their shortcut key, see `gui`). One list, one place they run.

use duckforge::prelude::*;

use super::diplo_screen::{Page as DiploPage, SpyPage};
use super::econ_screen::{Overlay, Tab};
use super::pause::Confirm;
use super::war_screen::Page as WarPage;
use super::{Lucky, Screen};

#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    // The pause menu and the "are you sure?" box.
    Resume,
    SaveMenu,
    SaveSlot(usize),
    LoadMenu,
    Settings,
    Help,
    /// Ask first, then save and leave to the main menu / the desktop.
    AskMainMenu,
    AskQuit,
    MainMenu,
    Quit,
    Cancel,
    Back,
    /// Load this save file (asked first when a game is running).
    AskLoad(std::path::PathBuf),
    Load(std::path::PathBuf),
    /// Delete this save file (asked first).
    DeleteSave(std::path::PathBuf),
    // The map's HUD.
    /// The economy window on a page, with a row chosen.
    Econ(Tab, usize),
    War(WarPage),
    Diplo(DiploPage, Option<usize>),
    Spies(SpyPage, Option<usize>),
    /// A nation's country card.
    Card(usize),
    /// The camera goes to a nation's capital.
    Centre(usize),
    /// The map view: nations, terrain, or an overlay.
    View(View),
    /// The camera goes to where the news strip's event happened.
    ShowNews,
    /// Zoom in (-1) or out (+1) a step.
    Zoom(i8),
    /// The selected province's details card.
    Details,
    /// Build in the selected province (the Chains page, aimed at it).
    BuildHere,
    /// One of the "Needs you" rows.
    Need(usize),
    /// The battle marker's battle: watch it.
    Battle(usize),
    /// A finished battle's report in full.
    Report(usize),
    /// The right-click menu.
    AskWar(usize),
    DeclareWar(usize),
    TradeWith(usize),
    CloseMenu,
    /// Propose this kind of treaty (an index into the kinds a player may propose) to the chosen nation.
    ProposeKind(usize),
    /// The tutorial card's buttons.
    TutorialSkip,
    TutorialEnd,
    /// A map overlay the five view buttons don't have (armies, relations).
    Overlay(Overlay),
    /// The pause menu (the top bar's Menu button).
    Menu,
    /// The wheels.
    Spin,
    SkipSpin,
    /// The list beside the wheel scrolls to its top / shows the result.
    Nothing,
}

/// The map's five views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Nations,
    Terrain,
    Resources,
    Industry,
    Alliances,
}

impl View {
    pub const ALL: [View; 5] = [View::Nations, View::Terrain, View::Resources, View::Industry, View::Alliances];

    pub fn name(self) -> &'static str {
        match self {
            View::Nations => "Nations",
            View::Terrain => "Terrain",
            View::Resources => "Resources",
            View::Industry => "Industry",
            View::Alliances => "Alliances",
        }
    }

    pub fn tip(self) -> &'static str {
        match self {
            View::Nations => "Each nation's colour over the land",
            View::Terrain => "The land itself: forests, mountains, deserts",
            View::Resources => "What lies under each province",
            View::Industry => "Where the factories are, by what they make",
            View::Alliances => "Your allies, your enemies and everyone's blocs",
        }
    }
}

impl Lucky {
    /// Runs a command.
    pub fn run(&mut self, ctx: &mut Context, cmd: Cmd) {
        match cmd {
            Cmd::Nothing => {}
            Cmd::Resume => self.close_pause(ctx),
            Cmd::Menu => self.open_pause(ctx),
            Cmd::TutorialSkip => self.tutorial_skip = true,
            Cmd::TutorialEnd => self.tutorial_end = true,
            Cmd::Overlay(o) => {
                self.close_windows();
                self.map_ui.econ.overlay = o;
                self.map_ui.econ.force_overlay();
                self.map_ui.war.all_armies = o == Overlay::War;
                self.map_ui.war.refresh();
                self.refresh_overlay();
            }
            Cmd::SaveMenu => {
                self.shell.slots = crate::save::list();
                self.shell.pause_slots = true;
                self.shell.row = 0;
                crate::ui::sfx(ctx, &self.assets.click, 0.7, 0.0);
            }
            Cmd::SaveSlot(k) => {
                let label = super::shell::SLOTS[k.min(super::shell::SLOTS.len() - 1)];
                let path = crate::save::path_for(label);
                self.save_game(ctx, label, &path);
                self.shell.pause_slots = false;
                self.shell.row = 0;
                self.shell.slots = crate::save::list();
            }
            Cmd::Back => {
                self.shell.pause_slots = false;
                self.shell.row = 0;
                crate::ui::sfx(ctx, &self.assets.click, 0.6, 0.0);
            }
            Cmd::LoadMenu => self.open_sub(Screen::Load),
            Cmd::Settings => self.open_sub(Screen::Settings),
            Cmd::Help => self.open_sub(Screen::Help),
            Cmd::AskMainMenu => {
                let words = if self.can_save() { "Your game is saved first: Continue on the main menu brings you back." } else { "This game can't be saved (you are only watching)." };
                self.ask(ctx, Confirm::new("Leave to the main menu?", words, "Save and leave", Cmd::MainMenu));
            }
            Cmd::AskQuit => {
                let in_game = self.in_game();
                let words = if !in_game { "" } else if self.can_save() { "Your game is saved first." } else { "This game can't be saved (you are only watching)." };
                let yes = if in_game && self.can_save() { "Save and quit" } else { "Quit" };
                self.ask(ctx, Confirm::new("Quit to the desktop?", words, yes, Cmd::Quit));
            }
            Cmd::MainMenu => {
                self.leave_game(ctx);
                self.open_menu();
            }
            Cmd::Quit => {
                if self.in_game() {
                    self.leave_game(ctx);
                }
                ctx.quit();
            }
            Cmd::Cancel => {
                self.shell.confirm = None;
                crate::ui::sfx(ctx, &self.assets.click, 0.6, 0.0);
            }
            Cmd::AskLoad(path) => {
                if self.in_game() && self.shell.back.is_some_and(|b| b != Screen::Menu) {
                    self.ask(ctx, Confirm::new("Load this save?", "Anything since your last save in the game you are playing is lost.", "Load", Cmd::Load(path)));
                } else {
                    self.run(ctx, Cmd::Load(path));
                }
            }
            Cmd::DeleteSave(path) => {
                match std::fs::remove_file(&path) {
                    Ok(()) => self.say(format!("Deleted {}", path.display())),
                    Err(e) => self.say(format!("Couldn't delete it: {e}")),
                }
                self.shell.slots = crate::save::list();
                self.shell.row = self.shell.row.min(self.shell.slots.len().saturating_sub(1));
            }
            Cmd::Load(path) => {
                self.shell.back = None;
                self.load_from(ctx, &path);
            }
            Cmd::Spin | Cmd::SkipSpin => {}
            other => self.run_map(ctx, other),
        }
    }

    /// A game is under way (anything from the first wheel on).
    pub fn in_game(&self) -> bool {
        let screen = if matches!(self.screen, Screen::Load | Screen::Settings | Screen::Help) { self.shell.back.unwrap_or(Screen::Menu) } else { self.screen };
        !matches!(screen, Screen::Menu | Screen::Title)
    }

    /// Whether this game can be saved now.
    pub fn can_save(&self) -> bool {
        self.in_game() && !self.spectating && !self.fate.outcome.is_some_and(|o| !o.can_play_on())
    }

    /// The map's own commands.
    fn run_map(&mut self, ctx: &mut Context, cmd: Cmd) {
        let click = |g: &Lucky, ctx: &mut Context| crate::ui::sfx(ctx, &g.assets.card_sound, 0.7, 0.0);
        match cmd {
            Cmd::Econ(tab, row) => {
                self.close_windows();
                self.map_ui.econ.open = Some(tab);
                self.map_ui.econ.row = row;
                click(self, ctx);
            }
            Cmd::War(page) => {
                self.close_windows();
                self.map_ui.war.open = Some(page);
                self.map_ui.war.row = 0;
                click(self, ctx);
            }
            Cmd::Diplo(page, nation) => {
                self.close_windows();
                self.open_diplo(ctx, false);
                self.map_ui.diplo.open = Some(page);
                if let Some(n) = nation {
                    self.map_ui.diplo.nation = Some(n);
                    self.diplo_row_to(n);
                }
            }
            Cmd::Spies(page, nation) => {
                self.close_windows();
                self.open_diplo(ctx, true);
                self.map_ui.diplo.spies = Some(page);
                if nation.is_some() {
                    self.map_ui.diplo.nation = nation;
                }
            }
            Cmd::Card(n) => {
                self.map_ui.menu = None;
                self.map_ui.card = Some(n);
                click(self, ctx);
            }
            Cmd::Centre(n) => {
                self.map_ui.menu = None;
                self.centre_on(n);
                crate::ui::sfx(ctx, &self.assets.select, 0.6, 0.0);
            }
            Cmd::View(v) => {
                self.set_view(v);
                crate::ui::sfx(ctx, &self.assets.click, 0.7, 0.0);
            }
            Cmd::ShowNews => {
                if let Some(at) = self.map_ui.news_at {
                    self.map_ui.glide = Some(at);
                    crate::ui::sfx(ctx, &self.assets.select, 0.6, 0.0);
                }
            }
            Cmd::Zoom(d) => {
                self.map_ui.zoom_goal = Some(self.map_view.as_ref().map_or(10.0, |v| v.orbit.distance) * if d < 0 { 0.7 } else { 1.0 / 0.7 });
            }
            Cmd::Details => {
                self.map_ui.details = !self.map_ui.details;
                click(self, ctx);
            }
            Cmd::BuildHere => {
                self.close_windows();
                self.map_ui.econ.open = Some(Tab::Chains);
                self.map_ui.econ.build_at = self.map_ui.selected;
                click(self, ctx);
            }
            Cmd::Need(k) => {
                if let Some(need) = self.needs().into_iter().nth(k) {
                    self.run(ctx, need.cmd);
                }
            }
            Cmd::Battle(b) => {
                self.map_ui.menu = None;
                self.watch_battle(ctx, b);
            }
            Cmd::Report(k) => {
                self.map_ui.war.full = Some(k);
                self.map_ui.war.card = None;
                click(self, ctx);
            }
            Cmd::AskWar(n) => {
                self.map_ui.menu = None;
                let name = self.world.as_ref().map_or(String::new(), |w| w.nations[n].name.clone());
                let level = crate::war::LEVELS[self.map_ui.war.declare_level.clamp(1, 4)];
                let mut c = Confirm::new(&format!("Declare war on {name}?"), &format!("It begins as a {}. Their allies may join them, and every treaty you have with them is broken.", level.to_lowercase()), "Declare war", Cmd::DeclareWar(n));
                c.danger = true;
                self.ask(ctx, c);
            }
            Cmd::DeclareWar(n) => self.declare_war(ctx, n),
            Cmd::TradeWith(n) => {
                self.close_windows();
                self.map_ui.econ.open = Some(Tab::Trade);
                self.map_ui.econ.row = self.trade_row_of(n);
                click(self, ctx);
            }
            Cmd::CloseMenu => self.map_ui.menu = None,
            Cmd::ProposeKind(k) => {
                // The Propose page with this treaty chosen, and Enter pressed: it is sent.
                self.map_ui.diplo.kind = k;
                self.map_ui.diplo.open = Some(DiploPage::Propose);
                super::gui::press(Key::Enter);
            }
            _ => {}
        }
    }

    /// Closes every window on the map (one at a time is open).
    pub fn close_windows(&mut self) {
        self.map_ui.menu = None;
        self.map_ui.econ.open = None;
        self.map_ui.war.open = None;
        self.map_ui.diplo.open = None;
        self.map_ui.diplo.spies = None;
        self.map_ui.details = false;
    }
}
