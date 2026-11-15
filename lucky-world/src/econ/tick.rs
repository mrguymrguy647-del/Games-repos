//! One day (docs/ECONOMY.md, section 12).

use super::data::{interpolate, EconData};
use super::{tech, Economy, NationEcon};

const EPS: f64 = 1e-9;

impl Economy {
    /// The whole world, one day on.
    pub fn tick(&mut self, ed: &EconData) {
        self.world_events(ed);
        for i in 0..self.nations.len() {
            self.nation_events(ed, i);
            self.domestic_day(ed, i);
        }
        self.world_market(ed);
        let mut tribute: Vec<(usize, f64)> = Vec::new();
        for i in 0..self.nations.len() {
            if let Some((to, w)) = self.accounts_day(ed, i) {
                tribute.push((to, w));
            }
        }
        for (to, w) in tribute {
            let n = &mut self.nations[to];
            let local = w / n.exchange;
            n.treasury += local;
            n.budget_today.tribute_in += local;
            n.budget_month.tribute_in += local;
        }
        self.world_averages();
        self.day += 1;
        if self.day.is_multiple_of(7) {
            self.record_history(ed);
        }
        if self.day.is_multiple_of(30) {
            for i in 0..self.nations.len() {
                self.monthly(ed, i);
            }
        }
    }

