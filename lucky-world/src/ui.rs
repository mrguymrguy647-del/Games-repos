//! Pictures, sounds and fonts, and small drawing helpers: text that fits,
//! buttons, sparkles and confetti.

use duckforge::prelude::*;

pub const W: f32 = 1280.0;
pub const H: f32 = 720.0;

pub const GOLD: Color = Color::from_hex(0xf5c542);
pub const GOLD_HI: Color = Color::from_hex(0xfff1b8);
pub const INK: Color = Color::from_hex(0x120a28);
pub const MUTED: Color = Color::from_hex(0xb7a9e0);
pub const SOFT: Color = Color::from_hex(0x8d80bd);
pub const BLUE: Color = Color::from_hex(0x7fa8ff);
pub const RED: Color = Color::from_hex(0xff6070);
pub const GREEN: Color = Color::from_hex(0x6ee08a);

/// Everything loaded from `assets/`, once, at the start.
pub struct Assets {
    pub font: Font,
    pub serif: Font,
    pub backdrop: Sprite,
    pub pointer: Sprite,
    pub hub: Sprite,
    pub glow: Sprite,
    pub sparkle: Sprite,
    pub rare: Sprite,
    pub logo: Sprite,
    pub button: Sprite,
    pub button_blue: Sprite,
    pub button_hot: Sprite,
    pub card_round: Sprite,
    pub card_final: Sprite,
    pub card_rival: Sprite,
    pub card_rival_you: Sprite,
    pub card_map: Sprite,
    pub card_map_on: Sprite,
    pub panel: Sprite,
    pub card_small: Sprite,
    pub card_small_on: Sprite,
    pub card_small_new: Sprite,
    pub map_panel: Sprite,
    pub topbar: Sprite,
    pub minimap_frame: Sprite,
    pub flag_cloth: Sprite,
    pub flag_pole: Sprite,
    pub icon_capital: Sprite,
    pub icon_town: Sprite,
    pub icon_landmark: Sprite,
    pub icon_river: Sprite,
    pub econ_window: Sprite,
    pub icon_coin: Sprite,
    pub icon_factory: Sprite,
    pub icon_scales: Sprite,
    pub icon_ship: Sprite,
    pub icon_crate: Sprite,
    pub icon_chart: Sprite,
    pub icon_swords: Sprite,
    pub icon_plane: Sprite,
    pub icon_army: Sprite,
    pub report_paper: Sprite,
    pub stand: Mesh,
    pub tick: Wav,
    pub tick_low: Wav,
    pub land: Wav,
    pub whoosh: Wav,
    pub fanfare: Wav,
    pub rare_sound: Wav,
    pub card_sound: Wav,
    pub world_ambience: Wav,
    pub click: Wav,
    pub select: Wav,
    pub open_map: Wav,
    pub pan_tick: Wav,
    pub coin: Wav,
    pub month: Wav,
    pub news: Wav,
    pub gunfire: Wav,
    pub explosion: Wav,
    pub war_drums: Wav,
}

impl Assets {
    pub fn load() -> Result<Assets, duckforge::LoadError> {
        let art = |name: &str| Sprite::load_png(format!("assets/art/{name}.png"));
        let sound = |name: &str| Wav::load(format!("assets/sound/{name}.wav"));
        Ok(Assets {
            font: Font::load("assets/fonts/Inter-Bold.otf")?,
            serif: Font::load("assets/fonts/DejaVuSerif-Bold.ttf")?,
            backdrop: art("backdrop")?,
            pointer: art("pointer")?,
            hub: art("hub")?,
            glow: art("glow")?,
            sparkle: art("sparkle")?,
            rare: art("rare")?,
            logo: art("logo")?,
            button: art("button")?,
            button_blue: art("button_blue")?,
            button_hot: art("button_hot")?,
            card_round: art("card_round")?,
            card_final: art("card_final")?,
            card_rival: art("card_rival")?,
            card_rival_you: art("card_rival_you")?,
            card_map: art("card_map")?,
            card_map_on: art("card_map_on")?,
            panel: art("panel")?,
            card_small: art("card_small")?,
            card_small_on: art("card_small_on")?,
            card_small_new: art("card_small_new")?,
            map_panel: art("map_panel")?,
            topbar: art("topbar")?,
            minimap_frame: art("minimap_frame")?,
            flag_cloth: art("flag_cloth")?,
            flag_pole: art("flag_pole")?,
            icon_capital: art("icon_capital")?,
            icon_town: art("icon_town")?,
            icon_landmark: art("icon_landmark")?,
            icon_river: art("icon_river")?,
            econ_window: art("econ_window")?,
            icon_coin: art("icon_coin")?,
            icon_factory: art("icon_factory")?,
            icon_scales: art("icon_scales")?,
            icon_ship: art("icon_ship")?,
            icon_crate: art("icon_crate")?,
            icon_chart: art("icon_chart")?,
            icon_swords: art("icon_swords")?,
            icon_plane: art("icon_plane")?,
            icon_army: art("icon_army")?,
            report_paper: art("report_paper")?,
            stand: Mesh::load_glb("assets/models/wheel_stand.glb")?,
            tick: sound("tick")?,
            tick_low: sound("tick_low")?,
            land: sound("land")?,
            whoosh: sound("whoosh")?,
            fanfare: sound("fanfare")?,
            rare_sound: sound("rare")?,
            card_sound: sound("card")?,
            world_ambience: sound("world_ambience")?,
            click: sound("click")?,
            select: sound("select")?,
            open_map: sound("open_map")?,
            pan_tick: sound("pan_tick")?,
            coin: sound("coin")?,
            month: sound("month")?,
            news: sound("news")?,
            gunfire: sound("gunfire")?,
            explosion: sound("explosion")?,
            war_drums: sound("war_drums")?,
        })
    }
}

