//! Treaties (docs/DIPLOMACY.md, sections 3-5 and 10): what a deal is
//! worth to each side and why, signing it, keeping it, breaking it; loans,
//! arms deals and sanctions; and the brains' monthly deals.

use super::{Answer, Diplomacy, EventKind, Inbox, Kind, Proposal, State, Treaty};
use crate::ai::data::{DEBT, SECURITY};
use crate::ai::Brains;
use crate::econ::{EconData, Economy};
use crate::war::Wars;

/// One weighed reason: what it is in words, and how much it counts.
pub type Reason = (String, f64);

/// What `x` sees of `y`'s power (its own: the truth; another's: its estimate).
fn seen_power(d: &Diplomacy, e: &Economy, b: Option<&Brains>, x: usize, y: usize) -> f64 {
    let truth = d.power(e, y);
    match b.and_then(|b| b.brain(x)) {
        Some(br) if x != y && br.estimates[y].gdp > 0.0 => (br.estimates[y].gdp + 60_000.0 * br.estimates[y].soldiers).max(1.0),
        _ => truth.max(1.0),
    }
}

/// The power `y` is at war with, or fears (what an ally of `y` is dragged into).
fn trouble(d: &Diplomacy, e: &Economy, w: &Wars, b: Option<&Brains>, x: usize, y: usize) -> f64 {
    let mut sum = 0.0;
    for z in 0..e.nations.len() {
        if z != x && z != y && w.at_war(y, z).is_some_and(|k| w.wars[k].level >= 1) {
            sum += seen_power(d, e, b, x, z);
        }
    }
    if let Some(t) = b.and_then(|b| b.brain(y)).and_then(|br| br.threat).filter(|&t| t != x) {
        let lvl = b.and_then(|b| b.brain(y)).map_or(0.0, |br| br.threat_level);
        sum += 0.5 * lvl * seen_power(d, e, b, x, t);
    }
    sum
}

fn trade_share(e: &Economy, x: usize, y: usize) -> f64 {
    let n = &e.nations[x];
    let total: f64 = n.imports_from_last.iter().sum::<f64>() + n.exports_to_last.iter().sum::<f64>();
    if total <= 0.0 { 0.0 } else { (n.imports_from_last[y] + n.exports_to_last[y]) / total }
}

/// The stockpile of a good against what the army would want (days of war).
fn cover(d: &Diplomacy, e: &Economy, ed: &EconData, x: usize, g: usize) -> f64 {
    let (soldiers, tanks) = d.armies[x];
    let want = if g == ed.tanks { tanks * 0.3 + 20.0 } else if g == ed.ammo { soldiers * 0.05 + 100.0 } else { soldiers * 0.0005 + 5.0 };
    e.nations[x].stockpile[g] / want.max(1.0)
}

