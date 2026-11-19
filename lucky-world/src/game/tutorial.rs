//! The tutorial: a short guided game on the Grand Continent, at easy. Each
//! step says what to do and waits until it's done: spinning, the map, the
//! clock, the economy, supply in war, diplomacy, then saving. Enter skips a
//! step, Backspace ends the tutorial; the game goes on afterwards.

use duckforge::prelude::*;

use super::{Lucky, Screen};
use crate::ui::{self, GOLD, MUTED, SOFT};

#[derive(Debug, Clone, PartialEq)]
pub struct Tutorial {
    pub step: usize,
    /// Seconds on this step (some steps wait a moment, then go on).
    pub time: f32,
}

/// What a step teaches, and when it's done.
pub struct Step {
    pub title: &'static str,
    pub words: &'static str,
    pub done: fn(&Lucky) -> bool,
}

pub const STEPS: &[Step] = &[
    Step {
        title: "1. Your fate on sixteen wheels",
        words: "In LUCKY WORLD you don't choose your nation: you spin for it. Sixteen wheels decide where you live, how many you are, how rich, how armed, who leads you and what luck follows you. The tutorial's world is the Grand Continent. Press SPACE to start spinning.",
        done: |g| g.screen != Screen::Title,
    },
    Step {
        title: "2. Spinning a wheel",
        words: "Each slice's size IS its chance (the panel lists the odds, and which earlier results shaped them). SPACE spins; SPACE again speeds it up, then goes to the next wheel. Spin two wheels.",
        done: |g| g.landed.len() >= 2 || matches!(g.screen, Screen::Final | Screen::Rivals | Screen::Map),
    },
    Step {
        title: "3. The rest at once",
        words: "A round card sums up every four wheels. When you know how it works, SKIP SPIN (S) rolls every wheel left at once, with the same odds. Press S now.",
        done: |g| matches!(g.screen, Screen::Final | Screen::Rivals | Screen::Map),
    },
    Step {
        title: "4. Your country card",
        words: "This is your nation. Every result counts: GDP, army and resources start your economy and war; ideology and leader shape your diplomacy; the blessing or curse lasts all game. A very weak start gets the underdog's luck (aid, and five years in which no brain attacks it). SPACE: meet your rivals.",
        done: |g| matches!(g.screen, Screen::Rivals | Screen::Map),
    },
    Step {
        title: "5. Your rivals",
        words: "Your rivals spun the same wheels, and more nations filled the empty land. Click any card to read it. SPACE (or BEGIN): onto the world map.",
        done: |g| g.screen == Screen::Map,
    },
    Step {
        title: "6. The world map",
        words: "Drag to pan, scroll (or R / F) to zoom, right-drag (or Q / E) to turn. Your nation's name is in gold. H takes you home. Click one of YOUR provinces to see what it holds.",
        done: |g| g.map_ui.selected.is_some_and(|p| g.world.as_ref().is_some_and(|w| w.owner[p] == Some(0))),
    },
    Step {
        title: "7. Time",
        words: "The clock runs: one tick is one day, and the whole world lives through it. SPACE pauses, 1 to 5 set the speed. Press 3 for a week a second.",
        done: |g| g.clock.speed >= 2 && !g.clock.paused,
    },
    Step {
        title: "8. The economy",
        words: "Press O: the economy window. Its first page is your BUDGET: taxes in, the army, welfare and building out. Hover any number to read what it is and what changes it.",
        done: |g| g.map_ui.econ.open.is_some(),
    },
    Step {
        title: "9. A lever",
        words: "Choose Income tax with Up / Down and raise it with Right (or click +). Higher taxes fill the treasury but slow growth and anger people. Your autopilot runs whatever you don't touch (A gives a lever back).",
        done: |g| g.toast.as_ref().is_some_and(|(t, _)| t.starts_with("Income tax")),
    },
    Step {
        title: "10. Chains",
        words: "Tab to CHAINS: ore and coal make steel, steel makes tanks. Break a link and everything after it stops. Then close the window (O or Esc).",
        done: |g| g.map_ui.econ.open.is_none(),
    },
    Step {
        title: "11. War and supply",
        words: "Press K: the war room. Wars are won by supply as much as soldiers: every round and shell comes from your factories. Tab to the SUPPLY page, where your generals ask for what they need.",
        done: |g| g.map_ui.war.open == Some(super::war_screen::Page::Supply),
    },
    Step {
        title: "12. Fronts and requests",
        words: "Left / Right set a front's priority; Y / N answer a general's request; T leaves it all on Auto. You never have to command the armies yourself. Close the war room (K or Esc).",
        done: |g| g.map_ui.war.open.is_none(),
    },
    Step {
        title: "13. Diplomacy",
        words: "Press P: diplomacy. Every nation's view of you, and WHY (trade, borders, ideology, old wars). Pick a friendly nation with Up / Down and press Enter to propose a treaty to it.",
        done: |g| g.map_ui.diplo.open == Some(super::diplo_screen::Page::Propose) || g.map_ui.diplo.message.is_some(),
    },
    Step {
        title: "14. A proposal",
        words: "Choose the treaty with Left / Right (a trade agreement is an easy start) and press Enter. They answer with a reason. Then close the window (P or Esc).",
        done: |g| g.map_ui.diplo.open.is_none() && g.map_ui.diplo.message.is_some(),
    },
    Step {
        title: "15. How to win",
        words: "Win by conquest (half the map), economic power (2.5 times the next economy for 10 years), diplomacy (lead an alliance holding 60% of the world for 5 years) or the best score when time runs out. Lose if you're conquered, overthrown, or made a vassal. Esc pauses and saves; F2 is the help. The tutorial is over: the world is yours! (Enter)",
        done: |_| false,
    },
];

