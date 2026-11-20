//! Mouse-first on the screens round the map: the buttons that stand in
//! for the last keys-only actions (the Load screen's Load / Delete / Back,
//! the Settings sliders, the rival reveal's pages, the tutorial's Skip),
//! and `click_targets`: everything that can be clicked right now, by
//! name, which the mouse-only playthrough uses to find its way.

use duckforge::prelude::*;

use super::cmd::Cmd;
use super::gui::{Act, Kind, Space, Widget};
use super::windows::WIN;
use super::{Lucky, Screen};
use crate::ui;

impl Lucky {
    /// A row of buttons along the bottom of a full-window screen.
    fn bottom_bar(&self, items: Vec<(&str, Act, &str, bool)>) -> Vec<Widget> {
        let style = TextStyle { size: 15.0, ..TextStyle::default() };
        let widths: Vec<f32> = items.iter().map(|(label, ..)| (self.assets.font.measure(label, &style).x + 28.0).max(72.0)).collect();
        let total: f32 = widths.iter().sum::<f32>() + 6.0 * (items.len().saturating_sub(1)) as f32;
        let mut x = WIN.center().x - total / 2.0;
        let mut out = Vec::new();
        for ((label, act, tip, gold), w) in items.into_iter().zip(widths) {
            out.push(Widget::new(Kind::Button, Rect::new(x, WIN.bottom() - 44.0, w, 32.0), label, act).tip(tip).on(gold));
            x += w + 6.0;
        }
        out
    }

    /// The extra buttons of the screen that is up (none on the map: its
    /// HUD and windows have their own).
    pub(super) fn screen_widgets(&self) -> Vec<Widget> {
        let mut ws = Vec::new();
        if matches!(self.screen, Screen::Load | Screen::Settings | Screen::Help) && self.shell.binding.is_none() {
            ws.push(super::windows::close_x().tip("Back"));
        }
        match self.screen {
            Screen::Load => {
                let any = !self.shell.slots.is_empty();
                let mut items = Vec::new();
                if any {
                    items.push(("Load this save", Act::Key(Key::Enter), "Open the chosen save", true));
                    items.push(("Delete it", Act::Key(Key::X), "Removes the chosen save file for good", false));
                }
                items.push(("Back", Act::Key(Key::Escape), "", false));
                ws.extend(self.bottom_bar(items));
            }
            Screen::Help => ws.extend(self.bottom_bar(vec![("Back", Act::Key(Key::Escape), "", false)])),
            Screen::Settings if self.shell.keys_page => {
                ws.extend(self.bottom_bar(vec![
                    ("Change this key", Act::Key(Key::Enter), "Then press the new key on the keyboard", true),
                    ("All keys back to their defaults", Act::Key(Key::Backspace), "", false),
                    ("Back", Act::Key(Key::Escape), "", false),
                ]));
            }
            Screen::Settings => {
                for (id, v) in self.settings_sliders() {
                    let r = Self::set_rect(id.0);
                    ws.push(Widget::new(Kind::Slider(v), Rect::new(r.x + 430.0, r.y + 2.0, 320.0, 30.0), "", Act::Slide(id.1, 0.0)).tip("Drag to set it"));
                }
            }
            Screen::Title if self.menu_started => {
                ws.push(Widget::new(Kind::Button, Rect::new(24.0, 664.0, 150.0, 34.0), "Back", Act::Key(Key::Escape)).tip("The main menu"));
            }
            Screen::Rivals if self.inspect.is_none() => {
                let n = self.rivals.len();
                if n > 24 {
                    ws.push(Widget::new(Kind::Button, Rect::new(720.0, 34.0, 120.0, 30.0), "Previous page", Act::Key(Key::Left)).tip("More nations"));
                    ws.push(Widget::new(Kind::Button, Rect::new(846.0, 34.0, 120.0, 30.0), "Next page", Act::Key(Key::Right)).tip("More nations"));
                }
            }
            _ => {}
        }
        // The tutorial's own buttons, on its card.
        if let (Some(r), false) = (self.tutorial_rect(), self.shell.pause) {
            ws.push(Widget::cmd(Rect::new(r.right() - 244.0, r.bottom() - 30.0, 110.0, 24.0), "Skip this step", Cmd::TutorialSkip).space(Space::Screen).tip("Go on to the next step"));
            ws.push(Widget::cmd(Rect::new(r.right() - 128.0, r.bottom() - 30.0, 118.0, 24.0), "End the tutorial", Cmd::TutorialEnd).space(Space::Screen).tip("The game goes on without the guide"));
        }
        ws
    }

    /// The Settings rows with a slider: ((row, slider id), its share 0..1).
    fn settings_sliders(&self) -> Vec<((usize, u8), f32)> {
        let s = &self.settings;
        vec![((3, 0), s.music), ((4, 1), s.effects), ((5, 2), (s.ui_scale - 0.9) / 0.25)]
    }

    /// Clicks on the screen's extra buttons. Returns whether the click was used.
    pub(super) fn update_screen_widgets(&mut self, ctx: &mut Context) -> bool {
        let ws = self.screen_widgets();
        if ws.is_empty() {
            return false;
        }
        match self.clicked(ctx, &ws) {
            Some(Act::Cmd(cmd)) => {
                self.run(ctx, cmd);
                true
            }
            Some(Act::Slide(id, t)) => {
                match id {
                    0 => self.settings.music = (t * 20.0).round() / 20.0,
                    1 => self.settings.effects = (t * 20.0).round() / 20.0,
                    _ => self.settings.ui_scale = ((0.9 + t * 0.25) * 20.0).round() / 20.0,
                }
                self.shell.row = [3, 4, 5][id.min(2) as usize];
                self.settings_changed(ctx);
                true
            }
            Some(_) => true,
            None => false,
        }
    }