/// What a proposal is worth to `x` (one of its two parties), and why
/// (docs/DIPLOMACY.md, section 4).
pub fn value(d: &Diplomacy, e: &Economy, ed: &EconData, w: &Wars, b: Option<&Brains>, x: usize, p: &Proposal) -> (f64, Vec<Reason>) {
    let a = &d.data.answers;
    let y = if x == p.from { p.to } else { p.from };
    let mut why: Vec<Reason> = Vec::new();
    if w.at_war(x, y).is_some() {
        why.push(("we are at war".to_string(), -5.0));
        return (-5.0, why);
    }
    // What everyone weighs: how they feel, whether the other keeps its word.
    let rel = d.relations[x][y] / 100.0 * a.relations_weight;
    why.push((if rel >= 0.0 { "we are on good terms".to_string() } else { "we don't like you".to_string() }, rel));
    let trust = (d.reputation[y] - d.data.reputation.start) / 50.0 * a.trust_weight;
    why.push((if trust >= 0.0 { "you keep your word".to_string() } else { "you are not to be trusted".to_string() }, trust));
    // What it remembers of this nation's word, lately.
    let recent = |k: EventKind| d.memory[x][y].iter().any(|m| m.kind == k && d.day.saturating_sub(m.day) < 3600);
    if recent(EventKind::BrokeTreatyWithAlly) {
        why.push(("you broke a pact with our ally".to_string(), -0.3));
    }
    if recent(EventKind::BrokeTreatyWithUs) {
        why.push(("you broke your word to us".to_string(), -0.5));
    }
    if recent(EventKind::RefusedCall) {
        why.push(("you left us alone in our war".to_string(), -0.5));
    }
    let t = d.traits[x];
    if p.kind != Kind::Arms && p.kind != Kind::Loan && t.love.abs() > 0.0 {
        why.push((if t.love > 0.0 { "we believe in treaties".to_string() } else { "we keep a free hand".to_string() }, t.love));
    }
    let bx = b.and_then(|b| b.brain(x));
    let by = b.and_then(|b| b.brain(y));
    let (px, py) = (seen_power(d, e, b, x, x), seen_power(d, e, b, x, y));
    let threat_level = bx.map_or(0.3, |br| br.threat_level);
    let fear = match bx {
        Some(br) if br.threat == Some(y) => br.threat_level + 0.2,
        _ => {
            if py > px && d.relations[x][y] < 0.0 { 0.15 } else { 0.0 }
        }
    };
    let designs = bx.is_some_and(|br| br.opportunities.iter().any(|o| o.target == y) || br.war_targets.contains(&y));
    let shared = matches!((bx.and_then(|br| br.threat), by.and_then(|br| br.threat)), (Some(t1), Some(t2)) if t1 == t2 && t1 != x && t1 != y);
    let friends = d.ideology[x] == d.ideology[y] && d.data.relations.ideological_friends.contains(&d.ideology[x]);
    let protection = 1.0 + t.protection;
    match p.kind {
        Kind::Trade => {
            let pot = trade_share(e, x, y) + 0.02;
            why.push((if pot > 0.1 { "we trade a lot".to_string() } else { "there is trade to be had".to_string() }, a.trade_gain * pot));
            if d.ideology[x] == "communism" {
                why.push(("a planned economy trades little".to_string(), -a.planned_cost));
            }
        }
        Kind::NonAggression => {
            why.push(("we fear you".to_string(), fear * protection + 0.1));
            if designs {
                why.push(("we have plans for your land".to_string(), -a.designs_cost));
            }
        }
        Kind::Defensive | Kind::Alliance => {
            let alliance = p.kind == Kind::Alliance;
            let help = (py / px).min(2.0) * 0.3 * (threat_level + 0.2) * protection;
            why.push(("you would protect us".to_string(), help));
            if shared {
                why.push(("we fear the same enemy".to_string(), a.shared_threat));
            }
            if friends {
                why.push((if d.ideology[x] == "communism" { "comrades in the revolution".to_string() } else { "brothers in faith".to_string() }, a.friends));
            }
            let drag = trouble(d, e, w, b, x, y) / px * a.dragged * if alliance { a.alliance_drag } else { 1.0 };
            if drag > 0.01 {
                why.push(("you would drag us into your wars".to_string(), -drag.min(3.0)));
            }
            if alliance && d.relations[x][y] < 20.0 {
                why.push(("we are not that close".to_string(), -0.5));
            }
            if designs {
                why.push(("we have plans for your land".to_string(), -a.designs_cost));
            }
        }
        Kind::Access => {
            if d.allied(x, y) {
                why.push(("we are allies".to_string(), 0.4));
            } else {
                why.push(("only allies cross our land".to_string(), -0.4));
            }
            why.push(("foreign armies on our roads".to_string(), -a.access_cost));
        }
        Kind::Guarantee => {
            if x == p.to {
                // The guaranteed.
                why.push(("you would protect us".to_string(), a.guarantee_gain * (threat_level + 0.1) * protection));
            } else {
                why.push(("our influence grows".to_string(), 0.2));
                let drag = trouble(d, e, w, b, x, y) / px;
                if drag > 0.01 {
                    why.push(("your enemies are too strong".to_string(), -drag.min(3.0)));
                }
            }
        }
        Kind::Arms => {
            let g = p.good.unwrap_or(ed.tanks);
            if x == p.from {
                // The buyer.
                let need = if w.in_any_war(x) { 1.0 } else { threat_level };
                why.push(("we need weapons".to_string(), need * (1.0 - cover(d, e, ed, x, g).min(1.0)) + 0.1));
                if e.nations[x].treasury < 0.0 {
                    why.push(("we can't pay for them".to_string(), -0.6));
                }
            } else {
                // The seller.
                let surplus = (cover(d, e, ed, x, g) - 1.0).clamp(-1.0, 1.0);
                why.push((if surplus > 0.0 { "we have weapons to spare".to_string() } else { "we need our weapons".to_string() }, 0.3 + 0.5 * surplus));
                if e.nations[x].stockpile[g] < p.amount {
                    why.push(("we don't have that many".to_string(), -1.0));
                }
                let enemies_are_friends = (0..d.n).any(|z| w.at_war(y, z).is_some() && (d.allied(x, z) || d.relations[x][z] > 30.0));
                if enemies_are_friends {
                    why.push(("they would be used against our friends".to_string(), -a.arms_enemy_cost));
                }
            }
        }
        Kind::Loan => {
            if x == p.from {
                let need = bx.map_or(0.5, |br| br.needs[DEBT]);
                why.push(("we need money".to_string(), need));
            } else {
                why.push(("the interest".to_string(), 0.3));
                if e.nations[y].rating >= 4 {
                    why.push(("you can't pay it back".to_string(), -0.6));
                }
                if e.nations[x].treasury * e.nations[x].exchange < p.amount {
                    why.push(("we can't spare it".to_string(), -2.0));
                }
            }
        }
        Kind::Sanctions => {
            if let Some(target) = p.target {
                let hate = (a.sanction_hate - d.relations[x][target]) / 50.0;
                why.push((if hate > 0.0 { format!("{} deserves it", d.names[target]) } else { format!("we have no quarrel with {}", d.names[target]) }, hate));
                let ts = trade_share(e, x, target);
                if ts > 0.01 {
                    why.push((format!("we trade with {}", d.names[target]), -2.0 * ts));
                }
            }
        }
    }
    let _ = SECURITY;
    let total: f64 = why.iter().map(|r| r.1).sum();
    (total, why)
}

