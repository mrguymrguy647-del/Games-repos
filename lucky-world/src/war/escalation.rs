//! Kinds of war and the escalation ladder (docs/WAR.md, sections 8, 9 and
//! 11): declaring, escalating, matching or backing down, what each level
//! does to the economy, war exhaustion, civil war, proxy and cold war,
//! and the brains' decisions to start, widen and end wars.

use super::peace::{self, PeaceTable, PeaceTerms};
use super::{Faction, Goal, Losses, War, Wars, LEVELS};
use crate::ai::memory::{self, MemoryKind};
use crate::ai::Brains;
use crate::econ::{EconData, Economy};

/// Starts a war: `attacker` against `defender` at `level`.
#[allow(clippy::too_many_arguments)]
pub fn declare(w: &mut Wars, e: &mut Economy, ed: &EconData, attacker: Faction, defender: Faction, level: usize, goal: Goal, cause: String) -> usize {
    let _ = (e, ed);
    let id = w.wars.len();
    let level = level.clamp(0, 4);
    w.wars.push(War {
        id,
        sides: [vec![attacker], vec![defender]],
        level,
        start_level: level,
        max_level: level,
        goal,
        cause: cause.clone(),
        start: w.day,
        end: None,
        result: None,
        pending: None,
        losses: [Losses::default(), Losses::default()],
        battles: 0,
        civil: w.is_rebel(attacker) || w.is_rebel(defender),
        proxies: [Vec::new(), Vec::new()],
        peace: None,
    });
    w.forces[attacker].last_war = Some(w.day);
    w.refresh_relations();
    let text = format!("{}: {} against {} ({cause})", LEVELS[level], w.names[attacker], w.names[defender]);
    w.say(vec![attacker, defender], text);
    id
}

/// Joins a war on a side (ganging up, layer 5c).
pub fn join(w: &mut Wars, war: usize, side: usize, f: Faction) {
    if w.wars[war].side_of(f).is_some() {
        return;
    }
    w.wars[war].sides[side].push(f);
    w.forces[f].last_war = Some(w.day);
    w.refresh_relations();
    let enemy = w.names[w.wars[war].sides[1 - side][0]].clone();
    let text = format!("{} joins the war against {enemy}", w.names[f]);
    w.say(vec![f], text);
}

/// One side climbs a rung: the other must match within `match_days`, or back down.
pub fn escalate(w: &mut Wars, war: usize, side: usize) {
    let wr = &mut w.wars[war];
    if wr.level >= 4 || wr.pending.is_some() || !wr.active() {
        return;
    }
    wr.level += 1;
    wr.max_level = wr.max_level.max(wr.level);
    let deadline = w.day + w.data.levels.match_days;
    wr.pending = Some((side, wr.level, deadline));
    let who = w.names[wr.sides[side][0]].clone();
    let level = LEVELS[w.wars[war].level];
    let f = w.wars[war].sides.concat();
    w.refresh_relations();
    w.say(f, format!("{who} escalates: {level}"));
}

/// The other side answers an escalation.
pub fn respond(w: &mut Wars, e: &mut Economy, ed: &EconData, war: usize, matches: bool) {
    let Some((by, _, _)) = w.wars[war].pending else { return };
    if matches {
        w.wars[war].pending = None;
        let who = w.names[w.wars[war].sides[1 - by][0]].clone();
        let f = w.wars[war].sides.concat();
        w.say(f, format!("{who} matches the escalation"));
    } else {
        // Backing down: the escalator gets its goal, at the old level's terms.
        let score = peace::score(w, e, war);
        let forced = if by == 0 { score.max(w.data.peace.reparations + 1.0) } else { score.min(-w.data.peace.reparations - 1.0) };
        let t = peace::terms(w, war, forced);
        let who = w.names[w.wars[war].sides[1 - by][0]].clone();
        peace::make_peace(w, e, ed, war, t, &format!("{who} backs down"));
    }
}

/// A day of the war ladder: economies, exhaustion, endings, civil wars,
/// proxies, and (with brains) the decisions to fight.
pub fn day(w: &mut Wars, e: &mut Economy, ed: &EconData, mut brains: Option<&mut Brains>) {
    peace::pay_reparations(w, e);
    levels(w, e, ed);
    exhaustion(w, e);
    prisoners(w, e);
    if !w.ai {
        return;
    }
    civil_wars(w, e, ed);
    for k in 0..w.wars.len() {
        if w.wars[k].active() {
            decide(w, e, ed, k, brains.as_deref_mut());
        }
    }
    proxies(w, e, ed);
    if let Some(b) = brains {
        start_wars(w, e, ed, b);
    }
}