impl Tutorial {
    pub fn at(step: usize) -> Tutorial {
        Tutorial { step: step.min(STEPS.len() - 1), time: 0.0 }
    }
}

impl Lucky {
    /// The tutorial game: the Grand Continent at easy, no time limit.
    pub(super) fn start_tutorial(&mut self, ctx: &mut Context) {
        self.restart();
        self.map = 0;
        self.random_map = false;
        self.make_title_wheel();
        self.setup = crate::save::Setup { difficulty: "easy".to_string(), years: 0, tutorial: true };
        self.tutorial = Some(Tutorial::at(0));
        self.settings.tutorial_offered = true;
        let _ = self.settings.save();
        crate::ui::sfx(ctx, &self.assets.card_sound, 0.8, 0.0);
    }

    pub(super) fn update_tutorial(&mut self, ctx: &mut Context) {
        let Some(t) = self.tutorial.as_ref() else { return };
        let step = t.step;
        let last = step + 1 >= STEPS.len();
        let mut next = (STEPS[step].done)(self) && t.time > 0.4;
        // Enter skips a step (not where Enter does something on the map).
        let windows = self.map_ui.econ.open.is_some() || self.map_ui.war.open.is_some() || self.map_ui.diplo.wants_escape();
        if ctx.input.was_pressed(Key::Enter) && (!windows || last) && self.screen == Screen::Map {
            next = true;
        }
        if ctx.input.was_pressed(Key::Backspace) {
            self.tutorial = None;
            self.notify(ctx, "Tutorial ended. F2: help, Esc: pause and save".to_string(), super::shell::CRITICAL);
            return;
        }
        let t = self.tutorial.as_mut().expect("still here");
        t.time += ctx.dt();
        if next {
            if last {
                self.tutorial = None;
                self.notify(ctx, "Tutorial complete! The game goes on: grow, trade, ally, and win".to_string(), super::shell::CRITICAL);
                crate::ui::sfx(ctx, &self.assets.fanfare, 0.7, 0.0);
            } else {
                t.step += 1;
                t.time = 0.0;
                crate::ui::sfx(ctx, &self.assets.land, 0.6, 0.0);
            }
        }
        ctx.report("tutorial_step", self.tutorial.as_ref().map_or(-1.0, |t| t.step as f64));
    }

    pub(super) fn draw_tutorial(&self, canvas: &mut Canvas) {
        let Some(t) = self.tutorial.as_ref() else { return };
        let a = &self.assets;
        let s = &STEPS[t.step];
        // Out of the way of each screen's own buttons (over a window's last line).
        let window = self.map_ui.econ.open.is_some() || self.map_ui.war.open.is_some() || self.map_ui.diplo.wants_escape();
        let y = match self.screen {
            Screen::Map if window => 596.0,
            Screen::Map => 486.0,
            Screen::Title => 120.0,
            Screen::Wheel => 560.0,
            _ => 8.0,
        };
        let x = if self.screen == Screen::Wheel { 20.0 } else { 300.0 };
        let r = Rect::new(x, y, 680.0, if window { 118.0 } else { 132.0 });
        let pulse = 0.5 + 0.5 * (self.time * 3.0).sin();
        canvas.fill_rect(r, Color::rgba(14, 8, 40, 240));
        canvas.draw_rect(r, ui::alpha(GOLD, 0.6 + 0.4 * pulse));
        canvas.draw_rect(Rect::new(r.x + 2.0, r.y + 2.0, r.w - 4.0, r.h - 4.0), ui::alpha(GOLD, 0.3));
        canvas.blit_sprite(&a.icon_book, vec2(r.x + 14.0, r.y + 12.0), Blit::default());
        ui::plain(canvas, &a.font, &format!("TUTORIAL · {}", s.title.to_uppercase()), vec2(r.x + 46.0, r.y + 12.0), 15.0, GOLD, Align::Left);
        ui::plain(canvas, &a.font, &format!("{} / {}", t.step + 1, STEPS.len()), vec2(r.right() - 14.0, r.y + 12.0), 13.0, SOFT, Align::Right);
        let mut st = ui::style(14.0, Color::WHITE);
        st.outline = 0.0;
        st.shadow = None;
        st.wrap = Some(r.w - 32.0);
        canvas.draw_font_text(&a.font, s.words, vec2(r.x + 16.0, r.y + 38.0), &st);
        ui::plain(canvas, &a.font, "Enter: skip this step (on the map)  ·  Backspace: end the tutorial", vec2(r.x + 16.0, r.bottom() - 20.0), 11.0, MUTED, Align::Left);
        canvas.label("tutorial", r);
    }
}
