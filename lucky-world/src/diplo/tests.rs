//! Diplomacy tests (docs/DIPLOMACY.md, section 13).

use super::{alliances, spies, treaties, DiploData, EventKind, Kind, Proposal, State};
use crate::war::escalation;
use crate::war::report::{rivals, WarWorld};

/// A world with diplomacy, run by hand: no brains deciding wars or deals.
fn quiet(map: usize, seed: u32) -> WarWorld {
    let mut ww = WarWorld::new(map, seed, true).unwrap().with_diplomacy(seed).unwrap();
    ww.wars.ai = false;
    ww.diplo.as_mut().unwrap().ai = false;
    ww
}

fn sign(ww: &mut WarWorld, kind: Kind, from: usize, to: usize) -> usize {
    let p = Proposal { from, to, kind, day: ww.wars.day, good: None, amount: 0.0, target: None };
    let d = ww.diplo.as_mut().unwrap();
    treaties::sign(d, &mut ww.e, &ww.ed, &mut ww.wars, &p)
}

#[test]
fn the_diplomacy_file_loads_and_a_typo_is_one_clear_sentence() {
    let d = DiploData::load().unwrap_or_else(|e| panic!("{e}"));
    assert!(d.missions.len() == 9);
    let text = std::fs::read_to_string("data/diplomacy/diplomacy.toml").unwrap().replace("half_life = 10.0", "half_lfe = 10.0");
    let err = DiploData::parse(&text).unwrap_err();
    assert!(err.contains("diplomacy.toml") && err.contains("half_lfe"), "{err}");
}

#[test]
fn the_same_seed_gives_the_same_diplomacy() {
    let run = || {
        let mut ww = WarWorld::new(0, 3, true).unwrap().with_diplomacy(3).unwrap();
        for _ in 0..(3 * 360) {
            ww.day();
        }
        let d = ww.diplo.unwrap();
        let treaties: Vec<(Kind, usize, usize, u32, bool)> = d.treaties.iter().map(|t| (t.kind, t.a, t.b, t.start, t.active())).collect();
        let reports: Vec<(usize, usize, bool, u32)> = d.spies.reports.iter().map(|r| (r.by, r.target, r.success, r.caught)).collect();
        (treaties, reports, d.reputation, d.relations)
    };
    let (a, b) = (run(), run());
    assert!(!a.0.is_empty(), "treaties were signed");
    assert_eq!(a.0, b.0, "the same treaties");
    assert_eq!(a.1, b.1, "the same spy missions");
    assert_eq!(a.2, b.2);
    assert_eq!(a.3, b.3);
}

#[test]
fn attacking_a_partner_breaks_the_pact_and_everyone_sees_it() {
    let mut ww = quiet(0, 1);
    let (a, v) = rivals(&ww.wars).unwrap();
    let ally = (0..ww.wars.nations).find(|&x| x != a && x != v).unwrap();
    let pact = sign(&mut ww, Kind::NonAggression, a, v);
    sign(&mut ww, Kind::Alliance, v, ally);
    let rep_before = ww.diplo.as_ref().unwrap().reputation[a];
    escalation::declare(&mut ww.wars, &mut ww.e, &ww.ed, a, v, 3, crate::war::Goal::Reparations, "a test".to_string());
    for _ in 0..31 {
        ww.day();
    }
    let d = ww.diplo.as_ref().unwrap();
    assert!(matches!(&d.treaties[pact].state, State::Broken { by, .. } if *by == a), "{:?}", d.treaties[pact].state);
    assert!((rep_before - d.reputation[a] - d.data.break_cost(Kind::NonAggression)).abs() < 1.0 + d.data.reputation.recover * 100.0, "reputation fell by the cost");
    assert!(d.memory[v][a].iter().any(|m| m.kind == EventKind::BrokeTreatyWithUs));
    assert!(d.memory[ally][a].iter().any(|m| m.kind == EventKind::BrokeTreatyWithAlly), "the victim's ally remembers");
    // Everyone else thinks less of the liar: its reputation is in every
    // nation's relations with it, (reputation − 50) × 0.2.
    let x = (0..ww.wars.nations).find(|&x| x != a && x != v && x != ally).unwrap();
    let rep_term = (d.reputation[a] - d.data.reputation.start) * d.data.relations.reputation;
    let before_term = (rep_before - d.data.reputation.start) * d.data.relations.reputation;
    assert!(rep_term < before_term - 2.0, "{x}'s view of {a} falls with its reputation");
    // The ally was called, and (honouring or refusing) the rules were applied.
    let k = ww.wars.wars.len() - 1;
    let joined = ww.wars.wars[k].side_of(ally) == Some(1);
    let refused = d.treaties.iter().any(|t| t.kind == Kind::Alliance && matches!(&t.state, State::Broken { by, .. } if *by == ally));
    assert!(joined || refused, "the ally either came or broke the alliance");
}