/// What each level does to its nations' economies.
fn levels(w: &mut Wars, e: &mut Economy, ed: &EconData) {
    let l = w.data.levels.clone();
    for i in 0..w.nations {
        let level = w.level_of(i);
        let n = &mut e.nations[i];
        if level >= 3 {
            let floor = l.arms_share[level];
            if n.arms_share(ed) < floor {
                n.arms = Some(floor);
            }
        }
        n.conversion = if level >= 4 { l.total_conversion } else { 0.0 };
    }
}

/// War exhaustion grows with losses, the level, occupation and blockade;
/// it drains stability and output; it fades in peace.
fn exhaustion(w: &mut Wars, e: &mut Economy) {
    let x = w.data.exhaustion.clone();
    let l = w.data.levels.clone();
    let es = w.data.escalation.clone();
    for f in 0..w.forces.len() {
        let nation = w.nation_of(f);
        let lost = w.forces[f].casualties - w.forces[f].casualties_seen;
        w.forces[f].casualties_seen = w.forces[f].casualties;
        let level = w.level_of(f);
        let blockaded = w.forces[f].blockaded;
        let mut e_now = w.forces[f].exhaustion;
        if level > 0 {
            let pop = e.nations[nation].population.max(1.0);
            let own: Vec<usize> = (0..w.owner.len()).filter(|&q| w.owner[q] == Some(nation)).collect();
            let occupied = own.iter().filter(|&&q| w.control[q] != Some(f)).count() as f64 / own.len().max(1) as f64;
            let occupied = if f < w.nations { occupied } else { 0.0 };
            e_now += x.losses * lost / pop + x.level * level as f64 + x.occupied * occupied + x.blockaded * blockaded;
        } else {
            e_now *= 1.0 - x.recover;
        }
        w.forces[f].exhaustion = e_now;
        if f >= w.nations {
            continue;
        }
        let ex = w.forces[f].exhaustion;
        let n = &mut e.nations[f];
        let mut stab = -x.stability * 100.0 * ex;
        let mut output = 1.0 - x.output * ex.min(1.0);
        if level >= 4 {
            stab += l.total_stability * 100.0;
            output -= l.total_output;
            n.happiness = (n.happiness + l.total_happiness).max(0.0);
        }
        // A democracy at war needs its people behind it.
        if level >= 2 && w.ideology[f] == "democracy" && n.stability < es.democracy_support {
            stab -= es.democracy_drain * 100.0;
            w.forces[f].war_unrest_days += 1;
        } else {
            w.forces[f].war_unrest_days = 0;
        }
        n.war_stability = stab;
        n.war_output = output.clamp(0.3, 1.0);
    }
}

/// Prisoners held too long after their war are let go.
fn prisoners(w: &mut Wars, e: &mut Economy) {
    let days = w.data.exhaustion.prisoner_release_days;
    if w.day % 30 != 0 {
        return;
    }
    for a in 0..w.forces.len() {
        for b in 0..w.forces.len() {
            if w.prisoners[a][b] > 0.0 && w.at_war(a, b).is_none() {
                let ended = w.wars.iter().filter(|x| x.side_of(a).is_some() && x.side_of(b).is_some()).filter_map(|x| x.end).max().unwrap_or(0);
                if w.day >= ended + days {
                    let q = w.prisoners[a][b];
                    w.prisoners[a][b] = 0.0;
                    e.nations[w.nation_of(b)].population += q;
                }
            }
        }
    }
}

/// The power of a side as an AI sees it (soldiers × quality, tanks, jets).
fn side_power(w: &Wars, e: &Economy, ed: &EconData, factions: &[Faction]) -> f64 {
    factions
        .iter()
        .map(|&f| {
            let (s, t) = w.strength(f);
            let jets = if f < w.nations { e.nations[f].stockpile[ed.jets] } else { 0.0 };
            s + 300.0 * t + 3000.0 * jets
        })
        .sum::<f64>()
        .max(1.0)
}

