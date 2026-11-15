//! One wheel on the stage: its slices (sized by the real odds), its 3D
//! mesh, its labels, and the spin: a pull back, a fast whirl, a long slow
//! tick-tick-tick, then the pointer comes to rest on the result.

use std::f32::consts::{PI, TAU};

use duckforge::prelude::*;

use crate::fate::{self, Odds};
use crate::ui::{self, Assets};

/// The wheel's middle, in the world (metres). It turns on the stand's axle.
pub const CENTER: Vec3 = Vec3 {
    x: 0.0,
    y: 1.55,
    z: -0.2,
};
/// The pointer sits at angle 0: the wheel's right-hand side.
const FACE_Z: f32 = 0.0;
const LIP_Z: f32 = -0.05;
const LABEL_Z: f32 = -0.004;

/// One slice on the wheel. An option with a big chance has several,
/// spread round the wheel.
#[derive(Debug, Clone)]
pub struct Slice {
    pub option: usize,
    pub start: f32,
    pub end: f32,
    pub color: Color,
}

impl Slice {
    pub fn width(&self) -> f32 {
        self.end - self.start
    }
    pub fn middle(&self) -> f32 {
        (self.start + self.end) / 2.0
    }
}

/// What happened during an update.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    /// A peg passed the pointer; `speed` is in radians per second.
    Tick { speed: f32 },
    /// The wheel stopped.
    Landed,
}

#[derive(Debug, Clone)]
struct Spin {
    from: f32,
    delta: f32,
    t: f32,
    windup: f32,
    duration: f32,
    hurry: bool,
}

impl Spin {
    fn angle(&self) -> f32 {
        if self.t < self.windup {
            // Pulled back a little first, like a hand winding up.
            return self.from - 0.09 * (PI * self.t / self.windup).sin();
        }
        let u = ((self.t - self.windup) / self.duration).min(1.0);
        self.from + self.delta * (1.0 - (1.0 - u).powi(4))
    }

    fn done(&self) -> bool {
        self.t >= self.windup + self.duration
    }
}

pub struct WheelView {
    pub odds: Odds,
    pub slices: Vec<Slice>,
    face: Mesh,
    /// Which slice each face triangle belongs to.
    face_slice: Vec<usize>,
    rim: Mesh,
    bulbs: [Mesh; 2],
    labels: Vec<(usize, Mesh)>,
    /// How far the wheel has turned (radians; up is anticlockwise on screen).
    pub angle: f32,
    spin: Option<Spin>,
    /// The slice it will land on (or has).
    pub target_slice: Option<usize>,
    pub landed: bool,
    /// The pointer's flap, in degrees (the tip lifted by a peg).
    pub flap: f32,
    under: usize,
    pub ticks: u32,
}

/// Lays the options round the wheel. Each option's share of the wheel is
/// exactly its chance; big ones are cut into several slices, spread out.
pub fn layout(odds: &Odds, target: usize) -> Vec<Slice> {
    let mut pieces: Vec<(f32, usize, f32)> = Vec::new(); // (place, option, width)
    let on: Vec<(usize, f64)> = odds.on_wheel().collect();
    for (n, &(option, chance)) in on.iter().enumerate() {
        let k = ((chance * target as f64).round() as usize).max(1);
        let width = (chance as f32) * TAU / k as f32;
        // Spread an option's pieces evenly, each option starting a little further on.
        let phase = (n as f32 * 0.618).fract();
        for j in 0..k {
            pieces.push((((j as f32 + phase) / k as f32).fract(), option, width));
        }
    }
    pieces.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut slices: Vec<Slice> = Vec::new();
    let mut at = 0.0;
    for (_, option, width) in pieces {
        match slices.last_mut() {
            // Two pieces of one option side by side are one slice.
            Some(last) if last.option == option => last.end += width,
            _ => slices.push(Slice {
                option,
                start: at,
                end: at + width,
                color: odds.options[option].color,
            }),
        }
        at += width;
    }
    // The first and last can meet too.
    if slices.len() > 1 && slices[0].option == slices[slices.len() - 1].option {
        let last = slices.pop().expect("more than one");
        slices[0].start = last.start - TAU;
    }
    // Floating-point sums: close the circle exactly.
    if let Some(last) = slices.last_mut() {
        last.end = slices_end(last.end);
    }
    slices
}

fn slices_end(end: f32) -> f32 {
    if (end - TAU).abs() < 1e-3 { TAU } else { end }
}

fn point(radius: f32, angle: f32, z: f32) -> Vec3 {
    vec3(radius * angle.cos(), radius * angle.sin(), z)
}

fn lighter(c: Color, amount: f32) -> Color {
    c.lerp(Color::WHITE, amount)
}