pub fn alpha(c: Color, a: f32) -> Color {
    Color::rgba(c.r, c.g, c.b, (a.clamp(0.0, 1.0) * c.a as f32) as u8)
}

pub fn style(size: f32, color: Color) -> TextStyle {
    TextStyle {
        size,
        color,
        outline: (size / 14.0).max(1.5),
        outline_color: INK,
        shadow: Some(vec2(0.0, size / 12.0)),
        shadow_color: Color::rgba(0, 0, 0, 140),
        shadow_blur: 2.0,
        ..TextStyle::default()
    }
}

/// Text at `pos` (top; left, centre or right by `align`), shrunk to fit
/// `max_width` if it's wider.
#[allow(clippy::too_many_arguments)]
pub fn text(
    canvas: &mut Canvas,
    font: &Font,
    words: &str,
    pos: Vec2,
    size: f32,
    color: Color,
    align: Align,
    max_width: f32,
) -> Rect {
    let mut s = style(size, color);
    s.align = align;
    let width = font.measure(words, &s).x;
    if width > max_width && width > 0.0 {
        let k = max_width / width;
        s = style(size * k, color);
        s.align = align;
    }
    canvas.draw_font_text(font, words, pos, &s)
}

/// Plain text with no outline or shadow (small print).
pub fn plain(canvas: &mut Canvas, font: &Font, words: &str, pos: Vec2, size: f32, color: Color, align: Align) -> Rect {
    canvas.draw_font_text(
        font,
        words,
        pos,
        &TextStyle {
            size,
            color,
            align,
            ..TextStyle::default()
        },
    )
}

/// A clickable button: 260 x 64, the size of the button pictures.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Button {
    pub at: Vec2,
    pub label: &'static str,
    pub key: &'static str,
    pub kind: ButtonKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Gold,
    Blue,
}

impl Button {
    pub const fn new(x: f32, y: f32, label: &'static str, key: &'static str, kind: ButtonKind) -> Button {
        Button {
            at: Vec2 { x, y },
            label,
            key,
            kind,
        }
    }

    pub fn rect(&self) -> Rect {
        Rect::new(self.at.x + 4.0, self.at.y + 4.0, 252.0, 52.0)
    }

    pub fn hovered(&self, mouse: Option<Vec2>) -> bool {
        mouse.is_some_and(|m| self.rect().contains(m))
    }

    pub fn draw(&self, canvas: &mut Canvas, assets: &Assets, mouse: Option<Vec2>, time: f32, pulse: bool) {
        let hot = self.hovered(mouse) || (pulse && (time * 2.5).sin() > 0.3);
        let sprite = match (self.kind, hot) {
            (_, true) => &assets.button_hot,
            (ButtonKind::Gold, false) => &assets.button,
            (ButtonKind::Blue, false) => &assets.button_blue,
        };
        let lift = if self.hovered(mouse) { -2.0 } else { 0.0 };
        canvas.blit_sprite(sprite, self.at + vec2(0.0, lift), Blit::default());
        let ink = match self.kind {
            ButtonKind::Blue if !hot => Color::from_hex(0x0a1540),
            _ => Color::from_hex(0x3a2400),
        };
        let center = self.at.x + 130.0;
        plain(canvas, &assets.font, self.label, vec2(center, self.at.y + 11.0 + lift), 22.0, ink, Align::Center);
        plain(canvas, &assets.font, self.key, vec2(center, self.at.y + 36.0 + lift), 12.0, alpha(ink, 0.7), Align::Center);
        canvas.label(&format!("button {}", self.label), self.rect());
    }
}