/// `x` answers a proposal: yes or no, and the reason (its biggest term).
pub fn answer(d: &Diplomacy, e: &Economy, ed: &EconData, w: &Wars, b: Option<&Brains>, x: usize, p: &Proposal) -> (bool, String) {
    let (v, why) = value(d, e, ed, w, b, x, p);
    let yes = v > d.data.answers.accept_threshold;
    let reason = if yes {
        why.iter().filter(|r| r.1 > 0.0).max_by(|a, b| a.1.total_cmp(&b.1)).map_or("it suits us".to_string(), |r| r.0.clone())
    } else {
        why.iter().filter(|r| r.1 < 0.0).min_by(|a, b| a.1.total_cmp(&b.1)).map_or("it isn't worth it for us".to_string(), |r| r.0.clone())
    };
    (yes, reason)
}

/// Whether a proposal can be made at all (and why not).
pub fn allowed(d: &Diplomacy, e: &Economy, w: &Wars, p: &Proposal) -> Result<(), String> {
    if p.from == p.to || p.from >= d.n || p.to >= d.n {
        return Err("Not with yourself".to_string());
    }
    if w.at_war(p.from, p.to).is_some() {
        return Err("You are at war: make peace first".to_string());
    }
    match p.kind {
        Kind::Arms | Kind::Loan | Kind::Sanctions => {}
        Kind::Defensive if d.has(p.from, p.to, Kind::Alliance) => return Err("You are already allies".to_string()),
        k => {
            if d.has(p.from, p.to, k) {
                return Err(format!("You already have a {}", k.name().to_lowercase()));
            }
        }
    }
    if p.kind == Kind::Sanctions && p.target.is_none_or(|t| t == p.from || t == p.to) {
        return Err("Sanctions need a target".to_string());
    }
    if p.kind == Kind::Loan && p.amount <= 0.0 {
        return Err("A loan needs an amount".to_string());
    }
    let _ = e;
    Ok(())
}