    /// Production, households, the state's shopping and prices.
    fn domestic_day(&mut self, ed: &EconData, i: usize) {
        let goods = ed.goods.len();
        let wpl = ed.workers_per_level;
        let n = &mut self.nations[i];
        n.ledger.clear();
        n.wanted.iter_mut().for_each(|x| *x = 0.0);
        n.got.iter_mut().for_each(|x| *x = 0.0);
        n.budget_today = super::Budget::default();
        let md = &ed.models[n.model];
        let t = tech(ed, n);
        let bm = n.black_market;

        // ---- Labour ----
        let labour = n.population * ed.participation;
        n.army = (n.population * n.duty).min(labour * 0.5);
        n.government = labour * md.gov_share;
        let mut remaining = (labour - n.army - n.government).max(0.0);
        let (mut farming, mut mining, mut industry) = (0.0, 0.0, 0.0);
        for &bi in &n.hire_order {
            let b = &mut n.buildings[bi];
            if !b.open || b.levels <= 0.0 {
                b.hired = 0.0;
                continue;
            }
            let h = (b.levels * b.utilization * wpl).min(remaining);
            b.hired = h;
            remaining -= h;
            match ed.buildings[b.kind].sector {
                super::data::Sector::Farming => farming += h,
                super::data::Sector::Mining => mining += h,
                super::data::Sector::Industry => industry += h,
            }
        }
        n.farming = farming;
        n.mining = mining;
        n.industry = industry;
        let services_target = n.services_spending / (n.wage * (1.0 + ed.services_markup)).max(EPS);
        n.services += 0.02 * (services_target.min(remaining) - n.services);
        n.services = n.services.clamp(0.0, remaining);
        n.unemployed = (remaining - n.services).max(0.0);

        // ---- Transport, from yesterday's tonnage ----
        let transport_mod = n.mods.get("transport");
        for &q in &n.provinces {
            let p = &mut self.provinces[q];
            let cap = p.hexes * (ed.transport_base + ed.transport_road * p.road + ed.transport_rail * p.rail) * transport_mod;
            // Smoothed, so a choked day and a free day don't take turns.
            let want = if p.tonnage > cap { cap / p.tonnage } else { 1.0 };
            p.transport += 0.2 * (want - p.transport);
            p.tonnage *= 0.5;
        }

        // ---- Production, tier by tier ----
        let out_mod: Vec<f64> = (0..goods).map(|g| super::output_mod(ed, n, g)).collect();
        let k = n.population / 1000.0;
        let lv = super::living(ed, n);
        let home_power = k * ed.basket.power * lv;
        let planned = md.planned;
        // What each building could make today with all its inputs.
        let mut potential = vec![0.0; n.buildings.len()];
        let mut food_reserve = vec![0.0; goods];
        let food_tier = ed.buildings.iter().find(|bd| bd.output == ed.food).map_or(4, |bd| bd.tier);
        for (bi, b) in n.buildings.iter().enumerate() {
            let bd = &ed.buildings[b.kind];
            if b.levels <= 0.0 || b.hired <= 0.0 {
                continue;
            }
            let staffing = (b.hired / (b.levels * b.utilization * wpl).max(EPS)).min(1.0) * b.utilization;
            let skill = super::skill_factor(n, bd);
            let rich = if bd.deposit { self.provinces[b.province].deposits[bd.output] } else { 1.0 };
            potential[bi] = b.levels * bd.amount * t * staffing * skill * rich * out_mod[bd.output] * self.provinces[b.province].transport;
            if bd.output == ed.food {
                for &(g, per) in &bd.inputs {
                    food_reserve[g] += potential[bi] * per;
                }
            }
        }
        // What can't be stored (power) is made to order: no more than is used.
        for g in 0..goods {
            if ed.goods[g].spoil < 1.0 {
                continue;
            }
            let made: f64 = n.buildings.iter().zip(&potential).filter(|(b, _)| ed.buildings[b.kind].output == g).map(|(_, q)| q).sum();
            let cap = 1.15 * n.use_rate[g] + 0.01 * k;
            if made > cap {
                let f = cap / made;
                for (bi, b) in n.buildings.iter().enumerate() {
                    if ed.buildings[b.kind].output == g {
                        potential[bi] *= f;
                    }
                }
            }
        }
        let mut request = vec![0.0; goods];
        for tier in 1..=4u8 {
            request.iter_mut().for_each(|x| *x = 0.0);
            let mut members: Vec<usize> = Vec::new();
            for (bi, b) in n.buildings.iter().enumerate() {
                let bd = &ed.buildings[b.kind];
                if bd.tier != tier {
                    continue;
                }
                members.push(bi);
                for &(g, per) in &bd.inputs {
                    request[g] += potential[bi] * per;
                }
            }
            if members.is_empty() {
                continue;
            }
            // What can be used: households' power is kept back, and so is
            // what the food plants of a later tier will need.
            let mut avail: Vec<f64> = n.stock.clone();
            let keep_back = home_power.min(1.1 * n.home_power_bought + 0.05 * home_power);
            avail[ed.electricity] = (avail[ed.electricity] - keep_back).max(0.0);
            if tier < food_tier {
                for g in 0..goods {
                    avail[g] = (avail[g] - food_reserve[g]).max(0.0);
                }
            }
            if planned {
                // In plan order: the first in line fill up.
                members.sort_by_key(|&bi| {
                    let out = ed.buildings[n.buildings[bi].kind].output;
                    ed.plan_priority.iter().position(|&g| g == out).unwrap_or(99)
                });
            }
            // Food is made first: people must eat. Everyone else shares what is left.
            let mut food_request = vec![0.0; goods];
            for &bi in &members {
                let bd = &ed.buildings[n.buildings[bi].kind];
                if bd.output == ed.food {
                    for &(g, per) in &bd.inputs {
                        food_request[g] += potential[bi] * per;
                    }
                }
            }
            let food_ratio: Vec<f64> = (0..goods).map(|g| if food_request[g] > EPS { (avail[g] / food_request[g]).min(1.0) } else { 1.0 }).collect();
            let ratio: Vec<f64> = (0..goods)
                .map(|g| {
                    let left = (avail[g] - food_request[g] * food_ratio[g]).max(0.0);
                    let rest = request[g] - food_request[g];
                    if rest > EPS { (left / rest).min(1.0) } else { 1.0 }
                })
                .collect();
            for &bi in &members {
                let bd = &ed.buildings[n.buildings[bi].kind];
                let q = potential[bi];
                let mut phi: f64 = 1.0;
                for &(g, per) in &bd.inputs {
                    let share = if bd.output == ed.food {
                        food_ratio[g]
                    } else if planned {
                        if q * per > EPS { (avail[g] / (q * per)).min(1.0) } else { 1.0 }
                    } else {
                        ratio[g]
                    };
                    phi = phi.min(share);
                }
                let phi = phi.max(0.0);
                let gross = q * phi;
                // What it would have used of each input if that input alone were plentiful.
                for &(g, per) in &bd.inputs {
                    let mut others: f64 = 1.0;
                    for &(h, _) in &bd.inputs {
                        if h != g {
                            others = others.min(if planned { 1.0 } else if bd.output == ed.food { food_ratio[h] } else { ratio[h] });
                        }
                    }
                    n.wanted[g] += q * per * others;
                }
                for &(g, per) in &bd.inputs {
                    let used = (gross * per).min(n.stock[g]);
                    n.stock[g] -= used;
                    if planned {
                        avail[g] -= used;
                    }
                    n.ledger.used[g] += used;
                    n.got[g] += used;
                }
                let defect = super::defect_rate(n, bd);
                let out = gross * (1.0 - defect);
                n.stock[bd.output] += out;
                n.ledger.produced[bd.output] += out;
                let b = &mut n.buildings[bi];
                b.output = out;
                b.input_factor = if q > EPS { phi } else { 1.0 };
                self.provinces[b.province].tonnage += 0.5 * out * ed.goods[bd.output].weight;
            }
        }

        // ---- Households ----
        let mut goods_wages = 0.0;
        for b in &n.buildings {
            goods_wages += b.hired * n.wage * (1.0 + ed.buildings[b.kind].skill * ed.skill_pay);
        }
        let army_pay = n.army * n.wage * ed.state.army_pay * n.mods.get("army_upkeep");
        let gov_pay = n.government * n.wage;
        let services_pay = n.services * n.wage;
        let wage_bill = goods_wages + army_pay + gov_pay + services_pay;
        let welfare = n.unemployed * n.wage * n.welfare_rate + ed.pension_share * wage_bill;
        let dividends = ed.payout * n.profits.max(0.0);
        let income_tax = n.income_tax * wage_bill * (1.0 - bm);
        let transfers = std::mem::take(&mut n.transfers);
        let disposable = (wage_bill + welfare + dividends + transfers - income_tax).max(0.0);
        let r = n.rate;
        let save = (ed.save_base + ed.save_rate_slope * (r - 0.04)).clamp(0.02, 0.4);
        let mut budget = (1.0 - save) * disposable * n.mods.get("demand") + 0.0005 * n.savings.max(0.0);
        n.savings += disposable - budget;
        n.household_spending = budget;
        let b = &ed.basket;
        // Food first (up to a share of the budget), then power and fuel, then
        // consumer goods up to their share; services get the rest.
        let needs = [
            (ed.food, k * b.food, b.food_share),
            (ed.electricity, home_power, b.essentials_share),
            (ed.fuel, k * b.fuel * lv, b.essentials_share),
            (ed.consumer_goods, k * b.goods * lv.powf(b.goods_exp), b.goods_share),
            (ed.consumer_goods, k * b.luxury * lv.powf(b.luxury_exp), b.goods_share),
        ];
        let mut sats = [1.0; 5];
        let mut sales = 0.0;
        let mut short: f64 = 0.0;
        let after_food_budget = |budget: f64| budget;
        let _ = after_food_budget;
        let mut pool = budget;
        let mut caps = [0.0; 5];
        for (j, &(g, need, share)) in needs.iter().enumerate() {
            if j == 1 {
                // Power and fuel share one cap; goods (basic and luxury) another.
                pool = budget;
                caps[1] = pool * share;
                caps[2] = caps[1];
            } else if j == 3 {
                pool = budget;
                caps[3] = pool * share;
                caps[4] = caps[3];
            } else if j == 0 {
                caps[0] = budget * share;
            }
            let _ = pool;
            let p_buy = n.price[g] * (1.0 + n.sales_tax);
            let cap = caps[j].min(budget);
            let demand = need.min(cap / p_buy.max(EPS)).max(0.0);
            let q = demand.min(n.stock[g]);
            n.stock[g] -= q;
            n.ledger.consumed[g] += q;
            n.wanted[g] += demand;
            n.got[g] += q;
            let spend = q * p_buy;
            budget -= spend;
            sales += spend;
            match j {
                1 => caps[2] -= spend,
                2 => {}
                3 => caps[4] -= spend,
                _ => {}
            }
            sats[j] = if need > EPS { q / need } else { 1.0 };
            if j < 4 {
                short = short.max(1.0 - sats[j]);
            }
        }
        n.home_power_bought = sats[1] * home_power;
        n.food_sat = sats[0];
        n.power_sat = sats[1];
        n.goods_sat = sats[3];
        n.lux_sat = sats[4];
        // What's left buys services (with shortages, people save some
        // instead: up to 10% of it when the shelves are bare).
        let services = budget * (1.0 - 0.1 * short.clamp(0.0, 1.0));
        n.savings += budget - services;
        // Building work and upkeep are spent at home too: on construction and services.
        let mut projects = 0.0;
        n.projects.retain_mut(|p| {
            projects += p.0;
            p.1 = p.1.saturating_sub(1);
            p.1 > 0
        });
        n.services_spending = services + std::mem::take(&mut n.injection) + projects;
        let sales_tax = sales * n.sales_tax / (1.0 + n.sales_tax) * (1.0 - bm);
        n.budget_today.income_tax += income_tax;
        n.budget_today.sales_tax += sales_tax;
        n.budget_today.welfare += welfare;
        n.budget_today.government += gov_pay;
        n.budget_today.army += army_pay;

        // ---- The state's shopping: training, then new weapons ----
        let mut army_goods = 0.0;
        for (g, per) in [(ed.fuel, 0.05), (ed.ammo, 0.002)] {
            let want = n.army / 1000.0 * per;
            let q = want.min(n.stock[g]);
            n.stock[g] -= q;
            n.ledger.consumed[g] += q;
            n.wanted[g] += want;
            n.got[g] += q;
            army_goods += q * n.price[g];
        }
        let upkeep = (n.stockpile[ed.tanks] * ed.state.upkeep_tank + n.stockpile[ed.jets] * ed.state.upkeep_jet) / n.exchange * n.mods.get("army_upkeep");
        n.budget_today.army += army_goods + upkeep;
        let share = ed.state.procurement.get(&n.army_option).copied().unwrap_or(0.0) * n.mods.get("procurement");
        let daily = share * n.gdp_local;
        let jets_ok = n.hdi >= 0.6;
        let split = if jets_ok { [(ed.ammo, 0.5), (ed.tanks, 0.3), (ed.jets, 0.2)] } else { [(ed.ammo, 0.5), (ed.tanks, 0.5), (ed.jets, 0.0)] };
        for (g, part) in split {
            let want = daily * part / n.price[g].max(EPS);
            let q = want.min(n.stock[g]);
            n.stock[g] -= q;
            n.stockpile[g] += q;
            n.ledger.stockpiled[g] += q;
            n.wanted[g] += want;
            n.got[g] += q;
            n.budget_today.procurement += q * n.price[g];
        }

        // ---- Spoilage (and unused power is gone) ----
        for g in 0..goods {
            let lost = n.stock[g] * ed.goods[g].spoil;
            n.stock[g] -= lost;
            n.ledger.spoiled[g] += lost;
        }

        // ---- Prices ----
        let pr = &ed.prices;
        let speed = if planned { pr.planned_price_speed } else { pr.price_speed };
        for g in 0..goods {
            // What it wanted to use, not just what it got: a starved nation still knows its need.
            let today_use = (n.ledger.used[g] + n.ledger.consumed[g] + n.ledger.stockpiled[g]).max(n.wanted[g]);
            n.use_rate[g] = 0.97 * n.use_rate[g] + 0.03 * today_use;
            let shortage = if n.wanted[g] > EPS { ((n.wanted[g] - n.got[g]) / n.wanted[g]).clamp(0.0, 1.0) } else { 0.0 };
            let x = if g == ed.electricity {
                let made = n.ledger.produced[g];
                let unused = if made > EPS { n.ledger.spoiled[g] / made } else { 0.0 };
                shortage - 0.5 * unused
            } else {
                let target = pr.days_of_cover * n.use_rate[g];
                let gap = (target - n.stock[g]) / target.max(n.stock[g]).max(EPS);
                gap + 0.5 * shortage
            };
            let mut dl = (speed * x).clamp(-pr.price_cap, pr.price_cap);
            if ed.goods[g].trade && n.trade_capacity > 0.0 {
                let (ip, ep) = parity(ed, n, g, self.world_price[g]);
                if n.price[g] > ip {
                    dl -= 0.08 * (n.price[g] / ip).ln();
                } else if n.price[g] < ep {
                    dl += 0.08 * (ep / n.price[g]).ln();
                }
                // And never far outside the band: traders would step in.
                n.price[g] = (n.price[g] * dl.exp()).clamp(0.6 * ep, 1.6 * ip);
                dl = 0.0;
            }
            n.price[g] *= dl.exp();
            // A guard rail for every good (what can't be traded, or a nation
            // cut off from trade): no more than a few times its W$ worth, or
            // much less.
            let worth = ed.goods[g].price / n.exchange.max(1e-12);
            n.price[g] = n.price[g].clamp(0.25 * worth, 4.0 * worth);
            if planned {
                n.price[g] = n.price[g].min(n.plan_price[g] * pr.planned_ceiling);
            }
            n.price[g] = n.price[g].max(1e-9);
        }
        n.black_market = (0.3 * (1.0 - n.goods_sat.min(n.food_sat)) + 0.5 * (n.tariff - 0.2).max(0.0) + 0.3 * n.corruption + n.mods.get("black_market"))
            .clamp(0.0, 0.6);
    }