/// Each side of an active war decides: escalate, answer an escalation,
/// offer peace, or fight on.
fn decide(w: &mut Wars, e: &mut Economy, ed: &EconData, k: usize, mut brains: Option<&mut Brains>) {
    let p = w.data.peace.clone();
    let day = w.day;
    let wr = w.wars[k].clone();
    // A civil war ends when one side is gone or the capital falls.
    if wr.civil {
        civil_outcome(w, e, ed, k);
        return;
    }
    // Someone has nothing left: it's over.
    for s in 0..2 {
        let alive = wr.sides[s].iter().any(|&f| w.strength(f).0 > 0.0) || wr.sides[s].iter().any(|&f| w.held(f).len() > 0 && w.capital_held(f));
        if !alive {
            let sc = if s == 0 { -100.0 } else { 100.0 };
            let t = peace::terms(w, k, sc);
            peace::make_peace(w, e, ed, k, t, "surrender");
            return;
        }
    }
    let score = peace::score(w, e, k);
    let player_side = w.player.and_then(|pl| wr.side_of(pl));
    // An escalation waiting for an answer.
    if let Some((by, _, deadline)) = wr.pending {
        let other = 1 - by;
        if player_side == Some(other) {
            if day >= deadline {
                respond(w, e, ed, k, true); // the generals fight on
            }
            return;
        }
        let ratio = side_power(w, e, ed, &wr.sides[other]) / side_power(w, e, ed, &wr.sides[by]);
        let lead = wr.sides[other][0];
        let cost = w.data.escalation.cost(&w.ideology[lead]);
        respond(w, e, ed, k, ratio / cost >= w.data.ai.match_ratio);
        return;
    }
    let years = (day - wr.start) as f64 / ed.days_per_year as f64;
    // A skirmish doesn't last: after 90 days it grows or fades.
    if wr.level == 1 && day >= wr.start + 90 {
        let s = if score >= 0.0 { 0 } else { 1 };
        let lead = wr.sides[s][0];
        if aggression(w, lead, brains.as_deref()) > 0.75 && player_side != Some(s) {
            escalate(w, k, s);
        } else {
            peace::make_peace(w, e, ed, k, PeaceTerms::white(score), "the border goes quiet");
        }
        return;
    }
    // A military operation that took its goal ends on the attacker's terms.
    if wr.level == 2 {
        if let Goal::Provinces(ps) = &wr.goal {
            if !ps.is_empty() && ps.iter().all(|&q| w.control[q].is_some_and(|c| wr.sides[0].contains(&c))) {
                let t = peace::terms(w, k, score.max(p.reparations + 1.0));
                let lead = wr.sides[1][0];
                if player_side != Some(1) && (peace::ByExhaustion.accepts(w, e, k, lead, &t) || years > 0.5) {
                    peace::make_peace(w, e, ed, k, t, "the operation reaches its goal");
                    return;
                }
            }
        }
    }
    // Too long: settled where the lines are.
    if years >= p.max_war_years {
        let t = peace::terms(w, k, score);
        peace::make_peace(w, e, ed, k, t, "both sides are worn out");
        return;
    }
    // Each side may escalate or offer peace (weekly).
    if (day + k as u32) % 7 != 0 {
        return;
    }
    for s in 0..2 {
        if player_side == Some(s) {
            continue;
        }
        let lead = wr.sides[s][0];
        let my = if s == 0 { score } else { -score };
        let ex = w.forces[lead].exhaustion;
        let limit = p.offer_exhaustion * (1.0 + risk(lead, brains.as_deref()));
        let old_enough = (day - wr.start) as f64 >= p.min_days;
        if ex > limit || (old_enough && (my < -40.0 || (my > 40.0 && years > 1.0))) {
            let t = peace::terms(w, k, score);
            let other = wr.sides[1 - s][0];
            let accept = if player_side == Some(1 - s) {
                // The newest offer replaces the last one for this war.
                w.offers.retain(|o| o.0 != k);
                w.offers.push((k, s, t.clone(), day));
                false
            } else {
                peace::ByExhaustion.accepts(w, e, k, other, &t)
            };
            if accept {
                let how = if t.winner.is_none() { "a white peace" } else { "a peace deal" };
                peace::make_peace(w, e, ed, k, t, how);
                if let Some(b) = brains.as_deref_mut() {
                    for &a in &wr.sides[0] {
                        for &c in &wr.sides[1] {
                            remember(b, a, c, day, MemoryKind::MadePeace, 0.1);
                            remember(b, c, a, day, MemoryKind::MadePeace, 0.1);
                        }
                    }
                }
                return;
            }
        }
        // Losing a war it cares about: climb a rung (if its ideology lets it).
        let cost = w.data.escalation.cost(&w.ideology[lead]) * (1.0 - risk(lead, brains.as_deref()));
        let stake = if s == 1 { 1.3 } else { 1.0 };
        if my < w.data.ai.escalate_score && wr.level < 4 && cost < stake && ex < limit * 0.8 {
            let roll = w.rng.next_f32() as f64;
            if roll < 0.15 {
                escalate(w, k, s);
                return;
            }
        }
    }
}

