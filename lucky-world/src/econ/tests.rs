//! The economy's promises (docs/ECONOMY.md, section 13).

use super::*;

pub(crate) fn world(map: usize, seed: u32) -> (EconData, Economy) {
    let (_, _, ed, _, e) = crate::make_world(map, seed).unwrap_or_else(|e| panic!("{e}"));
    (ed, e)
}

/// Every map's index, by id.
fn maps() -> Vec<usize> {
    let data = crate::data::Data::load().unwrap();
    ["grand", "isles", "five_shores"]
        .iter()
        .map(|id| data.maps.iter().position(|m| m.id == *id).unwrap_or_else(|| panic!("no map {id}")))
        .collect()
}

/// A fingerprint of everything that matters in the world, to compare runs.
fn fingerprint(e: &Economy) -> Vec<u64> {
    let mut out = vec![e.day as u64];
    for n in &e.nations {
        for x in [n.gdp, n.population, n.wage, n.exchange, n.money, n.treasury, n.debt, n.cpi, n.rate, n.savings] {
            out.push(x.to_bits());
        }
        out.extend(n.stock.iter().map(|x| x.to_bits()));
        out.extend(n.price.iter().map(|x| x.to_bits()));
        out.extend(n.stockpile.iter().map(|x| x.to_bits()));
        out.push(n.buildings.len() as u64);
        out.extend(n.buildings.iter().map(|b| b.levels.to_bits()));
    }
    out.extend(e.world_price.iter().map(|x| x.to_bits()));
    out.push(e.news.len() as u64);
    out
}

#[test]
fn the_economy_data_loads() {
    let data = crate::data::Data::load().unwrap();
    let ed = EconData::load(&data).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(ed.goods.len(), 16);
    assert!(ed.buildings.len() >= 17);
}

#[test]
fn the_same_seed_gives_the_same_economy() {
    for map in maps() {
        let (ed, mut a) = world(map, 7);
        let (_, mut b) = world(map, 7);
        assert_eq!(fingerprint(&a), fingerprint(&b), "day 1 differs on map {map}");
        a.run_days(&ed, 2 * ed.days_per_year);
        b.run_days(&ed, 2 * ed.days_per_year);
        assert_eq!(fingerprint(&a), fingerprint(&b), "two years in, map {map} differs between runs");
        let (_, mut c) = world(map, 8);
        c.run_days(&ed, 2 * ed.days_per_year);
        assert_ne!(fingerprint(&a), fingerprint(&c), "another seed should give another world");
    }
}

#[test]
fn nothing_is_ever_nan_or_a_negative_stock() {
    for map in maps() {
        let (ed, mut e) = world(map, 3);
        for _ in 0..(5 * ed.days_per_year) {
            e.tick(&ed);
            for n in &e.nations {
                let numbers = [
                    n.gdp, n.gdp_local, n.population, n.wage, n.wage_w, n.exchange, n.money, n.treasury, n.debt, n.cpi, n.rate, n.savings,
                    n.inflation, n.happiness, n.stability, n.services, n.unemployed,
                ];
                for (k, x) in numbers.iter().enumerate() {
                    assert!(x.is_finite(), "{}: number {k} is {x} on day {}", n.name, e.day);
                }
                for (g, (s, p)) in n.stock.iter().zip(&n.price).enumerate() {
                    assert!(s.is_finite() && *s >= -1e-6, "{}: stock of {} is {s} on day {}", n.name, ed.goods[g].id, e.day);
                    assert!(p.is_finite() && *p > 0.0, "{}: price of {} is {p} on day {}", n.name, ed.goods[g].id, e.day);
                }
                assert!(n.stockpile.iter().all(|x| x.is_finite() && *x >= 0.0), "{}: a bad stockpile", n.name);
                assert!(n.population > 0.0 && n.treasury >= -1e-6 && n.debt >= -1e-6, "{}: people, treasury or debt below zero", n.name);
            }
            assert!(e.world_price.iter().all(|p| p.is_finite() && *p > 0.0));
        }
    }
}