    /// Firms, wages, the budget, money, the central bank, people and GDP.
    /// Gives back a colony's tribute: (overlord, W$).
    fn accounts_day(&mut self, ed: &EconData, i: usize) -> Option<(usize, f64)> {
        let goods = ed.goods.len();
        let world_inflation = self.world_inflation;
        let world_rate = self.world_rate;
        let world_prices = self.world_price.clone();
        let n = &mut self.nations[i];
        let md = &ed.models[n.model];
        let dpy = ed.days_per_year as f64;
        let bm = n.black_market;

        // ---- Firms ----
        let mut value_added = 0.0;
        let mut profit = n.trade_margin;
        let mut upkeep_paid = 0.0;
        let mut wage_units = 0.0;
        let defects: Vec<f64> = ed.buildings.iter().map(|bd| super::defect_rate(n, bd)).collect();
        for b in n.buildings.iter_mut() {
            let bd = &ed.buildings[b.kind];
            let gross_out = if b.output > 0.0 { b.output } else { 0.0 };
            let revenue = gross_out * n.price[bd.output];
            // Inputs used: from the output and the defect rate is close enough; use the recipe on gross.
            let defect = defects[b.kind];
            let gross = gross_out / (1.0 - defect).max(0.1);
            let inputs: f64 = bd.inputs.iter().map(|&(g, per)| gross * per * n.price[g]).sum();
            let wages = b.hired * n.wage * (1.0 + bd.skill * ed.skill_pay);
            wage_units += b.hired * (1.0 + bd.skill * ed.skill_pay);
            let upkeep = b.levels * bd.upkeep / n.exchange;
            upkeep_paid += upkeep;
            let p = revenue - inputs - wages - upkeep;
            value_added += revenue - inputs;
            profit += p;
            let per_level = if b.levels > 0.0 { p / b.levels } else { 0.0 };
            b.profit = 0.97 * b.profit + 0.03 * per_level;
            b.losing_days = if per_level < 0.0 { b.losing_days + 1 } else { 0 };
            // Losing money: run less of it; earning: run more (the plan runs everything).
            if !md.planned {
                // Losing money: run less of it (unless the nation is short of
                // it); earning: run more (unless it's piling up). Gently, so
                // farms and mines don't swing from glut to shortage and back.
                let target = ed.prices.days_of_cover * n.use_rate[bd.output];
                let stock = n.stock[bd.output];
                let short = ed.goods[bd.output].spoil < 1.0 && stock < target;
                let glut = ed.goods[bd.output].spoil < 1.0 && stock > 2.0 * target;
                let step = if b.profit < 0.0 && !short {
                    -0.005
                } else if b.profit >= 0.0 && !glut {
                    0.005
                } else {
                    0.0
                };
                b.utilization = (b.utilization + step).clamp(0.2, 1.0);
            } else {
                // The plan goes by stocks, not profit: quotas are cut for
                // what piles up and raised for what runs short.
                let target = ed.prices.days_of_cover * n.use_rate[bd.output];
                let stock = n.stock[bd.output];
                let step = if ed.goods[bd.output].spoil >= 1.0 {
                    0.0
                } else if stock > 2.0 * target {
                    -0.01
                } else if stock < target {
                    0.01
                } else {
                    0.0
                };
                b.utilization = (b.utilization + step).clamp(0.2, 1.0);
            }
        }
        let services_profit = n.services_spending - n.services * n.wage;
        profit += services_profit;
        let corporate = n.corporate_tax * profit.max(0.0) * (1.0 - bm);
        n.budget_today.corporate_tax += corporate;
        n.profits = profit - corporate;
        // What firms don't pay out, they save (and losses come out of savings).
        if profit < 0.0 {
            n.savings += profit;
        } else {
            n.savings += (1.0 - ed.payout) * n.profits;
        }
        n.injection += upkeep_paid;

        // ---- Wage ----
        let labour = n.population * ed.participation - n.army;
        let u = if labour > 0.0 { n.unemployed / labour } else { 0.0 };
        let u_nat = interpolate(&ed.natural_unemployment_by_hdi, n.hdi);
        // Wages are struck in world money: they follow what the factories can
        // pay (`labour_share` of what each worker adds, in W$), with a premium
        // when labour is short and a cut when it is slack, and move slowly.
        // At home they are that, in the local currency (set after the
        // exchange rate below), so a wage push can't feed on itself.
        let tight = ((u_nat - u) / u_nat.max(0.01)).clamp(-1.0, 1.0);
        let base_w = n.wage0 * n.exchange_start * (tech(ed, n) / n.tech_start.max(EPS));
        if wage_units > 0.02 * labour.max(1.0) && value_added > 0.0 {
            let can_pay = ed.labour_share * value_added * n.exchange / wage_units;
            n.wage_anchor += 0.02 * (can_pay - n.wage_anchor);
        } else {
            n.wage_anchor += 0.02 * (base_w - n.wage_anchor);
        }
        // Mostly day 1's wage grown with tech; a little of what the firms
        // still running can pay (only a little: survivors are the best, and
        // following them alone would price the rest out).
        let mix = (0.75 * base_w.max(EPS).ln() + 0.25 * n.wage_anchor.max(EPS).ln()).exp();
        let target_w = mix * (1.0 + ed.wage_tightness * tight);
        n.wage_w *= (0.003 * (target_w / n.wage_w.max(EPS)).ln()).exp();
        n.wage_w = n.wage_w.clamp(0.25 * base_w, 4.0 * base_w);

        // ---- The budget ----
        let gross_revenue = n.budget_today.income_tax + n.budget_today.corporate_tax + n.budget_today.sales_tax + n.budget_today.tariffs;
        n.budget_today.corruption = n.corruption * gross_revenue;
        let mut infra = 0.0;
        for &q in &n.provinces {
            let p = &self.provinces[q];
            infra += (p.road + p.rail + p.port) * p.hexes * ed.state.infrastructure_upkeep;
        }
        n.budget_today.infrastructure += infra / n.exchange;
        n.injection += infra / n.exchange;
        n.budget_today.interest += n.debt * n.bond_rate / dpy;
        n.transfers += n.debt * n.bond_rate / dpy + 0.5 * n.budget_today.corruption;
        let mut tribute = None;
        if let Some(o) = n.overlord {
            let out = ed.state.tribute * gross_revenue;
            n.budget_today.tribute_out += out;
            tribute = Some((o, out * n.exchange));
        }
        let balance = n.budget_today.revenue() - n.budget_today.spending();
        n.treasury += balance;
        let gdp_year = (n.gdp_local * dpy).max(EPS);
        n.deficit_ema = 0.99 * n.deficit_ema + 0.01 * (-balance * dpy / gdp_year);
        n.printed = 0.0;
        if n.treasury < 0.0 {
            let short = -n.treasury;
            let ceiling = ed.ratings[n.rating].ceiling * gdp_year;
            let can_borrow = (ceiling - n.debt).max(0.0).min(short);
            n.debt += can_borrow;
            n.savings -= can_borrow;
            let print = short - can_borrow;
            n.printed = print;
            n.printed_month += print;
            n.money += print;
            n.treasury = 0.0;
        } else if n.treasury > 0.04 * gdp_year && n.debt > 0.0 {
            let repay = (n.treasury - 0.02 * gdp_year).min(n.debt);
            n.debt -= repay;
            n.treasury -= repay;
            n.savings += repay;
        }
        let today = n.budget_today.clone();
        n.budget_month.add(&today);

        // ---- Money and prices ----
        let m = &ed.money;
        let lend = (m.credit_growth_base + m.credit_slope * (m.r_neutral - n.rate)) / dpy;
        let growth_m = lend + n.printed / n.money.max(EPS);
        n.money *= 1.0 + lend;
        n.money_growth += (growth_m - n.money_growth) / 90.0;
        // Money growing faster than real output lifts every price (smoothed, and capped).
        let drift = (m.pass_through * (n.money_growth - m.credit_growth_base / dpy - n.real_growth)).clamp(-0.0004, 0.004);
        // (Traded goods follow through the currency instead: see below.)
        if drift != 0.0 {
            let f = drift.exp();
            for g in 0..goods {
                if !ed.goods[g].trade {
                    n.price[g] *= f;
                }
            }
            n.plan_price.iter_mut().for_each(|p| *p *= f);
        }
        if md.planned {
            // The plan sets prices at their W$ worth, in the local currency.
            for g in 0..goods {
                n.plan_price[g] = ed.goods[g].price / n.exchange.max(1e-12);
            }
        }
        let pr = &ed.prices;
        let rel = |g: usize, n: &NationEcon| n.price[g] * n.exchange0() / ed.goods[g].price;
        let cpi = pr.cpi_food * rel(ed.food, n)
            + pr.cpi_goods * rel(ed.consumer_goods, n)
            + pr.cpi_fuel * rel(ed.fuel, n)
            + pr.cpi_power * rel(ed.electricity, n)
            + pr.cpi_services * n.wage / n.wage0.max(EPS);
        n.cpi = cpi.max(1e-6);
        let ring = n.cpi_ring.len();
        let old = n.cpi_ring[(self.day as usize) % ring];
        n.cpi_ring[(self.day as usize) % ring] = n.cpi;
        let annual = ((n.cpi / old.max(1e-9)).ln() * dpy / ring as f64).clamp(-0.9, 20.0);
        n.inflation = 0.9 * n.inflation + 0.1 * annual;
        // Goods alone: wages are indexed to these, not to themselves.
        let goods_w = pr.cpi_food + pr.cpi_goods + pr.cpi_fuel + pr.cpi_power;
        let goods_cpi = (pr.cpi_food * rel(ed.food, n) + pr.cpi_goods * rel(ed.consumer_goods, n) + pr.cpi_fuel * rel(ed.fuel, n) + pr.cpi_power * rel(ed.electricity, n))
            / goods_w.max(EPS);
        n.goods_cpi = goods_cpi.max(1e-6);
        let old = n.goods_ring[(self.day as usize) % ring];
        n.goods_ring[(self.day as usize) % ring] = n.goods_cpi;
        let annual = ((n.goods_cpi / old.max(1e-9)).ln() * dpy / ring as f64).clamp(-0.9, 20.0);
        n.goods_inflation = 0.9 * n.goods_inflation + 0.1 * annual;
        // Expectations stay near the central bank's target while inflation
        // does; the further it strays, the more people go by what they see.
        let seen = n.goods_inflation.clamp(-0.2, 2.0);
        let stray = ((seen - m.pi_target).abs() / 0.5).clamp(0.0, 1.0);
        let weight = 0.3 + 0.6 * stray;
        let anchored = (1.0 - weight) * m.pi_target + weight * seen;
        n.expected_inflation += 0.01 * (anchored - n.expected_inflation);

        // ---- Central bank ----
        let gap = if n.real_trend > EPS { n.real_gdp / n.real_trend - 1.0 } else { 0.0 };
        let target = n.manual_rate.unwrap_or(m.r_neutral + m.taylor_pi * (n.inflation - m.pi_target) + m.taylor_gap * gap).clamp(0.0, m.rate_max);
        let step = if n.manual_rate.is_some() { 1.0 } else { m.rate_step };
        n.rate += (target - n.rate).clamp(-step, step);
        n.bond_rate = n.rate + ed.ratings[n.rating].premium + n.mods.get("rating").min(0.0) * -0.0005;

        // ---- Exchange rate ----
        let gdp_w = (n.gdp_local * n.exchange * dpy).max(EPS);
        let t = &ed.trade;
        // The currency is worth what its money buys: its start value, down
        // as money grows faster than the nation's capacity to produce (its
        // working people times their tech), up with world prices, and tilted
        // (within bounds) by the trade balance and interest rates. Printing
        // or cheap credit weakens it; a high rate slows lending and so
        // strengthens it. Traded goods' home prices follow it, inside their
        // trade band; wages are struck in W$ (above), so a wage push can't
        // feed on itself.
        let world_index = {
            let (mut num, mut den) = (0.0, 0.0);
            for (g, w) in [(ed.food, pr.cpi_food), (ed.consumer_goods, pr.cpi_goods), (ed.fuel, pr.cpi_fuel)] {
                num += w * world_prices[g] / ed.goods[g].price;
                den += w;
            }
            if den > 0.0 { num / den } else { 1.0 }
        };
        let tilt = (t.fx_trade * n.balance_ema * dpy / gdp_w - t.fx_inflation * (n.inflation - world_inflation) + t.fx_rate * (n.rate - world_rate)).clamp(-0.3, 0.3);
        let capacity = (n.population * ed.participation).max(1.0) * tech(ed, n);
        n.real_supply += (capacity - n.real_supply) / dpy;
        let real = (n.real_supply / n.real_start.max(EPS)).max(1e-6);
        let money = (n.money / n.money_start.max(EPS)).max(1e-12);
        let target = n.exchange_start * world_index * real / money * tilt.exp();
        n.exchange *= (t.fx_ppp * (target / n.exchange).ln()).clamp(-0.01, 0.01).exp();
        n.wage = n.wage_w / n.exchange.max(1e-12);

        // ---- Technical progress: a little more from each worker each year ----
        n.tech_progress *= (ed.tech_growth / dpy).exp();

        // ---- People ----
        let food = n.food_sat.min(1.0);
        let happy = 100.0
            * (0.35 * food + 0.20 * n.goods_sat.min(1.0) + 0.10 * n.lux_sat.min(1.0) + 0.20 * (1.0 - (u / 0.25).min(1.0))
                + 0.15 * (1.0 - (n.inflation.max(0.0) / 0.30).min(1.0)));
        n.happiness += 0.03 * (happy - n.happiness);
        let target_stab = 0.6 * n.happiness + 0.4 * md.base_stability + n.mods.get("stability");
        n.stability = (n.stability + 0.01 * (target_stab - n.stability)).clamp(0.0, 100.0);
        let birth = interpolate(&ed.birth_by_hdi, n.hdi);
        let famine = 0.2 * ((0.85 - food).max(0.0) / 0.85);
        let growth = birth - famine - ed.birth_unemployment * (u - 0.2).max(0.0) + n.mods.get("growth");
        n.population *= (growth / dpy).exp();
        n.population = n.population.max(1000.0);
        // HDI drifts towards what its income per person would give.
        // (From day 1's level, and gently: richer means more tech, and more
        // tech means richer, so a strong pull would feed on itself.)
        let per_person = gdp_w / n.population;
        let start_per_person = (n.gdp_start / n.population_start.max(1.0)).max(1.0);
        let hdi_target = (n.hdi_start + ed.hdi_income_slope * (per_person.max(1.0) / start_per_person).ln()).clamp(0.25, 0.97);
        n.hdi += (hdi_target - n.hdi) * 0.02 / dpy;
        n.skilled = (interpolate(&ed.skill_by_hdi, n.hdi) + n.mods.get("skill")).clamp(0.01, 0.9);

        // ---- GDP ----
        let gov_wages = n.budget_today.government + n.army * n.wage * ed.state.army_pay;
        n.gdp_local = (value_added + n.services_spending + gov_wages + n.trade_margin_today()).max(EPS);
        let real = n.gdp_local / n.cpi;
        let last = n.real_gdp.max(EPS);
        n.real_gdp = real;
        n.real_growth += ((real / last).ln().clamp(-0.01, 0.01) - n.real_growth) / dpy;
        n.real_trend += (real - n.real_trend) / dpy;
        n.gdp = 0.97 * n.gdp + 0.03 * n.gdp_local * n.exchange * dpy;
        tribute
    }

