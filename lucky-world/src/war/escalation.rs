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
        motive: String::new(),
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
    // A side has at most `max_side` nations: wars don't swallow the world.
    if w.wars[war].side_of(f).is_some() || w.wars[war].sides[side].len() >= w.data.ai.max_side {
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
    w.count("escalations", 1.0);
    let wr = &mut w.wars[war];
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
    // The economy's overlords are the truth: note who became a vassal when.
    for i in 0..w.nations {
        if e.nations[i].overlord != w.overlords[i] {
            w.overlords[i] = e.nations[i].overlord;
            w.vassal_since[i] = w.day;
        }
    }
    levels(w, e, ed);
    exhaustion(w, e);
    prisoners(w, e);
    if !w.ai {
        return;
    }
    civil_wars(w, e, ed);
    if w.day % 30 == 20 {
        independence(w, e, ed, brains.as_deref_mut());
    }
    for k in 0..w.wars.len() {
        if w.wars[k].active() {
            decide(w, e, ed, k, brains.as_deref_mut());
        }
    }
    proxies(w, e, ed);
    if let Some(b) = brains {
        if w.day % 30 == 15 {
            coalitions(w, e, ed, b);
        }
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
            let parts = [x.losses * lost / pop, x.level[level.min(4)], x.occupied * occupied, x.blockaded * blockaded];
            e_now += parts.iter().sum::<f64>();
            for (k, name) in ["exhaustion from losses", "exhaustion from the level", "exhaustion from occupation", "exhaustion from blockade"].into_iter().enumerate() {
                w.stats.entry(name).and_modify(|v| *v += parts[k]).or_insert(parts[k]);
            }
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
        // Vassals cost their overlord: garrisons, governors, resentment.
        let vassals = (0..w.nations).filter(|&v| w.overlords[v] == Some(f)).count() as f64;
        stab -= w.data.ai.vassal_stability * vassals;
        output -= w.data.ai.vassal_output * vassals;
        let n = &mut e.nations[f];
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
    if wr.level == 1 && day >= wr.start + w.data.levels.skirmish_days {
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
        let limit = p.offer_exhaustion * p.stake[wr.level.min(4)] * (1.0 + risk(lead, brains.as_deref()));
        // With diplomacy, peace is negotiated in its talks (docs/DIPLOMACY.md, section 7).
        if w.peace_talks && !wr.civil {
            let cost = w.data.escalation.cost(&w.ideology[lead]) * (1.0 - risk(lead, brains.as_deref()));
            let stake = if s == 1 { 1.3 } else { 1.0 };
            let needed = if wr.level >= 3 { w.data.ai.total_war_score } else { w.data.ai.escalate_score };
            if my < needed && wr.level < 4 && cost < stake && ex < limit * 0.8 && (w.rng.next_f32() as f64) < w.data.ai.escalate_chance {
                escalate(w, k, s);
                return;
            }
            continue;
        }
        let old_enough = (day - wr.start) as f64 >= p.min_days;
        // A side sues for peace when worn out, when the war is hopeless for
        // it (deeper wars are fought on longer), or when it has won and
        // the war has gone on long enough.
        let lv = wr.level.min(4);
        let hopeless = my < -p.hopeless[lv] && (day - wr.start) as f64 >= p.min_days * p.stake[lv];
        let won = my > 40.0 && years > p.won_years[lv];
        if ex > limit || (old_enough && (hopeless || won)) {
            w.count(if ex > limit { "peace offered: worn out" } else if hopeless { "peace offered: hopeless" } else { "peace offered: won" }, 1.0);
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
        // Total war is the last resort: only a side losing badly goes there.
        let needed = if wr.level >= 3 { w.data.ai.total_war_score } else { w.data.ai.escalate_score };
        if my < needed && wr.level < 4 && cost < stake && ex < limit * 0.8 {
            let roll = w.rng.next_f32() as f64;
            if roll < w.data.ai.escalate_chance {
                escalate(w, k, s);
                return;
            }
        }
    }
}

/// A faction's taste for war (docs/WAR.md, section 11).
/// A brain's error in judging a war's odds: a factor spread by `judgement`
/// round 1, fixed for a pair in a month (a hash, so it uses no dice).
fn misjudge(judgement: f64, i: usize, j: usize, month: u32) -> f64 {
    if judgement <= 0.0 {
        return 1.0;
    }
    let mut h = (i as u32).wrapping_mul(0x9E37_79B9) ^ (j as u32).wrapping_mul(0x85EB_CA6B) ^ month.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    let u = (h as f64 / u32::MAX as f64) * 2.0 - 1.0;
    (1.0 + judgement * u).max(0.2)
}

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
    let at_war_share = (0..n).filter(|&x| w.level_of(x) >= 3).count() as f64 / n.max(1) as f64;
    for i in 0..n {
        if !(day + i as u32 * 7).is_multiple_of(b.difficulty.war_every) || Some(i) == w.player {
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
        // Candidates: weak neighbours, the counterweight target, grudges,
        // lost land, neighbours with what it lacks, other ideologies.
        let mut cands: Vec<usize> = brain.opportunities.iter().map(|o| o.target).collect();
        if let Some(c) = brain.counter {
            cands.push(c);
        }
        for j in 0..n {
            if j != i && !cands.contains(&j) && (brain.relations[j].regard < -0.4 || w.lost.iter().any(|l| l.0 == i && w.owner[l.1] == Some(j)) || w.ideology[j] != w.ideology[i] || touches(w, i, j)) {
                cands.push(j);
            }
        }
        let mut best: Option<(usize, f64, Motive)> = None;
        for &j in &cands {
            // The underdog rule: the world's eye is on a weak start for its first years.
            if j == i || e.underdog.protected(j, day, ed) || w.forces[i].truces.iter().any(|&(t, until)| t == j && day < until) || !reachable(w, e, ed, i, j) || w.strength(j).0 <= 0.0 {
                continue;
            }
            // A weaker planner misjudges the odds (difficulty), the same way each month.
            let ratio = my_power / brain.estimates[j].power(&b.data).max(1.0) * misjudge(b.difficulty.judgement, i, j, day / 30);
            let ms = motives(w, e, ed, b, i, j, ratio, aggr);
            // A world already at war makes a new one riskier (who would
            // come to whose aid?): the more nations at war, the less desire.
            let costs = a.exhaustion_cost * ex + a.debt_cost * debt + a.world_at_war_cost * at_war_share;
            // Diplomacy: the treaties it would break, and the target's friends.
            let diplo = w.diplo_cost.get(i).and_then(|row| row.get(j)).copied().unwrap_or(0.0);
            // Weakness on its own needs the power to win; the other motives
            // add to it (a grudge, a lost province, a holy war...).
            let desire = ms.iter().map(|m| m.1).sum::<f64>() - costs - diplo;
            if desire <= a.war_threshold && desire + diplo > a.war_threshold && ratio > 0.7 && ms.iter().any(|m| m.0 != "strength" && m.1 > 0.0) && !w.prevented.iter().any(|p| p.0 == i && p.1 == j && day - p.2 < 360) {
                w.prevented.push((i, j, day));
                w.count("wars prevented by diplomacy", 1.0);
            }
            let reasons = ms.iter().filter(|m| m.0 != "strength" && m.1 > 0.0).count();
            let top = ms.into_iter().max_by(|x, y| x.1.total_cmp(&y.1)).unwrap();
            if reasons == 0 {
                continue;
            }
            if desire > a.war_threshold && ratio > 0.7 && best.as_ref().is_none_or(|(_, d, _)| desire > *d) {
                best = Some((j, desire, top));
            }
        }
        let Some((j, desire, motive)) = best else { continue };
        // How far to go, and what for.
        let level = if aggr + 0.2 * (desire - a.war_threshold) > a.declare_aggression { 3 } else if aggr > a.operation_aggression { 2 } else { 1 };
        let mine: Vec<usize> = w.held(i);
        let mut border: Vec<usize> = w.held(j).into_iter().filter(|&q| w.geo.neighbours[q].iter().any(|r| mine.contains(r))).collect();
        if border.is_empty() {
            // Across the sea: the coast nearest its own.
            border = w.held(j).into_iter().filter(|&q| w.geo.coastal[q]).take(1).collect();
        }
        border.sort_by_key(|&q| (w.geo.city[q], q));
        // A war for something wants that something first.
        if let Some(q) = motive.3 {
            border.retain(|&x| x != q);
            border.insert(0, q);
        }
        border.retain(|q| !w.capital.contains(q));
        let ratio = my_power / brain.estimates[j].power(&b.data).max(1.0);
        let goal = if motive.0 == "ideology" && level >= 3 {
            Goal::Ideology
        } else if level >= 3 && ratio > 3.0 && aggr > 0.7 {
            Goal::Vassal
        } else if level >= 3 && ratio > 2.0 && w.ideology[i] != w.ideology[j] && aggr > 0.6 {
            Goal::Ideology
        } else if border.is_empty() {
            Goal::Reparations
        } else {
            Goal::Provinces(border.into_iter().take(if level >= 3 { 3 } else { 1 }).collect())
        };
        let why = motive.2;
        let k = declare(w, e, ed, i, j, level, goal, why);
        w.wars[k].motive = motive.0.to_string();
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

/// Vassals weigh their freedom each month (docs/WAR.md, section 10): an
/// overlord at war, worn out, weaker than them or holding too many; years
/// under it; another ideology; resentment. A vassal stronger than its
/// overlord just leaves; one that wants it enough fights for it, and its
/// overlord's other restless vassals join in.
fn independence(w: &mut Wars, e: &mut Economy, ed: &EconData, mut brains: Option<&mut Brains>) {
    let a = w.data.ai.clone();
    let year = ed.days_per_year as f64;
    let day = w.day;
    let liberty = |w: &Wars, b: Option<&Brains>, v: usize, o: usize| -> f64 {
        let years = (day - w.vassal_since[v]) as f64 / year;
        let pv = w.strength(v).0 + 1.0;
        let po = w.strength(o).0 + 1.0;
        let resent = b.and_then(|b| b.brain(v)).map_or(0.0, |br| (-br.relations[o].regard).max(0.0));
        0.2 + 0.5 * (years / 20.0).min(1.0)
            + 0.8 * (pv / po).min(1.5)
            + if w.level_of(o) >= 2 { 0.4 } else { 0.0 }
            + 0.5 * w.forces[o].exhaustion.min(1.0)
            + 0.2 * (w.vassals_of(o).len() as f64 - 1.0).max(0.0)
            + if w.ideology[v] != w.ideology[o] { 0.3 } else { 0.0 }
            + 0.3 * resent
            // Diplomacy: a loyal vassal wants it less, a resentful one more.
            - w.loyalty.get(v).map_or(0.0, |l| (l - 50.0) / 100.0)
    };
    for v in 0..w.nations {
        let Some(o) = w.overlords[v] else { continue };
        if Some(v) == w.player || w.at_war(v, o).is_some() || w.in_any_war(v) {
            continue;
        }
        let want = liberty(w, brains.as_deref(), v, o);
        if w.strength(v).0 > 1.3 * w.strength(o).0 {
            e.nations[v].overlord = None;
            w.overlords[v] = None;
            w.count("vassals walked away", 1.0);
            let text = format!("{} breaks free of {}: its overlord can't hold it", w.names[v], w.names[o]);
            w.say(vec![v, o], text);
            continue;
        }
        // A real chance is needed: an army a third of its overlord's (or
        // the overlord busy at war), no truce, no war of its own lately.
        let truce = w.forces[v].truces.iter().any(|&(t, until)| t == o && day < until);
        let cooldown = w.forces[v].last_war.is_some_and(|d| (day - d) as f64 / year < a.cooldown_years);
        let chance = w.strength(v).0 >= 0.33 * w.strength(o).0 || w.level_of(o) >= 2;
        if want <= a.liberty_threshold || truce || cooldown || !chance {
            continue;
        }
        let k = declare(w, e, ed, v, o, 2, Goal::Independence, format!("a war of independence from {}", w.names[o]));
        w.wars[k].motive = "independence".to_string();
        w.count("wars of independence", 1.0);
        for u in w.vassals_of(o) {
            if u != v && Some(u) != w.player && !w.in_any_war(u) && liberty(w, brains.as_deref(), u, o) > 0.7 * a.liberty_threshold {
                join(w, k, 0, u);
            }
        }
        if let Some(b) = brains.as_deref_mut() {
            remember(b, o, v, day, MemoryKind::AttackedUs, -0.3);
        }
    }
}

/// The balance of power: nations that fear the same hegemon (the player
/// too) go to war on it together, at most once in `coalition_years`.
fn coalitions(w: &mut Wars, e: &mut Economy, ed: &EconData, b: &mut Brains) {
    let a = w.data.ai.clone();
    let year = ed.days_per_year as f64;
    let day = w.day;
    // Not in the first two years (a hegemon has to be seen to grow), and
    // not while much of the world is already at war.
    let busy = (0..w.nations).filter(|&x| w.level_of(x) >= 3).count() as f64 / w.nations.max(1) as f64;
    if (day as f64) < 2.0 * year || busy > 0.1 {
        return;
    }
    for h in 0..w.nations {
        let recent = w.wars.iter().any(|x| x.cause.starts_with("a coalition") && x.sides[1].contains(&h) && (day - x.start) as f64 / year < a.coalition_years);
        if recent || w.strength(h).0 <= 0.0 {
            continue;
        }
        let mut members: Vec<usize> = (0..w.nations)
            .filter(|&i| i != h && Some(i) != w.player && !w.in_any_war(i) && w.strength(i).0 > 0.0)
            .filter(|&i| b.brain(i).is_some_and(|br| br.hegemon == Some(h)))
            .filter(|&i| !w.forces[i].truces.iter().any(|&(t, until)| t == h && day < until))
            .filter(|&i| e.nations[i].overlord.is_none_or(|o| o != h))
            .filter(|&i| reachable(w, e, ed, i, h))
            .collect();
        if members.len() < a.coalition_min {
            continue;
        }
        members.sort_by(|&x, &y| w.strength(y).0.total_cmp(&w.strength(x).0).then(x.cmp(&y)));
        members.truncate(a.max_side);
        let lead = members[0];
        let mine = w.held(lead);
        let goal = match w.held(h).into_iter().filter(|&q| !w.capital.contains(&q) && w.geo.neighbours[q].iter().any(|r| mine.contains(r))).take(2).collect::<Vec<_>>() {
            ps if ps.is_empty() => Goal::Reparations,
            ps => Goal::Provinces(ps),
        };
        let cause = format!("a coalition against {}, grown too strong", w.names[h]);
        let k = declare(w, e, ed, lead, h, 3, goal, cause);
        w.wars[k].motive = "coalition".to_string();
        for &m in &members[1..] {
            join(w, k, 0, m);
        }
        for &m in &members {
            remember(b, h, m, day, MemoryKind::AttackedUs, -0.4);
        }
    }
}

/// One reason to go to war: (kind, weight, in words, the province it is about).
type Motive = (&'static str, f64, String, Option<usize>);

/// Whether two nations share a land border.
fn touches(w: &Wars, i: usize, j: usize) -> bool {
    w.held(i).iter().any(|&q| w.geo.neighbours[q].iter().any(|&r| w.control[r] == Some(j)))
}

/// Why `i` might go to war on `j`, each reason weighed (docs/WAR.md, section 11).
#[allow(clippy::too_many_arguments)]
fn motives(w: &Wars, e: &Economy, ed: &EconData, b: &Brains, i: usize, j: usize, ratio: f64, aggr: f64) -> Vec<Motive> {
    let a = &w.data.ai;
    let Some(brain) = b.brain(i) else { return Vec::new() };
    let year = ed.days_per_year as f64;
    let mut out: Vec<Motive> = Vec::new();
    // Weakness (and plain strength): the old motive.
    let opp = brain.opportunities.iter().find(|o| o.target == j);
    let weak = opp.map_or(0.0, |o| o.reasons.len() as f64 / 3.0);
    // Being the stronger is a reason only next to another one (half of it
    // with no weakness to exploit).
    let strength = a.strength * aggr * (ratio - 1.0).min(2.0);
    if let Some(o) = opp {
        out.push(("weakness", strength + a.weakness * weak, format!("it sees weakness: {}", o.reasons.join(", ")), None));
    } else {
        out.push(("strength", 0.5 * strength, "it is the stronger".to_string(), None));
    }
    // Grudges: memory. Being attacked weighs most.
    let rel = &brain.relations[j];
    let attacked = rel.log.iter().filter(|m| m.kind == MemoryKind::AttackedUs).count() as f64;
    let grudge = a.grudge * ((-rel.regard).max(0.0) + 0.3 * attacked.min(3.0));
    if grudge > 0.0 {
        let last = rel.log.iter().rev().find(|m| m.kind == MemoryKind::AttackedUs || m.kind == MemoryKind::EmbargoedUs || m.kind == MemoryKind::ArmedOurEnemy);
        let words = last.map_or_else(|| "an old grudge".to_string(), |m| format!("an old grudge: it {} in year {:.0}", m.kind.words(), m.day as f64 / year + 1.0));
        out.push(("grudge", grudge, words, None));
    }
    // Lost land: provinces taken from it in a peace deal, held by j.
    if let Some(l) = w.lost.iter().filter(|l| l.0 == i && w.owner[l.1] == Some(j) && (w.day - l.2) as f64 / year < a.revanche_years).max_by_key(|l| l.2) {
        out.push(("revanche", a.revanche, format!("to take back {}, lost in year {:.0}", w.geo.names[l.1], l.2 as f64 / year + 1.0), Some(l.1)));
    }
    // Ideology: spreading the revolution, a holy war, the empire, democracy.
    if w.ideology[i] != w.ideology[j] {
        if let Some(&zeal) = a.crusade.get(&w.ideology[i]) {
            let words = match w.ideology[i].as_str() {
                "communism" => "to spread the revolution",
                "theocracy" => "a holy war",
                "imperialism" => "for the empire",
                "democracy" => "to bring it democracy",
                _ => "to bring it order",
            };
            out.push(("ideology", zeal * aggr.max(0.2), format!("{words} ({} rules there)", w.ideology[j].replace('_', " ")), None));
        }
    }
    // Resources: a raw good it must import, rich in a border province of j's.
    if i < e.nations.len() {
        let n = &e.nations[i];
        let mine = w.held(i);
        let mut best: Option<(f64, usize, usize)> = None;
        for g in 0..ed.goods.len() {
            if ed.goods[g].tier != crate::econ::data::Tier::Raw {
                continue;
            }
            let need = n.ledger.used[g] + n.ledger.consumed[g];
            if need <= 0.0 || n.ledger.imported[g] < 0.15 * need {
                continue;
            }
            for q in w.held(j) {
                if w.geo.neighbours[q].iter().any(|r| mine.contains(r)) && e.provinces[q].deposits[g] >= 0.4 && !w.capital.contains(&q) {
                    let v = e.provinces[q].deposits[g] * n.ledger.imported[g] / need;
                    if best.is_none_or(|x| v > x.0) {
                        best = Some((v, g, q));
                    }
                }
            }
        }
        if let Some((v, g, q)) = best {
            out.push(("resources", a.resources * v.min(1.0), format!("for the {} it lacks ({})", ed.goods[g].name.to_lowercase(), w.geo.names[q]), Some(q)));
        }
    }
    out
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
pub fn rise(w: &mut Wars, e: &mut Economy, ed: &EconData, i: usize) {
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
    // What broke it: the biggest drag on its people's mood.
    let n = &e.nations[i];
    let drags = [
        (-n.war_stability, "the war has worn the people out".to_string()),
        (100.0 * (0.85 - n.food_sat).max(0.0), "famine".to_string()),
        (100.0 * (n.unemployment(ed) - 0.12).max(0.0), format!("{:.0}% out of work", n.unemployment(ed) * 100.0)),
        (if n.rating >= 6 { 15.0 } else { 0.0 }, "a debt crisis".to_string()),
        (w.forces[i].lost_wars as f64 * 8.0, "lost wars".to_string()),
    ];
    let why = drags.iter().max_by(|a, b| a.0.total_cmp(&b.0)).filter(|d| d.0 > 0.0).map_or("stability collapsed".to_string(), |d| format!("stability collapsed: {}", d.1));
    let k = declare(w, e, ed, r, i, 3, Goal::Rebellion, why);
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
    // Years of stalemate: a settlement. The rebels lay down their arms for
    // an amnesty and rejoin the army; their provinces go back.
    let years = (w.day - w.wars[k].start) as f64 / ed.days_per_year as f64;
    if rebels_left && years >= w.data.other.civil_war_years {
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
        e.nations[i].stability = (e.nations[i].stability + 15.0).min(100.0);
        w.forces[i].low_stability_days = 0;
        peace::make_peace(w, e, ed, k, PeaceTerms::white(0.0), "a settlement: the rebels lay down their arms for an amnesty");
        return;
    }
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