#[test]
fn refusing_a_call_to_arms_breaks_the_pact_and_honouring_it_joins_the_war() {
    for honour in [0.05, 1.0] {
        let mut ww = quiet(0, 2);
        let (a, v) = rivals(&ww.wars).unwrap();
        let x = (0..ww.wars.nations).find(|&x| x != a && x != v && ww.wars.at_war(x, a).is_none()).unwrap();
        let id = sign(&mut ww, Kind::Defensive, v, x);
        {
            let d = ww.diplo.as_mut().unwrap();
            d.traits[x].honour = honour;
            d.relations[x][v] = if honour > 0.5 { 80.0 } else { -50.0 };
            d.relations[x][a] = if honour > 0.5 { -80.0 } else { 80.0 };
        }
        escalation::declare(&mut ww.wars, &mut ww.e, &ww.ed, a, v, 3, crate::war::Goal::Reparations, "a test".to_string());
        {
            let d = ww.diplo.as_mut().unwrap();
            alliances::watch_wars(d, &mut ww.e, &ww.ed, &mut ww.wars, None);
        }
        let d = ww.diplo.as_ref().unwrap();
        let k = ww.wars.wars.len() - 1;
        if honour > 0.5 {
            assert_eq!(ww.wars.wars[k].side_of(x), Some(1), "an honourable ally comes");
            assert!(d.treaties[id].active());
            assert!(d.memory[v][x].iter().any(|m| m.kind == EventKind::CameToAid));
        } else {
            assert_eq!(ww.wars.wars[k].side_of(x), None);
            assert!(matches!(&d.treaties[id].state, State::Broken { by, .. } if *by == x), "refusing breaks the pact");
            assert!(d.memory[v][x].iter().any(|m| m.kind == EventKind::RefusedCall));
        }
    }
}

#[test]
fn a_loan_is_repaid_or_its_default_breaks_it() {
    let mut ww = quiet(1, 1);
    let (lender, borrower) = (0, 1);
    let amount = 0.01 * ww.e.nations[lender].gdp;
    let p = Proposal { from: borrower, to: lender, kind: Kind::Loan, day: 0, good: None, amount, target: None };
    let id = {
        let d = ww.diplo.as_mut().unwrap();
        treaties::sign(d, &mut ww.e, &ww.ed, &mut ww.wars, &p)
    };
    for _ in 0..90 {
        ww.day();
    }
    let owed_now = ww.diplo.as_ref().unwrap().treaties[id].owed;
    assert!(owed_now < amount, "it is being repaid");
    // Then the borrower falls into default: three months later the loan is broken.
    let d_index = ww.ed.ratings.len() - 1;
    ww.e.nations[borrower].rating = d_index;
    let d = ww.diplo.as_mut().unwrap();
    for _ in 0..3 {
        d.day += 30;
        treaties::month(d, &mut ww.e, &ww.ed, &mut ww.wars);
    }
    let d = ww.diplo.as_ref().unwrap();
    assert!(matches!(&d.treaties[id].state, State::Broken { by, .. } if *by == borrower), "{:?}", d.treaties[id].state);
    assert!(d.memory[lender][borrower].iter().any(|m| m.kind == EventKind::Defaulted));
}

#[test]
fn an_arms_deal_moves_every_weapon_and_its_price() {
    let mut ww = quiet(0, 1);
    let g = ww.ed.tanks;
    let (buyer, seller) = (0, 1);
    ww.e.nations[seller].stockpile[g] = 1000.0;
    let before = (ww.e.nations[buyer].stockpile[g], ww.e.nations[seller].stockpile[g], ww.e.nations[seller].treasury);
    let p = Proposal { from: buyer, to: seller, kind: Kind::Arms, day: 0, good: Some(g), amount: 100.0, target: None };
    let id = {
        let d = ww.diplo.as_mut().unwrap();
        treaties::sign(d, &mut ww.e, &ww.ed, &mut ww.wars, &p)
    };
    let t = ww.diplo.as_ref().unwrap().treaties[id].clone();
    let q = t.rate;
    assert!(q > 0.0 && q <= 100.0);
    assert_eq!(ww.e.nations[seller].stockpile[g], before.1 - q, "the seller's stockpile");
    assert_eq!(ww.e.nations[buyer].stockpile[g], before.0 + q, "the buyer's stockpile");
    assert_eq!(ww.wars.forces[seller].flows.given[g], q);
    assert_eq!(ww.wars.forces[buyer].flows.received[g], q);
    assert!(ww.e.nations[seller].treasury > before.2, "the seller was paid");
}