impl WheelView {
    pub fn new(odds: Odds, slice_target: usize, font: &Font) -> WheelView {
        let slices = layout(&odds, slice_target);
        let (face, face_slice) = build_face(&slices);
        let rim = build_rim(&slices);
        let bulbs = build_bulbs();
        let labels = build_labels(&slices, &odds, font);
        WheelView {
            odds,
            slices,
            face,
            face_slice,
            rim,
            bulbs,
            labels,
            angle: 0.0,
            spin: None,
            target_slice: None,
            landed: false,
            flap: 0.0,
            under: 0,
            ticks: 0,
        }
    }

    /// The local angle under the pointer.
    fn local_under(&self) -> f32 {
        (-self.angle).rem_euclid(TAU)
    }

    /// The slice under the pointer now.
    pub fn slice_under(&self) -> usize {
        let a = self.local_under();
        self.slices
            .iter()
            .position(|s| {
                let (lo, hi) = (s.start.rem_euclid(TAU), s.end.rem_euclid(TAU));
                if s.width() >= TAU - 1e-4 {
                    true
                } else if lo < hi || (hi == 0.0 && s.end > s.start) {
                    a >= lo && (a < hi || hi == 0.0)
                } else {
                    a >= lo || a < hi
                }
            })
            .unwrap_or(0)
    }

    pub fn spinning(&self) -> bool {
        self.spin.is_some()
    }

    /// Spins the wheel. The result is decided now, by the odds (a slice's
    /// size is its chance), and the wheel then turns to it: returns the
    /// option it will land on.
    pub fn start_spin(&mut self, rng: &mut Rng) -> usize {
        let option = fate::roll_option(&self.odds, rng);
        let mine: Vec<usize> = (0..self.slices.len()).filter(|&i| self.slices[i].option == option).collect();
        // One of the option's slices, by size, then somewhere inside it, not on a peg.
        let widths: Vec<f64> = mine.iter().map(|&i| self.slices[i].width() as f64).collect();
        let total: f64 = widths.iter().sum();
        let chances: Vec<f64> = widths.iter().map(|w| w / total).collect();
        let slice = mine[fate::pick_by(&chances, rng.next_f32() as f64)];
        let s = &self.slices[slice];
        let inside = s.start + s.width() * rng.range(0.15, 0.85);
        let final_angle = -inside;
        let turns = 4.0 + rng.range_i32(0, 2) as f32;
        let delta = turns * TAU + (final_angle - self.angle).rem_euclid(TAU);
        self.spin = Some(Spin {
            from: self.angle,
            delta,
            t: 0.0,
            windup: 0.32,
            duration: rng.range(4.1, 4.8),
            hurry: false,
        });
        self.target_slice = Some(slice);
        self.landed = false;
        self.under = self.slice_under();
        option
    }

    /// Spins the rest of the way four times faster.
    pub fn hurry(&mut self) {
        if let Some(spin) = &mut self.spin {
            spin.hurry = true;
        }
    }

    pub fn update(&mut self, dt: f32) -> Vec<Event> {
        let mut events = Vec::new();
        self.flap *= (-11.0 * dt).exp();
        let Some(spin) = &mut self.spin else {
            return events;
        };
        let before = self.angle;
        spin.t += if spin.hurry { dt * 4.0 } else { dt };
        self.angle = spin.angle();
        let speed = (self.angle - before).abs() / dt.max(1e-6);
        let done = spin.done();
        let under = self.slice_under();
        if under != self.under {
            self.under = under;
            self.ticks += 1;
            // A peg lifts the pointer's tip the way the wheel is going.
            let way = if self.angle >= before { 1.0 } else { -1.0 };
            self.flap = 24.0 * way;
            events.push(Event::Tick { speed });
        }
        if done {
            self.spin = None;
            self.landed = true;
            self.light_up();
            events.push(Event::Landed);
        }
        events
    }

    /// The landed option's slices glow; the rest go dim.
    fn light_up(&mut self) {
        let Some(target) = self.target_slice else { return };
        let winner = self.slices[target].option;
        for (t, &s) in self.face_slice.iter().enumerate() {
            if self.slices[s].option == winner {
                self.face.set_glow(t, if s == target { 1.0 } else { 0.6 });
            } else {
                let c = self.face.triangles[t].color;
                self.face.triangles[t].color = c.lerp(Color::from_hex(0x1a1030), 0.55);
            }
        }
    }

    /// The option landed on (once it has).
    pub fn result(&self) -> Option<usize> {
        if self.landed { self.target_slice.map(|s| self.slices[s].option) } else { None }
    }

    pub fn transform(&self) -> Transform {
        Transform::at(CENTER).rotated(vec3(0.0, 0.0, self.angle))
    }