/// A faction's taste for war (docs/WAR.md, section 11).
pub fn aggression(w: &Wars, f: Faction, brains: Option<&Brains>) -> f64 {
    let a = &w.data.ai;
    a.base_aggression + a.leader.get(&w.leader[f]).copied().unwrap_or(0.0) + a.ideology.get(&w.ideology[f]).copied().unwrap_or(0.0) + risk(f, brains)
}

fn risk(f: Faction, brains: Option<&Brains>) -> f64 {
    brains.and_then(|b| b.brain(f)).map_or(0.1, |b| b.personality.t.risk)
}

fn remember(b: &mut Brains, who: usize, other: usize, day: u32, kind: MemoryKind, change: f64) {
    let ai = b.data.clone();
    if let Some(Some(brain)) = b.brains.get_mut(who) {
        if other < brain.relations.len() {
            memory::remember(brain, &ai, other, day, kind, change);
        }
    }
}

/// The brains look for wars to start (or join), once a month each.
fn start_wars(w: &mut Wars, e: &mut Economy, ed: &EconData, b: &mut Brains) {
    let a = w.data.ai.clone();
    let day = w.day;
    let year = ed.days_per_year as f64;
    let n = w.nations;
    for i in 0..n {
        if (day + i as u32 * 7) % 30 != 0 || Some(i) == w.player {
            continue;
        }
        let Some(brain) = b.brain(i) else { continue };
        if e.nations[i].population < 1.0 || w.strength(i).0 < 1000.0 {
            continue;
        }
        let busy = w.in_any_war(i);
        let cooldown = w.forces[i].last_war.is_some_and(|d| (day - d) as f64 / year < a.cooldown_years);
        let aggr = aggression(w, i, Some(b));
        let ex = w.forces[i].exhaustion;
        let debt = brain.needs[crate::ai::data::DEBT].min(1.0);
        let my_power = brain.estimates[i].power(&b.data).max(1.0);
        // Ganging up on a hegemon already at war.
        if let Some(h) = brain.hegemon.filter(|&h| h != i) {
            if let Some(k) = (0..w.wars.len()).find(|&k| w.wars[k].active() && w.wars[k].side_of(h).is_some() && w.wars[k].side_of(i).is_none() && !w.wars[k].civil) {
                let reach = reachable(w, e, ed, i, h);
                let desire = aggr + a.gang_up - a.exhaustion_cost * ex - a.debt_cost * debt;
                if reach && desire > a.gang_up_threshold && !busy {
                    let side = 1 - w.wars[k].side_of(h).unwrap();
                    join(w, k, side, i);
                    continue;
                }
            }
        }
        if busy || cooldown {
            continue;
        }
        // Candidates: weak neighbours, the counterweight target, grudges.
        let mut cands: Vec<usize> = brain.opportunities.iter().map(|o| o.target).collect();
        if let Some(c) = brain.counter {
            cands.push(c);
        }
        for j in 0..n {
            if j != i && brain.relations[j].regard < -0.4 && !cands.contains(&j) {
                cands.push(j);
            }
        }
        let mut best: Option<(usize, f64)> = None;
        for &j in &cands {
            if j == i || w.forces[i].truces.iter().any(|&(t, until)| t == j && day < until) || !reachable(w, e, ed, i, j) || w.strength(j).0 <= 0.0 {
                continue;
            }
            let ratio = my_power / brain.estimates[j].power(&b.data).max(1.0);
            let weak = brain.opportunities.iter().find(|o| o.target == j).map_or(0.0, |o| o.reasons.len() as f64 / 3.0);
            let grudge = (-brain.relations[j].regard).max(0.0);
            let gang = if brain.hegemon == Some(j) { a.gang_up * 0.5 } else { 0.0 };
            let desire = aggr * (ratio - 1.0).min(2.0) + a.weakness * weak + a.grudge * grudge + gang - a.exhaustion_cost * ex - a.debt_cost * debt;
            if desire > a.war_threshold && best.is_none_or(|(_, d)| desire > d) {
                best = Some((j, desire));
            }
        }
        let Some((j, desire)) = best else { continue };
        // How far to go, and what for.
        let level = if aggr + 0.2 * (desire - a.war_threshold) > a.declare_aggression { 3 } else if aggr > a.operation_aggression { 2 } else { 1 };
        let mine: Vec<usize> = w.held(i);
        let mut border: Vec<usize> = w.held(j).into_iter().filter(|&q| w.geo.neighbours[q].iter().any(|r| mine.contains(r))).collect();
        if border.is_empty() {
            // Across the sea: the coast nearest its own.
            border = w.held(j).into_iter().filter(|&q| w.geo.coastal[q]).take(1).collect();
        }
        border.sort_by_key(|&q| (w.geo.city[q], q));
        let ratio = my_power / brain.estimates[j].power(&b.data).max(1.0);
        let goal = if level >= 3 && ratio > 3.0 && aggr > 0.7 {
            Goal::Vassal
        } else if level >= 3 && ratio > 2.0 && w.ideology[i] != w.ideology[j] && aggr > 0.6 {
            Goal::Ideology
        } else if border.is_empty() {
            Goal::Reparations
        } else {
            Goal::Provinces(border.into_iter().take(if level >= 3 { 3 } else { 1 }).collect())
        };
        let why = brain
            .opportunities
            .iter()
            .find(|o| o.target == j)
            .map(|o| format!("it sees weakness: {}", o.reasons.join(", ")))
            .unwrap_or_else(|| if brain.counter == Some(j) { "it fears its power".to_string() } else { "an old grudge".to_string() });
        let k = declare(w, e, ed, i, j, level, goal, why);
        pick_proxies(w, b, k);
        remember(b, j, i, day, MemoryKind::AttackedUs, -0.6);
        for x in 0..n {
            if x != i && x != j {
                remember(b, x, i, day, MemoryKind::AttackedUs, -0.05);
            }
        }
    }
    // Cold wars: hostile powers who fear each other arm, without shooting.
    if day % 90 == 0 {
        // Only the great powers (the eight richest) wage cold wars, against
        // the power they fear most, when one of them holds a grudge.
        let mut by_power: Vec<usize> = (0..n).collect();
        by_power.sort_by(|&x, &y| e.nations[y].gdp.total_cmp(&e.nations[x].gdp).then(x.cmp(&y)));
        let great: Vec<usize> = by_power.into_iter().take(8).collect();
        for i in 0..n {
            let Some(bi) = b.brain(i) else { continue };
            let Some(j) = bi.threat else { continue };
            if j == i || w.at_war(i, j).is_some() || !great.contains(&i) || !great.contains(&j) {
                continue;
            }
            let Some(bj) = b.brain(j) else { continue };
            let hostile = bi.relations[j].regard < -0.4 || bj.relations[i].regard < -0.4;
            let recent = w.wars.iter().any(|x| x.level == 0 && x.side_of(i).is_some() && x.side_of(j).is_some() && x.end.is_none_or(|d| (day - d) as f64 / year < 10.0));
            if hostile && !recent {
                declare(w, e, ed, i, j, 0, Goal::Reparations, "an arms race".to_string());
            }
        }
        // Cold wars thaw when the hostility fades, after ten years at most.
        for k in 0..w.wars.len() {
            let x = &w.wars[k];
            if !x.active() || x.level != 0 {
                continue;
            }
            let (i, j) = (x.sides[0][0], x.sides[1][0]);
            let thaw = b.brain(i).is_none_or(|bi| bi.relations[j].regard > -0.2) || (day - x.start) as f64 / year > 10.0;
            if thaw {
                peace::make_peace(w, e, ed, k, PeaceTerms::white(0.0), "the cold war thaws");
            }
        }
    }
    // Arms races: both sides of a cold war keep their arms up.
    for x in w.wars.iter().filter(|x| x.active() && x.level == 0) {
        for &f in x.sides.iter().flatten() {
            if f < n {
                let nn = &mut e.nations[f];
                if nn.arms_share(ed) < 0.03 {
                    nn.arms = Some(0.03);
                }
            }
        }
    }
}

