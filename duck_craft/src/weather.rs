//! Weather: clear, fog, rain and storms, each for a few minutes.
//!
//! Fog pulls the edge of the world in. Rain greys the sky and dims the
//! day; storms darker still, with lightning and thunder. The dimming is
//! real: `DuckCraft::sky` is what everything else reads, so on a stormy
//! evening the dark (and the Hollows) come early.
//!
//! What comes next is fixed by the island's seed (the same island has the
//! same weather), and more often bad at night.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Weather {
    #[default]
    Clear,
    Fog,
    Rain,
    Storm,
}

impl Weather {
    pub const ALL: [Weather; 4] = [Weather::Clear, Weather::Fog, Weather::Rain, Weather::Storm];

    /// How much of the day's light gets through.
    pub fn light(self) -> f32 {
        match self {
            Weather::Clear => 1.0,
            Weather::Fog => 0.85,
            Weather::Rain => 0.7,
            Weather::Storm => 0.5,
        }
    }

    /// How far you see, as a share of a clear day's.
    pub fn sight(self) -> f32 {
        match self {
            Weather::Clear => 1.0,
            Weather::Fog => 0.3,
            Weather::Rain => 0.6,
            Weather::Storm => 0.45,
        }
    }

    /// How grey the sky is (0 none).
    pub fn grey(self) -> f32 {
        match self {
            Weather::Clear => 0.0,
            Weather::Fog => 0.65,
            Weather::Rain => 0.7,
            Weather::Storm => 0.85,
        }
    }

    /// How hard it rains (0 dry).
    pub fn rain(self) -> f32 {
        match self {
            Weather::Rain => 1.0,
            Weather::Storm => 1.6,
            _ => 0.0,
        }
    }

    /// After the clock on the HUD.
    pub fn note(self) -> &'static str {
        match self {
            Weather::Clear => "",
            Weather::Fog => "  FOG",
            Weather::Rain => "  RAIN",
            Weather::Storm => "  STORM",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Weather::Clear => "clear",
            Weather::Fog => "fog",
            Weather::Rain => "rain",
            Weather::Storm => "storm",
        }
    }
}

/// The weather now, the weather it's turning from, and the storm's flashes.
#[derive(Debug, Clone)]
pub struct Sky {
    pub now: Weather,
    pub before: Weather,
    /// 0 just turned, 1 fully `now` (it takes `TURN_SECONDS`).
    pub blend: f32,
    /// Seconds till it turns again.
    pub left: f32,
    /// How many times it's turned (picks the next, with the seed).
    pub turns: u32,
    /// Seconds of lightning's flash left.
    pub flash: f32,
    /// Thunder on its way after a flash.
    pub thunder_in: Option<f32>,
    /// Seconds till the next lightning (in a storm).
    pub strike_wait: f32,
}

/// Seconds a change of weather takes to come in.
pub const TURN_SECONDS: f32 = 10.0;
/// The first weather lasts this long (a clear first morning).
pub const FIRST_SECONDS: f32 = 240.0;

impl Default for Sky {
    fn default() -> Sky {
        Sky {
            now: Weather::Clear,
            before: Weather::Clear,
            blend: 1.0,
            left: FIRST_SECONDS,
            turns: 0,
            flash: 0.0,
            thunder_in: None,
            strike_wait: 10.0,
        }
    }
}

impl Sky {
    /// Something of each kind of weather, blended from before to now.
    fn mix(&self, of: impl Fn(Weather) -> f32) -> f32 {
        of(self.before) + (of(self.now) - of(self.before)) * self.blend
    }

    pub fn light(&self) -> f32 {
        self.mix(Weather::light)
    }

    pub fn sight(&self) -> f32 {
        self.mix(Weather::sight)
    }

    pub fn grey(&self) -> f32 {
        self.mix(Weather::grey)
    }

    pub fn rain(&self) -> f32 {
        self.mix(Weather::rain)
    }