    pub fn draw(&self, canvas: &mut Canvas, camera: &Camera3D, assets: &Assets, time: f32) {
        let place = self.transform();
        canvas.draw_mesh(&self.face, &place, camera);
        let winner = self.result();
        for (slice, label) in &self.labels {
            match winner {
                Some(w) if self.slices[*slice].option != w => {
                    canvas.draw_mesh_tinted(label, &place, camera, Color::rgba(120, 110, 140, 255));
                }
                _ => canvas.draw_mesh(label, &place, camera),
            }
        }
        canvas.draw_mesh(&self.rim, &place, camera);
        // Marquee bulbs chase round the rim; faster while it spins.
        let rate = if self.spinning() { 12.0 } else { 3.0 };
        let phase = ((time * rate) as usize) % 2;
        let on = if self.landed { ui::GOLD_HI } else { Color::from_hex(0xfff3c0) };
        for (i, bulbs) in self.bulbs.iter().enumerate() {
            let color = if i == phase { on } else { Color::from_hex(0x7a5520) };
            let mesh = bulbs.clone().with_color(color);
            let mesh = if i == phase { mesh.glowing() } else { mesh };
            canvas.draw_mesh(&mesh, &place, camera);
        }

        // The hub and the pointer are Duck Quill pictures, placed where the 3D wheel is.
        let Some((_, radius)) = self.on_screen(canvas, camera) else { return };
        if let Some((hub, _)) = camera.project(CENTER + vec3(0.0, 0.0, LIP_Z - 0.03), canvas) {
            let size = 0.36 * radius / 120.0;
            canvas.draw_placed(
                &assets.hub,
                hub,
                Placed {
                    pivot: vec2(70.0, 70.0),
                    scale: vec2(size, size),
                    turn: -self.angle.to_degrees(),
                    ..Placed::default()
                },
            );
            canvas.label("hub", Rect::from_center(hub, 120.0 * size, 120.0 * size));
        }
        if let Some((tip, _)) = camera.project(CENTER + point(1.0, 0.0, LIP_Z - 0.06), canvas) {
            let size = 0.42 * radius / 100.0;
            canvas.draw_placed(
                &assets.pointer,
                tip + vec2(72.0 * size, 0.0),
                Placed {
                    pivot: vec2(92.0, 42.0),
                    scale: vec2(size, size),
                    turn: self.flap,
                    ..Placed::default()
                },
            );
            canvas.label("pointer", Rect::from_center(tip + vec2(45.0 * size, 0.0), 90.0 * size, 50.0 * size));
        }
    }

    /// Where the wheel's middle and rim are on screen: (centre, radius).
    pub fn on_screen(&self, canvas: &Canvas, camera: &Camera3D) -> Option<(Vec2, f32)> {
        let (c, _) = camera.project(CENTER, canvas)?;
        let (r, _) = camera.project(CENTER + vec3(1.0, 0.0, 0.0), canvas)?;
        Some((c, r.distance(c)))
    }
}

fn build_face(slices: &[Slice]) -> (Mesh, Vec<usize>) {
    let mut m = Mesh::new();
    let mut owner = Vec::new();
    let (r1, r2) = (0.42, 0.955);
    for (si, s) in slices.iter().enumerate() {
        let steps = ((s.width() / 0.05).ceil() as usize).max(1);
        let inner = lighter(s.color, 0.22);
        for k in 0..steps {
            let a0 = s.start + s.width() * k as f32 / steps as f32;
            let a1 = s.start + s.width() * (k + 1) as f32 / steps as f32;
            let c = m.add_vertex(vec3(0.0, 0.0, FACE_Z));
            let i0 = m.add_vertex(point(r1, a0, FACE_Z));
            let i1 = m.add_vertex(point(r1, a1, FACE_Z));
            let o0 = m.add_vertex(point(r2, a0, FACE_Z));
            let o1 = m.add_vertex(point(r2, a1, FACE_Z));
            // Clockwise as the camera sees it (x right, y up).
            m.add_triangle(c, i1, i0, inner);
            m.add_quad(i0, i1, o1, o0, s.color);
            owner.extend([si, si, si]);
        }
        // A thin white line on each slice's leading edge.
        let a = s.start;
        let w = 0.006;
        let p = |r: f32, side: f32| {
            let n = vec3(-a.sin(), a.cos(), 0.0) * side;
            point(r, a, FACE_Z - 0.001) + n
        };
        let (q0, q1, q2, q3) = (m.add_vertex(p(0.0, -w)), m.add_vertex(p(0.0, w)), m.add_vertex(p(r2, w)), m.add_vertex(p(r2, -w)));
        m.add_quad(q0, q1, q2, q3, Color::from_hex(0xfdf6e3));
        owner.extend([si, si]);
    }
    (m, owner)
}