    fn world_averages(&mut self) {
        let total: f64 = self.nations.iter().map(|n| n.gdp).sum::<f64>().max(EPS);
        self.world_inflation = self.nations.iter().map(|n| n.inflation * n.gdp).sum::<f64>() / total;
        self.world_rate = self.nations.iter().map(|n| n.rate * n.gdp).sum::<f64>() / total;
    }

    fn record_history(&mut self, ed: &EconData) {
        let day = self.day;
        for n in self.nations.iter_mut() {
            let h = &mut n.history;
            h.day.push(day);
            h.gdp.push(n.gdp as f32);
            h.inflation.push(n.inflation as f32);
            let labour = (n.population * ed.participation - n.army).max(1.0);
            h.unemployment.push((n.unemployed / labour) as f32);
            h.stability.push(n.stability as f32);
            let gdp_year = (n.gdp_local * ed.days_per_year as f64).max(EPS);
            h.debt.push((n.debt / gdp_year) as f32);
            h.rate.push(n.rate as f32);
            h.trade_balance.push(n.balance_ema as f32);
            for (g, p) in n.price.iter().enumerate() {
                h.prices[g].push(*p as f32);
            }
        }
        for (g, w) in self.world_price.iter().enumerate() {
            self.world_history[g].push(*w as f32);
        }
    }
}

