//! One look and one way of clicking for every screen.
//!
//! - **The look**: dark navy panels with a thin bronze border, ivory text,
//!   grey labels and a gold accent (`panel`, the colours below).
//! - **Widgets**: a screen describes what can be clicked as a list of
//!   `Widget`s (a rectangle, a kind, a label, what it does, a tooltip). The
//!   same list is used to draw them and to find the one under the mouse, so
//!   what you see is what you can click.
//! - **Mouse first**: a widget's action is usually a key (`Act::Key`): a
//!   click "presses" that key for one frame (`press`, `pressed`), so every
//!   keyboard shortcut is also a button, and the two can never drift apart.
//! - **Spaces**: a widget sits in a `Space` (a corner or the middle of the
//!   screen); the UI scale grows it round that anchor, so panels stay on
//!   their edge of the screen at every scale.

use std::cell::Cell;

use duckforge::prelude::*;

use super::cmd::Cmd;
use crate::ui::{self, Assets};

pub const NAVY: Color = Color::rgba(13, 23, 34, 243);
pub const NAVY_SOLID: Color = Color::from_hex(0x0d1722);
pub const NAVY_HI: Color = Color::from_hex(0x182b3f);
pub const NAVY_ROW: Color = Color::rgba(233, 226, 207, 14);
pub const BRONZE: Color = Color::from_hex(0x8a6d2f);
pub const IVORY: Color = Color::from_hex(0xe9e2cf);
pub const LABEL: Color = Color::from_hex(0xa9b3bd);
pub const DIM: Color = Color::from_hex(0x6c7884);
pub const GOLD: Color = Color::from_hex(0xe2b84a);
pub const GOLD_INK: Color = Color::from_hex(0x1c1508);
pub const RED: Color = Color::from_hex(0xd8574a);
pub const GREEN: Color = Color::from_hex(0x7bc47f);

thread_local! {
    /// The key a click "pressed": (for the next frame, for this frame).
    static VKEY: Cell<(Option<Key>, Option<Key>)> = const { Cell::new((None, None)) };
    /// Real key presses seen this frame are counted (the mouse-only test checks for zero).
    static REAL_KEYS: Cell<u64> = const { Cell::new(0) };
}

/// Each frame: last frame's click becomes this frame's key press.
pub fn begin_frame() {
    VKEY.with(|v| {
        let (next, _) = v.get();
        v.set((None, next));
    });
}

/// A click presses `key` (seen by `pressed` on the next frame).
pub fn press(key: Key) {
    VKEY.with(|v| {
        let (_, now) = v.get();
        v.set((Some(key), now));
    });
}

/// Alt+Enter was pressed (fullscreen); `pressed(Enter)` is false that frame.
pub fn alt_enter(ctx: &Context) -> bool {
    (ctx.input.is_down(Key::LeftAlt) || ctx.input.is_down(Key::RightAlt)) && ctx.input.was_pressed(Key::Enter)
}

/// Whether `key` was pressed this frame, on the keyboard or by a click.
pub fn pressed(ctx: &Context, key: Key) -> bool {
    if key == Key::Enter && alt_enter(ctx) {
        return false;
    }
    let real = ctx.input.was_pressed(key);
    if real {
        REAL_KEYS.with(|k| k.set(k.get() + 1));
    }
    real || VKEY.with(|v| v.get().1 == Some(key))
}

/// How many real key presses the game has acted on (on this thread).
pub fn real_keys() -> u64 {
    REAL_KEYS.with(|k| k.get())
}

/// Where a widget is anchored: the UI scale grows it round this point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Space {
    /// The middle of the screen (windows and menus).
    #[default]
    Center,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    BottomCenter,
    /// Screen pixels, never scaled (things pinned to the 3D map).
    Screen,
}

impl Space {
    pub fn anchor(self) -> Vec2 {
        match self {
            Space::Center => vec2(ui::W / 2.0, ui::H / 2.0),
            Space::TopLeft | Space::Screen => Vec2::ZERO,
            Space::TopRight => vec2(ui::W, 0.0),
            Space::BottomLeft => vec2(0.0, ui::H),
            Space::BottomRight => vec2(ui::W, ui::H),
            Space::BottomCenter => vec2(ui::W / 2.0, ui::H),
        }
    }

    fn scale(self, s: f32) -> f32 {
        if self == Space::Screen { 1.0 } else { s }
    }

