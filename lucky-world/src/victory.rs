//! How a game is won and lost, and the story of it (docs/VICTORY.md).
//!
//! Wins:
//! - **Conquest**: hold `conquest_share` of the map's provinces.
//! - **Economic power**: a GDP `economic_lead` times the next biggest, for
//!   `economic_years` in a row.
//! - **Diplomatic**: lead an alliance (allies, defensive partners and
//!   vassals) that holds `diplomatic_share` of the map, with at least
//!   `diplomatic_allies` allies, as its richest member.
//! - **Score**: the best score when the game's length runs out.
//!
//! Losses: conquered (no provinces left), overthrown (rebels take the
//! capital in a civil war), or made a vassal. A vassal may play on and fight
//! for independence; winning it back clears the defeat.
//!
//! `Fate` also keeps the story: each of the player's wars, the best and
//! worst moments, and the nation's numbers year by year.

use crate::diplo::{Diplomacy, Kind};
use crate::econ::{EconData, Economy};
use crate::war::Wars;

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VictoryRules {
    pub conquest_share: f64,
    pub economic_lead: f64,
    pub economic_years: f64,
    pub diplomatic_share: f64,
    pub diplomatic_allies: usize,
    pub diplomatic_years: f64,
    pub score_gdp: f64,
    pub score_land: f64,
    pub score_people: f64,
    pub score_tech: f64,
    pub score_stability: f64,
    pub lengths: Vec<u32>,
    pub default_length: u32,
}

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VictoryData {
    pub victory: VictoryRules,
    pub underdog: crate::econ::underdog::UnderdogRules,
}

impl VictoryData {
    pub fn load() -> Result<VictoryData, String> {
        let path = "data/victory.toml";
        let text = crate::files::read_text(path).map_err(|e| e.to_string())?;
        Self::parse(&text).map_err(|e| format!("{path}: {e}"))
    }

    pub fn parse(text: &str) -> Result<VictoryData, String> {
        let d: VictoryData = toml::from_str(text).map_err(|e| e.message().to_string())?;
        let v = &d.victory;
        if !(0.05..=1.0).contains(&v.conquest_share) || !(0.05..=1.0).contains(&v.diplomatic_share) {
            return Err("conquest_share and diplomatic_share must be between 0.05 and 1".into());
        }
        if v.economic_lead < 1.0 {
            return Err("economic_lead must be at least 1".into());
        }
        if v.lengths.is_empty() || !v.lengths.contains(&v.default_length) {
            return Err(format!("default_length {} isn't one of the lengths {:?}", v.default_length, v.lengths));
        }
        Ok(d)
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Win {
    Conquest,
    Economic,
    Diplomatic,
    Score,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loss {
    Conquered,
    Overthrown,
    Vassal,
    /// The game's length ran out and someone else had the best score.
    OutScored,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Won(Win),
    Lost(Loss),
}

impl Outcome {
    pub fn title(self) -> &'static str {
        match self {
            Outcome::Won(Win::Conquest) => "VICTORY BY CONQUEST",
            Outcome::Won(Win::Economic) => "ECONOMIC VICTORY",
            Outcome::Won(Win::Diplomatic) => "DIPLOMATIC VICTORY",
            Outcome::Won(Win::Score) => "VICTORY ON SCORE",
            Outcome::Lost(Loss::Conquered) => "DEFEAT: CONQUERED",
            Outcome::Lost(Loss::Overthrown) => "DEFEAT: OVERTHROWN",
            Outcome::Lost(Loss::Vassal) => "DEFEAT: A VASSAL",
            Outcome::Lost(Loss::OutScored) => "TIME'S UP",
        }
    }

    pub fn won(self) -> bool {
        matches!(self, Outcome::Won(_))
    }

    /// Whether the player can keep going after this.
    pub fn can_play_on(self) -> bool {
        !matches!(self, Outcome::Lost(Loss::Conquered))
    }
}

/// One moment of the story, good (weight > 0) or bad.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct Moment {
    pub day: u32,
    pub text: String,
    pub weight: f64,
}

/// One of the player's wars, as the story tells it.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct WarStory {
    /// Index into `Wars::wars`.
    pub war: usize,
    pub enemy: String,
    pub we_started: bool,
    pub civil: bool,
    pub start: u32,
    pub end: Option<u32>,
    /// "won", "lost" or "drawn" once it's over, and how.
    pub result: Option<String>,
    pub won: Option<bool>,
    pub killed: f64,
}