/// A proposal is made: to the player, it waits in the inbox; to a brain,
/// it is answered now. Returns the answer in words.
pub fn propose(d: &mut Diplomacy, e: &mut Economy, ed: &EconData, w: &mut Wars, b: Option<&Brains>, p: Proposal) -> (bool, String) {
    if let Err(why) = allowed(d, e, w, &p) {
        return (false, why);
    }
    d.count(&format!("proposed: {}", p.kind.key()), 1.0);
    if Some(p.to) == d.player {
        if !d.inbox.iter().any(|x| matches!(x, Inbox::Proposal(q) if q.from == p.from && q.kind == p.kind)) {
            d.inbox.push(Inbox::Proposal(p));
        }
        return (false, "waiting for the player".to_string());
    }
    let (yes, reason) = answer(d, e, ed, w, b, p.to, &p);
    if !yes {
        d.refusals.insert((p.from, p.to, p.kind), d.day);
    }
    d.answers.push(Answer { day: d.day, from: p.from, to: p.to, kind: p.kind, yes, reason: reason.clone() });
    if d.answers.len() > 600 {
        d.answers.drain(..200);
    }
    if yes {
        sign(d, e, ed, w, &p);
    }
    let words = format!("{}: {reason}", if yes { "Yes" } else { "No" });
    (yes, words)
}

/// Signs a treaty and does what it does at once (docs/DIPLOMACY.md, section 3).
pub fn sign(d: &mut Diplomacy, e: &mut Economy, ed: &EconData, w: &mut Wars, p: &Proposal) -> usize {
    let day = d.day;
    let id = d.treaties.len();
    let years = d.data.years(p.kind);
    let mut t = Treaty {
        id,
        kind: p.kind,
        a: p.from,
        b: p.to,
        members: Vec::new(),
        target: None,
        start: day,
        end: day + (years * 360.0) as u32,
        state: State::Active,
        amount: 0.0,
        rate: 0.0,
        owed: 0.0,
        missed: 0,
        good: None,
    };
    match p.kind {
        Kind::Alliance => {
            // An alliance takes the place of a defensive pact.
            for k in d.active.clone() {
                let x = &mut d.treaties[k];
                if x.active() && x.kind == Kind::Defensive && x.between(p.from, p.to) {
                    x.state = State::Ended;
                }
            }
        }
        Kind::Arms => {
            // The buyer is `from`, the seller `to`.
            let (buyer, seller) = (p.from, p.to);
            let g = p.good.unwrap_or(ed.tanks);
            let q = p.amount.min((e.nations[seller].stockpile[g] * d.data.treaties.arms_share).floor()).max(0.0);
            let price = q * e.world_price[g] * d.data.treaties.arms_markup;
            let fq = e.nations[seller].stockpile_faulty[g];
            e.nations[seller].stockpile[g] -= q;
            w.forces[seller].flows.given[g] += q;
            w.store(e, buyer, g, q, fq);
            w.forces[buyer].flows.received[g] += q;
            let (eb, es) = (e.nations[buyer].exchange.max(1e-12), e.nations[seller].exchange.max(1e-12));
            e.nations[buyer].treasury -= price / eb;
            e.nations[seller].treasury += price / es;
            t.good = Some(g);
            t.rate = q;
            t.amount = price;
            t.a = seller;
            t.b = buyer;
            t.state = State::Expired;
            t.end = day;
            d.remember(buyer, seller, EventKind::SoldArms);
            for z in 0..d.n {
                if w.at_war(buyer, z).is_some() {
                    d.remember(z, seller, EventKind::ArmedEnemy);
                }
            }
            d.count("arms sold (W$ bn)", price / 1e9);
        }
        Kind::Loan => {
            // The borrower is `from`, the lender `to`.
            let (borrower, lender) = (p.from, p.to);
            let tr = &d.data.treaties;
            let rate = tr.loan_rate + (1.0 - d.reputation[borrower] / 100.0) * tr.loan_risk;
            let amount = p.amount;
            e.nations[lender].treasury -= amount / e.nations[lender].exchange.max(1e-12);
            e.nations[borrower].treasury += amount / e.nations[borrower].exchange.max(1e-12);
            t.a = lender;
            t.b = borrower;
            t.amount = amount;
            t.owed = amount;
            t.rate = rate;
            t.end = day + (tr.loan_years * 360.0) as u32;
            d.remember(borrower, lender, EventKind::LentMoney);
            d.count("lent (W$ bn)", amount / 1e9);
        }
        Kind::Sanctions => {
            let target = p.target.unwrap_or(p.to);
            // Joining sanctions already in force.
            if let Some(x) = d.treaties.iter_mut().find(|x| x.active() && x.kind == Kind::Sanctions && x.target == Some(target)) {
                for m in [p.from, p.to] {
                    if !x.members.contains(&m) {
                        x.members.push(m);
                    }
                    e.nations[m].embargo[target] = true;
                }
                return x.id;
            }
            t.members = vec![p.from, p.to];
            t.target = Some(target);
            for m in [p.from, p.to] {
                e.nations[m].embargo[target] = true;
            }
        }
        _ => {}
    }
    let words = format!("{}: {}", p.kind.name(), t.words(&d.names));
    let who = vec![t.a, t.b];
    d.treaties.push(t);
    d.reindex();
    d.count(&format!("signed: {}", p.kind.key()), 1.0);
    if !matches!(p.kind, Kind::Arms) {
        d.say(who, words);
    }
    id
}

