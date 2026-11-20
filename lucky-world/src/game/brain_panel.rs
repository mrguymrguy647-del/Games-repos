//! Spectator mode's brain panel (docs/AI.md, section 10): what the NPC
//! brain of the clicked nation is thinking. Its goal and why, how urgent
//! each need is, the three best options it weighed with their scores and
//! reasons, what it did, and what it makes of its neighbours.

use duckforge::prelude::*;

use super::Lucky;
use crate::ai::data::NEEDS;
use crate::ai::goal_name;
use crate::country;
use crate::ui::{self, GOLD, MUTED, SOFT};

/// Where the panel sits (right of the map, above the minimap).
pub const BRAIN_PANEL: Rect = Rect { x: 828.0, y: 64.0, w: 378.0, h: 440.0 };

const BAR_GOOD: Color = Color::rgb(107, 212, 138);
const BAR_WARN: Color = Color::rgb(245, 197, 66);
const BAR_BAD: Color = Color::rgb(240, 106, 106);

fn small(canvas: &mut Canvas, font: &Font, words: &str, at: Vec2, size: f32, color: Color, wrap: f32) -> f32 {
    let mut s = ui::style(size, color);
    s.outline = 0.0;
    s.shadow = None;
    s.wrap = Some(wrap);
    canvas.draw_font_text(font, words, at, &s).h
}

