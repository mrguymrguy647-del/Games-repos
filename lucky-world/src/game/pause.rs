//! Esc, the pause menu and the "are you sure?" box.
//!
//! The rules (the player's): Esc never throws you out of a game. It closes
//! whatever is open on top; with nothing open it opens the pause menu, on
//! every in-game screen, the wheels included. Leaving to the main menu or
//! the desktop always asks first, and saves first.

use duckforge::prelude::*;

use super::cmd::Cmd;
use super::gui::{self, Act, Kind, Widget};
use super::{Lucky, Screen};
use crate::save;
use crate::ui;

/// An "are you sure?" box: `cmd` runs on yes.
#[derive(Debug, Clone, PartialEq)]
pub struct Confirm {
    pub title: String,
    pub words: String,
    pub yes: String,
    pub cmd: Cmd,
    /// A warning (declaring war): the yes button is red.
    pub danger: bool,
}

impl Confirm {
    pub fn new(title: &str, words: &str, yes: &str, cmd: Cmd) -> Confirm {
        Confirm { title: title.to_string(), words: words.to_string(), yes: yes.to_string(), cmd, danger: false }
    }
}

const PAUSE: Rect = Rect { x: 430.0, y: 132.0, w: 420.0, h: 456.0 };
const SLOTS_PANEL: Rect = Rect { x: 320.0, y: 110.0, w: 640.0, h: 500.0 };
const ASK: Rect = Rect { x: 400.0, y: 250.0, w: 480.0, h: 210.0 };

impl Lucky {
    /// Draws widgets (each in its own space), lights the one under the
    /// mouse when this layer is `live`, and notes their tooltips.
    pub fn draw_widgets(&self, canvas: &mut Canvas, ws: &[Widget], live: bool) {
        let s = self.settings.ui_scale;
        let hot = if live { self.mouse.and_then(|m| ws.iter().rposition(|w| w.contains(s, m))) } else { None };
        for (i, w) in ws.iter().enumerate() {
            w.space.push(canvas, s);
            gui::draw_widget(canvas, &self.assets, w, hot == Some(i), self.mouse_down);
            canvas.pop_camera();
            if live {
                let tip = w.tip_words();
                if !tip.is_empty() {
                    self.tip_screen(w.screen_rect(s), tip);
                    if w.kind == Kind::Menu {
                        self.tips.beside.borrow_mut().push(w.screen_rect(s));
                    }
                }
            }
        }
    }

    /// The command (or key press) of the widget clicked this frame.
    pub fn clicked(&mut self, ctx: &Context, ws: &[Widget]) -> Option<Act> {
        let s = self.settings.ui_scale;
        let act = self.pointer.click(ctx, self.mouse, s, ws);
        if matches!(act, Some(Act::Key(_)) | Some(Act::Cmd(_)) | Some(Act::Row(_))) {
            self.clicks += 1;
        }
        act
    }

    // ---------------------------------------------------------------
    // Esc
    // ---------------------------------------------------------------

    /// Esc on a screen with nothing of its own to close. Returns whether
    /// it was dealt with here (the screen's own update is skipped).
    pub(super) fn escape(&mut self, ctx: &mut Context) -> bool {
        match self.screen {
            // These close their own things first (and the map opens the pause menu itself).
            Screen::Menu | Screen::Load | Screen::Settings | Screen::Help | Screen::Map => false,
            Screen::Rivals if self.inspect.is_some() => false,
            // Choosing a new game: nothing is under way yet, so Esc is "back".
            Screen::Title if self.menu_started => {
                self.stop_game_sounds(ctx);
                self.open_menu();
                true
            }
            _ => {
                self.open_pause(ctx);
                true
            }
        }
    }

    // ---------------------------------------------------------------
    // "Are you sure?"
    // ---------------------------------------------------------------

    pub fn ask(&mut self, ctx: &mut Context, c: Confirm) {
        self.shell.confirm = Some(c);
        crate::ui::sfx(ctx, &self.assets.card_sound, 0.6, 0.0);
    }

    pub(super) fn confirm_widgets_pub(&self) -> Vec<Widget> {
        self.confirm_widgets()
    }

    fn confirm_widgets(&self) -> Vec<Widget> {
        let Some(c) = self.shell.confirm.as_ref() else { return Vec::new() };
        let mut yes = Widget::cmd(Rect::new(ASK.x + 24.0, ASK.bottom() - 62.0, 208.0, 40.0), c.yes.clone(), c.cmd.clone()).tip("Enter or Y");
        if c.danger {
            yes = yes.danger();
        } else {
            yes = yes.on(true);
        }
        vec![Widget::ghost(ASK), yes, Widget::cmd(Rect::new(ASK.right() - 232.0, ASK.bottom() - 62.0, 208.0, 40.0), "Cancel", Cmd::Cancel).tip("Esc or N")]
    }