/// A treaty is broken by `by` (docs/DIPLOMACY.md, section 5): its
/// reputation falls with everyone, and the victims remember.
pub fn break_treaty(d: &mut Diplomacy, e: &mut Economy, id: usize, by: usize, how: &str) {
    if !d.treaties[id].active() {
        return;
    }
    let kind = d.treaties[id].kind;
    let day = d.day;
    d.treaties[id].state = State::Broken { by, day, how: how.to_string() };
    d.broken.push((by, day));
    d.reindex();
    let cost = d.data.break_cost(kind);
    d.reputation[by] = (d.reputation[by] - cost).max(0.0);
    let victims: Vec<usize> = {
        let t = &d.treaties[id];
        let mut v: Vec<usize> = vec![t.a, t.b];
        v.extend(t.members.iter().copied());
        v.retain(|&x| x != by);
        v.sort_unstable();
        v.dedup();
        v
    };
    for &v in &victims {
        d.remember(v, by, EventKind::BrokeTreatyWithUs);
        // The victim's allies remember too.
        let allies: Vec<usize> = (0..d.n).filter(|&z| z != by && z != v && d.allied(z, v)).collect();
        for z in allies {
            d.remember(z, by, EventKind::BrokeTreatyWithAlly);
        }
    }
    if kind == Kind::Sanctions {
        if let Some(t) = d.treaties[id].target {
            e.nations[by].embargo[t] = false;
        }
    }
    d.count(&format!("broken: {}", kind.key()), 1.0);
    let names = victims.iter().map(|&v| d.names[v].clone()).collect::<Vec<_>>().join(", ");
    let text = format!("{} breaks its {} with {names} ({how})", d.names[by], kind.name().to_lowercase());
    let mut who = victims;
    who.push(by);
    d.say(who, text);
}

/// Ends a treaty early by agreement (or because a war ended it).
pub fn end_treaty(d: &mut Diplomacy, e: &mut Economy, id: usize) {
    if !d.treaties[id].active() {
        return;
    }
    d.treaties[id].state = State::Ended;
    lift(d, e, id);
    d.reindex();
}

fn lift(d: &Diplomacy, e: &mut Economy, id: usize) {
    let t = &d.treaties[id];
    if t.kind == Kind::Sanctions {
        if let Some(target) = t.target {
            for &m in &t.members {
                let other = d.live().any(|x| x.id != id && x.kind == Kind::Sanctions && x.target == Some(target) && x.members.contains(&m));
                if !other {
                    e.nations[m].embargo[target] = false;
                }
            }
        }
    }
}