    /// A screen point in this space's own coordinates.
    pub fn to_local(self, s: f32, p: Vec2) -> Vec2 {
        let a = self.anchor();
        a + (p - a) * (1.0 / self.scale(s))
    }

    /// A rectangle of this space, on the screen.
    pub fn to_screen(self, s: f32, r: Rect) -> Rect {
        let (a, k) = (self.anchor(), self.scale(s));
        Rect::new(a.x + (r.x - a.x) * k, a.y + (r.y - a.y) * k, r.w * k, r.h * k)
    }

    /// Draw in this space until `canvas.pop_camera()`.
    pub fn push(self, canvas: &mut Canvas, s: f32) {
        let (a, k) = (self.anchor(), self.scale(s));
        let c = vec2(ui::W / 2.0, ui::H / 2.0);
        let mut cam = Camera2D::new(ui::W, ui::H);
        cam.zoom = k;
        // The anchor stays where it is: (a - position) * k + c = a.
        cam.position = a - (a - c) * (1.0 / k);
        canvas.push_camera(&cam);
    }
}

/// How a widget looks and behaves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// A framed button with its label in the middle.
    Button,
    /// A page tab (gold underline when on).
    Tab,
    /// A list row: lights up under the mouse, gold bar when chosen.
    Row,
    /// A round icon button with its label under it (the right rail).
    Round,
    /// Icon, small label, big value (the top bar).
    Stat,
    /// A small square icon button.
    Icon,
    /// A slider with its knob at this share (0..1) of the track.
    Slider(f32),
    /// A line of a pop-up menu.
    Menu,
    /// Nothing drawn: a clickable or click-blocking area (may have a tooltip).
    Ghost,
}

/// What a click does.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    /// Nothing (the widget only blocks the map under it, or shows a tooltip).
    None,
    /// Presses this key: the shortcut and the button are one thing.
    Key(Key),
    /// A command with no key of its own.
    Cmd(Cmd),
    /// Chooses row `n` of the open list.
    Row(usize),
    /// A slider was dragged: (which, its new share 0..1).
    Slide(u8, f32),
}

#[derive(Debug, Clone)]
pub struct Widget {
    pub space: Space,
    pub rect: Rect,
    pub kind: Kind,
    pub label: String,
    /// Small print: a stat's label, a round button's caption, a row's right side.
    pub sub: String,
    pub act: Act,
    pub tip: String,
    /// Chosen / active: drawn in gold.
    pub on: bool,
    /// Can't be used now: greyed out, clicks do nothing.
    pub off: bool,
    /// A warning action (declare war, quit): drawn in red.
    pub danger: bool,
    pub icon: Option<&'static str>,
    pub badge: usize,
}

impl Widget {
    pub fn new(kind: Kind, rect: Rect, label: impl Into<String>, act: Act) -> Widget {
        Widget { space: Space::Center, rect, kind, label: label.into(), sub: String::new(), act, tip: String::new(), on: false, off: false, danger: false, icon: None, badge: 0 }
    }

    pub fn button(rect: Rect, label: impl Into<String>, act: Act) -> Widget {
        Widget::new(Kind::Button, rect, label, act)
    }

    /// A button that presses `key` (its shortcut).
    pub fn key(rect: Rect, label: impl Into<String>, key: Key) -> Widget {
        Widget::new(Kind::Button, rect, label, Act::Key(key))
    }

    pub fn cmd(rect: Rect, label: impl Into<String>, cmd: Cmd) -> Widget {
        Widget::new(Kind::Button, rect, label, Act::Cmd(cmd))
    }

    pub fn ghost(rect: Rect) -> Widget {
        Widget::new(Kind::Ghost, rect, "", Act::None)
    }

    pub fn kind(mut self, k: Kind) -> Widget {
        self.kind = k;
        self
    }

    pub fn space(mut self, s: Space) -> Widget {
        self.space = s;
        self
    }

    pub fn tip(mut self, t: impl Into<String>) -> Widget {
        self.tip = t.into();
        self
    }

    pub fn sub(mut self, t: impl Into<String>) -> Widget {
        self.sub = t.into();
        self
    }

    pub fn on(mut self, on: bool) -> Widget {
        self.on = on;
        self
    }

    pub fn off(mut self, off: bool) -> Widget {
        self.off = off;
        self
    }