    pub(super) fn update_confirm(&mut self, ctx: &mut Context) {
        ctx.idle_ok();
        let Some(c) = self.shell.confirm.clone() else { return };
        if gui::pressed(ctx, Key::Escape) || gui::pressed(ctx, Key::N) {
            self.run(ctx, Cmd::Cancel);
            return;
        }
        if gui::pressed(ctx, Key::Enter) || gui::pressed(ctx, Key::Y) {
            self.shell.confirm = None;
            self.run(ctx, c.cmd);
            return;
        }
        let ws = self.confirm_widgets();
        match self.clicked(ctx, &ws) {
            Some(Act::Cmd(Cmd::Cancel)) => self.run(ctx, Cmd::Cancel),
            Some(Act::Cmd(cmd)) => {
                self.shell.confirm = None;
                self.run(ctx, cmd);
            }
            Some(_) => {}
            // A click outside the box is "no".
            None if ctx.input.was_mouse_pressed(MouseButton::Left) => self.run(ctx, Cmd::Cancel),
            None => {}
        }
    }

    pub(super) fn draw_confirm(&self, canvas: &mut Canvas) {
        let Some(c) = self.shell.confirm.as_ref() else { return };
        let a = &self.assets;
        ui::dim(canvas, 0.55);
        gui::Space::Center.push(canvas, self.settings.ui_scale);
        gui::panel(canvas, a, ASK);
        ui::plain(canvas, &a.serif, &c.title, vec2(ASK.center().x, ASK.y + 26.0), 24.0, if c.danger { gui::RED } else { gui::IVORY }, Align::Center);
        let style = TextStyle { size: 15.0, color: gui::LABEL, align: Align::Center, wrap: Some(ASK.w - 60.0), ..TextStyle::default() };
        canvas.draw_font_text(&a.font, &c.words, vec2(ASK.center().x, ASK.y + 72.0), &style);
        canvas.label("confirm", ASK);
        canvas.pop_camera();
        self.draw_widgets(canvas, &self.confirm_widgets(), true);
    }

    // ---------------------------------------------------------------
    // The pause menu
    // ---------------------------------------------------------------

    pub(super) fn open_pause(&mut self, ctx: &mut Context) {
        if self.shell.pause {
            return;
        }
        self.shell.pause = true;
        self.shell.pause_slots = false;
        self.shell.row = 0;
        self.shell.key_nav = false;
        self.shell.was_paused = self.clock.paused;
        self.clock.paused = true;
        crate::ui::sfx(ctx, &self.assets.card_sound, 0.6, 0.0);
    }

    pub(super) fn close_pause(&mut self, ctx: &mut Context) {
        self.shell.pause = false;
        self.shell.pause_slots = false;
        self.clock.paused = self.shell.was_paused;
        crate::ui::sfx(ctx, &self.assets.click, 0.6, 0.0);
    }

    /// The pause menu's buttons (or its save slots), top to bottom.
    pub(super) fn pause_widgets(&self) -> Vec<Widget> {
        if self.shell.pause_slots {
            let mut ws = vec![Widget::ghost(SLOTS_PANEL)];
            for (i, label) in super::shell::SLOTS.iter().enumerate() {
                let path = save::path_for(label);
                let what = self.shell.slots.iter().find(|s| s.path == path).map_or("empty".to_string(), |s| match &s.summary {
                    Ok(x) => format!("{} · {} · {} · {}", x.nation, x.date, x.difficulty, save::ago(x.saved_at)),
                    Err(e) => format!("can't be loaded: {e}"),
                });
                ws.push(Widget::new(Kind::Row, Rect::new(SLOTS_PANEL.x + 24.0, SLOTS_PANEL.y + 84.0 + i as f32 * 56.0, SLOTS_PANEL.w - 48.0, 50.0), *label, Act::Cmd(Cmd::SaveSlot(i))).sub(what).icon("save").tip("Save here (what was in the slot is replaced)"));
            }
            ws.push(Widget::cmd(Rect::new(SLOTS_PANEL.center().x - 100.0, SLOTS_PANEL.bottom() - 56.0, 200.0, 38.0), "Back", Cmd::Back).tip("Esc"));
            return ws;
        }
        let x = PAUSE.x + 40.0;
        let row = |i: usize| Rect::new(x, PAUSE.y + 100.0 + i as f32 * 50.0, PAUSE.w - 80.0, 42.0);
        let save_tip = if self.can_save() { "Save in one of six slots" } else if self.spectating { "A watched world can't be saved" } else { "This game is over" };
        vec![
            Widget::ghost(PAUSE),
            Widget::cmd(row(0), "Resume", Cmd::Resume).on(true).icon("resume").tip("Back to the game"),
            Widget::cmd(row(1), "Save game", Cmd::SaveMenu).icon("save").off(!self.can_save()).tip(save_tip),
            Widget::cmd(row(2), "Load game", Cmd::LoadMenu).icon("load").tip("Open a saved game"),
            Widget::cmd(row(3), "Settings", Cmd::Settings).icon("gear").tip("Picture, sound, keys"),
            Widget::cmd(row(4), "How to play", Cmd::Help).icon("help").tip("The help pages"),
            Widget::cmd(Rect::new(x, PAUSE.y + 356.0, 166.0, 40.0), "Main menu", Cmd::AskMainMenu).icon("home").tip("Asks first, then saves and leaves"),
            Widget::cmd(Rect::new(PAUSE.right() - 40.0 - 166.0, PAUSE.y + 356.0, 166.0, 40.0), "Quit to desktop", Cmd::AskQuit).icon("quit").tip("Asks first, then saves and quits"),
        ]
    }