/// Whether `i` can get at `j`: a land border, or the sea with transports
/// and a coast on each side.
pub fn reachable(w: &Wars, e: &Economy, ed: &EconData, i: Faction, j: Faction) -> bool {
    let mine = w.held(i);
    let theirs = w.held(j);
    if theirs.iter().any(|&q| w.geo.neighbours[q].iter().any(|r| mine.contains(r))) {
        return true;
    }
    super::sea::transports(w, e, ed, i) >= 5.0 && mine.iter().any(|&q| w.geo.coastal[q] && e.provinces[q].port >= 1.0) && theirs.iter().any(|&q| w.geo.coastal[q])
}

/// Proxy wars: a nation that hates one side of someone else's war arms the
/// other side, once a month, unseen.
fn proxies(w: &mut Wars, e: &mut Economy, ed: &EconData) {
    if w.day % 30 != 15 {
        return;
    }
    let share = w.data.other.proxy_share;
    for k in 0..w.wars.len() {
        if !w.wars[k].active() || w.wars[k].level == 0 {
            continue;
        }
        for s in 0..2 {
            for pi in 0..w.wars[k].proxies[s].len() {
                let donor = w.wars[k].proxies[s][pi];
                let to = w.wars[k].sides[s][0];
                if donor >= w.nations {
                    continue;
                }
                for g in [ed.ammo, ed.rifles, ed.tanks] {
                    let q = (e.nations[donor].stockpile[g] * share).floor();
                    if q <= 0.0 {
                        continue;
                    }
                    let fq = e.nations[donor].stockpile_faulty[g];
                    e.nations[donor].stockpile[g] -= q;
                    w.forces[donor].flows.given[g] += q;
                    w.store(e, to, g, q, fq);
                    w.forces[to].flows.received[g] += q;
                }
            }
        }
    }
}

