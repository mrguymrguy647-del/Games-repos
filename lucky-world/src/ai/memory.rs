//! Memory (docs/AI.md, section 7): how every other nation has treated this
//! one, from what it can see (its own trade, embargoes, which are public).
//! Diplomacy and war will read it; strategy already uses `regard`.

use super::data::SUPPLY;
use super::{AiData, Brain};
use crate::econ::{EconData, Economy};

/// What happened between two nations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryKind {
    EmbargoedUs,
    LiftedEmbargo,
    TradeKept,
    TradeBroken,
    Helped,
}

impl MemoryKind {
    pub fn words(self) -> &'static str {
        match self {
            MemoryKind::EmbargoedUs => "embargoed us",
            MemoryKind::LiftedEmbargo => "lifted its embargo",
            MemoryKind::TradeKept => "kept trading with us for a year",
            MemoryKind::TradeBroken => "cut its trade with us",
            MemoryKind::Helped => "sold us what we were short of",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MemoryEvent {
    pub day: u32,
    pub kind: MemoryKind,
}

/// One nation's standing with another.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Relation {
    /// From -1 (hates) to +1 (trusts).
    pub regard: f64,
    /// The latest events, oldest first.
    pub log: Vec<MemoryEvent>,
}

/// What the brain saw last time, to notice changes.
#[derive(Debug, Clone)]
pub struct Watch {
    month: u32,
    embargoed_by: Vec<bool>,
    /// Their sales to us and ours to them, the last three months' average (W$).
    bought: Vec<f64>,
    sold: Vec<f64>,
    steady_months: Vec<u32>,
    last_help: Vec<u32>,
    last_broken: Vec<u32>,
}

impl Watch {
    pub fn new(count: usize) -> Watch {
        Watch {
            month: u32::MAX,
            embargoed_by: vec![false; count],
            bought: vec![0.0; count],
            sold: vec![0.0; count],
            steady_months: vec![0; count],
            last_help: vec![0; count],
            last_broken: vec![0; count],
        }
    }
}

fn remember(brain: &mut Brain, ai: &AiData, other: usize, day: u32, kind: MemoryKind, change: f64) {
    let r = &mut brain.relations[other];
    r.regard = (r.regard + change).clamp(-1.0, 1.0);
    r.log.push(MemoryEvent { day, kind });
    if r.log.len() > ai.mem_log {
        let extra = r.log.len() - ai.mem_log;
        r.log.drain(..extra);
    }
}

/// Weekly: notices embargoes at once, and trade kept, broken or helpful
/// once a month (when last month's books close).
pub fn watch(brain: &mut Brain, ai: &AiData, e: &Economy, _ed: &EconData) {
    let me = brain.nation;
    let day = e.day;
    for j in 0..e.nations.len() {
        if j == me {
            continue;
        }
        let on = e.nations[j].embargo.get(me).copied().unwrap_or(false);
        if on != brain.memory.embargoed_by[j] {
            brain.memory.embargoed_by[j] = on;
            if on {
                remember(brain, ai, j, day, MemoryKind::EmbargoedUs, ai.mem_embargo);
            } else {
                remember(brain, ai, j, day, MemoryKind::LiftedEmbargo, ai.mem_lift);
            }
        }
    }
    let month = day / 30;
    if month == brain.memory.month {
        return;
    }
    let first = brain.memory.month == u32::MAX;
    brain.memory.month = month;
    let n = &e.nations[me];
    let short = brain.needs[SUPPLY] > 0.3;
    // What it bought from everyone, on average: a partner is one that sold it
    // a real share of that.
    let all_bought: f64 = brain.memory.bought.iter().sum();
    for j in 0..e.nations.len() {
        if j == me {
            continue;
        }
        let bought = n.imports_from_last.get(j).copied().unwrap_or(0.0);
        let sold = n.exports_to_last.get(j).copied().unwrap_or(0.0);
        let (avg_b, avg_s) = (brain.memory.bought[j], brain.memory.sold[j]);
        if !first {
            // A partner's sales falling away (not ours to them, not an
            // embargo either way), at most once a year: trade swings from
            // month to month, and a swing isn't a betrayal.
            let we_cut = n.embargo[j] || sold < 0.2 * avg_s;
            let partner = avg_b > ai.mem_partner * all_bought;
            let fresh = brain.memory.last_broken[j] == 0 || month >= brain.memory.last_broken[j] + 12;
            if partner && fresh && bought < 0.2 * avg_b && !we_cut && !brain.memory.embargoed_by[j] {
                brain.memory.last_broken[j] = month;
                remember(brain, ai, j, day, MemoryKind::TradeBroken, ai.mem_broken);
            }
            if bought + sold > 0.0 {
                brain.memory.steady_months[j] += 1;
                if brain.memory.steady_months[j].is_multiple_of(12) {
                    remember(brain, ai, j, day, MemoryKind::TradeKept, ai.mem_kept);
                }
            } else {
                brain.memory.steady_months[j] = 0;
            }
            if short && bought > 0.0 && month >= brain.memory.last_help[j] + 3 {
                brain.memory.last_help[j] = month;
                remember(brain, ai, j, day, MemoryKind::Helped, ai.mem_help);
            }
        }
        brain.memory.bought[j] = if first { bought } else { (2.0 * avg_b + bought) / 3.0 };
        brain.memory.sold[j] = if first { sold } else { (2.0 * avg_s + sold) / 3.0 };
    }
}

/// Quarterly: old feelings fade.
pub fn fade(brain: &mut Brain, ai: &AiData) {
    for r in &mut brain.relations {
        r.regard *= 1.0 - ai.mem_decay;
    }
}