    /// The pause menu is open: nothing else moves.
    pub(super) fn update_pause(&mut self, ctx: &mut Context) {
        ctx.idle_ok();
        let ws = self.pause_widgets();
        // The buttons, in order (the first widget is the panel itself).
        let buttons: Vec<usize> = (0..ws.len()).filter(|&i| ws[i].kind != Kind::Ghost).collect();
        let n = buttons.len().max(1);
        if gui::pressed(ctx, Key::Escape) {
            if self.shell.pause_slots {
                self.run(ctx, Cmd::Back);
            } else {
                self.close_pause(ctx);
            }
            return;
        }
        if self.mouse_moved {
            self.shell.key_nav = false;
        }
        let before = self.shell.row;
        if gui::pressed(ctx, Key::Down) {
            self.shell.row = (self.shell.row + 1) % n;
            self.shell.key_nav = true;
        }
        if gui::pressed(ctx, Key::Up) {
            self.shell.row = (self.shell.row + n - 1) % n;
            self.shell.key_nav = true;
        }
        if self.shell.row != before {
            crate::ui::sfx(ctx, &self.assets.menu_move, 0.6, 0.0);
        }
        let mut act = self.clicked(ctx, &ws);
        if gui::pressed(ctx, Key::Enter) {
            if let Some(w) = buttons.get(self.shell.row.min(n - 1)).map(|&i| &ws[i]) {
                act = Some(if w.off { Act::None } else { w.act.clone() });
            }
        }
        match act {
            Some(Act::Cmd(cmd)) => {
                crate::ui::sfx(ctx, &self.assets.click, 0.8, 0.0);
                self.run(ctx, cmd);
            }
            Some(Act::None) if !ctx.input.was_mouse_pressed(MouseButton::Left) => crate::ui::sfx(ctx, &self.assets.denied, 0.6, 0.0),
            _ => {}
        }
    }

    pub(super) fn draw_pause(&self, canvas: &mut Canvas, live: bool) {
        let a = &self.assets;
        ui::dim(canvas, 0.62);
        let s = self.settings.ui_scale;
        gui::Space::Center.push(canvas, s);
        if self.shell.pause_slots {
            gui::panel(canvas, a, SLOTS_PANEL);
            ui::plain(canvas, &a.serif, "Save game", vec2(SLOTS_PANEL.center().x, SLOTS_PANEL.y + 22.0), 28.0, gui::IVORY, Align::Center);
            ui::plain(canvas, &a.font, "Choose a slot. Saves are in the \"saves\" folder beside the game.", vec2(SLOTS_PANEL.center().x, SLOTS_PANEL.y + 58.0), 13.0, gui::LABEL, Align::Center);
            canvas.label("save slots", SLOTS_PANEL);
        } else {
            gui::panel(canvas, a, PAUSE);
            ui::plain(canvas, &a.serif, "Game paused", vec2(PAUSE.center().x, PAUSE.y + 24.0), 30.0, gui::IVORY, Align::Center);
            let nation = self.player.as_ref().map_or("Your nation is still being spun".to_string(), |p| p.name.clone());
            let when = match self.economy.as_ref() {
                Some(e) if self.screen == Screen::Map || self.screen == Screen::Outcome => e.date(&self.econ_data),
                _ if self.player.is_none() => format!("Wheel {} of {}", (self.landed.len() + 1).min(self.data.wheels.len()), self.data.wheels.len()),
                _ => "Before the first day".to_string(),
            };
            gui::fit(canvas, &a.font, &format!("{nation}  ·  {when}"), vec2(PAUSE.center().x, PAUSE.y + 64.0), 15.0, gui::LABEL, Align::Center, PAUSE.w - 40.0);
            canvas.fill_rect(Rect::new(PAUSE.x + 40.0, PAUSE.y + 342.0, PAUSE.w - 80.0, 1.0), Color::rgba(138, 109, 47, 140));
            ui::plain(canvas, &a.font, "Leaving always asks first and saves your game.", vec2(PAUSE.center().x, PAUSE.y + 414.0), 13.0, gui::LABEL, Align::Center);
            canvas.label("pause menu", PAUSE);
        }
        canvas.pop_camera();
        let ws = self.pause_widgets();
        self.draw_widgets(canvas, &ws, live);
        // The keyboard's place (Up / Down), when the keys are in use.
        if live && self.shell.key_nav {
            let buttons: Vec<&Widget> = ws.iter().filter(|w| w.kind != Kind::Ghost).collect();
            if let Some(w) = buttons.get(self.shell.row.min(buttons.len().saturating_sub(1))) {
                gui::Space::Center.push(canvas, s);
                let r = w.rect;
                canvas.draw_rect(Rect::new(r.x - 3.0, r.y - 3.0, r.w + 6.0, r.h + 6.0), gui::GOLD);
                canvas.pop_camera();
            }
        }
    }
}