#[test]
fn goods_are_never_made_from_nothing() {
    // Each day, for each nation and good: what's in store now is what was
    // there, plus what was made and bought in, less everything that left.
    let (ed, mut e) = world(maps()[0], 5);
    for _ in 0..(2 * ed.days_per_year) {
        let before: Vec<Vec<f64>> = e.nations.iter().map(|n| n.stock.clone()).collect();
        let piles: Vec<Vec<f64>> = e.nations.iter().map(|n| n.stockpile.clone()).collect();
        e.tick(&ed);
        for (i, n) in e.nations.iter().enumerate() {
            let l = &n.ledger;
            for g in 0..ed.goods.len() {
                let expect = before[i][g] + l.produced[g] + l.imported[g] - l.used[g] - l.consumed[g] - l.exported[g] - l.spoiled[g] - l.stockpiled[g];
                let scale = before[i][g].abs() + l.produced[g] + l.imported[g] + 1.0;
                assert!(
                    (n.stock[g] - expect).abs() <= 1e-6 * scale,
                    "{}, {} on day {}: stock {} but the books say {}",
                    n.name, ed.goods[g].id, e.day, n.stock[g], expect
                );
                // Weapons only reach the stockpile by being bought.
                assert!(n.stockpile[g] - piles[i][g] <= l.stockpiled[g] + 1e-6);
            }
        }
        // What the world exported, it imported: nothing is made at sea.
        for g in 0..ed.goods.len() {
            let out: f64 = e.nations.iter().map(|n| n.ledger.exported[g]).sum();
            let inn: f64 = e.nations.iter().map(|n| n.ledger.imported[g]).sum();
            assert!((out - inn).abs() <= 1e-6 * (out + 1.0), "{}: exported {out} but imported {inn}", ed.goods[g].id);
        }
    }
}

#[test]
fn a_broken_chain_stops_everything_after_it() {
    // A nation with no bauxite, none to buy and none in store makes no
    // aluminium; with no aluminium, no jets.
    let (ed, mut e) = world(maps()[0], 1);
    let bauxite = ed.good_index("bauxite").unwrap();
    let aluminium = ed.good_index("aluminium").unwrap();
    let i = e
        .nations
        .iter()
        .position(|n| n.buildings.iter().any(|b| ed.buildings[b.kind].output == ed.jets))
        .expect("some nation builds jets");
    {
        let n = &mut e.nations[i];
        n.buildings.retain(|b| ed.buildings[b.kind].output != bauxite);
        n.hire_order = (0..n.buildings.len()).collect();
        for g in [bauxite, aluminium] {
            n.stock[g] = 0.0;
            n.trade_policy[g] = TradePolicy::NoImports;
        }
        n.state_autopilot = false;
    }
    e.tick(&ed);
    let n = &e.nations[i];
    assert_eq!(n.ledger.produced[aluminium], 0.0, "{} made aluminium from no bauxite", n.name);
    assert_eq!(n.ledger.produced[ed.jets], 0.0, "{} made jets from no aluminium", n.name);
    for b in n.buildings.iter().filter(|b| ed.buildings[b.kind].output == aluminium || ed.buildings[b.kind].output == ed.jets) {
        assert_eq!(b.output, 0.0);
        assert!(b.input_factor < 1e-9, "a starved building says it had its inputs");
    }
    // Everything before the break still works.
    assert!(n.ledger.produced[ed.electricity] > 0.0, "power stopped too");
    // And a month on, still nothing (the investors can't build bauxite mines out of the plan either: the chain stays broken while nobody digs).
    let mut e2 = e.clone();
    for _ in 0..30 {
        let n = &mut e2.nations[i];
        n.buildings.retain(|b| ed.buildings[b.kind].output != bauxite);
        n.construction.retain(|c| ed.buildings[c.kind].output != bauxite);
        n.hire_order = (0..n.buildings.len()).collect();
        e2.tick(&ed);
        assert_eq!(e2.nations[i].ledger.produced[ed.jets], 0.0);
    }
}

