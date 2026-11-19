//! The underdog rule: a world rule (every nation, the player's or a brain's,
//! under the same terms) that keeps a terrible spin playable.
//!
//! On day 1 each nation's start is measured: its share of the world's GDP
//! and of its soldiers, against the median nation. A nation far below the
//! median is an *underdog*, with a strength `u` from 0 to 1 (how far below).
//! For its first `years`, an underdog gets:
//!
//! - **development aid** from a world fund, `aid × u` of its GDP a year,
//!   paid monthly into its treasury (the budget shows it as a line);
//! - **the world's eye**: for its first `grace_years` no brain starts a war
//!   on it (brains still answer wars it starts);
//! - **a rally round the flag**: its stability never falls below
//!   `rally × u` points.
//!
//! The help fades out linearly over its last third. Nothing here is hidden:
//! the country card and the help screen say who is an underdog and why.

use super::{EconData, Economy, News};

/// The rule's numbers (`data/victory.toml`, `[underdog]`).
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UnderdogRules {
    /// A nation whose start is below this share of the median is an underdog.
    pub below_median: f64,
    pub years: f64,
    pub grace_years: f64,
    /// Aid a year, as a share of its GDP, at full strength.
    pub aid: f64,
    /// Stability floor at full strength.
    pub rally: f64,
}

impl Default for UnderdogRules {
    fn default() -> UnderdogRules {
        UnderdogRules { below_median: 0.5, years: 12.0, grace_years: 5.0, aid: 0.06, rally: 25.0 }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Underdog {
    pub on: bool,
    pub rules: UnderdogRules,
    /// Each nation's strength as an underdog (0: not one).
    pub strength: Vec<f64>,
    /// Each nation's start, as a share of the median nation's.
    pub start: Vec<f64>,
    /// Aid paid so far (W$), for the stories.
    pub paid: Vec<f64>,
}

impl Underdog {
    /// Measures every nation's start and turns the rule on.
    pub fn start(e: &Economy, rules: UnderdogRules) -> Underdog {
        let n = e.nations.len();
        let gdp_total: f64 = e.nations.iter().map(|x| x.gdp).sum::<f64>().max(1.0);
        let army_total: f64 = e.nations.iter().map(|x| x.army).sum::<f64>().max(1.0);
        let power: Vec<f64> = e.nations.iter().map(|x| 0.65 * x.gdp / gdp_total + 0.35 * x.army / army_total).collect();
        let mut sorted = power.clone();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let median = sorted.get(n / 2).copied().unwrap_or(1.0).max(1e-9);
        let start: Vec<f64> = power.iter().map(|p| p / median).collect();
        let strength = start.iter().map(|&s| if s < rules.below_median { (1.0 - s / rules.below_median).clamp(0.0, 1.0).sqrt() } else { 0.0 }).collect();
        Underdog { on: true, rules, strength, start, paid: vec![0.0; n] }
    }

    /// How strong the help is for nation `i` today (fading over the last third).
    pub fn help(&self, i: usize, day: u32, ed: &EconData) -> f64 {
        if !self.on {
            return 0.0;
        }
        let u = self.strength.get(i).copied().unwrap_or(0.0);
        let years = day as f64 / ed.days_per_year as f64;
        let r = &self.rules;
        if u <= 0.0 || years >= r.years {
            return 0.0;
        }
        let fade_from = r.years * 2.0 / 3.0;
        let fade = if years <= fade_from { 1.0 } else { 1.0 - (years - fade_from) / (r.years - fade_from) };
        u * fade
    }

    /// Whether brains may not start a war on `i` today.
    pub fn protected(&self, i: usize, day: u32, ed: &EconData) -> bool {
        self.on && self.strength.get(i).is_some_and(|&u| u > 0.0) && (day as f64) < self.rules.grace_years * ed.days_per_year as f64
    }

    pub fn is_underdog(&self, i: usize) -> bool {
        self.on && self.strength.get(i).is_some_and(|&u| u > 0.0)
    }
}

impl Economy {
    /// Once a month: the world fund pays, and the rally holds stability up.
    pub(crate) fn underdog_month(&mut self, ed: &EconData) {
        if !self.underdog.on {
            return;
        }
        let day = self.day;
        for i in 0..self.nations.len() {
            let h = self.underdog.help(i, day, ed);
            if h <= 0.0 {
                continue;
            }
            let n = &mut self.nations[i];
            if n.population < 1.0 {
                continue;
            }
            let w = n.gdp * self.underdog.rules.aid * h / 12.0;
            let local = w / n.exchange.max(1e-12);
            n.treasury += local;
            n.budget_month.aid_in += local;
            n.stability = n.stability.max(self.underdog.rules.rally * h);
            self.underdog.paid[i] += w;
            if day == 30 {
                let text = format!("Underdog's luck: the world fund pledges {} a year to {} for its first years", crate::country::money(n.gdp * self.underdog.rules.aid * h), n.name);
                self.news.push(News { day, nation: Some(i), text, good: true });
            }
        }
    }
}