/// The player's nation at the end of each year.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct YearStat {
    pub year: u32,
    pub gdp: f64,
    pub land: f64,
    pub score: f64,
    pub rank: usize,
    pub stability: f64,
}

/// How the game is going for the player.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Fate {
    /// The latest result, once decided.
    pub outcome: Option<Outcome>,
    /// The result screen was shown for `outcome` (the game may go on).
    pub shown: bool,
    /// The player chose to play on after a result.
    pub playing_on: bool,
    /// Wins already had (no second screen for the same one).
    pub wins: Vec<Win>,
    /// Days in a row with the economic lead, and leading the diplomatic bloc.
    pub lead_days: u32,
    pub bloc_days: u32,
    pub moments: Vec<Moment>,
    pub wars: Vec<WarStory>,
    pub years: Vec<YearStat>,
    /// Treaties already told (by id).
    told_treaties: usize,
    was_vassal: bool,
    /// The first day was seen (a colony starts under an overlord: that
    /// isn't a defeat, only becoming a vassal later is).
    #[serde(default)]
    seen_start: bool,
    /// Still the colony we began as, under the same overlord.
    #[serde(default)]
    born_colony: Option<usize>,
    first_place: bool,
    gdp_last_year: f64,
    /// The best and worst ranks so far, and the GDP the story last doubled from.
    best_rank: usize,
    worst_rank: usize,
    gdp_mark: f64,
    aid_told: bool,
}

/// What each nation's score is made of right now.
pub fn scores(v: &VictoryRules, e: &Economy, ed: &EconData, w: &Wars) -> Vec<f64> {
    let n = e.nations.len();
    let gdp_t: f64 = e.nations.iter().map(|x| x.gdp.max(0.0)).sum::<f64>().max(1.0);
    let pop_t: f64 = e.nations.iter().map(|x| x.population.max(0.0)).sum::<f64>().max(1.0);
    let prov = w.owner.len().max(1) as f64;
    let tech: Vec<f64> = e.nations.iter().map(|x| crate::econ::tech(ed, x)).collect();
    let tech_top = tech.iter().cloned().fold(1e-9, f64::max);
    (0..n)
        .map(|i| {
            let x = &e.nations[i];
            if x.population < 1.0 {
                return 0.0;
            }
            let land = w.owner.iter().filter(|o| **o == Some(i)).count() as f64 / prov;
            let mut s = v.score_gdp * x.gdp.max(0.0) / gdp_t + v.score_land * land + v.score_people * x.population / pop_t + v.score_tech * tech[i] / tech_top + v.score_stability * x.stability;
            // A vassal counts for half.
            if x.overlord.is_some() {
                s *= 0.5;
            }
            s
        })
        .collect()
}

/// The player's share of the map, and its bloc's (its allies by alliance
/// treaty and its vassals): (mine, the bloc's, allies, whether it leads).
pub fn shares(e: &Economy, w: &Wars, d: &Diplomacy, me: usize) -> (f64, f64, usize, bool) {
    let prov = w.owner.len().max(1) as f64;
    let n = e.nations.len();
    let allies: Vec<usize> = (0..n).filter(|&j| j != me && d.has(me, j, Kind::Alliance)).collect();
    let vassals: Vec<usize> = (0..n).filter(|&j| e.nations[j].overlord == Some(me)).collect();
    let mut bloc = vec![me];
    bloc.extend(&allies);
    bloc.extend(&vassals);
    bloc.sort_unstable();
    bloc.dedup();
    let mine = w.owner.iter().filter(|o| **o == Some(me)).count() as f64 / prov;
    let held = w.owner.iter().filter(|o| o.is_some_and(|x| bloc.contains(&x))).count() as f64 / prov;
    let leads = bloc.iter().all(|&j| j == me || e.nations[j].gdp <= e.nations[me].gdp);
    (mine, held, allies.len(), leads)
}