/// Picks proxy supporters for a war (from the brains' grudges).
pub fn pick_proxies(w: &mut Wars, b: &Brains, k: usize) {
    for s in 0..2 {
        let enemy = w.wars[k].sides[1 - s][0];
        for i in 0..w.nations {
            if w.wars[k].side_of(i).is_some() || w.wars[k].proxies[s].contains(&i) || w.in_any_war(i) {
                continue;
            }
            if let Some(bi) = b.brain(i) {
                if enemy < bi.relations.len() && bi.relations[enemy].regard < -0.5 {
                    w.wars[k].proxies[s].push(i);
                }
            }
        }
    }
}

/// Civil war: stability that stays at rock bottom sometimes breaks into
/// rebellion; rebels take provinces, divisions and weapons.
fn civil_wars(w: &mut Wars, e: &mut Economy, ed: &EconData) {
    let o = w.data.other.clone();
    let es = w.data.escalation.clone();
    for i in 0..w.nations {
        let st = e.nations[i].stability;
        if st < o.civil_war_stability {
            w.forces[i].low_stability_days += 1;
        } else {
            w.forces[i].low_stability_days = 0;
        }
        // A democracy whose war has lost its people: the government falls.
        if w.forces[i].war_unrest_days > es.democracy_days {
            w.forces[i].war_unrest_days = 0;
            let wars: Vec<usize> = (0..w.wars.len()).filter(|&k| w.wars[k].active() && w.wars[k].side_of(i).is_some() && !w.wars[k].civil).collect();
            for k in wars {
                let sc = peace::score(w, e, k);
                peace::make_peace(w, e, ed, k, PeaceTerms::white(sc), "the government falls; the new one makes peace");
            }
            e.nations[i].stability = (e.nations[i].stability - 10.0).max(0.0);
        }
        let rebels = w.nations + i;
        let already = w.wars.iter().any(|x| x.active() && x.civil && x.side_of(rebels).is_some());
        if already || w.forces[i].low_stability_days < o.civil_war_days {
            continue;
        }
        if (w.rng.next_f32() as f64) >= o.civil_war_chance {
            continue;
        }
        rise(w, e, ed, i);
    }
}