    /// The next weather, for turn `turns` of island `seed`: clear most
    /// often by day; at night fog and storms come more.
    pub fn pick(seed: u32, turns: u32, night: bool) -> (Weather, f32) {
        let r = random01(turns as i32, 3, 0, seed.wrapping_add(90));
        let odds: [f32; 4] = if night {
            [0.35, 0.3, 0.2, 0.15]
        } else {
            [0.55, 0.15, 0.2, 0.1]
        };
        let mut sum = 0.0;
        let mut kind = Weather::Clear;
        for (weather, odd) in Weather::ALL.into_iter().zip(odds) {
            sum += odd;
            if r < sum {
                kind = weather;
                break;
            }
        }
        let seconds = 90.0 + 150.0 * random01(turns as i32, 4, 0, seed.wrapping_add(90));
        (kind, seconds)
    }
}

impl DuckCraft {
    /// The sky as the duck sees it: the sun's light for the time of day,
    /// dimmed by the weather.
    pub fn sky(&self) -> menus::Sky {
        let mut sky = menus::sky_at(self.time_of_day());
        sky.daylight *= self.weather.light();
        sky
    }

    /// Turns the weather when it's time, and makes lightning and thunder.
    pub fn update_weather(&mut self, ctx: &mut Context) {
        let dt = ctx.dt();
        let night = self.time_of_day() >= menus::DUSK - 0.05;
        let seed = self.world.seed;
        let w = &mut self.weather;
        w.blend = (w.blend + dt / TURN_SECONDS).min(1.0);
        w.left -= dt;
        if w.left <= 0.0 {
            w.turns += 1;
            let (next, seconds) = Sky::pick(seed, w.turns, night);
            w.before = w.now;
            w.now = next;
            w.blend = if w.before == w.now { 1.0 } else { 0.0 };
            w.left = seconds;
        }
        w.flash = (w.flash - dt).max(0.0);
        if w.now == Weather::Storm && w.blend > 0.5 {
            w.strike_wait -= dt;
            if w.strike_wait <= 0.0 {
                w.strike_wait = ctx.rng.range(8.0, 25.0);
                w.flash = 0.18;
                w.thunder_in = Some(ctx.rng.range(0.3, 2.2));
            }
        }
        let mut thunder = false;
        if let Some(t) = &mut w.thunder_in {
            *t -= dt;
            if *t <= 0.0 {
                w.thunder_in = None;
                thunder = true;
            }
        }
        if thunder {
            let side = ctx.rng.range(-0.6, 0.6);
            ctx.play_wav_panned(&self.art.sounds.thunder, side);
        }
        ctx.report("weather", self.weather.now.name());
    }

    /// Whether the duck's eye is under the open sky (rain falls on it).
    pub fn outdoors(&self) -> bool {
        let eye = self.eye();
        let [x, y, z] = [eye.x, eye.y, eye.z].map(|v| v.floor() as i32);
        !World::inside(x, 0, z) || y >= self.world.sky[z as usize * SIZE + x as usize]
    }

    /// Rain over the view (when outdoors), and lightning's flash.
    pub fn draw_weather(&self, canvas: &mut Canvas) {
        let rain = self.weather.rain();
        if rain > 0.0 && self.outdoors() {
            let drops = (70.0 * rain) as i32;
            let fall = 260.0;
            for i in 0..drops {
                let speed = fall * (0.8 + 0.4 * random01(i, 1, 0, 33));
                let x = (random01(i, 0, 0, 33) * (WIDTH + 40.0) - self.time * speed * 0.18)
                    .rem_euclid(WIDTH + 40.0)
                    - 20.0;
                let y = (random01(i, 2, 0, 33) * (HEIGHT + 20.0) + self.time * speed)
                    .rem_euclid(HEIGHT + 20.0)
                    - 10.0;
                let length = 6.0 + 4.0 * random01(i, 3, 0, 33);
                canvas.draw_line(
                    vec2(x, y),
                    vec2(x - length * 0.18, y + length),
                    Color::rgba(176, 186, 206, (95.0 + 55.0 * rain.min(1.0)) as u8),
                );
            }
            canvas.label("rain", Rect::new(0.0, 0.0, WIDTH, HEIGHT));
        }
        if self.weather.flash > 0.0 {
            let alpha = (self.weather.flash / 0.18 * 150.0) as u8;
            canvas.fill_rect(
                Rect::new(0.0, 0.0, WIDTH, HEIGHT),
                Color::rgba(230, 236, 255, alpha),
            );
        }
    }
}