impl Fate {
    fn say(&mut self, day: u32, text: String, weight: f64) {
        self.moments.push(Moment { day, text, weight });
    }

    /// Once a day after the world's day: the story, then the result.
    /// Returns a new result the game should show.
    pub fn day(&mut self, v: &VictoryRules, years_limit: u32, e: &Economy, ed: &EconData, w: &Wars, d: &Diplomacy, me: usize) -> Option<Outcome> {
        let day = e.day;
        self.tell_wars(e, w, me, day);
        if day.is_multiple_of(30) {
            self.tell_treaties(d, me, day);
        }
        // The vassal's road: made one, and free again.
        let vassal = e.nations[me].overlord.is_some();
        if !self.seen_start {
            self.seen_start = true;
            self.was_vassal = vassal;
            self.born_colony = e.nations[me].overlord;
            if let Some(lord) = e.nations[me].overlord {
                self.say(day, format!("We began as a colony of {}", e.nations[lord].name), -10.0);
            }
        }
        if self.born_colony.is_some() && e.nations[me].overlord != self.born_colony {
            // Free, or handed to another master: no longer the colony we began as.
            self.born_colony = None;
            if vassal {
                let lord = e.nations[e.nations[me].overlord.unwrap()].name.clone();
                self.say(day, format!("We became a vassal of {lord}"), -60.0);
            }
        }
        if vassal && !self.was_vassal {
            let lord = e.nations[e.nations[me].overlord.unwrap()].name.clone();
            self.say(day, format!("We became a vassal of {lord}"), -60.0);
        } else if !vassal && self.was_vassal {
            self.say(day, "Independence! We are free again".to_string(), 55.0);
            if self.outcome == Some(Outcome::Lost(Loss::Vassal)) {
                self.outcome = None;
                self.shown = false;
            }
        }
        self.was_vassal = vassal;
        if day.is_multiple_of(ed.days_per_year) && day > 0 {
            self.year_end(v, e, ed, w, me, day);
        }
        // Economic lead, counted daily.
        let mine = e.nations[me].gdp;
        let next = (0..e.nations.len()).filter(|&j| j != me).map(|j| e.nations[j].gdp).fold(0.0, f64::max);
        if mine >= v.economic_lead * next && !vassal {
            self.lead_days += 1;
        } else {
            self.lead_days = 0;
        }
        // The diplomatic bloc, counted every 10 days.
        if day.is_multiple_of(10) {
            let (_, held, allies, leads) = shares(e, w, d, me);
            self.bloc_days = if held >= v.diplomatic_share && allies >= v.diplomatic_allies && leads && !vassal { self.bloc_days + 10 } else { 0 };
        }
        if !day.is_multiple_of(10) && !(years_limit > 0 && day == years_limit * ed.days_per_year) {
            return None;
        }
        let outcome = self.judge(v, years_limit, e, ed, w, d, me);
        if let Some(o) = outcome {
            if self.outcome == Some(o) {
                return None;
            }
            if let Outcome::Won(win) = o {
                if self.wins.contains(&win) {
                    return None;
                }
                self.wins.push(win);
            }
            self.outcome = Some(o);
            self.shown = false;
            let text = match o {
                Outcome::Won(Win::Conquest) => "Victory by conquest!".to_string(),
                Outcome::Won(Win::Economic) => "Economic victory!".to_string(),
                Outcome::Won(Win::Diplomatic) => "Diplomatic victory!".to_string(),
                Outcome::Won(Win::Score) => "Victory on score!".to_string(),
                Outcome::Lost(Loss::Conquered) => "Conquered: our last province fell".to_string(),
                Outcome::Lost(Loss::Overthrown) => "Overthrown: rebels took the capital".to_string(),
                Outcome::Lost(Loss::Vassal) => "Made a vassal".to_string(),
                Outcome::Lost(Loss::OutScored) => format!("Time's up: we finished number {}", self.years.last().map_or(0, |y| y.rank)),
            };
            self.say(day, text, if o.won() { 80.0 } else { -80.0 });
            return Some(o);
        }
        None
    }

