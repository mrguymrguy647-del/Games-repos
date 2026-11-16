//! Fate: the odds of each wheel, given what the earlier wheels landed on,
//! and whole countries rolled from them.
//!
//! The player's wheels and the rivals' use exactly this code: the player
//! watches each wheel spin, the rivals' are rolled at once.

use duckforge::prelude::Rng;

use crate::data::{Data, Kind, Test, Unique, WheelOption};

/// What one wheel landed on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Landed {
    /// Index into the wheel's options (on this map, for the Continent wheel).
    pub option: usize,
    /// The exact number, for number wheels (area, people, dollars, percent,
    /// HDI); 0 for pick wheels.
    pub value: f64,
    /// What rules and follows on later wheels see: log10(value) on log
    /// wheels, otherwise the option's level.
    pub level: f64,
}

/// A wheel's chances, ready to draw and to spin.
#[derive(Debug, Clone)]
pub struct Odds {
    pub wheel: usize,
    pub options: Vec<WheelOption>,
    /// The chance of each option, adding up to 1. Options that can't come
    /// up are 0 (and aren't drawn on the wheel).
    pub chances: Vec<f64>,
    /// Where this wheel's follow points (its level scale), if it has one.
    pub target: Option<(f64, f64)>,
    /// The earlier wheels that changed these odds (a rule that held, or a
    /// follow), to show the player why.
    pub because: Vec<usize>,
}

impl Odds {
    /// The options that are on the wheel, with their chances.
    pub fn on_wheel(&self) -> impl Iterator<Item = (usize, f64)> + '_ {
        self.chances
            .iter()
            .copied()
            .enumerate()
            .filter(|&(_, c)| c > 0.0)
    }

    pub fn chance(&self, option: usize) -> f64 {
        self.chances[option]
    }
}

/// Another nation already in the world, for `unique` options.
pub trait Nation {
    fn landed(&self) -> &[Landed];
}

/// Works out a wheel's chances from what came before.
pub fn odds(
    data: &Data,
    map: usize,
    wheel: usize,
    landed: &[Landed],
    others: &[&dyn Nation],
) -> Odds {
    let def = &data.wheels[wheel];
    let mut options = data.options_on(wheel, map);
    // A band per person becomes a real band once the population is known.
    if let Some(people) = data.wheel_index("population").and_then(|p| landed.get(p)).map(|l| l.value).filter(|v| *v > 0.0) {
        for o in options.iter_mut() {
            if let Some([lo, hi]) = o.per_person {
                o.min = lo * people;
                o.max = hi * people;
                if def.log {
                    o.level = (o.min.log10() + o.max.log10()) / 2.0;
                }
            }
        }
    }
    let n = options.len();

    // 1. Weights, and each rule that holds.
    let mut weight: Vec<f64> = options.iter().map(|o| o.weight).collect();
    let mut share: Vec<Option<f64>> = options.iter().map(|o| o.share).collect();
    let mut shift = 0.0;
    let mut because = Vec::new();
    for rule in &def.rules {
        if !rule.conditions.iter().all(|c| holds(data, map, c, landed)) {
            continue;
        }
        because.extend(rule.conditions.iter().map(|c| c.wheel));
        for (name, factor) in &rule.times {
            for (i, o) in options.iter().enumerate() {
                let hit = match name.strip_prefix('#') {
                    Some(tag) => o.tags.iter().any(|t| t == tag),
                    None => &o.id == name,
                };
                if hit {
                    weight[i] *= factor;
                    share[i] = share[i].map(|s| s * factor);
                }
            }
        }
        if !rule.only.is_empty() {
            for (i, o) in options.iter().enumerate() {
                if !rule.only.contains(&o.id) {
                    weight[i] = 0.0;
                    share[i] = share[i].map(|_| 0.0);
                }
            }
        }
        shift += rule.shift;
    }
    let after_rules = weight.clone();

    // 2. Follows pull the levels towards a target.
    let mut target = None;
    for follow in &def.follows {
        because.extend(follow.from.iter().map(|&(w, _)| w));
        let t = follow.shift
            + shift
            + follow
                .from
                .iter()
                .filter_map(|&(w, scale)| landed.get(w).map(|l| l.level * scale))
                .sum::<f64>();
        if target.is_none() {
            target = Some((t, follow.spread));
        }
        for (i, o) in options.iter().enumerate() {
            if o.share.is_none() {
                let d = (o.level - t) / follow.spread;
                weight[i] *= (-0.5 * d * d).exp();
            }
        }
    }

    // 3. Unique options already taken.
    for (i, o) in options.iter().enumerate() {
        let taken = match o.unique {
            None => false,
            Some(Unique::World) => others.iter().any(|c| c.landed().get(wheel).is_some_and(|l| l.option == i)),
            Some(Unique::Continent) => {
                let mine = landed.first().map(|l| l.option);
                others.iter().any(|c| {
                    let theirs = c.landed();
                    theirs.first().map(|l| l.option) == mine
                        && theirs.get(wheel).is_some_and(|l| l.option == i)
                })
            }
        };
        if taken {
            weight[i] = 0.0;
            share[i] = share[i].map(|_| 0.0);
        }
    }

    // 4. To chances: fixed shares first, the weights share the rest.
    let mut chances = normalize(&weight, &share);
    if chances.iter().all(|&c| c == 0.0) {
        // Every option was ruled out by follows: fall back to the rules alone.
        chances = normalize(&after_rules, &share);
    }
    if chances.iter().all(|&c| c == 0.0) {
        chances = vec![1.0 / n as f64; n];
    }

    // 5. Options too thin to matter come off the wheel.
    let best = chances.iter().cloned().fold(0.0, f64::max);
    for c in chances.iter_mut() {
        if *c < data.min_share && *c < best {
            *c = 0.0;
        }
    }
    let total: f64 = chances.iter().sum();
    for c in chances.iter_mut() {
        *c /= total;
    }

    because.sort();
    because.dedup();
    Odds {
        wheel,
        options,
        chances,
        target,
        because,
    }
}