    pub(super) fn draw_screen_widgets(&self, canvas: &mut Canvas) {
        let live = !self.shell.pause && self.shell.confirm.is_none();
        self.draw_widgets(canvas, &self.screen_widgets(), live);
    }

    /// Everything that can be clicked right now, by name, with a screen
    /// point inside it: buttons by their label (or their icon's name),
    /// rows as "row N". Only the layer on top answers clicks, so only it is
    /// listed.
    pub fn click_targets(&self) -> Vec<(String, Vec2)> {
        let s = self.settings.ui_scale;
        let from = |ws: Vec<Widget>| -> Vec<(String, Vec2)> {
            ws.into_iter()
                .filter(|w| !w.off)
                .filter_map(|w| {
                    let name = match (&w.act, w.kind) {
                        (Act::Row(i), Kind::Ghost) => format!("row {i}"),
                        (Act::None, _) => return None,
                        (Act::Slide(id, _), _) => format!("slider {id}"),
                        (_, Kind::Ghost) if w.label.is_empty() => return None,
                        _ if !w.label.is_empty() => w.label.clone(),
                        _ => w.icon.unwrap_or("").to_string(),
                    };
                    (!name.is_empty()).then(|| (name, w.screen_rect(s).center()))
                })
                .collect()
        };
        let centre = |r: Rect| Space::Center.to_screen(s, r).center();
        if self.shell.confirm.is_some() {
            return from(self.confirm_widgets_pub());
        }
        let sub = matches!(self.screen, Screen::Menu | Screen::Load | Screen::Settings | Screen::Help);
        if self.shell.pause && !sub {
            return from(self.pause_widgets());
        }
        let mut out = from(self.screen_widgets());
        let button = |b: &ui::Button| (b.label.to_string(), b.rect().center());
        match self.screen {
            Screen::Menu => {
                for (i, item) in self.menu_items().iter().enumerate() {
                    out.push((item.words().0.to_string(), centre(Self::menu_rect(i))));
                }
            }
            Screen::Title => {
                out.extend([button(&super::TITLE_SPIN), button(&super::TITLE_SKIP), button(&super::TITLE_WATCH)]);
                for (i, name) in ["world", "difficulty", "length"].into_iter().enumerate() {
                    out.push((name.to_string(), super::title_chip(i).center()));
                }
                for (i, m) in self.data.maps.iter().enumerate() {
                    out.push((m.name.clone(), super::map_card_rect(i).center()));
                }
            }
            Screen::Wheel => out.extend([button(&super::WHEEL_GO), button(&super::WHEEL_SKIP), ("wheel".to_string(), super::WHEEL_AREA.center())]),
            Screen::Round(_) => out.extend([button(&super::CARD_NEXT), button(&super::CARD_SKIP)]),
            Screen::Final => out.push(button(&super::FINAL_GO)),
            Screen::Rivals if self.inspect.is_some() => out.push(button(&super::INSPECT_BACK)),
            Screen::Rivals => {
                out.push(button(&super::RIVALS_GO));
                out.push(("rival 1".to_string(), super::rival_rect(0, self.rivals.len()).center()));
            }
            Screen::Load => {
                for i in 0..self.shell.slots.len().min(9) {
                    out.push((format!("save {i}"), centre(Self::load_rect(i))));
                }
            }
            Screen::Settings if !self.shell.keys_page => {
                for (i, name) in Self::set_names().into_iter().enumerate() {
                    let r = Self::set_rect(i);
                    out.push((format!("{name} +"), centre(Rect::new(r.right() - 60.0, r.y, 60.0, r.h))));
                    out.push((format!("{name} -"), centre(Rect::new(r.x + 240.0, r.y, 40.0, r.h))));
                }
            }
            Screen::Settings | Screen::Help => {}
            Screen::Outcome => {
                if self.fate.outcome.is_some_and(|o| o.can_play_on()) {
                    out.push(("PLAY ON".to_string(), centre(super::shell::OUTCOME_PLAY)));
                }
                out.push(("MAIN MENU".to_string(), centre(super::shell::OUTCOME_MENU)));
            }
            Screen::Map => {
                if self.notes.log_open {
                    return from(self.log_widgets());
                }
                if self.map_ui.card.is_some() {
                    return vec![button(&super::INSPECT_BACK)];
                }
                if self.map_ui.war.full.is_some() {
                    return vec![("report".to_string(), vec2(ui::W / 2.0, ui::H / 2.0))];
                }
                let window = if self.map_ui.econ.open.is_some() {
                    self.econ_widgets()
                } else if self.map_ui.war.open.is_some() {
                    self.war_widgets()
                } else if self.map_ui.diplo.open.is_some() {
                    self.diplo_widgets()
                } else if self.map_ui.diplo.spies.is_some() {
                    self.spy_widgets()
                } else {
                    Vec::new()
                };
                if !window.is_empty() {
                    return from(window);
                }
                if self.map_ui.menu.is_some() {
                    return from(self.menu_widgets());
                }
                let mut hud = self.hud_widgets();
                hud.extend(self.details_widgets());
                out.extend(from(hud));
            }
        }
        out
    }
}