fn build_rim(slices: &[Slice]) -> Mesh {
    let mut m = Mesh::new();
    let gold = ui::GOLD;
    let dark = Color::from_hex(0xa8740e);
    let n = 96;
    for k in 0..n {
        let a0 = TAU * k as f32 / n as f32;
        let a1 = TAU * (k + 1) as f32 / n as f32;
        // The front lip, a ring standing proud of the face.
        let (i0, i1) = (m.add_vertex(point(0.955, a0, LIP_Z)), m.add_vertex(point(0.955, a1, LIP_Z)));
        let (o0, o1) = (m.add_vertex(point(1.06, a0, LIP_Z)), m.add_vertex(point(1.06, a1, LIP_Z)));
        m.add_quad(i0, i1, o1, o0, gold);
        // The inside of the lip, down to the face.
        let (f0, f1) = (m.add_vertex(point(0.955, a0, FACE_Z)), m.add_vertex(point(0.955, a1, FACE_Z)));
        m.add_quad(i0, f0, f1, i1, dark);
        m.add_quad(i0, i1, f1, f0, dark);
        // The outside edge, seen from above and below: drawn both ways.
        let (b0, b1) = (m.add_vertex(point(1.06, a0, 0.06)), m.add_vertex(point(1.06, a1, 0.06)));
        m.add_quad(o0, b0, b1, o1, dark);
        m.add_quad(o0, o1, b1, b0, dark);
    }
    // A peg at every slice edge.
    let peg = Mesh::sphere(Color::from_hex(0xfff1b8), 8);
    for s in slices {
        m.merge(&peg, &Transform::at(point(0.925, s.start, LIP_Z + 0.01)).sized(0.05));
    }
    m
}

fn build_bulbs() -> [Mesh; 2] {
    let bulb = Mesh::sphere(Color::WHITE, 6);
    let mut out = [Mesh::new(), Mesh::new()];
    let n = 40;
    for k in 0..n {
        let a = TAU * (k as f32 + 0.5) / n as f32;
        out[k % 2].merge(&bulb, &Transform::at(point(1.008, a, LIP_Z - 0.015)).sized(0.04));
    }
    out
}

/// Each slice's words, painted on a picture of the slice's own colour and
/// laid along the slice from the middle outwards.
fn build_labels(slices: &[Slice], odds: &Odds, font: &Font) -> Vec<(usize, Mesh)> {
    let mut out = Vec::new();
    let r_out = 0.9;
    for (si, s) in slices.iter().enumerate() {
        if s.width() < 0.11 {
            continue; // too thin for words: its colour and the odds list say it
        }
        let words = &odds.options[s.option].label;
        let style = TextStyle {
            size: 34.0,
            color: Color::WHITE,
            outline: 3.0,
            outline_color: Color::rgba(20, 10, 40, 255),
            ..TextStyle::default()
        };
        let size = font.measure(words, &style);
        let (tw, th) = ((size.x + 16.0).ceil() as usize, 46usize);
        let mut c = Canvas::new(tw, th);
        let bg = lighter(s.color, 0.0);
        c.clear(bg);
        c.draw_font_text(font, words, vec2(8.0, 3.0), &style);
        let pixels: Vec<Color> = (0..th)
            .flat_map(|y| (0..tw).map(move |x| (x, y)))
            .map(|(x, y)| c.get_pixel(x as i32, y as i32).unwrap_or(bg))
            .collect();
        let picture = Sprite::from_pixels(tw, th, pixels);

        // As big as fits: at most 0.12 tall and 0.66 long, inside the slice.
        let aspect = th as f32 / tw as f32;
        let mut height = (0.66 * aspect).min(0.12);
        let mut fits = false;
        for _ in 0..12 {
            let length = height / aspect;
            let r_in = r_out - length;
            if r_in > 0.2 && height / 2.0 <= r_in * (s.width() / 2.0).sin() * 0.85 {
                fits = true;
                break;
            }
            height *= 0.88;
        }
        if !fits || height < 0.045 {
            continue;
        }
        let length = height / aspect;
        let r_in = r_out - length;
        let m_angle = s.middle();
        let (sin, cos) = m_angle.sin_cos();
        let at = |x: f32, y: f32| vec3(x * cos - y * sin, x * sin + y * cos, LABEL_Z);
        let mut mesh = Mesh::new();
        let bl = mesh.add_vertex(at(r_in, -height / 2.0));
        let tl = mesh.add_vertex(at(r_in, height / 2.0));
        let tr = mesh.add_vertex(at(r_out, height / 2.0));
        let br = mesh.add_vertex(at(r_out, -height / 2.0));
        mesh.add_quad_uv(bl, tl, tr, br, [vec2(0.0, 1.0), vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0)], Color::WHITE);
        out.push((si, mesh.with_texture(picture)));
    }
    out
}
