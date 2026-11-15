//! Luck, rot and the autopilot (docs/ECONOMY.md, sections 7, 10 and 11):
//! events, construction, ratings, taxes, investment, closures and
//! infrastructure.

use super::data::{EconData, EventKind, Sector, Tier};
use super::init::add_building;
use super::{tech, ActiveEvent, Construction, Economy, News};

const EPS: f64 = 1e-9;

impl Economy {
    fn news(&mut self, nation: Option<usize>, text: String, good: bool) {
        self.news.push(News { day: self.day, nation, text, good });
        if self.news.len() > 300 {
            self.news.drain(0..100);
        }
    }

    /// World events (an oil shock), and old shocks wearing off.
    pub(super) fn world_events(&mut self, ed: &EconData) {
        self.world_shocks.retain_mut(|s| {
            s.2 = s.2.saturating_sub(1);
            s.2 > 0
        });
        let dpy = ed.days_per_year as f64;
        for (e, ev) in ed.events.iter().enumerate() {
            if !ev.world {
                continue;
            }
            if (self.rng.next_f32() as f64) < ev.chance / dpy {
                for &(g, m) in &ev.price {
                    self.world_shocks.push((g, m, ev.days));
                }
                let text = format!("{}: {}", ev.name, ev.text);
                self.news(None, text, ev.good);
                let _ = e;
            }
        }
    }

    /// A nation's events, its modifiers for today, and construction finishing.
    pub(super) fn nation_events(&mut self, ed: &EconData, i: usize) {
        let dpy = ed.days_per_year as f64;
        // Old events wear off; building goes on.
        {
            let n = &mut self.nations[i];
            let before = n.events.len();
            n.events.retain_mut(|e| {
                e.days_left = e.days_left.saturating_sub(1);
                e.days_left > 0
            });
            n.mods_dirty |= n.events.len() != before;
            let mut done = Vec::new();
            for c in n.construction.iter_mut() {
                c.days_left = c.days_left.saturating_sub(1);
                if c.days_left == 0 {
                    done.push(c.clone());
                }
            }
            n.construction.retain(|c| c.days_left > 0);
            for c in done {
                add_building(&mut n.buildings, c.kind, c.province, c.levels, c.by_player);
            }
            if n.hire_order.len() != n.buildings.len() {
                n.hire_order = (0..n.buildings.len()).collect();
            }
        }
        // New events.
        for (e, ev) in ed.events.iter().enumerate() {
            if ev.world {
                continue;
            }
            let chance = ev.chance / dpy * self.nations[i].mod_event[e];
            if (self.rng.next_f32() as f64) >= chance {
                continue;
            }
            let roll = self.rng.next_f32() as f64;
            let text = match ev.kind {
                EventKind::Modifier => {
                    self.nations[i].events.push(ActiveEvent { event: e, days_left: ev.days });
                    self.nations[i].mods_dirty = true;
                    format!("{}: {}", ev.name, ev.text)
                }
                EventKind::Discovery => {
                    let n = &self.nations[i];
                    let q = n.provinces[(roll * n.provinces.len() as f64) as usize % n.provinces.len()];
                    let raws: Vec<usize> = (0..ed.goods.len()).filter(|&g| ed.goods[g].tier == Tier::Raw).collect();
                    let g = raws[(self.rng.next_u32() as usize) % raws.len()];
                    self.provinces[q].deposits[g] += 0.8;
                    format!("{}: {} ({})", ev.name, ev.text, ed.goods[g].name)
                }
                EventKind::Fire => {
                    let n = &mut self.nations[i];
                    let factories: Vec<usize> = (0..n.buildings.len())
                        .filter(|&b| ed.buildings[n.buildings[b].kind].sector == Sector::Industry && n.buildings[b].levels > 0.0)
                        .collect();
                    if factories.is_empty() {
                        continue;
                    }
                    let b = factories[(roll * factories.len() as f64) as usize % factories.len()];
                    let [lo, hi] = ev.destroy;
                    let share = lo + (hi - lo) * self.rng.next_f32() as f64;
                    n.buildings[b].levels *= 1.0 - share;
                    format!("{}: {} ({}, {:.0}% lost)", ev.name, ev.text, ed.buildings[n.buildings[b].kind].name, share * 100.0)
                }
                EventKind::Damage => {
                    let n = &mut self.nations[i];
                    let q = n.provinces[(roll * n.provinces.len() as f64) as usize % n.provinces.len()];
                    let p = &mut self.provinces[q];
                    p.road = (p.road - 1.0).max(0.0);
                    p.rail = (p.rail - 1.0).max(0.0);
                    n.events.push(ActiveEvent { event: e, days_left: ev.days.max(1) });
                    n.mods_dirty = true;
                    format!("{}: {}", ev.name, ev.text)
                }
            };
            self.news(Some(i), text, ev.good);
        }
        // Today's modifiers: the wheels, with events on top.
        let n = &mut self.nations[i];
        if n.mods_dirty {
            let mut mods = n.base_effects.clone();
            for a in &n.events {
                mods.stack(&ed.events[a.event].effects);
            }
            n.mods = mods;
            n.refresh_fast_mods(ed);
            n.mods_dirty = false;
        }
        let md = &ed.models[n.model];
        n.corruption = (md.corruption + n.mods.get("corruption")).clamp(0.0, 0.6);
    }