#[test]
fn landlocked_nations_pay_more_to_trade() {
    let mut checked = 0;
    for map in maps() {
        let (ed, mut e) = world(map, 1);
        let land: Vec<usize> = (0..e.nations.len()).filter(|&i| e.nations[i].landlocked).collect();
        let sea: Vec<usize> = (0..e.nations.len()).filter(|&i| !e.nations[i].landlocked).collect();
        if land.is_empty() {
            continue;
        }
        // By the rules: every good costs a landlocked nation more to move.
        for &a in &land {
            for &b in &sea {
                for g in (0..ed.goods.len()).filter(|&g| ed.goods[g].trade) {
                    let w = e.world_price[g];
                    let (la, sb) = (tick::trade_cost(&ed, &e.nations[a], g, w), tick::trade_cost(&ed, &e.nations[b], g, w));
                    let fair = la / e.nations[a].mod_trade_cost > sb / e.nations[b].mod_trade_cost;
                    assert!(fair, "{} (landlocked) pays {la} for {} but {} pays {sb}", e.nations[a].name, ed.goods[g].id, e.nations[b].name);
                }
            }
        }
        // And in practice: over a year, transport eats a bigger share of what they trade.
        e.run_days(&ed, ed.days_per_year);
        let share = |idx: &[usize]| {
            let v: Vec<f64> = idx.iter().map(|&i| e.nations[i].transport_share).filter(|s| *s > 0.0).collect();
            v.iter().sum::<f64>() / v.len().max(1) as f64
        };
        assert!(share(&land) > share(&sea), "landlocked {} vs coastal {}", share(&land), share(&sea));
        checked += 1;
    }
    assert!(checked > 0, "no map has a landlocked nation");
}

#[test]
fn default_nations_neither_spiral_nor_collapse_in_fifty_years() {
    for map in maps() {
        let (ed, mut e) = world(map, 1);
        let dpy = ed.days_per_year;
        e.run_days(&ed, 49 * dpy);
        let cpi_49: Vec<f64> = e.nations.iter().map(|n| n.cpi).collect();
        e.run_days(&ed, dpy);
        for (n, c49) in e.nations.iter().zip(&cpi_49) {
            let yearly = n.cpi / c49 - 1.0;
            assert!(yearly < 0.6, "{}: prices rose {:.0}% in year 50 (hyperinflation)", n.name, yearly * 100.0);
            assert!(yearly > -0.6, "{}: prices fell {:.0}% in year 50", n.name, -yearly * 100.0);
            assert!(n.gdp >= n.gdp_start / 5.0, "{}: GDP fell from {:.3e} to {:.3e}", n.name, n.gdp_start, n.gdp);
            assert!(n.population >= 0.25 * n.population_start, "{}: most of the people are gone", n.name);
            assert!(n.unemployment(&ed) < 0.6, "{}: {:.0}% out of work", n.name, n.unemployment(&ed) * 100.0);
        }
        let start: f64 = e.nations.iter().map(|n| n.gdp_start).sum();
        let now: f64 = e.nations.iter().map(|n| n.gdp).sum();
        assert!(now > start, "the world got poorer over 50 years on map {map}");
    }
}

/// The balance rules (docs/ECONOMY.md, section 14), watched every month for
/// 50 years on every map with six seeds (18 worlds, one thread each):
/// - no nation above $300k GDP per head;
/// - no nation above 25% unemployment for more than 5 years straight;
/// - world inflation between -3% and 8% at every year's end;
/// - never more than 2 nations above 15% inflation at once;
/// - and no nation below a fifth of its starting GDP at the end.
#[test]
fn the_balance_rules_hold_for_fifty_years_on_six_seeds() {
    let jobs: Vec<(usize, u32)> = maps().into_iter().flat_map(|m| (1..=6).map(move |s| (m, s))).collect();
    let results: Vec<(usize, u32, report::Health)> = std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .iter()
            .map(|&(map, seed)| {
                scope.spawn(move || {
                    let (ed, mut e) = world(map, seed);
                    (map, seed, report::run_and_watch(&mut e, &ed, 50))
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("a world panicked")).collect()
    });
    let broken: Vec<String> = results
        .iter()
        .filter(|(.., h)| !h.problems().is_empty())
        .map(|(map, seed, h)| format!("map {map} seed {seed}: {}", h.problems().join("; ")))
        .collect();
    assert!(broken.is_empty(), "{} of {} worlds broke a balance rule:\n{}", broken.len(), results.len(), broken.join("\n"));
}