fn normalize(weight: &[f64], share: &[Option<f64>]) -> Vec<f64> {
    let fixed: f64 = share.iter().flatten().sum::<f64>().min(1.0);
    let free: f64 = weight
        .iter()
        .zip(share)
        .filter(|(_, s)| s.is_none())
        .map(|(w, _)| *w)
        .sum();
    weight
        .iter()
        .zip(share)
        .map(|(w, s)| match s {
            Some(s) => *s,
            None if free > 0.0 => w / free * (1.0 - fixed),
            None => 0.0,
        })
        .collect()
}

fn holds(data: &Data, map: usize, c: &crate::data::Condition, landed: &[Landed]) -> bool {
    let Some(l) = landed.get(c.wheel) else {
        return false;
    };
    let wheel = &data.wheels[c.wheel];
    let option = match wheel.from_map {
        true => &data.maps[map].continents[l.option].id,
        false => &wheel.options[l.option].id,
    };
    let tags = match wheel.from_map {
        true => &data.maps[map].continents[l.option].tags,
        false => &wheel.options[l.option].tags,
    };
    let number = if wheel.kind == Kind::Number { l.value } else { l.level };
    let yes = match &c.test {
        Test::Is(ids) => ids.iter().any(|id| id == option),
        Test::Tagged(want) => want.iter().any(|t| tags.contains(t)),
        Test::Below(x) => number < *x,
        Test::Above(x) => number > *x,
    };
    yes != c.negated
}

/// Picks an option by its chance.
pub fn roll_option(odds: &Odds, rng: &mut Rng) -> usize {
    pick_by(&odds.chances, rng.next_f32() as f64)
}

/// The option that `u` (from 0 to 1) falls in, laying the chances end to end.
pub fn pick_by(chances: &[f64], u: f64) -> usize {
    let mut left = u;
    let mut last = 0;
    for (i, &c) in chances.iter().enumerate() {
        if c <= 0.0 {
            continue;
        }
        last = i;
        if left < c {
            return i;
        }
        left -= c;
    }
    last
}

/// Rolls the exact value inside a landed band (number wheels), nearer the
/// follow's target, and gives the result.
pub fn land(data: &Data, odds: &Odds, option: usize, rng: &mut Rng) -> Landed {
    let def = &data.wheels[odds.wheel];
    let o = &odds.options[option];
    if def.kind == Kind::Pick {
        return Landed {
            option,
            value: 0.0,
            level: o.level,
        };
    }
    let value = if o.min == o.max {
        o.min
    } else if def.log {
        let (lo, hi) = (o.min.log10(), o.max.log10());
        let level = match odds.target {
            Some((t, spread)) if o.share.is_none() => truncated_normal(t, spread, lo, hi, rng),
            _ => lo + (hi - lo) * rng.next_f32() as f64,
        };
        10f64.powf(level)
    } else {
        o.min + (o.max - o.min) * rng.next_f32() as f64
    };
    let level = if def.log { value.log10() } else { o.level };
    Landed {
        option,
        value,
        level,
    }
}

/// A number between `lo` and `hi`, most likely near `mean`.
fn truncated_normal(mean: f64, spread: f64, lo: f64, hi: f64, rng: &mut Rng) -> f64 {
    for _ in 0..64 {
        let x = lo + (hi - lo) * rng.next_f32() as f64;
        let d = (x - mean) / spread;
        // Accept by the bell's height, relative to its highest point inside the band.
        let peak = mean.clamp(lo, hi);
        let dp = (peak - mean) / spread;
        let p = (-0.5 * (d * d - dp * dp)).exp();
        if (rng.next_f32() as f64) < p {
            return x;
        }
    }
    mean.clamp(lo, hi)
}

/// Rolls every wheel not yet landed, in order: what Skip Spin does, and how
/// rivals are made.
pub fn roll_rest(
    data: &Data,
    map: usize,
    landed: &mut Vec<Landed>,
    others: &[&dyn Nation],
    rng: &mut Rng,
) {
    while landed.len() < data.wheels.len() {
        let o = odds(data, map, landed.len(), landed, others);
        let option = roll_option(&o, rng);
        let l = land(data, &o, option, rng);
        landed.push(l);
    }
}
