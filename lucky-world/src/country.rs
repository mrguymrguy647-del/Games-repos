//! A country: what its 16 wheels landed on, its name and its flag.

use duckforge::prelude::*;

use crate::data::{Data, Kind, Unit};
use crate::fate::{self, Landed, Nation};

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Country {
    pub map: usize,
    pub landed: Vec<Landed>,
    pub name: String,
    pub flag: Flag,
    pub player: bool,
}

impl Nation for Country {
    fn landed(&self) -> &[Landed] {
        &self.landed
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct Flag {
    pub layout: String,
    #[serde(with = "crate::save_codec::color3")]
    pub colors: [Color; 3],
    pub emblem: Option<Emblem>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum EmblemShape {
    Star,
    Disc,
    Diamond,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct Emblem {
    pub shape: EmblemShape,
    #[serde(with = "crate::save_codec::color")]
    pub color: Color,
}

impl Country {
    /// A country whose wheels have all landed: gives it a name and a flag.
    pub fn finish(data: &Data, map: usize, landed: Vec<Landed>, player: bool, taken: &[&str], rng: &mut Rng) -> Country {
        let mut name = make_name(data, map, &landed, rng);
        for _ in 0..20 {
            if !taken.contains(&name.as_str()) {
                break;
            }
            name = make_name(data, map, &landed, rng);
        }
        let flag = make_flag(data, rng);
        Country {
            map,
            landed,
            name,
            flag,
            player,
        }
    }

    /// The label of what wheel `id` landed on.
    pub fn label(&self, data: &Data, id: &str) -> String {
        let w = data.wheel(id);
        data.options_on(w, self.map)[self.landed[w].option].label.clone()
    }

    pub fn option_id(&self, data: &Data, id: &str) -> String {
        let w = data.wheel(id);
        data.options_on(w, self.map)[self.landed[w].option].id.clone()
    }

    pub fn value(&self, data: &Data, id: &str) -> f64 {
        self.landed[data.wheel(id)].value
    }

    /// What a wheel landed on, in words: the exact number for number wheels.
    pub fn result_text(&self, data: &Data, wheel: usize) -> String {
        result_text(data, self.map, wheel, &self.landed[wheel])
    }

    /// Soldiers on active duty.
    pub fn troops(&self, data: &Data) -> f64 {
        self.value(data, "population") * self.value(data, "duty") / 100.0
    }

    pub fn gdp_per_person(&self, data: &Data) -> f64 {
        self.value(data, "gdp") / self.value(data, "population")
    }
}

/// What one wheel landed on, in words.
pub fn result_text(data: &Data, map: usize, wheel: usize, l: &Landed) -> String {
    let def = &data.wheels[wheel];
    let options = data.options_on(wheel, map);
    let o = &options[l.option];
    if def.kind == Kind::Pick {
        return o.label.clone();
    }
    match def.unit {
        Unit::Km2 => format!("{} km²", group(l.value)),
        Unit::People => people(l.value),
        Unit::Usd => money(l.value),
        Unit::Percent => format!("{:.2}%", l.value),
        Unit::Hdi => format!("{:.3} ({})", l.value, o.label),
        Unit::None => format!("{}", l.value),
    }
}

/// 1234567 as "1,234,567".
pub fn group(value: f64) -> String {
    let digits = format!("{}", value.round().max(0.0) as u64);
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub fn people(value: f64) -> String {
    if value >= 1e9 {
        format!("{:.2} billion", value / 1e9)
    } else if value >= 1e6 {
        format!("{:.1} million", value / 1e6)
    } else {
        group(value)
    }
}

pub fn money(value: f64) -> String {
    if value >= 1e12 {
        format!("${:.2} trillion", value / 1e12)
    } else if value >= 1e9 {
        format!("${:.1} billion", value / 1e9)
    } else {
        format!("${:.0} million", value / 1e6)
    }
}

fn pick<'a>(list: &'a [String], rng: &mut Rng) -> &'a str {
    &list[(rng.next_u32() as usize) % list.len()]
}

fn make_name(data: &Data, map: usize, landed: &[Landed], rng: &mut Rng) -> String {
    let continent = &data.maps[map].continents[landed[0].option];
    let style = &data.names.styles[&continent.style];
    let mut base = pick(&style.start, rng).to_string();
    if !style.middle.is_empty() && (rng.next_f32() as f64) < data.names.middle_chance {
        base.push_str(pick(&style.middle, rng));
    }
    let end = pick(&style.end, rng);
    // "Nor" + "a" + "ar": don't double the joining letter.
    if base.chars().last() == end.chars().next() {
        base.pop();
    }
    base.push_str(end);
    let ideology = data.wheel_index("ideology").map(|w| &data.wheels[w].options[landed[w].option].id);
    match ideology.and_then(|id| data.names.forms.get(id)) {
        Some(forms) => pick(forms, rng).replace("{}", &base),
        None => base,
    }
}

fn make_flag(data: &Data, rng: &mut Rng) -> Flag {
    let names = &data.names;
    let weights: Vec<f64> = names.flag_layouts.iter().map(|(_, w)| *w).collect();
    let total: f64 = weights.iter().sum();
    let chances: Vec<f64> = weights.iter().map(|w| w / total).collect();
    let layout = names.flag_layouts[fate::pick_by(&chances, rng.next_f32() as f64)].0.clone();
    let mut colors = Vec::new();
    while colors.len() < 3 {
        let c = names.flag_colors[(rng.next_u32() as usize) % names.flag_colors.len()];
        if !colors.contains(&c) {
            colors.push(c);
        }
    }
    let emblem = if (rng.next_f32() as f64) < names.emblem_chance {
        let shape = match rng.next_u32() % 3 {
            0 => EmblemShape::Star,
            1 => EmblemShape::Disc,
            _ => EmblemShape::Diamond,
        };
        Some(Emblem {
            shape,
            // Something that shows on the field: the third colour.
            color: colors[2],
        })
    } else {
        None
    };
    Flag {
        layout,
        colors: [colors[0], colors[1], colors[2]],
        emblem,
    }
}

/// Rolls a whole country at once, as the rivals (and Skip Spin) do.
pub fn roll_country(
    data: &Data,
    map: usize,
    mut landed: Vec<Landed>,
    world: &[Country],
    player: bool,
    rng: &mut Rng,
) -> Country {
    let others: Vec<&dyn Nation> = world.iter().map(|c| c as &dyn Nation).collect();
    fate::roll_rest(data, map, &mut landed, &others, rng);
    let taken: Vec<&str> = world.iter().map(|c| c.name.as_str()).collect();
    Country::finish(data, map, landed, player, &taken, rng)
}

/// The rivals: each spins every wheel, knowing who came before it.
pub fn roll_rivals(data: &Data, player: &Country, rng: &mut Rng) -> Vec<Country> {
    let mut world = vec![player.clone()];
    for _ in 0..data.maps[player.map].rivals {
        let rival = roll_country(data, player.map, Vec::new(), &world, false, rng);
        world.push(rival);
    }
    world.remove(0);
    world
}

/// Draws a flag in `rect`.
pub fn draw_flag(canvas: &mut Canvas, rect: Rect, flag: &Flag) {
    let [a, b, c] = flag.colors;
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);
    match flag.layout.as_str() {
        "horizontal" => {
            for (i, color) in [a, b, c].into_iter().enumerate() {
                canvas.fill_rect(Rect::new(x, y + h * i as f32 / 3.0, w, h / 3.0 + 1.0), color);
            }
        }
        "vertical" => {
            for (i, color) in [a, b, c].into_iter().enumerate() {
                canvas.fill_rect(Rect::new(x + w * i as f32 / 3.0, y, w / 3.0 + 1.0, h), color);
            }
        }
        "bicolor" => {
            canvas.fill_rect(Rect::new(x, y, w, h / 2.0 + 1.0), a);
            canvas.fill_rect(Rect::new(x, y + h / 2.0, w, h / 2.0), b);
        }
        "cross" => {
            canvas.fill_rect(rect, a);
            let t = h * 0.2;
            canvas.fill_rect(Rect::new(x, y + (h - t) / 2.0, w, t), b);
            canvas.fill_rect(Rect::new(x + w * 0.33 - t / 2.0, y, t, h), b);
        }
        "canton" => {
            canvas.fill_rect(Rect::new(x, y, w, h / 2.0 + 1.0), a);
            canvas.fill_rect(Rect::new(x, y + h / 2.0, w, h / 2.0), b);
            canvas.fill_rect(Rect::new(x, y, w * 0.42, h * 0.55), c);
        }
        "diagonal" => {
            canvas.fill_rect(rect, a);
            canvas.fill_triangle(vec2(x + w, y), vec2(x + w, y + h), vec2(x, y + h), b);
        }
        _ => {
            // "circle": a field and a disc.
            canvas.fill_rect(rect, a);
            canvas.fill_circle(vec2(x + w / 2.0, y + h / 2.0), h * 0.3, b);
        }
    }
    if let Some(emblem) = flag.emblem {
        let center = match flag.layout.as_str() {
            "canton" => vec2(x + w * 0.21, y + h * 0.275),
            "cross" => vec2(x + w * 0.33, y + h / 2.0),
            _ => vec2(x + w / 2.0, y + h / 2.0),
        };
        let r = h * 0.2;
        // On a field already that colour, the emblem would vanish: use white or black.
        let behind = flag.colors;
        let color = if behind.contains(&emblem.color) && flag.layout != "canton" {
            if emblem.color == Color::WHITE { Color::BLACK } else { Color::WHITE }
        } else {
            emblem.color
        };
        draw_emblem(canvas, center, r, emblem.shape, color);
    }
    canvas.draw_rect(rect, Color::rgba(0, 0, 0, 120));
}

pub fn draw_emblem(canvas: &mut Canvas, center: Vec2, r: f32, shape: EmblemShape, color: Color) {
    match shape {
        EmblemShape::Disc => canvas.fill_circle(center, r * 0.8, color),
        EmblemShape::Diamond => {
            let (t, rt, bt, l) = (
                center + vec2(0.0, -r),
                center + vec2(r * 0.7, 0.0),
                center + vec2(0.0, r),
                center + vec2(-r * 0.7, 0.0),
            );
            canvas.fill_triangle(t, rt, bt, color);
            canvas.fill_triangle(t, bt, l, color);
        }
        EmblemShape::Star => {
            let point = |i: usize, radius: f32| {
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                center + vec2(a.cos(), a.sin()) * radius
            };
            for i in 0..10 {
                let radius = |k: usize| if k.is_multiple_of(2) { r } else { r * 0.42 };
                canvas.fill_triangle(center, point(i, radius(i)), point(i + 1, radius(i + 1)), color);
            }
        }
    }
}