    /// Once a month: ratings, taxes, investment, closures, infrastructure.
    pub(super) fn monthly(&mut self, ed: &EconData, i: usize) {
        let dpy = ed.days_per_year as f64;
        {
            let n = &mut self.nations[i];
            n.budget_last_month = std::mem::take(&mut n.budget_month);
            let count = n.exports_to.len();
            n.exports_to_last = std::mem::replace(&mut n.exports_to, vec![0.0; count]);
            n.imports_from_last = std::mem::replace(&mut n.imports_from, vec![0.0; count]);
            n.transport_share = if n.traded_value > EPS { n.transport_paid / n.traded_value } else { n.transport_share };
            n.transport_paid = 0.0;
            n.traded_value = 0.0;

            // Credit rating.
            let debt_ratio = n.debt_ratio(ed);
            let score = 100.0 - 60.0 * debt_ratio - 300.0 * n.deficit_ema.max(0.0) - 80.0 * (n.inflation - 0.05).max(0.0)
                + 0.2 * (n.stability - 50.0)
                + n.mods.get("rating");
            n.rating = ed.rating_for(score);

            // Taxes (unless the player has taken them over).
            if !n.manual_taxes {
                let md = &ed.models[n.model];
                let step = ed.state.tax_step;
                // Counter-cyclical: in a slump (unemployment well above its
                // natural rate) a deficit is borrowed, not taxed away, and a
                // small one is let grow.
                let u = n.unemployment(ed);
                let u_nat = super::data::interpolate(&ed.natural_unemployment_by_hdi, n.hdi);
                let slump = u > 1.8 * u_nat + 0.03;
                let mut tighten = 0.0;
                if n.deficit_ema > ed.state.deficit_target && !slump {
                    tighten = 1.0;
                } else if n.deficit_ema < -0.02 || (slump && n.deficit_ema < ed.state.deficit_target) {
                    tighten = -1.0;
                }
                if debt_ratio > 0.9 {
                    tighten += 1.0;
                }
                // Printing money last month: hard austerity.
                if n.printed_month > 0.0 {
                    tighten += 2.0;
                }
                n.printed_month = 0.0;
                n.income_tax = (n.income_tax + tighten * step).clamp(0.05, 0.55);
                n.sales_tax = (n.sales_tax + tighten * step * 0.5).clamp(0.0, 0.25);
                n.welfare_rate = (n.welfare_rate - tighten * 0.01).clamp(0.1, ed.welfare_rate.max(md.income_tax));
            }
        }
        self.invest(ed, i);
        self.close_losers(ed, i);
        if self.nations[i].state_autopilot {
            self.build_infrastructure(ed, i);
        }
        self.refresh_hire_order(ed, i);
        let _ = dpy;
        if i + 1 == self.nations.len() {
            self.refresh_trade_capacity(ed);
        }
    }