    fn judge(&self, v: &VictoryRules, years_limit: u32, e: &Economy, ed: &EconData, w: &Wars, d: &Diplomacy, me: usize) -> Option<Outcome> {
        let day = e.day;
        // Losses first. Conquered: no province is ours any more (occupied
        // land is still ours until a peace deal hands it over).
        if !w.owner.iter().any(|o| *o == Some(me)) || e.nations[me].population < 1.0 {
            return Some(Outcome::Lost(Loss::Conquered));
        }
        let overthrown = w.wars.iter().any(|x| x.civil && x.sides[1].first() == Some(&me) && x.end.is_some_and(|end| day.saturating_sub(end) < 30) && x.result.as_deref().is_some_and(|r| r.contains("rebels take the capital")));
        if overthrown && self.outcome != Some(Outcome::Lost(Loss::Overthrown)) {
            return Some(Outcome::Lost(Loss::Overthrown));
        }
        if e.nations[me].overlord.is_some() && self.born_colony.is_none() {
            return Some(Outcome::Lost(Loss::Vassal));
        }
        let (mine, _, _, _) = shares(e, w, d, me);
        if mine >= v.conquest_share {
            return Some(Outcome::Won(Win::Conquest));
        }
        if self.lead_days as f64 >= v.economic_years * ed.days_per_year as f64 {
            return Some(Outcome::Won(Win::Economic));
        }
        if self.bloc_days as f64 >= v.diplomatic_years * ed.days_per_year as f64 {
            return Some(Outcome::Won(Win::Diplomatic));
        }
        if years_limit > 0 && day >= years_limit * ed.days_per_year {
            let s = scores(v, e, ed, w);
            let best = (0..s.len()).max_by(|&a, &b| s[a].total_cmp(&s[b]).then(b.cmp(&a))).unwrap_or(me);
            return Some(if best == me { Outcome::Won(Win::Score) } else { Outcome::Lost(Loss::OutScored) });
        }
        None
    }

    /// Wars the player is in: the start and the end of each.
    fn tell_wars(&mut self, e: &Economy, w: &Wars, me: usize, day: u32) {
        for (k, war) in w.wars.iter().enumerate() {
            let Some(side) = war.side_of(me) else { continue };
            let enemy_f = war.sides[1 - side].first().copied().unwrap_or(0);
            let enemy = if war.civil { "rebels".to_string() } else { w.names.get(enemy_f).cloned().unwrap_or_default() };
            let at = self.wars.iter().position(|s| s.war == k);
            let at = match at {
                Some(a) => a,
                None => {
                    let we_started = side == 0 && !war.civil;
                    let (text, weight) = if war.civil {
                        (format!("Civil war: {}", war.cause), -35.0)
                    } else if we_started {
                        (format!("We went to war with {enemy} ({})", war.motive), -2.0)
                    } else {
                        (format!("{enemy} attacked us"), -15.0)
                    };
                    self.say(war.start, text, weight);
                    self.wars.push(WarStory { war: k, enemy, we_started, civil: war.civil, start: war.start, end: None, result: None, won: None, killed: 0.0 });
                    self.wars.len() - 1
                }
            };
            let lost_men = war.losses[side].killed;
            let s = &mut self.wars[at];
            s.killed = lost_men;
            if s.end.is_none() && war.end.is_some() {
                s.end = war.end;
                let winner = war.peace.as_ref().and_then(|p| p.winner);
                // A civil war's side 0 is the rebels.
                let won = winner.map(|x| x == side);
                s.won = won;
                let how = war.result.clone().unwrap_or_default();
                s.result = Some(match won {
                    Some(true) => format!("won: {how}"),
                    Some(false) => format!("lost: {how}"),
                    None => format!("drawn: {how}"),
                });
                let enemy = s.enemy.clone();
                let provinces = war.peace.as_ref().map_or(0, |p| p.provinces.len()) as f64;
                let (text, weight) = match won {
                    Some(true) => (format!("Victory over {enemy}: {how}"), 25.0 + 5.0 * provinces),
                    Some(false) => (format!("Defeat by {enemy}: {how}"), -25.0 - 5.0 * provinces),
                    None => (format!("Peace with {enemy}: {how}"), 3.0),
                };
                let _ = e;
                self.say(day.max(war.end.unwrap_or(day)), text, weight);
            }
        }
    }