impl Lucky {
    pub(super) fn draw_brain_panel(&self, canvas: &mut Canvas, nation: usize) {
        let (Some(brains), Some(world), Some(e)) = (self.brains.as_ref(), self.world.as_ref(), self.economy.as_ref()) else { return };
        let Some(brain) = brains.brain(nation) else { return };
        let a = &self.assets;
        let r = BRAIN_PANEL;
        canvas.fill_rect(r, super::gui::NAVY);
        canvas.draw_rect(r, super::gui::BRONZE);
        canvas.label("brain panel", r);
        let (x, w) = (r.x + 14.0, r.w - 28.0);
        let mut y = r.y + 10.0;

        // Who, and what kind of leader.
        country::draw_flag(canvas, Rect::new(x, y + 4.0, 36.0, 24.0), &world.nations[nation].flag);
        ui::plain(canvas, &a.font, "WHAT ITS BRAIN IS THINKING", vec2(x + 46.0, y), 11.0, GOLD, Align::Left);
        ui::text(canvas, &a.serif, &world.nations[nation].name, vec2(x + 46.0, y + 12.0), 18.0, Color::WHITE, Align::Left, w - 46.0);
        y += 36.0;
        let who = format!("{}  ·  spies: {}  ·  {}", brain.personality.labels.join(" · "), brain.secret_service, brains.difficulty_name);
        ui::plain(canvas, &a.font, &who, vec2(x, y), 12.0, SOFT, Align::Left);
        y += 18.0;

        // The goal.
        ui::plain(canvas, &a.font, "GOAL", vec2(x, y + 3.0), 11.0, SOFT, Align::Left);
        ui::text(canvas, &a.font, &goal_name(brain.goal).to_uppercase(), vec2(x + 40.0, y - 1.0), 18.0, GOLD, Align::Left, 120.0);
        y += 22.0;
        y += small(canvas, &a.font, &brain.goal_why, vec2(x, y), 13.0, MUTED, w) + 6.0;

        // The needs, as bars.
        let cols = 3;
        let cell = w / cols as f32;
        for (c, name) in NEEDS.iter().enumerate() {
            let (cx, cy) = (x + (c % cols) as f32 * cell, y + (c / cols) as f32 * 19.0);
            let v = brain.needs[c].clamp(0.0, 1.0) as f32;
            ui::plain(canvas, &a.font, name, vec2(cx, cy), 11.0, Color::WHITE, Align::Left);
            let bar = Rect::new(cx + 50.0, cy + 3.0, cell - 58.0, 8.0);
            canvas.fill_rect(bar, Color::rgba(255, 255, 255, 30));
            let color = if v > 0.6 { BAR_BAD } else if v > 0.25 { BAR_WARN } else { BAR_GOOD };
            canvas.fill_rect(Rect::new(bar.x, bar.y, bar.w * v.max(0.02), bar.h), color);
        }
        y += 3.0 * 19.0 + 6.0;

        // The best three options it weighed, with scores and reasons.
        let t = &brain.thinking;
        ui::plain(canvas, &a.font, &format!("TOP OPTIONS ({}, hold still = {:.2})", e.date(&self.econ_data), brains.data.hold_score), vec2(x, y), 11.0, GOLD, Align::Left);
        y += 16.0;
        if t.options.is_empty() {
            y += small(canvas, &a.font, "It hasn't planned yet.", vec2(x, y), 13.0, MUTED, w);
        }
        for (k, o) in t.options.iter().take(3).enumerate() {
            let color = if o.score > brains.data.hold_score { Color::WHITE } else { SOFT };
            ui::plain(canvas, &a.font, &format!("{}.", k + 1), vec2(x, y), 14.0, GOLD, Align::Left);
            ui::text(canvas, &a.font, &o.label, vec2(x + 18.0, y), 14.0, color, Align::Left, w - 76.0);
            ui::plain(canvas, &a.font, &format!("{:+.3}", o.score), vec2(x + w, y), 14.0, if o.score > brains.data.hold_score { BAR_GOOD } else { SOFT }, Align::Right);
            y += 18.0;
            if !o.reasons.is_empty() {
                y += small(canvas, &a.font, &o.reasons.join("; "), vec2(x + 18.0, y), 11.5, MUTED, w - 18.0) + 3.0;
            }
        }
        y += 4.0;

        // What it did.
        ui::plain(canvas, &a.font, "IT DID", vec2(x, y), 11.0, GOLD, Align::Left);
        y += 15.0;
        if t.taken.is_empty() {
            y += small(canvas, &a.font, "Held still: nothing beat doing nothing.", vec2(x, y), 12.0, MUTED, w);
        }
        for d in t.taken.iter().take(3) {
            y += small(canvas, &a.font, d, vec2(x, y), 12.0, Color::WHITE, w) + 1.0;
        }
        y += 6.0;

        // Its world view: threat, who it plans against, who looks weak.
        let name = |j: usize| world.nations[j].name.clone();
        let threat = match brain.threat {
            Some(j) if brain.threat_level > 0.0 => format!("Biggest threat: {} ({:.0}%)", name(j), brain.threat_level * 100.0),
            Some(j) => format!("Biggest threat: {} (not worried)", name(j)),
            None => "No threats".to_string(),
        };
        y += small(canvas, &a.font, &threat, vec2(x, y), 12.0, Color::WHITE, w) + 1.0;
        if let Some(c) = brain.counter {
            let why = if brain.hegemon == Some(c) { "too strong for anyone's comfort" } else { "strong and hostile" };
            y += small(canvas, &a.font, &format!("Plans against: {} ({why})", name(c)), vec2(x, y), 12.0, BAR_WARN, w) + 1.0;
        }
        if let Some(o) = brain.opportunities.first() {
            let words = format!("Sees weakness in: {} ({})", name(o.target), o.reasons.join(", "));
            y += small(canvas, &a.font, &words, vec2(x, y), 12.0, BAR_BAD, w) + 1.0;
        }

        // Its memory: who it trusts least and most, and the last thing they did.
        let mut by: Vec<usize> = (0..brain.relations.len()).filter(|&j| j != nation && !brain.relations[j].log.is_empty()).collect();
        by.sort_by(|&p, &q| brain.relations[p].regard.total_cmp(&brain.relations[q].regard).then(p.cmp(&q)));
        let remember = |j: usize| {
            let r = &brain.relations[j];
            let last = r.log.last().map_or("", |m| m.kind.words());
            format!("{} ({:+.2}: {last})", name(j), r.regard)
        };
        if !by.is_empty() && y < r.bottom() - 40.0 {
            y += 4.0;
            ui::plain(canvas, &a.font, "IT REMEMBERS", vec2(x, y), 11.0, GOLD, Align::Left);
            y += 15.0;
            let least = by[0];
            y += small(canvas, &a.font, &format!("Trusts least: {}", remember(least)), vec2(x, y), 12.0, Color::WHITE, w) + 1.0;
            if let Some(&most) = by.last().filter(|&&m| m != least) {
                if y < r.bottom() - 20.0 {
                    small(canvas, &a.font, &format!("Trusts most: {}", remember(most)), vec2(x, y), 12.0, Color::WHITE, w);
                }
            }
        }
    }
}
