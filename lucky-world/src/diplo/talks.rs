//! Peace talks (docs/DIPLOMACY.md, section 7): offers, counteroffers and
//! concessions, week by week, each side judging the war score through its
//! own intel; allies on the winning side add their own demands.

use std::collections::BTreeMap;

use super::Diplomacy;
use crate::ai::Brains;
use crate::econ::{EconData, Economy};
use crate::war::{peace, Wars, LEVELS};

/// One round of talks, for the log.
#[derive(Debug, Clone, PartialEq)]
pub struct Round {
    pub day: u32,
    pub war: usize,
    /// Who offered, to whom.
    pub from: usize,
    pub to: usize,
    /// The terms, in words, and the answer.
    pub offer: String,
    pub answer: String,
    pub agreed: bool,
}

/// Where the talks over one war stand.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TalkState {
    pub weeks: u32,
    /// Each side's demand, in its own view of the score.
    pub demand: [f64; 2],
    pub opener: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Talks {
    pub rounds: Vec<Round>,
    pub state: BTreeMap<usize, TalkState>,
}

/// A side's view of the score: its own losses it knows, the enemy's it
/// judges through its spies (a bad service is hopeful or gloomy).
fn view(d: &Diplomacy, w: &Wars, k: usize, s: usize, score0: f64) -> f64 {
    let lead = w.nation_of(w.wars[k].sides[s][0]);
    let err = if lead < d.n { d.intel_error[lead] * d.spies.intel_factor(d.day, lead, w.nation_of(w.wars[k].sides[1 - s][0]), &d.data) } else { 0.2 };
    let h = (k as u32).wrapping_mul(2_654_435_761) ^ (s as u32).wrapping_mul(40_503) ^ 0x7a1c;
    let u = ((h ^ (h >> 15)).wrapping_mul(2_246_822_519) & 0xffff) as f64 / 65535.0 - 0.5;
    let mine = if s == 0 { score0 } else { -score0 };
    mine + err * 100.0 * u
}

/// What a side must at least get to sign: its view, less what it gives up
/// to exhaustion and to a long negotiation.
fn reserve(d: &Diplomacy, w: &Wars, k: usize, s: usize, v: f64, weeks: u32) -> f64 {
    let t = &d.data.talks;
    let lead = w.wars[k].sides[s][0];
    let stake = w.data.peace.stake[w.wars[k].level.min(4)];
    v - t.give * w.forces[lead].exhaustion / stake - t.give_weeks * weeks as f64 / 52.0
}

/// The terms a side-0 score buys, in words.
fn words(w: &Wars, k: usize, score0: f64) -> String {
    peace::terms(w, k, score0).words(w)
}