    /// Who hires first: by profit (market, mixed) or by the plan.
    pub(super) fn refresh_hire_order(&mut self, ed: &EconData, i: usize) {
        let n = &mut self.nations[i];
        let planned = ed.models[n.model].planned;
        let mut order: Vec<usize> = (0..n.buildings.len()).collect();
        if planned {
            // People eat first; then down the chains (what feeds a factory
            // is staffed before the factory), in plan order within a tier.
            order.sort_by_key(|&b| {
                let bd = &ed.buildings[n.buildings[b].kind];
                let out = bd.output;
                let basic = out == ed.electricity || out == ed.food || out == ed.grain;
                (!basic, bd.tier, ed.plan_priority.iter().position(|&g| g == out).unwrap_or(99), b)
            });
        } else {
            // Food and power first (nobody lets those stop), then by profit.
            order.sort_by(|&a, &b| {
                let key = |x: usize| {
                    let out = ed.buildings[n.buildings[x].kind].output;
                    let basic = out == ed.electricity || out == ed.food || out == ed.grain;
                    (!basic, -n.buildings[x].profit)
                };
                let (ka, kb) = (key(a), key(b));
                ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1)).then(a.cmp(&b))
            });
        }
        n.hire_order = order;
    }

    /// Private firms build where profit is best; the state builds to its plan.
    fn invest(&mut self, ed: &EconData, i: usize) {
        let dpy = ed.days_per_year as f64;
        let wpl = ed.workers_per_level;
        let (private_budget, state_budget, planned) = {
            let n = &self.nations[i];
            let md = &ed.models[n.model];
            let rate_factor = (-6.0 * (n.rate - 0.03)).exp().clamp(0.2, 2.0);
            let private = n.savings.max(0.0) * 0.15 * md.private_investment * n.mods.get("investment") * rate_factor;
            let state = if n.state_autopilot && n.deficit_ema < 0.08 {
                md.state_investment * 0.004 * n.gdp_local * dpy * n.mods.get("investment")
            } else {
                0.0
            };
            (private, state, md.planned)
        };

        // How much each good is short at home (use beyond what is made and bought in).
        let shortage: Vec<f64> = {
            let n = &self.nations[i];
            (0..ed.goods.len())
                .map(|g| {
                    let supply = n.ledger.produced[g] + n.ledger.imported[g];
                    if n.use_rate[g] > EPS { ((n.use_rate[g] - supply) / n.use_rate[g]).clamp(-1.0, 1.0) } else { 0.0 }
                })
                .collect()
        };

        // Candidates: each building kind, its best place, and what a level would earn.
        let mut options: Vec<(f64, usize, usize, f64)> = Vec::new(); // (score, kind, province, max levels)
        {
            let n = &self.nations[i];
            let t = tech(ed, n);
            for (k, bd) in ed.buildings.iter().enumerate() {
                if bd.output == ed.jets && n.hdi < 0.6 {
                    continue;
                }
                // Where: the richest deposit with room, or where the people are.
                let mut best: Option<(usize, f64, f64)> = None; // (province, richness, room)
                for &q in &n.provinces {
                    let p = &self.provinces[q];
                    let (rich, room) = if bd.deposit {
                        let d = p.deposits[bd.output];
                        if d < 0.2 {
                            continue;
                        }
                        let have: f64 = n.buildings.iter().filter(|b| b.kind == k && b.province == q).map(|b| b.levels).sum::<f64>()
                            + n.construction.iter().filter(|c| c.kind == k && c.province == q).map(|c| c.levels).sum::<f64>();
                        (d.min(3.0), d.min(3.0) * p.hexes * bd.cap_per_hex - have)
                    } else {
                        (1.0, f64::INFINITY)
                    };
                    if room <= 0.5 {
                        continue;
                    }
                    let value = if bd.deposit { rich } else { p.people + if q == n.capital { 0.2 } else { 0.0 } };
                    if best.is_none_or(|(_, v, _)| value > v) {
                        best = Some((q, value, room));
                    }
                }
                let Some((q, rich_or_people, room)) = best else { continue };
                let rich = if bd.deposit { rich_or_people } else { 1.0 };
                let existing: Vec<&super::Building> = n.buildings.iter().filter(|b| b.kind == k && b.levels > 0.0).collect();
                let levels: f64 = existing.iter().map(|b| b.levels).sum();
                let per_level = if levels > 0.5 {
                    existing.iter().map(|b| b.profit * b.levels).sum::<f64>() / levels
                } else {
                    let out = bd.amount * t * rich * super::skill_factor(n, bd) * super::output_mod(ed, n, bd.output) * (1.0 - super::defect_rate(n, bd));
                    let inputs: f64 = bd.inputs.iter().map(|&(g, per)| out * per * n.price[g]).sum();
                    out * n.price[bd.output] - inputs - wpl * n.wage * (1.0 + bd.skill * ed.skill_pay) - bd.upkeep / n.exchange
                };
                // An input nobody has, made at home or bought in, rules it out.
                let starved = bd.inputs.iter().any(|&(g, _)| n.stock[g] <= EPS && n.ledger.produced[g] + n.ledger.imported[g] <= EPS);
                if starved {
                    continue;
                }
                // A glut at home: no new building for it, whatever it earns.
                let out = bd.output;
                if shortage[out] < 0.05 && n.stock[out] > 1.5 * ed.prices.days_of_cover * n.use_rate[out] {
                    continue;
                }
                let cost = bd.cost * n.mods.get("build_cost") / n.exchange;
                let roi = per_level * dpy / cost.max(EPS);
                let need = shortage[bd.output];
                let score = if planned { need * 2.0 + roi.clamp(-1.0, 1.0) * 0.2 } else { roi + need * 0.3 };
                // Grow by about what is missing (a little at least), not by leaps.
                let room = room.min(levels * (0.03 + need.max(0.0)) + 2.0);
                options.push((score, k, q, room));
            }
        }
        options.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));

        // Private firms: the best returns above the hurdle (market and mixed).
        let hurdle = self.nations[i].rate + 0.08;
        let mut budget = private_budget;
        for &(score, k, q, room) in options.iter().take(4) {
            if planned || budget <= EPS || score < hurdle {
                break;
            }
            let spent = self.order(ed, i, k, q, room, budget / 2.0, false);
            budget -= spent;
            self.nations[i].savings -= spent;
        }
        // The state: what is short (planned and mixed), basic goods first.
        let mut budget = state_budget;
        let mut state_options: Vec<&(f64, usize, usize, f64)> = options
            .iter()
            .filter(|o| {
                let out = ed.buildings[o.1].output;
                let basic = out == ed.electricity || out == ed.food || out == ed.grain || ed.goods[out].military;
                shortage[out] > 0.02 && (planned || basic)
            })
            .collect();
        state_options.sort_by(|a, b| shortage[ed.buildings[b.1].output].total_cmp(&shortage[ed.buildings[a.1].output]).then(a.1.cmp(&b.1)));
        for &&(_, k, q, room) in state_options.iter().take(4) {
            if budget <= EPS {
                break;
            }
            let spent = self.order(ed, i, k, q, room, budget / 2.0, true);
            budget -= spent;
            let n = &mut self.nations[i];
            n.treasury -= spent;
            cover_treasury(n);
            n.budget_today.construction += spent;
            n.budget_month.construction += spent;
        }
    }

    /// Orders new levels of a building, spending at most `money` (and the
    /// build goods from stock). Gives back what it spent.
    #[allow(clippy::too_many_arguments)]
    fn order(&mut self, ed: &EconData, i: usize, kind: usize, province: usize, room: f64, money: f64, by_state: bool) -> f64 {
        let n = &mut self.nations[i];
        let bd = &ed.buildings[kind];
        let cost = bd.cost * n.mods.get("build_cost") / n.exchange;
        let workers = n.unemployed * 0.4 / ed.workers_per_level;
        let mut levels = (money / cost.max(EPS)).min(room).min(workers.max(0.0));
        for &(g, per) in &bd.build_goods {
            if per > 0.0 {
                levels = levels.min(n.stock[g] * 0.5 / per);
            }
        }
        if levels < 0.05 {
            return 0.0;
        }
        for &(g, per) in &bd.build_goods {
            let q = per * levels;
            n.stock[g] -= q;
            n.ledger.used[g] += q;
        }
        n.construction.push(Construction {
            kind,
            province,
            levels,
            days_left: bd.build_days,
            by_state,
            by_player: false,
        });
        // The money is spent at home over the building time.
        n.projects.push((levels * cost / bd.build_days as f64, bd.build_days));
        levels * cost
    }

    /// Buildings losing money for half a year shrink (not in planned economies,
    /// and never the player's own).
    fn close_losers(&mut self, ed: &EconData, i: usize) {
        let n = &mut self.nations[i];
        if ed.models[n.model].planned {
            return;
        }
        // What the nation is short of stays open, losing or not (the state
        // keeps the lights on): only surplus makers close.
        let short: Vec<bool> = (0..ed.goods.len())
            .map(|g| {
                let wanted_short = n.wanted[g] > 1e-9 && n.got[g] < 0.95 * n.wanted[g];
                let low_stock = ed.goods[g].spoil < 1.0 && n.stock[g] < 0.5 * ed.prices.days_of_cover * n.use_rate[g];
                wanted_short || low_stock
            })
            .collect();
        for b in n.buildings.iter_mut() {
            if b.losing_days >= 90 && !b.player_built && !short[ed.buildings[b.kind].output] {
                b.levels *= 0.9;
                b.losing_days = 60;
            }
        }
        n.buildings.retain(|b| b.levels > 0.01 || b.player_built);
        n.hire_order = (0..n.buildings.len()).collect();
    }

    /// Roads and rail where goods are stuck, ports where trade is full.
    fn build_infrastructure(&mut self, ed: &EconData, i: usize) {
        let n = &self.nations[i];
        if n.deficit_ema > 0.06 {
            return;
        }
        let e = n.exchange;
        let mut spend = 0.0;
        let mut choked: Vec<usize> = n.provinces.iter().copied().filter(|&q| self.provinces[q].transport < 0.97).collect();
        choked.sort_by(|&a, &b| self.provinces[a].transport.total_cmp(&self.provinces[b].transport).then(a.cmp(&b)));
        for q in choked.into_iter().take(3) {
            let p = &mut self.provinces[q];
            if p.road < 6.0 {
                p.road += 0.5;
                spend += ed.state.road_cost * p.hexes * 0.5 / e;
            } else if p.rail < super::init::MAX_INFRA {
                p.rail += 0.5;
                spend += ed.state.rail_cost * p.hexes * 0.5 / e;
            }
        }
        if !n.landlocked && n.trade_used > 0.85 * n.trade_capacity {
            let port = n
                .provinces
                .iter()
                .copied()
                .filter(|&q| self.provinces[q].coastal && self.provinces[q].port < 10.0)
                .min_by(|&a, &b| self.provinces[a].port.total_cmp(&self.provinces[b].port));
            if let Some(q) = port {
                self.provinces[q].port += 0.5;
                spend += ed.state.port_cost * 0.5 / e;
            }
        }
        let n = &mut self.nations[i];
        n.treasury -= spend;
        cover_treasury(n);
        n.budget_month.infrastructure += spend;
        if spend > 0.0 {
            n.projects.push((spend / 60.0, 60));
        }
    }
}

/// The state never holds less than nothing: a gap in the treasury is borrowed
/// from savers at once (the daily budget then settles it as usual).
fn cover_treasury(n: &mut super::NationEcon) {
    if n.treasury < 0.0 {
        let gap = -n.treasury;
        n.debt += gap;
        n.savings -= gap;
        n.treasury = 0.0;
    }
}