/// The month: treaties expire, loans are paid (or not), the market learns
/// who has trade agreements.
pub fn month(d: &mut Diplomacy, e: &mut Economy, ed: &EconData, w: &mut Wars) {
    let day = d.day;
    for id in d.active.clone() {
        if !d.treaties[id].active() {
            continue;
        }
        if d.treaties[id].kind == Kind::Loan {
            let (lender, borrower) = (d.treaties[id].a, d.treaties[id].b);
            let t = &d.treaties[id];
            let months = (d.data.treaties.loan_years * 12.0).max(1.0);
            let principal = (t.amount / months).min(t.owed);
            let pay = principal + t.owed * t.rate / 12.0;
            let eb = e.nations[borrower].exchange.max(1e-12);
            // It pays (borrowing on the bond market if it must) unless it is
            // in default itself.
            let in_default = e.nations[borrower].rating + 1 >= ed.ratings.len();
            if !in_default {
                e.nations[borrower].treasury -= pay / eb;
                e.nations[lender].treasury += pay / e.nations[lender].exchange.max(1e-12);
                let t = &mut d.treaties[id];
                t.owed -= principal;
                t.missed = 0;
                if t.owed <= 1.0 {
                    t.state = State::Expired;
                    d.count("loans repaid", 1.0);
                }
            } else {
                d.treaties[id].missed += 1;
                if d.treaties[id].missed >= d.data.treaties.default_months {
                    d.remember(lender, borrower, EventKind::Defaulted);
                    break_treaty(d, e, id, borrower, "defaulted on the loan");
                }
            }
            continue;
        }
        if d.treaties[id].end <= day {
            d.treaties[id].state = State::Expired;
            lift(d, e, id);
        }
    }
    d.reindex();
    // Sanctions stay in force while they last.
    for t in d.live().filter(|t| t.kind == Kind::Sanctions) {
        if let Some(target) = t.target {
            for &m in &t.members {
                e.nations[m].embargo[target] = true;
            }
        }
    }
    // The market: who trades without tariffs.
    let n = d.n;
    let mut pact = vec![vec![false; n]; n];
    for t in d.live().filter(|t| t.kind == Kind::Trade) {
        pact[t.a][t.b] = true;
        pact[t.b][t.a] = true;
    }
    e.trade_pact = pact;
    e.trade_pref = d.data.treaties.trade_pref;
    let _ = w;
}