impl NationEcon {
    /// The exchange rate prices were set against on day 1, so the CPI starts at 1.
    pub fn exchange0(&self) -> f64 {
        self.exchange_start
    }

    fn trade_margin_today(&self) -> f64 {
        self.trade_margin
    }


    /// GDP in W$ per person per year.
    pub fn gdp_per_person(&self) -> f64 {
        self.gdp / self.population.max(1.0)
    }

    pub fn unemployment(&self, ed: &EconData) -> f64 {
        let labour = (self.population * ed.participation - self.army).max(1.0);
        self.unemployed / labour
    }

    pub fn debt_ratio(&self, ed: &EconData) -> f64 {
        self.debt / (self.gdp_local * ed.days_per_year as f64).max(EPS)
    }
}

/// A nation's import and export parity for a good (local money).
pub fn parity(ed: &EconData, n: &NationEcon, g: usize, world: f64) -> (f64, f64) {
    let c = trade_cost(ed, n, g, world);
    (world * (1.0 + c + n.tariff) / n.exchange, world * (1.0 - c).max(0.05) / n.exchange)
}

/// The transport cost of trading a good, as a share of its value.
pub fn trade_cost(ed: &EconData, n: &NationEcon, g: usize, world: f64) -> f64 {
    let t = &ed.trade;
    let w = ed.goods[g].weight;
    let base = if n.landlocked {
        t.land_cost_share + t.transit_fee + t.land_cost_weight * w / world.max(EPS)
    } else {
        t.sea_cost_share + t.sea_cost_weight * w / world.max(EPS)
    };
    base * n.mod_trade_cost
}
