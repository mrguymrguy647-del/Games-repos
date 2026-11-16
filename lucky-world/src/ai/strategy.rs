//! Strategy, every few months (docs/AI.md, section 5): what the secret
//! service says about everyone, who is strong, who is a threat, who is too
//! strong, who is weak, and so what the nation is after.

use super::data::{DEBT, GROWTH, JOBS, SUPPORT, TRADE};
use super::{memory, AiData, Brain, Estimate, Geo, Opportunity, GOALS};
use crate::econ::{EconData, Economy};

/// A normal number (mean 0, sd 1) from the brain's own randomness.
fn normal(brain: &mut Brain) -> f64 {
    let u1 = (brain.rng.next_f32() as f64).max(1e-6);
    let u2 = brain.rng.next_f32() as f64;
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// What the brain can learn about nation `j`: public facts as they are, the
/// rest through its secret service, with errors (itself: the truth).
pub fn observe(brain: &mut Brain, e: &Economy, ed: &EconData, j: usize) -> Estimate {
    let n = &e.nations[j];
    let stock: f64 = n.stockpile.iter().zip(&e.world_price).map(|(q, p)| q * p).sum();
    let truth = Estimate {
        gdp: n.gdp,
        soldiers: n.army,
        stockpile: stock,
        unemployment: n.unemployment(ed),
        stability: n.stability,
        inflation: n.inflation,
        junk: n.rating >= 4,
        collapsed: false,
    };
    let mut x = truth;
    if j != brain.nation {
        let err = brain.intel_error * brain.intel_factor.get(j).copied().unwrap_or(1.0);
        x.gdp *= (err * normal(brain)).exp();
        x.soldiers *= (err * normal(brain)).exp();
        x.stockpile *= (err * normal(brain)).exp();
        x.unemployment = (x.unemployment * (err * normal(brain)).exp()).clamp(0.0, 1.0);
        x.stability = (x.stability * (err * normal(brain)).exp()).clamp(0.0, 100.0);
        x.inflation += err * 0.1 * normal(brain);
    }
    // The wheels' GDP is public: an estimate under a fifth of it is a collapse.
    x.collapsed = x.gdp < n.gdp_start / 5.0;
    x
}

/// The quarter's strategy.
pub fn think(brain: &mut Brain, ai: &AiData, geo: &Geo, e: &Economy, ed: &EconData) {
    let me = brain.nation;
    let count = e.nations.len();
    memory::fade(brain, ai);
    let estimates: Vec<Estimate> = (0..count).map(|j| observe(brain, e, ed, j)).collect();
    if !brain.estimates.iter().all(|x| x.gdp == 0.0) {
        brain.history.push(brain.estimates.clone());
        if brain.history.len() > 4 {
            brain.history.remove(0);
        }
    }
    brain.estimates = estimates;
    let est = &brain.estimates;
    let year_ago = brain.history.first().cloned();
    let p = &brain.personality.t;

    // Power, and shares of it.
    let power: Vec<f64> = est.iter().map(|x| x.power(ai).max(0.0)).collect();
    let total: f64 = power.iter().sum::<f64>().max(1.0);
    let share: Vec<f64> = power.iter().map(|x| x / total).collect();

    // Threats.
    let mut best: Option<(usize, f64)> = None;
    for j in 0..count {
        if j == me {
            continue;
        }
        let reach = (geo.proximity[me][j] + p.reach).clamp(0.05, 1.5);
        let mut hostility = (-brain.relations[j].regard).max(0.0);
        if e.nations[me].overlord == Some(j) {
            hostility += p.overlord;
        }
        let arming = match &year_ago {
            Some(old) if old[j].stockpile > 0.0 => 1.0 + (est[j].stockpile / old[j].stockpile - 1.0).clamp(0.0, 1.0),
            _ => 1.0,
        };
        let t = (share[j] / share[me].max(1e-6)).sqrt() * reach * (ai.threat_calm + hostility) * arming;
        if best.is_none_or(|(_, b)| t > b) {
            best = Some((j, t));
        }
    }
    brain.threat = best.map(|(j, _)| j);
    let top = best.map_or(0.0, |(_, t)| t);
    brain.threat_level = ((top - ai.threat_ok) / ai.threat_span).clamp(0.0, 1.0);

    // A nation too strong for everyone's comfort (perhaps the player).
    let mut order: Vec<usize> = (0..count).collect();
    order.sort_by(|&a, &b| share[b].total_cmp(&share[a]).then(a.cmp(&b)));
    let (first, second) = (order[0], order.get(1).copied().unwrap_or(order[0]));
    brain.hegemon = (share[first] > ai.hegemon_share && share[first] > ai.hegemon_lead * share[second]).then_some(first);
    brain.alliance_wishes.clear();
    brain.counter = None;
    if let Some(h) = brain.hegemon.filter(|&h| h != me) {
        brain.counter = Some(h);
        brain.threat_level = (brain.threat_level + (ai.hegemon_pressure + p.hegemon) * share[h] * 0.5).min(1.0);
        brain.alliance_wishes = (0..count).filter(|&j| j != me && j != h && geo.proximity[j][h] >= ai.threat_continent).collect();
    } else if let Some(j) = brain.threat.filter(|&j| brain.relations[j].regard < -0.5 && share[j] > share[me]) {
        brain.counter = Some(j);
    }

    // Weak neighbours: opportunities for the war phase.
    brain.opportunities.clear();
    for j in 0..count {
        if j == me || (geo.proximity[me][j] + p.reach) < ai.opp_reach || power[me] <= power[j] {
            continue;
        }
        let x = &est[j];
        let mut reasons = Vec::new();
        if x.unemployment > ai.opp_unemployment {
            reasons.push(format!("{:.0}% out of work", x.unemployment * 100.0));
        }
        if x.stability < ai.opp_stability {
            reasons.push(format!("unrest (stability {:.0})", x.stability));
        }
        if x.junk {
            reasons.push("junk credit".to_string());
        }
        if let Some(old) = &year_ago {
            if old[j].gdp > 0.0 && x.gdp < (1.0 - ai.opp_gdp_drop) * old[j].gdp {
                reasons.push(format!("GDP down {:.0}% in a year", (1.0 - x.gdp / old[j].gdp) * 100.0));
            }
        }
        if x.collapsed {
            reasons.push("collapsed".to_string());
        }
        if !reasons.is_empty() {
            brain.opportunities.push(Opportunity { target: j, day: e.day, ratio: power[me] / power[j].max(1.0), reasons });
        }
    }
    brain.opportunities.sort_by(|a, b| b.ratio.total_cmp(&a.ratio).then(a.target.cmp(&b.target)));

    // The goal.
    let needs = brain.needs;
    let crisis = needs[JOBS].max(needs[DEBT]).max(needs[SUPPORT]).max(brain.threat_level);
    let lead = share[me] - order.iter().filter(|&&j| j != me).map(|&j| share[j]).fold(0.0, f64::max);
    let trade_open = 1.0 - needs[TRADE];
    let scores = [
        p.goal[0] + 0.5 + 0.5 * needs[GROWTH],
        p.goal[1] + 0.6 * (1.0 - needs[DEBT]) + 0.4 * trade_open,
        p.goal[2] + 1.5 * lead.max(0.0) / share[me].max(1e-6) + if order[0] == me { 0.3 } else { 0.0 },
        p.goal[3] + 1.2 * crisis,
    ];
    let mut pick = brain.goal;
    for g in 0..GOALS.len() {
        if scores[g] > scores[pick] + ai.goal_stickiness {
            pick = g;
        }
    }
    brain.goal = pick;
    brain.goal_why = match pick {
        0 => format!("Room to grow (growth need {:.0}%)", needs[GROWTH] * 100.0),
        1 => format!("Sound money and open trade ({}% of its trade room free)", (trade_open * 100.0).round()),
        2 => format!("The strongest around ({:.0}% of the world's power)", share[me] * 100.0),
        _ => format!("In trouble: {}", worst_words(brain)),
    };

    // Who it would fight first, when there is war.
    brain.war_targets = brain.opportunities.iter().map(|o| o.target).take(3).collect();
    if pick == 2 {
        if let Some(c) = brain.counter {
            if !brain.war_targets.contains(&c) {
                brain.war_targets.push(c);
            }
        }
    }
}

fn worst_words(brain: &Brain) -> String {
    let mut worst = 0;
    for c in 0..brain.needs.len() {
        if brain.needs[c] > brain.needs[worst] {
            worst = c;
        }
    }
    if brain.threat_level > brain.needs[worst] {
        return "a threat next door".to_string();
    }
    let words = &brain.need_words[worst];
    if words.is_empty() { super::data::NEEDS[worst].to_string() } else { words.clone() }
}