    pub fn danger(mut self) -> Widget {
        self.danger = true;
        self
    }

    pub fn icon(mut self, name: &'static str) -> Widget {
        self.icon = Some(name);
        self
    }

    pub fn badge(mut self, n: usize) -> Widget {
        self.badge = n;
        self
    }

    /// The tooltip with its shortcut ("... (shortcut: O)").
    pub fn tip_words(&self) -> String {
        let key = match &self.act {
            Act::Key(k) => Some(key_name(*k)),
            _ => None,
        };
        match (self.tip.is_empty(), key) {
            (true, None) => String::new(),
            (true, Some(k)) => format!("Shortcut: {k}"),
            (false, None) => self.tip.clone(),
            (false, Some(k)) => format!("{}  (shortcut: {k})", self.tip),
        }
    }

    pub fn screen_rect(&self, s: f32) -> Rect {
        self.space.to_screen(s, self.rect)
    }

    pub fn contains(&self, s: f32, mouse: Vec2) -> bool {
        self.rect.contains(self.space.to_local(s, mouse))
    }
}

/// A key's name as a player reads it.
pub fn key_name(k: Key) -> String {
    let n = k.name();
    match n.strip_prefix("Digit") {
        Some(d) => d.to_string(),
        None => match n.as_str() {
            "Escape" => "Esc".to_string(),
            _ => n,
        },
    }
}

/// What the mouse did to a list of widgets this frame.
#[derive(Debug, Default)]
pub struct Pointer {
    /// The slider being dragged.
    drag: Option<u8>,
    /// The last left click: where and when (for double clicks).
    last_click: Option<(Vec2, f32)>,
}

impl Pointer {
    /// The action of the widget clicked this frame (the topmost: the last in
    /// the list), or of the slider being dragged. `mouse` is in screen pixels.
    pub fn click(&mut self, ctx: &Context, mouse: Option<Vec2>, s: f32, widgets: &[Widget]) -> Option<Act> {
        let m = mouse?;
        let down = ctx.input.is_mouse_down(MouseButton::Left);
        let share = |w: &Widget| {
            let p = w.space.to_local(s, m);
            ((p.x - w.rect.x - 8.0) / (w.rect.w - 16.0).max(1.0)).clamp(0.0, 1.0)
        };
        if let Some(id) = self.drag {
            if !down {
                self.drag = None;
                return None;
            }
            let w = widgets.iter().find(|w| matches!(w.act, Act::Slide(i, _) if i == id))?;
            return Some(Act::Slide(id, share(w)));
        }
        if !ctx.input.was_mouse_pressed(MouseButton::Left) {
            return None;
        }
        let w = widgets.iter().rev().find(|w| w.contains(s, m))?;
        if w.off {
            return Some(Act::None);
        }
        match &w.act {
            Act::Slide(id, _) => {
                self.drag = Some(*id);
                Some(Act::Slide(*id, share(w)))
            }
            Act::Key(k) => {
                press(*k);
                Some(w.act.clone())
            }
            other => Some(other.clone()),
        }
    }

    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Whether this left click is the second of a double click.
    pub fn double(&mut self, at: Vec2, time: f32) -> bool {
        let twice = self.last_click.is_some_and(|(p, t)| time - t < 0.4 && p.distance(at) < 8.0);
        self.last_click = if twice { None } else { Some((at, time)) };
        twice
    }
}

/// Whether the mouse is over any of these widgets.
pub fn over(widgets: &[Widget], s: f32, mouse: Option<Vec2>) -> bool {
    mouse.is_some_and(|m| widgets.iter().any(|w| w.contains(s, m)))
}

/// A panel: navy, a thin bronze border, rounded corners (`gold`: a gold border).
pub fn panel(canvas: &mut Canvas, a: &Assets, r: Rect) {
    frame(canvas, a, r, NAVY, BRONZE, a.corner.as_ref());
}

pub fn panel_gold(canvas: &mut Canvas, a: &Assets, r: Rect) {
    frame(canvas, a, r, NAVY, GOLD, a.corner_gold.as_ref());
}