    fn tell_treaties(&mut self, d: &Diplomacy, me: usize, day: u32) {
        let from = self.told_treaties.min(d.treaties.len());
        for t in &d.treaties[from..] {
            if !t.involves(me) {
                continue;
            }
            let other = d.names.get(t.other(me)).cloned().unwrap_or_default();
            let weight = match t.kind {
                Kind::Alliance => 12.0,
                Kind::Defensive => 8.0,
                Kind::Guarantee | Kind::Trade => 4.0,
                Kind::Sanctions => -10.0,
                _ => 0.0,
            };
            if weight != 0.0 {
                self.say(t.start.max(day.saturating_sub(30)), format!("{} with {other}", t.kind.name()), weight);
            }
        }
        self.told_treaties = d.treaties.len();
    }

    fn year_end(&mut self, v: &VictoryRules, e: &Economy, ed: &EconData, w: &Wars, me: usize, day: u32) {
        let s = scores(v, e, ed, w);
        let rank = 1 + (0..s.len()).filter(|&j| j != me && s[j] > s[me]).count();
        let gdp = e.nations[me].gdp;
        let land = w.owner.iter().filter(|o| **o == Some(me)).count() as f64 / w.owner.len().max(1) as f64;
        let year = day / ed.days_per_year;
        if self.gdp_last_year > 0.0 {
            let g = gdp / self.gdp_last_year - 1.0;
            if g > 0.08 {
                self.say(day, format!("A boom year: the economy grew {:.0}%", g * 100.0), 100.0 * g);
            } else if g < -0.05 {
                self.say(day, format!("A bad year: the economy shrank {:.0}%", -g * 100.0), 100.0 * g);
            }
        }
        if rank == 1 && !self.first_place {
            self.first_place = true;
            self.say(day, "We became the world's leading nation".to_string(), 30.0);
        }
        // New heights and new lows in the world's ranking.
        let n = s.len();
        if self.best_rank == 0 {
            self.best_rank = rank;
            self.worst_rank = rank;
        } else if rank < self.best_rank {
            self.say(day, format!("We rose to number {rank} of {n} in the world"), 4.0 * (self.best_rank - rank) as f64 + 6.0);
            self.best_rank = rank;
        } else if rank > self.worst_rank {
            self.say(day, format!("We fell to number {rank} of {n} in the world"), -(4.0 * (rank - self.worst_rank) as f64 + 6.0));
            self.worst_rank = rank;
        }
        // The economy doubled (from the start, or the last doubling).
        if self.gdp_mark <= 0.0 {
            self.gdp_mark = e.nations[me].gdp_start.max(1.0);
        }
        if gdp >= 2.0 * self.gdp_mark {
            self.say(day, format!("Our economy doubled, to {}", crate::country::money(gdp)), 20.0);
            self.gdp_mark = gdp;
        }
        if !self.aid_told && e.underdog.paid.get(me).is_some_and(|&p| p > 0.0) {
            self.aid_told = true;
            self.say(day, "The world fund's aid helped our weak start (the underdog rule)".to_string(), 8.0);
        }
        self.gdp_last_year = gdp;
        self.years.push(YearStat { year, gdp, land, score: s[me], rank, stability: e.nations[me].stability });
    }

    /// The best (`good`) or worst moments, most striking first.
    pub fn top(&self, good: bool, count: usize) -> Vec<&Moment> {
        let mut m: Vec<&Moment> = self.moments.iter().filter(|x| if good { x.weight > 0.0 } else { x.weight < 0.0 }).collect();
        m.sort_by(|a, b| b.weight.abs().total_cmp(&a.weight.abs()).then(a.day.cmp(&b.day)));
        m.truncate(count);
        m
    }
}