#[test]
fn spies_never_reveal_more_than_they_should() {
    let mut ww = quiet(0, 1);
    let d = ww.diplo.as_mut().unwrap();
    let (by, j, k) = (0, 1, 2);
    let floor = d.data.spies.intel_floor;
    let truth_j = d.spies.counter_intel(&d.data, j);
    let truth_k = d.spies.counter_intel(&d.data, k);
    let before_k = spies::estimate_ci(d, by, k);
    // Without intel: the estimate is off, by up to the service's error.
    let mut off = 0;
    for m in 0..24 {
        d.day = m * 30;
        let est = spies::estimate_ci(d, by, j);
        let err = (est / truth_j).ln().abs();
        assert!(err <= 2.0 * 4.0 * d.intel_error[by] + 1e-9, "never wilder than four sigma");
        if err > 1e-6 {
            off += 1;
        }
    }
    assert!(off >= 20, "an estimate without intel is not the truth");
    // Intel on j narrows j's error, never under the floor; k is untouched.
    d.day = 0;
    d.spies.intel_until[by][j] = 10_000;
    for m in 0..24 {
        d.day = m * 30;
        let est = spies::estimate_ci(d, by, j);
        let err = (est / truth_j).ln().abs();
        let sigma = (d.intel_error[by] * d.data.spies.intel_factor).max(floor);
        assert!(err <= 2.0 * 4.0 * sigma + 1e-9);
    }
    d.day = 0;
    assert_eq!(spies::estimate_ci(d, by, k), before_k, "intel on one nation says nothing about another");
    assert!((spies::estimate_ci(d, by, k) / truth_k - 1.0).abs() > 0.0 || d.intel_error[by] == 0.0);
    // The war and the brains get intel only on that nation.
    let f = d.spies.intel_factors(d.day, d.n, &d.data);
    assert!(f[by][j] < 1.0 && f[by][k] == 1.0 && f[j][by] == 1.0);
}

#[test]
fn answers_say_why() {
    let ww = quiet(0, 1);
    let d = ww.diplo.as_ref().unwrap();
    let p = Proposal { from: 0, to: 1, kind: Kind::Alliance, day: 0, good: None, amount: 0.0, target: None };
    let (_, reason) = treaties::answer(d, &ww.e, &ww.ed, &ww.wars, ww.brains.as_ref(), 1, &p);
    assert!(!reason.is_empty());
    // Relations come with their reasons, and add up.
    let sum: f64 = d.reasons[0][1].iter().map(|r| r.1).sum();
    assert!((sum.clamp(-100.0, 100.0) - d.relations[0][1]).abs() < 1e-9);
}

/// The full game for 50 years on every map and six seeds: the world economy
/// still grows 1.5-3% a year with war on, and diplomacy prevents wars and
/// ends others at the table (docs/DIPLOMACY.md, section 14).
#[test]
fn fifty_years_of_the_full_game_grow_and_diplomacy_keeps_some_peace() {
    let jobs: Vec<(usize, u32)> = (1..=6).flat_map(|s| (0..3).map(move |m| (m, s))).collect();
    let results: Vec<super::report::WorldDiplo> = std::thread::scope(|scope| {
        let handles: Vec<_> = jobs.iter().map(|&(m, s)| scope.spawn(move || super::report::run(m, s, 50.0, true).unwrap_or_else(|e| panic!("{e}")))).collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let growth = results.iter().map(|r| r.growth).sum::<f64>() / results.len() as f64;
    assert!((0.015..=0.03).contains(&growth), "world growth {:.2}% a year", growth * 100.0);
    let stat = |k: &str| results.iter().map(|r| r.stats.get(k).copied().unwrap_or(0.0)).sum::<f64>();
    assert!(stat("wars prevented by diplomacy") > 0.0, "diplomacy prevented no war");
    let talks: f64 = results.iter().flat_map(|r| r.stats.iter()).filter(|(k, _)| k.starts_with("negotiated peace")).map(|(_, v)| v).sum();
    assert!(talks > 0.0, "no war ended at the table");
    assert!(stat("calls to arms honoured") > 0.0);
    let signed: f64 = results.iter().flat_map(|r| r.stats.iter()).filter(|(k, _)| k.starts_with("signed: ")).map(|(_, v)| v).sum();
    assert!(signed > 100.0, "{signed} treaties signed");
}

#[test]
fn a_treaty_in_force_is_not_signed_twice() {
    let mut ww = quiet(0, 1);
    let id = sign(&mut ww, Kind::Trade, 0, 1);
    let d = ww.diplo.as_ref().unwrap();
    assert!(d.has(0, 1, Kind::Trade) && d.has(1, 0, Kind::Trade), "the index finds it, both ways");
    assert_eq!(d.live().count(), 1);
    let again = Proposal { from: 1, to: 0, kind: Kind::Trade, day: 0, good: None, amount: 0.0, target: None };
    assert!(treaties::allowed(d, &ww.e, &ww.wars, &again).is_err());
    // Ended, it can be signed again.
    let d = ww.diplo.as_mut().unwrap();
    treaties::end_treaty(d, &mut ww.e, id);
    assert!(!d.has(0, 1, Kind::Trade));
    assert!(treaties::allowed(d, &ww.e, &ww.wars, &again).is_ok());
}
