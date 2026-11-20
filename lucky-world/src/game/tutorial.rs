//! The tutorial: a short guided game on the Grand Continent, at easy. Each
//! step says what to do and waits until it's done: spinning, the map, the
//! clock, the economy, supply in war, diplomacy, then saving. Enter skips a
//! step, Backspace ends the tutorial; the game goes on afterwards.

use duckforge::prelude::*;

use super::{Lucky, Screen};
use crate::ui::{self, GOLD};

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
        title: "1. Your fate on the wheels",
        words: "In LUCKY WORLD you don't choose your nation: you spin for it. The wheels decide where you live, how many you are, how rich, how armed, who leads you and what luck follows you. The tutorial's world is the Grand Continent. Click Spin the wheels to begin.",
        done: |g| g.screen != Screen::Title,
    },
    Step {
        title: "2. Spinning a wheel",
        words: "Each slice's size IS its chance (the list beside the wheel gives every slice's odds). A wheel waits for you: click Spin, or click the wheel itself. Click again to hurry it; when it lands, click again for the next wheel. Spin two wheels.",
        done: |g| g.landed.len() >= 2 || matches!(g.screen, Screen::Final | Screen::Rivals | Screen::Map),
    },
    Step {
        title: "3. The rest at once",
        words: "A round card sums up each round of wheels. When you know how it works, Skip spin rolls every wheel left at once, with the same odds. Click Skip spin now.",
        done: |g| matches!(g.screen, Screen::Final | Screen::Rivals | Screen::Map),
    },
    Step {
        title: "4. Your country card",
        words: "This is your nation. Every result counts: GDP, army and resources start your economy and war; ideology and leader shape your diplomacy; your blessing and your curse last all game. A very weak start gets the underdog's luck (aid, and five years in which no brain attacks it). Click Meet your rivals.",
        done: |g| matches!(g.screen, Screen::Rivals | Screen::Map),
    },
    Step {
        title: "5. Your rivals",
        words: "Your rivals spun the same wheels, and more nations filled the empty land. Click any card to read it. Click Begin to go onto the world map.",
        done: |g| g.screen == Screen::Map,
    },
    Step {
        title: "6. The world map",
        words: "Drag with the left button to move the map, roll the mouse wheel to zoom, drag with the right button to turn it. Your nation has a gold border; Go home brings you back. Click one of YOUR provinces to see what it holds.",
        done: |g| g.map_ui.selected.is_some_and(|p| g.world.as_ref().is_some_and(|w| w.owner[p] == Some(0))),
    },
    Step {
        title: "7. Time",
        words: "The clock runs: one tick is one day, and the whole world lives through it. The buttons at the top right stop it and set its speed. Click 3 for a week a second.",
        done: |g| g.clock.speed >= 2 && !g.clock.paused,
    },
    Step {
        title: "8. The economy",
        words: "Click Economy on the right. Its first page is your BUDGET: taxes in, the army, welfare and building out. Hover any number to read what it is and what changes it.",
        done: |g| g.map_ui.econ.open.is_some(),
    },
    Step {
        title: "9. A lever",
        words: "Drag the Income tax slider to the right, or click its +. Higher taxes fill the treasury but slow growth and anger people. Your autopilot runs whatever you don't touch (Hand back to the autopilot gives a lever back).",
        done: |g| g.toast.as_ref().is_some_and(|(t, _)| t.starts_with("Income tax")),
    },
    Step {
        title: "10. Chains",
        words: "Click the Chains tab: ore and coal make steel, steel makes tanks. Break a link and everything after it stops. Then close the window: its X, or a click outside it.",
        done: |g| g.map_ui.econ.open.is_none(),
    },
    Step {
        title: "11. War and supply",
        words: "Click War room on the right. Wars are won by supply as much as soldiers: every round and shell comes from your factories. Click the Supply tab, where your generals ask for what they need.",
        done: |g| g.map_ui.war.open == Some(super::war_screen::Page::Supply),
    },
    Step {
        title: "12. Fronts and requests",
        words: "The buttons along the bottom set a front's priority and answer a general's request; Auto leaves it all to them. You never have to command the armies yourself. Close the war room.",
        done: |g| g.map_ui.war.open.is_none(),
    },
    Step {
        title: "13. Diplomacy",
        words: "Click Diplomacy on the right. Every nation's view of you, and WHY (trade, borders, ideology, old wars). Click a friendly nation in the list, then a treaty button along the bottom to propose it.",
        done: |g| g.map_ui.diplo.open == Some(super::diplo_screen::Page::Propose) || g.map_ui.diplo.message.is_some(),
    },
    Step {
        title: "14. A proposal",
        words: "A trade agreement is an easy start. They answer with a reason. Then close the window.",
        done: |g| g.map_ui.diplo.open.is_none() && g.map_ui.diplo.message.is_some(),
    },
    Step {
        title: "15. How to win",
        words: "Win by conquest (half the map), economic power (2.5 times the next economy for 10 years), diplomacy (lead an alliance holding 60% of the world for 5 years) or the best score when time runs out. Lose if you're conquered, overthrown, or made a vassal. Menu, at the top right, pauses and saves, and has How to play. The tutorial is over: the world is yours!",
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
        if crate::game::gui::pressed(ctx, Key::Enter) && (!windows || last) && self.screen == Screen::Map {
            next = true;
        }
        if std::mem::take(&mut self.tutorial_skip) {
            next = true;
        }
        if crate::game::gui::pressed(ctx, Key::Backspace) || std::mem::take(&mut self.tutorial_end) {
            self.tutorial = None;
            self.notify(ctx, "Tutorial ended. Menu (top right): help, save and settings".to_string(), super::shell::CRITICAL);
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

    /// Where the tutorial's card sits on this screen (None: no tutorial).
    /// Under an open window it is a strip along the bottom.
    pub(super) fn tutorial_rect(&self) -> Option<Rect> {
        self.tutorial.as_ref()?;
        let window = self.screen == Screen::Map && self.map_ui.window_open();
        Some(match self.screen {
            Screen::Map if window => Rect::new(110.0, 666.0, 1060.0, 52.0),
            Screen::Map => Rect::new(300.0, 500.0, 680.0, 140.0),
            Screen::Title => Rect::new(300.0, 118.0, 680.0, 140.0),
            Screen::Wheel => Rect::new(16.0, 572.0, 640.0, 140.0),
            Screen::Menu | Screen::Load | Screen::Settings | Screen::Help | Screen::Outcome => return None,
            _ => Rect::new(300.0, 6.0, 680.0, 140.0),
        })
    }

    pub(super) fn draw_tutorial(&self, canvas: &mut Canvas) {
        let (Some(t), Some(r)) = (self.tutorial.as_ref(), self.tutorial_rect()) else { return };
        let a = &self.assets;
        let s = &STEPS[t.step.min(STEPS.len() - 1)];
        let strip = r.h < 100.0;
        super::gui::panel_gold(canvas, a, r);
        super::gui::icon(canvas, a, "help", vec2(r.x + 12.0, r.y + 9.0), 18.0, GOLD);
        let head = format!("TUTORIAL {} OF {} · {}", t.step + 1, STEPS.len(), s.title.to_uppercase());
        ui::plain(canvas, &a.bold, &head, vec2(r.x + 38.0, r.y + 10.0), 13.0, GOLD, Align::Left);
        let style = TextStyle { size: if strip { 13.0 } else { 15.0 }, color: super::gui::IVORY, wrap: Some(r.w - if strip { 290.0 } else { 32.0 }), ..TextStyle::default() };
        let at = if strip { vec2(r.x + 16.0, r.y + 28.0) } else { vec2(r.x + 16.0, r.y + 34.0) };
        if strip {
            // One line under an open window: the step's first sentence or two.
            let short: String = s.words.split_inclusive(". ").take(2).collect();
            super::gui::fit(canvas, &a.font, short.trim(), at, 13.0, super::gui::IVORY, Align::Left, r.w - 290.0);
        } else {
            canvas.draw_font_text(&a.font, s.words, at, &style);
        }
        canvas.label("tutorial", r);
    }
}