fn frame(canvas: &mut Canvas, _a: &Assets, r: Rect, fill: Color, edge: Color, corner: Option<&Sprite>) {
    let c = corner.map_or(0.0, |s| s.width() as f32).min(r.w / 2.0).min(r.h / 2.0);
    let Some(corner) = corner.filter(|_| c >= 2.0) else {
        canvas.fill_rect(r, fill);
        canvas.draw_rect(r, edge);
        return;
    };
    // The middle, the left and right columns between the corners.
    canvas.fill_rect(Rect::new(r.x + c, r.y, r.w - 2.0 * c, r.h), fill);
    canvas.fill_rect(Rect::new(r.x, r.y + c, c, r.h - 2.0 * c), fill);
    canvas.fill_rect(Rect::new(r.right() - c, r.y + c, c, r.h - 2.0 * c), fill);
    for (fx, fy) in [(false, false), (true, false), (false, true), (true, true)] {
        let at = vec2(if fx { r.right() - c } else { r.x }, if fy { r.bottom() - c } else { r.y });
        canvas.blit_sprite(corner, at, Blit { flip_x: fx, flip_y: fy, ..Blit::default() });
    }
    canvas.fill_rect(Rect::new(r.x + c, r.y, r.w - 2.0 * c, 1.0), edge);
    canvas.fill_rect(Rect::new(r.x + c, r.bottom() - 1.0, r.w - 2.0 * c, 1.0), edge);
    canvas.fill_rect(Rect::new(r.x, r.y + c, 1.0, r.h - 2.0 * c), edge);
    canvas.fill_rect(Rect::new(r.right() - 1.0, r.y + c, 1.0, r.h - 2.0 * c), edge);
}

/// A small dark chip (map labels): navy, no border, slightly see-through.
pub fn chip(canvas: &mut Canvas, a: &Assets, r: Rect) {
    let fill = Color::rgba(13, 23, 34, 218);
    match a.corner_plain.as_ref() {
        Some(_) if r.h < 14.0 => canvas.fill_rect(r, fill),
        Some(c) => {
            let k = (r.h / 2.0).min(5.0).floor();
            let scale = k / c.width() as f32;
            canvas.fill_rect(Rect::new(r.x + k, r.y, r.w - 2.0 * k, r.h), fill);
            canvas.fill_rect(Rect::new(r.x, r.y + k, k, r.h - 2.0 * k), fill);
            canvas.fill_rect(Rect::new(r.right() - k, r.y + k, k, r.h - 2.0 * k), fill);
            for (fx, fy) in [(false, false), (true, false), (false, true), (true, true)] {
                let at = vec2(if fx { r.right() - k } else { r.x }, if fy { r.bottom() - k } else { r.y });
                canvas.blit_sprite(c, at, Blit { flip_x: fx, flip_y: fy, scale, opacity: 0.9, ..Blit::default() });
            }
        }
        None => canvas.fill_rect(r, fill),
    }
}

/// A section title in gold capitals.
pub fn title(canvas: &mut Canvas, a: &Assets, words: &str, at: Vec2) -> Rect {
    ui::plain(canvas, &a.bold, &words.to_uppercase(), at, 12.0, GOLD, Align::Left)
}

/// An icon by name, tinted, its top-left at `at`, `size` pixels square.
pub fn icon(canvas: &mut Canvas, a: &Assets, name: &str, at: Vec2, size: f32, tint: Color) {
    if let Some(s) = a.icons.get(name) {
        canvas.blit_sprite(s, at, Blit { scale: size / s.width().max(1) as f32, tint, ..Blit::default() });
    }
}

/// Text that shrinks to fit `width`.
pub fn fit(canvas: &mut Canvas, font: &Font, words: &str, at: Vec2, size: f32, color: Color, align: Align, width: f32) -> Rect {
    let mut style = TextStyle { size, color, align, ..TextStyle::default() };
    let w = font.measure(words, &style).x;
    if w > width && w > 0.0 {
        style.size = (size * width / w).max(6.0);
    }
    canvas.draw_font_text(font, words, at, &style)
}