/// A brain's month of deals (docs/DIPLOMACY.md, section 10).
pub fn think(d: &mut Diplomacy, e: &mut Economy, ed: &EconData, w: &mut Wars, b: &Brains, i: usize) {
    let Some(brain) = b.brain(i) else { return };
    let threat_level = brain.threat_level;
    let debt = brain.needs[DEBT];
    let threat = brain.threat;
    let day = d.day;
    // Not the same refused proposal again within a year.
    let refused = |d: &Diplomacy, to: usize, k: Kind| d.refusals.get(&(i, to, k)).is_some_and(|&r| day.saturating_sub(r) < 360);
    // 1. The treaty it values most, with whoever.
    let mut best: Option<(f64, Proposal)> = None;
    for j in 0..d.n {
        if j == i || w.at_war(i, j).is_some() || d.armies[j].0 <= 0.0 && e.nations[j].gdp <= 0.0 {
            continue;
        }
        for kind in [Kind::Trade, Kind::NonAggression, Kind::Defensive, Kind::Alliance, Kind::Access, Kind::Guarantee] {
            if kind == Kind::Guarantee && d.power(e, i) < 2.0 * d.power(e, j) {
                continue;
            }
            if kind == Kind::Access && !d.allied(i, j) {
                continue;
            }
            let p = Proposal { from: i, to: j, kind, day, good: None, amount: 0.0, target: None };
            if allowed(d, e, w, &p).is_err() || refused(d, j, kind) {
                continue;
            }
            let (v, _) = value(d, e, ed, w, Some(b), i, &p);
            if v > d.data.answers.propose_threshold && best.as_ref().is_none_or(|x| v > x.0) {
                best = Some((v, p));
            }
        }
    }
    if let Some((v, p)) = best {
        // To the player, only a deal it really wants, and one a month from
        // the whole world (the player's inbox is not a mailbox for every whim).
        let to_player = Some(p.to) == d.player;
        let recent = d.inbox.iter().any(|x| matches!(x, Inbox::Proposal(q) if day.saturating_sub(q.day) < 30));
        if !to_player || (v > d.data.answers.propose_threshold + 0.3 && !recent) {
            propose(d, e, ed, w, Some(b), p);
        }
    }
    // 2. Arms: at war or threatened, with too few tanks or too little ammo.
    if w.in_any_war(i) || threat_level > 0.5 {
        for g in [ed.ammo, ed.tanks] {
            if cover(d, e, ed, i, g) >= 0.5 || e.nations[i].treasury <= 0.0 {
                continue;
            }
            let seller = (0..d.n)
                .filter(|&j| j != i && w.at_war(i, j).is_none() && Some(j) != threat && cover(d, e, ed, j, g) > 1.5)
                .max_by(|&x, &y| d.relations[i][x].total_cmp(&d.relations[i][y]).then(y.cmp(&x)));
            if let Some(s) = seller {
                let amount = (e.nations[s].stockpile[g] * 0.1).floor();
                let p = Proposal { from: i, to: s, kind: Kind::Arms, day, good: Some(g), amount, target: None };
                if amount >= 1.0 && !refused(d, s, Kind::Arms) {
                    propose(d, e, ed, w, Some(b), p);
                }
            }
        }
    }
    // 3. A loan, deep in debt, from the friendliest rich nation.
    if debt > 0.6 && !d.live().any(|t| t.kind == Kind::Loan && t.b == i) {
        let amount_for = |j: usize| d.data.treaties.loan_size * e.nations[j].gdp;
        let lender = (0..d.n)
            .filter(|&j| j != i && e.nations[j].treasury * e.nations[j].exchange > 2.0 * amount_for(j) && e.nations[j].rating <= 2)
            .max_by(|&x, &y| d.relations[i][x].total_cmp(&d.relations[i][y]).then(y.cmp(&x)));
        if let Some(l) = lender {
            let p = Proposal { from: i, to: l, kind: Kind::Loan, day, good: None, amount: amount_for(l), target: None };
            if !refused(d, l, Kind::Loan) {
                propose(d, e, ed, w, Some(b), p);
            }
        }
    }
    // 4. Sanctions on a nation that broke its word lately, with those who dislike it.
    let hate = d.data.answers.sanction_hate;
    if let Some(t) = (0..d.n).find(|&t| {
        t != i && d.relations[i][t] < hate && d.broken.iter().rev().take_while(|b| day.saturating_sub(b.1) < 720).any(|b| b.0 == t)
    }) {
        let already = d.live().any(|x| x.kind == Kind::Sanctions && x.target == Some(t) && x.members.contains(&i));
        if !already {
            let mut partners: Vec<usize> = (0..d.n).filter(|&j| j != i && j != t && d.relations[j][t] < hate).collect();
            partners.sort_by(|&x, &y| d.relations[x][t].total_cmp(&d.relations[y][t]).then(x.cmp(&y)));
            for j in partners.into_iter().take(2) {
                let p = Proposal { from: i, to: j, kind: Kind::Sanctions, day, good: None, amount: 0.0, target: Some(t) };
                if !refused(d, j, Kind::Sanctions) {
                    propose(d, e, ed, w, Some(b), p);
                }
            }
        }
    }
}

/// The player answers something in the inbox.
pub fn answer_inbox(d: &mut Diplomacy, e: &mut Economy, ed: &EconData, w: &mut Wars, b: Option<&Brains>, k: usize, yes: bool) -> String {
    if k >= d.inbox.len() {
        return String::new();
    }
    match d.inbox.remove(k) {
        Inbox::Proposal(p) => {
            if allowed(d, e, w, &p).is_err() {
                return "That deal can no longer be made".to_string();
            }
            d.answers.push(Answer { day: d.day, from: p.from, to: p.to, kind: p.kind, yes, reason: "the player's choice".to_string() });
            if yes {
                sign(d, e, ed, w, &p);
                format!("Signed: {} with {}", p.kind.name(), d.names[p.from])
            } else {
                format!("Turned down: {} with {}", p.kind.name(), d.names[p.from])
            }
        }
        Inbox::Call { ally, war, side, kind, .. } => super::alliances::player_call(d, e, ed, w, b, ally, war, side, kind, yes),
    }
}