/// A week of talks over every war.
pub fn week(d: &mut Diplomacy, e: &mut Economy, ed: &EconData, w: &mut Wars, b: Option<&Brains>) {
    let p = w.data.peace.clone();
    let day = d.day;
    for k in 0..w.wars.len() {
        let wr = &w.wars[k];
        if !wr.active() || wr.civil || wr.level == 0 {
            continue;
        }
        let (l0, l1) = (wr.sides[0][0], wr.sides[1][0]);
        if l0 >= d.n || l1 >= d.n {
            continue;
        }
        let score0 = peace::score(w, e, k);
        let views = [view(d, w, k, 0, score0), view(d, w, k, 1, score0)];
        let lv = wr.level.min(4);
        let years = (day - wr.start) as f64 / ed.days_per_year as f64;
        let old_enough = (day - wr.start) as f64 >= p.min_days;
        let risk = |f: usize| b.and_then(|b| b.brain(f)).map_or(0.1, |br| br.personality.t.risk);
        let wants = |s: usize| {
            let lead = wr.sides[s][0];
            let ex = w.forces[lead].exhaustion;
            let limit = p.offer_exhaustion * p.stake[lv] * (1.0 + risk(lead));
            let hopeless = views[s] < -p.hopeless[lv] && (day - wr.start) as f64 >= p.min_days * p.stake[lv];
            let won = views[s] > 40.0 && years > p.won_years[lv];
            ex > limit || (old_enough && (hopeless || won))
        };
        let state = d.talks.state.get(&k).copied();
        let st = match state {
            Some(mut st) => {
                st.weeks += 1;
                for s in 0..2 {
                    st.demand[s] -= d.data.talks.concession;
                }
                st
            }
            None => {
                let (w0, w1) = (wants(0), wants(1));
                if !w0 && !w1 {
                    continue;
                }
                let opener = if w0 && w1 { if views[0] < views[1] { 0 } else { 1 } } else if w0 { 0 } else { 1 };
                let greed = |s: usize| {
                    let lead = wr.sides[s][0];
                    d.data.talks.greed * (crate::war::escalation::aggression(w, lead, b) - 0.5)
                };
                let claims = |s: usize| 5.0 * (wr.sides[s].len() as f64 - 1.0);
                let demand = [views[0] + greed(0) + claims(0), views[1] + greed(1) + claims(1)];
                d.count("peace talks opened", 1.0);
                TalkState { weeks: 0, demand, opener }
            }
        };
        // The sides take turns to offer.
        let o = if st.weeks % 2 == 0 { st.opener } else { 1 - st.opener };
        let r = 1 - o;
        let (lo, lr) = (wr.sides[o][0], wr.sides[r][0]);
        let to_side0 = |s: usize, dm: f64| if s == 0 { dm } else { -dm };
        let offer0 = to_side0(o, st.demand[o]);
        d.talks.state.insert(k, st);
        if Some(lr) == d.player {
            // The player answers in the war room.
            let t = peace::terms(w, k, offer0);
            w.offers.retain(|x| x.0 != k);
            w.offers.push((k, o, t, day));
            log(d, k, lo, lr, words(w, k, offer0), "waiting for the player".to_string(), false);
            continue;
        }
        if Some(lo) == d.player {
            continue; // the player makes its own offers (the Talks page)
        }
        d.count("peace talk rounds", 1.0);
        let reserve_r = reserve(d, w, k, r, views[r], st.weeks);
        if -st.demand[o] >= reserve_r {
            log(d, k, lo, lr, words(w, k, offer0), "accepted".to_string(), true);
            conclude(d, e, ed, w, k, offer0, "a negotiated peace");
            continue;
        }
        // A counteroffer.
        let counter0 = to_side0(r, st.demand[r]);
        let reserve_o = reserve(d, w, k, o, views[o], st.weeks);
        if -st.demand[r] >= reserve_o {
            log(d, k, lo, lr, words(w, k, offer0), format!("countered with {}, and it was accepted", words(w, k, counter0)), true);
            conclude(d, e, ed, w, k, counter0, "a negotiated peace");
            continue;
        }
        let why = counter_reason(d, w, k, r, views[r]);
        log(d, k, lo, lr, words(w, k, offer0), format!("No: {why}. Counteroffer: {}", words(w, k, counter0)), false);
    }
}

/// Why a side won't sign yet, in words.
fn counter_reason(d: &Diplomacy, w: &Wars, k: usize, s: usize, v: f64) -> String {
    let wr = &w.wars[k];
    let held = (0..w.control.len()).filter(|&q| w.control[q].is_some_and(|c| wr.sides[s].contains(&c)) && w.owner[q].is_some_and(|o| wr.sides[1 - s].contains(&o))).count();
    let mine: f64 = wr.sides[s].iter().map(|&f| w.strength(f).0).sum();
    let theirs: f64 = wr.sides[1 - s].iter().map(|&f| w.strength(f).0).sum();
    let _ = d;
    if held > 0 {
        format!("we hold {held} of your provinces")
    } else if wr.sides[s].len() > 1 && v > 0.0 {
        "our allies want their share".to_string()
    } else if mine > theirs {
        "our army is still standing".to_string()
    } else if v > -20.0 {
        "you have not beaten us".to_string()
    } else {
        "those terms are too harsh".to_string()
    }
}