/// Rebels rise in nation `i`.
fn rise(w: &mut Wars, e: &mut Economy, ed: &EconData, i: usize) {
    let r = w.nations + i;
    let share = w.data.other.rebel_share;
    // The provinces furthest from the capital, about a third of them.
    let cap = w.capital[i];
    let dist = w.geo.hops(cap, |q| w.owner[q] == Some(i));
    let mut own: Vec<usize> = w.held(i).into_iter().filter(|&q| q != cap).collect();
    own.sort_by_key(|&q| (std::cmp::Reverse(dist[q].map_or(0, |x| x.0)), q));
    let take = (own.len() as f64 / 3.0).ceil() as usize;
    if take == 0 {
        return;
    }
    let taken: Vec<usize> = own.into_iter().take(take).collect();
    for &q in &taken {
        w.set_control(e, q, Some(r));
    }
    // The least loyal divisions go over, with their weapons.
    let mut divs: Vec<usize> = (0..w.divisions.len()).filter(|&d| w.divisions[d].alive && w.divisions[d].faction == i).collect();
    divs.sort_by(|&a, &b| w.divisions[a].morale.total_cmp(&w.divisions[b].morale).then(a.cmp(&b)));
    let go = ((divs.len() as f64 * share).floor() as usize).max(1).min(divs.len());
    for (k, &d) in divs.iter().take(go).enumerate() {
        let dv = &mut w.divisions[d];
        dv.faction = r;
        dv.province = taken[k % taken.len()];
        dv.moving = None;
        dv.battle = None;
        dv.order = super::Order::Auto;
    }
    // A share of the stockpile goes with them.
    for g in [ed.ammo, ed.rifles, ed.tanks, ed.food, ed.fuel] {
        let pool = if ed.goods[g].military { e.nations[i].stockpile[g] } else { e.nations[i].stock[g] };
        let q = (pool * share * 0.5).floor();
        if q <= 0.0 {
            continue;
        }
        if ed.goods[g].military {
            let fq = e.nations[i].stockpile_faulty[g];
            e.nations[i].stockpile[g] -= q;
            w.forces[i].flows.captured_from[g] += q;
            w.forces[r].stores_faulty[g] = fq;
            w.forces[r].flows.captured_by[g] += q;
        } else {
            e.nations[i].stock[g] -= q;
        }
        w.forces[r].stores[g] += q;
    }
    w.forces[r].exhaustion = 0.0;
    let k = declare(w, e, ed, r, i, 3, Goal::Rebellion, "stability collapsed".to_string());
    w.wars[k].civil = true;
    let who = w.names[i].clone();
    w.say(vec![i, r], format!("Civil war in {who}: rebels take {} provinces", taken.len()));
}

/// A civil war ends: the rebels take the capital (a new government), or
/// their last division falls.
fn civil_outcome(w: &mut Wars, e: &mut Economy, ed: &EconData, k: usize) {
    let r = w.wars[k].sides[0][0];
    let i = w.wars[k].sides[1][0];
    let cap = w.capital[i];
    if w.control[cap] == Some(r) {
        // The rebels win: a new government, the army reunited.
        let options = ["democracy", "military_rule", "communism", "monarchy", "authoritarianism"];
        let old = w.ideology[i].clone();
        let pick = options.iter().filter(|&&x| x != old).nth((w.rng.next_u32() as usize) % 4).copied().unwrap_or("democracy");
        for d in w.divisions.iter_mut().filter(|d| d.faction == r) {
            d.faction = i;
            d.battle = None;
            d.moving = None;
        }
        for q in 0..w.control.len() {
            if w.control[q] == Some(r) {
                w.set_control(e, q, Some(i));
            }
        }
        peace::set_ideology(w, e, ed, i, pick);
        let mut t = PeaceTerms::white(100.0);
        t.winner = Some(0);
        peace::make_peace(w, e, ed, k, t, &format!("the rebels take the capital; a new government ({})", pick.replace('_', " ")));
        return;
    }
    let rebels_left = w.divisions.iter().any(|d| d.alive && d.faction == r);
    if !rebels_left {
        for q in 0..w.control.len() {
            if w.control[q] == Some(r) {
                w.set_control(e, q, Some(i));
            }
        }
        let mut t = PeaceTerms::white(-100.0);
        t.winner = Some(1);
        e.nations[i].stability = (e.nations[i].stability + 10.0).min(100.0);
        w.forces[i].low_stability_days = 0;
        peace::make_peace(w, e, ed, k, t, "the rebellion is crushed");
    }
}