/// Draws one widget in its own space's coordinates (the caller pushed the space).
pub fn draw_widget(canvas: &mut Canvas, a: &Assets, w: &Widget, hot: bool, held: bool) {
    let r = w.rect;
    let hot = hot && !w.off;
    let ink = if w.off { DIM } else if w.danger { RED } else { IVORY };
    match w.kind {
        Kind::Ghost => {}
        Kind::Button => {
            let lift = if held && hot { 1.0 } else { 0.0 };
            if w.on {
                canvas.fill_rect(r, if hot { Color::from_hex(0xf0cb66) } else { GOLD });
                canvas.draw_rect(r, Color::from_hex(0xf6e3a8));
            } else {
                canvas.fill_rect(r, if hot { NAVY_HI } else { Color::from_hex(0x101e2c) });
                canvas.draw_rect(r, if w.off { Color::from_hex(0x3a4450) } else if w.danger { RED } else if hot { GOLD } else { BRONZE });
            }
            let color = if w.on { GOLD_INK } else { ink };
            let size = (r.h * 0.52).clamp(11.0, 17.0);
            let mut x = r.center().x;
            if let Some(name) = w.icon {
                let s = (r.h - 8.0).min(18.0);
                icon(canvas, a, name, vec2(r.x + 7.0, r.y + (r.h - s) / 2.0 + lift), s, color);
                x += s / 2.0 + 2.0;
            }
            let font = if w.on { &a.bold } else { &a.font };
            fit(canvas, font, &w.label, vec2(x, r.y + (r.h - size) / 2.0 - 1.0 + lift), size, color, Align::Center, r.w - 10.0 - if w.icon.is_some() { 20.0 } else { 0.0 });
        }
        Kind::Tab => {
            canvas.fill_rect(r, if w.on { Color::from_hex(0x1a2c3f) } else if hot { NAVY_HI } else { Color::from_hex(0x101e2c) });
            canvas.fill_rect(Rect::new(r.x, r.bottom() - 2.0, r.w, 2.0), if w.on { GOLD } else { Color::from_hex(0x2a3948) });
            let mut x = r.x + 12.0;
            if let Some(name) = w.icon {
                icon(canvas, a, name, vec2(r.x + 10.0, r.y + (r.h - 18.0) / 2.0), 18.0, if w.on { GOLD } else { LABEL });
                x += 24.0;
            }
            fit(canvas, &a.bold, &w.label, vec2(x, r.y + (r.h - 15.0) / 2.0 - 1.0), 15.0, if w.on { GOLD } else { ink }, Align::Left, r.right() - x - 6.0);
        }
        Kind::Row => {
            if w.on {
                canvas.fill_rect(r, Color::rgba(226, 184, 74, 38));
                canvas.fill_rect(Rect::new(r.x, r.y, 3.0, r.h), GOLD);
            } else if hot {
                canvas.fill_rect(r, NAVY_ROW);
            }
            if !w.label.is_empty() {
                let size = (r.h * 0.6).clamp(11.0, 15.0);
                let mut x = r.x + 10.0;
                if let Some(name) = w.icon {
                    icon(canvas, a, name, vec2(r.x + 8.0, r.y + (r.h - 16.0) / 2.0), 16.0, if w.danger { RED } else { GOLD });
                    x += 22.0;
                }
                let right = if w.sub.is_empty() { 0.0 } else { a.font.measure(&w.sub, &TextStyle { size: size - 1.0, ..TextStyle::default() }).x + 10.0 };
                fit(canvas, &a.font, &w.label, vec2(x, r.y + (r.h - size) / 2.0 - 1.0), size, ink, Align::Left, r.right() - x - 8.0 - right);
                if !w.sub.is_empty() {
                    ui::plain(canvas, &a.font, &w.sub, vec2(r.right() - 8.0, r.y + (r.h - size) / 2.0), size - 1.0, LABEL, Align::Right);
                }
            }
        }
        Kind::Menu => {
            if hot {
                canvas.fill_rect(r, if w.danger { Color::rgba(216, 87, 74, 60) } else { Color::rgba(226, 184, 74, 46) });
            }
            let mut x = r.x + 10.0;
            if let Some(name) = w.icon {
                icon(canvas, a, name, vec2(r.x + 8.0, r.y + (r.h - 16.0) / 2.0), 16.0, if w.danger { RED } else { LABEL });
                x += 24.0;
            }
            ui::plain(canvas, &a.font, &w.label, vec2(x, r.y + (r.h - 14.0) / 2.0 - 1.0), 14.0, ink, Align::Left);
        }
        Kind::Round => {
            let d = r.w.min(r.h - 14.0);
            let at = vec2(r.center().x - d / 2.0, r.y);
            let ring = if w.on || hot { a.rail_ring_on.as_ref() } else { a.rail_ring.as_ref() };
            match ring {
                Some(s) => canvas.blit_sprite(s, at, Blit { scale: d / s.width() as f32, ..Blit::default() }),
                None => {
                    canvas.fill_circle(at + vec2(d / 2.0, d / 2.0), d / 2.0, if w.on || hot { GOLD } else { BRONZE });
                    canvas.fill_circle(at + vec2(d / 2.0, d / 2.0), d / 2.0 - 2.0, NAVY_SOLID);
                }
            }
            if let Some(name) = w.icon {
                let s = d * 0.5;
                icon(canvas, a, name, at + vec2((d - s) / 2.0, (d - s) / 2.0), s, if w.on || hot { GOLD } else { ink });
            }
            let cap = Rect::new(r.x - 6.0, r.y + d + 1.0, r.w + 12.0, 13.0);
            chip(canvas, a, cap);
            fit(canvas, &a.font, &w.label, vec2(r.center().x, r.y + d + 1.0), 11.0, if w.on || hot { GOLD } else { IVORY }, Align::Center, cap.w - 4.0);
            if w.badge > 0 {
                let b = vec2(at.x + d - 13.0, r.y - 3.0);
                match a.badge.as_ref() {
                    Some(s) => canvas.blit_sprite(s, b, Blit::default()),
                    None => canvas.fill_circle(b + vec2(9.0, 9.0), 9.0, GOLD),
                }
                let words = if w.badge > 9 { "9+".to_string() } else { w.badge.to_string() };
                ui::plain(canvas, &a.bold, &words, b + vec2(9.0, 2.0), 12.0, GOLD_INK, Align::Center);
            }
        }
        Kind::Stat => {
            if hot {
                canvas.fill_rect(r, NAVY_ROW);
                canvas.fill_rect(Rect::new(r.x, r.bottom() - 2.0, r.w, 2.0), GOLD);
            }
            let mut x = r.x + 6.0;
            if let Some(name) = w.icon {
                icon(canvas, a, name, vec2(r.x + 5.0, r.y + (r.h - 22.0) / 2.0), 22.0, if hot { GOLD } else { LABEL });
                x += 26.0;
            }
            ui::plain(canvas, &a.font, &w.sub, vec2(x, r.y + 5.0), 11.0, LABEL, Align::Left);
            fit(canvas, &a.bold, &w.label, vec2(x, r.y + 18.0), 18.0, if w.danger { RED } else { IVORY }, Align::Left, r.right() - x - 3.0);
        }
        Kind::Icon => {
            if w.on {
                canvas.fill_rect(r, GOLD);
            } else {
                canvas.fill_rect(r, if hot { NAVY_HI } else { Color::from_hex(0x101e2c) });
                canvas.draw_rect(r, if hot { GOLD } else { BRONZE });
            }
            let color = if w.on { GOLD_INK } else { ink };
            match w.icon {
                Some(name) => {
                    let s = (r.w.min(r.h) - 6.0).min(20.0);
                    icon(canvas, a, name, vec2(r.center().x - s / 2.0, r.center().y - s / 2.0), s, color);
                }
                None => {
                    let size = (r.h * 0.6).clamp(11.0, 16.0);
                    ui::plain(canvas, &a.bold, &w.label, vec2(r.center().x, r.y + (r.h - size) / 2.0 - 1.0), size, color, Align::Center);
                }
            }
        }
        Kind::Slider(t) => {
            let y = r.center().y;
            let (x0, x1) = (r.x + 8.0, r.right() - 8.0);
            canvas.fill_rect(Rect::new(x0, y - 2.0, x1 - x0, 4.0), Color::from_hex(0x24323f));
            let kx = x0 + (x1 - x0) * t.clamp(0.0, 1.0);
            canvas.fill_rect(Rect::new(x0, y - 2.0, kx - x0, 4.0), if w.off { DIM } else { GOLD });
            match a.slider_knob.as_ref().filter(|_| !w.off) {
                Some(s) => {
                    let k = if hot || held { 1.0 } else { 0.875 };
                    canvas.blit_sprite(s, vec2(kx - 8.0 * k, y - 8.0 * k), Blit { scale: k, ..Blit::default() });
                }
                None => canvas.fill_circle(vec2(kx, y), if hot { 7.0 } else { 6.0 }, if w.off { DIM } else { GOLD }),
            }
        }
    }
}