fn log(d: &mut Diplomacy, war: usize, from: usize, to: usize, offer: String, answer: String, agreed: bool) {
    d.talks.rounds.push(Round { day: d.day, war, from, to, offer, answer, agreed });
    if d.talks.rounds.len() > 800 {
        d.talks.rounds.drain(..200);
    }
}

/// Peace is agreed at a side-0 score: the lead's terms (WAR.md section 10),
/// and the winning allies' own demands.
pub fn conclude(d: &mut Diplomacy, e: &mut Economy, ed: &EconData, w: &mut Wars, k: usize, score0: f64, how: &str) {
    let t = peace::terms(w, k, score0);
    let wr = w.wars[k].clone();
    if let Some(ws) = t.winner {
        let losers: Vec<usize> = wr.sides[1 - ws].iter().copied().filter(|&f| f < d.n).collect();
        // Allies take the loser's provinces they hold next to their own land.
        let n = d.n;
        let allies: Vec<usize> = wr.sides[ws].iter().skip(1).copied().filter(|&x| x < n).collect();
        if score0.abs() >= w.data.peace.provinces {
            for &x in &allies {
                let mine = w.held(x);
                let claims: Vec<usize> = (0..w.control.len())
                    .filter(|&q| w.control[q] == Some(x) && w.owner[q].is_some_and(|o| losers.contains(&o)) && !w.capital.contains(&q))
                    .filter(|&q| w.geo.neighbours[q].iter().any(|r| mine.contains(r) && w.owner[*r] == Some(x)))
                    .collect();
                for q in claims {
                    let from = w.owner[q].unwrap();
                    let left = (0..w.owner.len()).filter(|&p| w.owner[p] == Some(from)).count();
                    if left <= 1 {
                        continue;
                    }
                    e.transfer_province(ed, q, from, x);
                    w.owner[q] = Some(x);
                    w.lost.push((from, q, w.day));
                    d.count("provinces to allies", 1.0);
                }
            }
        }
        // Allies take their share of reparations.
        if t.reparations > 0.0 {
            if let Some(&loser) = losers.first() {
                for &x in &allies {
                    w.reparations.push((loser, x, t.reparations * d.data.talks.ally_reparations, (t.years * ed.days_per_year as f64) as u32));
                    d.count("reparations to allies", 1.0);
                }
            }
        }
    }
    let level = LEVELS[wr.level.min(4)];
    d.count(&format!("negotiated peace: {}", level.to_lowercase()), 1.0);
    peace::make_peace(w, e, ed, k, t, how);
    d.talks.state.remove(&k);
}

/// The player offers terms (the Talks page): a demand from white peace (0)
/// to the war's goal (3). The other side answers now, with its reason.
pub fn player_offer(d: &mut Diplomacy, e: &mut Economy, ed: &EconData, w: &mut Wars, k: usize, level: usize) -> String {
    let Some(me) = d.player else { return String::new() };
    if k >= w.wars.len() || !w.wars[k].active() {
        return "That war is over".to_string();
    }
    let Some(s) = w.wars[k].side_of(me) else { return "You are not in that war".to_string() };
    let p = &w.data.peace;
    let demand = [0.0, p.white + 1.0, p.reparations + 1.0, p.provinces + 1.0][level.min(3)];
    let r = 1 - s;
    let score0 = peace::score(w, e, k);
    let v = view(d, w, k, r, score0);
    let weeks = d.talks.state.get(&k).map_or(0, |x| x.weeks);
    let offer0 = if s == 0 { demand } else { -demand };
    let them = w.wars[k].sides[r][0];
    d.count("peace talk rounds", 1.0);
    if -demand >= reserve(d, w, k, r, v, weeks) {
        log(d, k, me, them, words(w, k, offer0), "accepted".to_string(), true);
        let terms = words(w, k, offer0);
        conclude(d, e, ed, w, k, offer0, "a negotiated peace");
        return format!("{} accepts: {terms}", d.names[them]);
    }
    let why = counter_reason(d, w, k, r, v);
    log(d, k, me, them, words(w, k, offer0), format!("No: {why}"), false);
    format!("{} says no: {why}", d.names[them])
}