/// A spark or a scrap of confetti.
#[derive(Debug, Clone, Copy)]
pub struct Particle {
    pub pos: Vec2,
    pub vel: Vec2,
    pub life: f32,
    pub max_life: f32,
    pub size: f32,
    pub turn: f32,
    pub spin: f32,
    pub color: Color,
    pub confetti: bool,
}

pub fn burst(particles: &mut Vec<Particle>, rng: &mut Rng, at: Vec2, count: usize, speed: f32) {
    for _ in 0..count {
        let a = rng.range(0.0, std::f32::consts::TAU);
        let v = rng.range(0.3, 1.0) * speed;
        let life = rng.range(0.5, 1.1);
        particles.push(Particle {
            pos: at,
            vel: vec2(a.cos(), a.sin()) * v,
            life,
            max_life: life,
            size: rng.range(0.4, 1.0),
            turn: rng.range(0.0, 90.0),
            spin: rng.range(-200.0, 200.0),
            color: Color::WHITE,
            confetti: false,
        });
    }
}

pub fn confetti(particles: &mut Vec<Particle>, rng: &mut Rng, count: usize) {
    const COLORS: [u32; 6] = [0xf5c542, 0xe8505b, 0x3caea3, 0x5cc8f0, 0xed6aa6, 0x9bd34d];
    for _ in 0..count {
        let life = rng.range(2.5, 4.5);
        particles.push(Particle {
            pos: vec2(rng.range(0.0, W), rng.range(-200.0, -10.0)),
            vel: vec2(rng.range(-40.0, 40.0), rng.range(80.0, 220.0)),
            life,
            max_life: life,
            size: rng.range(6.0, 12.0),
            turn: rng.range(0.0, 360.0),
            spin: rng.range(-300.0, 300.0),
            color: Color::from_hex(COLORS[(rng.next_u32() as usize) % COLORS.len()]),
            confetti: true,
        });
    }
}

pub fn update_particles(particles: &mut Vec<Particle>, dt: f32) {
    for p in particles.iter_mut() {
        p.life -= dt;
        p.pos += p.vel * dt;
        if p.confetti {
            p.vel.x += (p.turn.to_radians()).sin() * 30.0 * dt;
        } else {
            p.vel = p.vel * (1.0 - 2.5 * dt) + vec2(0.0, 120.0 * dt);
        }
        p.turn += p.spin * dt;
    }
    particles.retain(|p| p.life > 0.0 && p.pos.y < H + 40.0);
}

pub fn draw_particles(canvas: &mut Canvas, assets: &Assets, particles: &[Particle]) {
    for p in particles {
        let fade = (p.life / p.max_life).clamp(0.0, 1.0);
        if p.confetti {
            let (s, c) = p.turn.to_radians().sin_cos();
            let half = vec2(p.size, p.size * 0.45 * (p.turn * 0.05).cos().abs().max(0.2));
            let corner = |x: f32, y: f32| p.pos + vec2(x * half.x * c - y * half.y * s, x * half.x * s + y * half.y * c);
            let color = alpha(p.color, fade.min(1.0) * 1.5);
            canvas.fill_triangle(corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), color);
            canvas.fill_triangle(corner(-1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0), color);
        } else {
            canvas.draw_placed(
                &assets.sparkle,
                p.pos,
                Placed {
                    pivot: vec2(24.0, 24.0),
                    scale: vec2(p.size, p.size) * (0.5 + fade * 0.7),
                    turn: p.turn,
                    opacity: fade,
                    ..Placed::default()
                },
            );
        }
    }
}

/// A soft glow centred on `at`, `size` pixels across.
pub fn glow(canvas: &mut Canvas, assets: &Assets, at: Vec2, size: f32, opacity: f32) {
    canvas.draw_placed(
        &assets.glow,
        at,
        Placed {
            pivot: vec2(128.0, 128.0),
            scale: vec2(size / 256.0, size / 256.0),
            opacity,
            ..Placed::default()
        },
    );
}

/// The screen darkened behind a card.
pub fn dim(canvas: &mut Canvas, amount: f32) {
    canvas.fill_rect(Rect::new(0.0, 0.0, W, H), Color::rgba(5, 2, 15, (amount * 255.0) as u8));
}

/// 0 to 1, overshooting a little before settling: cards that pop in.
pub fn ease_out_back(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    let c = 1.70158;
    1.0 + (c + 1.0) * (u - 1.0).powi(3) + c * (u - 1.0).powi(2)
}

pub fn ease_out(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    1.0 - (1.0 - u).powi(3)
}
