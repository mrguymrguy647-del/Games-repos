//! Day 1: every nation's economy from its wheels, its provinces and its
//! place on the map (docs/ECONOMY.md, sections 3, 9 and 11).

use duckforge::prelude::Rng;

use super::data::{interpolate, EconData, Effects, Tier};
use super::{defect_rate, living, output_mod, skill_factor, tech, Budget, Building, Economy, History, Ledger, NationEcon, ProvinceEcon, TradePolicy};
use crate::data::Data;
use crate::mapdata::Atlas;
use crate::world::{Coast, World};

/// The most road or rail a province can hold.
pub(super) const MAX_INFRA: f64 = 12.0;

/// Log-uniform between two bounds.
fn log_range(rng: &mut Rng, lo: f64, hi: f64) -> f64 {
    (lo.ln() + (hi.ln() - lo.ln()) * rng.next_f32() as f64).exp()
}

impl Economy {
    /// The world's economy on day 1. `seed` makes it repeatable.
    pub fn new(data: &Data, ed: &EconData, atlas: &Atlas, world: &World, seed: u32) -> Economy {
        let mut rng = Rng::new(seed.wrapping_mul(2_654_435_761).wrapping_add(0x5eed));
        let map = atlas.map(world.map);
        let goods = ed.goods.len();

        // Provinces: deposits from their terrain (and rivers and lakes), with a spread.
        let mut provinces = Vec::new();
        for prov in &map.provinces {
            let hexes = prov.cells.len() as f64;
            let mut deposits = vec![0.0; goods];
            for &cell in &prov.cells {
                let terrain = atlas.terrain_of(map, cell);
                for (id, rich) in &terrain.deposits {
                    if let Some(g) = ed.good_index(id) {
                        deposits[g] += rich / hexes;
                    }
                }
            }
            if prov.river {
                deposits[ed.grain] += 0.3;
            }
            if prov.lake {
                deposits[ed.grain] += 0.15;
            }
            let [lo, hi] = ed.deposit_spread;
            for (g, d) in deposits.iter_mut().enumerate() {
                if ed.goods[g].tier == Tier::Raw && *d > 0.0 {
                    *d *= lo + (hi - lo) * rng.next_f32() as f64;
                }
            }
            provinces.push(ProvinceEcon {
                deposits,
                hexes,
                coastal: prov.coastal,
                road: 0.0,
                rail: 0.0,
                port: 0.0,
                people: 0.0,
                tonnage: 0.0,
                transport: 1.0,
            });
        }

        let base_world_price: Vec<f64> = ed.goods.iter().map(|g| g.price).collect();
        let mut economy = Economy {
            day: 0,
            nations: Vec::new(),
            provinces,
            world_price: base_world_price.clone(),
            base_world_price,
            world_shocks: Vec::new(),
            news: Vec::new(),
            world_history: vec![Vec::new(); goods],
            rng: Rng::new(seed ^ 0xec0),
            world_inflation: 0.02,
            world_rate: 0.04,
        };
        for i in 0..world.nations.len() {
            let n = economy.make_nation(data, ed, atlas, world, i, &mut rng);
            economy.nations.push(n);
        }
        // What some nations can't dig, others with spare land do.
        economy.balance_raw(ed);
        economy.fit_transport(ed);
        for i in 0..economy.nations.len() {
            let mut n = economy.nations[i].clone();
            let gdp = n.gdp_start;
            economy.calibrate(ed, &mut n, gdp);
            economy.nations[i] = n;
        }
        // Colonies pay tribute to the strongest nation that isn't one.
        let strongest = (0..economy.nations.len())
            .filter(|&i| !ed.models[economy.nations[i].model].id.eq("colony"))
            .max_by(|&a, &b| economy.nations[a].gdp.total_cmp(&economy.nations[b].gdp));
        for i in 0..economy.nations.len() {
            if ed.models[economy.nations[i].model].id == "colony" && strongest != Some(i) {
                economy.nations[i].overlord = strongest;
            }
        }
        let count = economy.nations.len();
        for n in economy.nations.iter_mut() {
            n.embargo = vec![false; count];
            n.exports_to = vec![0.0; count];
            n.imports_from = vec![0.0; count];
            n.exports_to_last = vec![0.0; count];
            n.imports_from_last = vec![0.0; count];
        }
        economy.refresh_trade_capacity(ed);
        for i in 0..count {
            economy.refresh_hire_order(ed, i);
        }
        economy
    }

    fn make_nation(&mut self, data: &Data, ed: &EconData, atlas: &Atlas, world: &World, i: usize, rng: &mut Rng) -> NationEcon {
        let c = &world.nations[i];
        let map = atlas.map(world.map);
        let goods = ed.goods.len();

        // Every wheel result's effects, and the continent's climate.
        let mut effects = Effects::default();
        let mut model_id = "mixed".to_string();
        for (w, wheel) in data.wheels.iter().enumerate() {
            if wheel.from_map {
                continue;
            }
            let option = &wheel.options[c.landed[w].option].id;
            if let Some((model, e)) = ed.wheel_effects.get(&(wheel.id.clone(), option.clone())) {
                effects.stack(e);
                if let Some(m) = model {
                    model_id = m.clone();
                }
            }
        }
        let continent = &data.maps[world.map].continents[c.landed[0].option];
        for tag in &continent.tags {
            if let Some(e) = ed.continent_effects.get(tag) {
                effects.stack(e);
            }
        }
        let model = ed.models.iter().position(|m| m.id == model_id).unwrap_or(0);
        let md = ed.models[model].clone();

        let provinces = world.provinces_of(i);
        let border_provinces: Vec<usize> = provinces
            .iter()
            .copied()
            .filter(|&q| map.provinces[q].neighbours.iter().any(|&o| world.owner[o] != Some(i)))
            .collect();
        let landlocked = Coast::of(data, c) == Coast::Landlocked || !provinces.iter().any(|&p| map.provinces[p].coastal);
        let resource_id = c.option_id(data, "resource");
        let main_resource = ed.good_index(&resource_id).or_else(|| ed.good_index(&format!("{resource_id}_ore")));

        // People: spread over its provinces by how good the land is to live on.
        let mut weight_sum = 0.0;
        for &p in &provinces {
            let pe = &self.provinces[p];
            let live = pe.hexes * (0.15 + pe.deposits[ed.grain] + if pe.coastal { 0.3 } else { 0.0 });
            weight_sum += live;
        }
        for &p in &provinces {
            let pe = &mut self.provinces[p];
            let live = pe.hexes * (0.15 + pe.deposits[ed.grain] + if pe.coastal { 0.3 } else { 0.0 });
            pe.people = live / weight_sum.max(1e-9);
        }

        // Main Resource: double deposits, and one rich deposit for sure.
        if let Some(g) = main_resource {
            for &p in &provinces {
                self.provinces[p].deposits[g] *= ed.main_resource_bonus;
            }
            let best = provinces
                .iter()
                .copied()
                .max_by(|&a, &b| self.provinces[a].deposits[g].total_cmp(&self.provinces[b].deposits[g]))
                .unwrap_or(world.capital[i]);
            let d = &mut self.provinces[best].deposits[g];
            *d = d.max(1.2 * ed.main_resource_bonus * 0.5);
        }

        // Infrastructure from HDI and the wheels.
        let hdi = c.value(data, "hdi");
        let infra = effects.get("infrastructure");
        for &p in &provinces {
            let pe = &mut self.provinces[p];
            pe.road = (2.0 + 6.0 * (hdi - 0.5) + infra).clamp(0.5, 10.0);
            pe.rail = (1.0 + 7.0 * (hdi - 0.6) + infra).clamp(0.0, 10.0);
            pe.port = if pe.coastal { (1.0 + 3.0 * hdi + infra * 0.5).clamp(1.0, 10.0) } else { 0.0 };
        }
        let cap = world.capital[i];
        self.provinces[cap].road = (self.provinces[cap].road + 1.0).min(10.0);
        if self.provinces[cap].coastal {
            self.provinces[cap].port = (self.provinces[cap].port + 1.0).min(10.0);
        }

        let population = c.value(data, "population");
        let gdp_w = c.value(data, "gdp");
        let duty = c.value(data, "duty") / 100.0;
        let army_option = c.option_id(data, "army");
        // The currency: one local unit is worth between W$0.002 and W$2.
        let exchange = log_range(rng, 0.002, 2.0);
        let base = base_name(&c.name);
        let style = &continent.style;
        let names = ed.currencies.get(style).cloned().unwrap_or_else(|| vec!["Crown".to_string()]);
        let currency_word = &names[(rng.next_u32() as usize) % names.len()];
        let symbol: String = base.chars().filter(|ch| ch.is_alphabetic()).take(3).collect::<String>().to_uppercase();

        let mut n = NationEcon {
            name: c.name.clone(),
            currency: format!("{base} {currency_word}"),
            symbol,
            model,
            base_effects: effects.clone(),
            mods: effects.clone(),
            mod_tech: 1.0,
            mod_defect: 1.0,
            mod_out: Vec::new(),
            mod_event: Vec::new(),
            mod_trade_cost: 1.0,
            mods_dirty: true,
            events: Vec::new(),
            overlord: None,
            landlocked,
            provinces: provinces.clone(),
            capital: cap,
            army_option: army_option.clone(),
            main_resource,
            population,
            hdi,
            tech_cal: 1.0,
            tech_progress: 1.0,
            need_cal: 1.0,
            duty,
            skilled: 0.0,
            farming: 0.0,
            mining: 0.0,
            industry: 0.0,
            services: 0.0,
            government: 0.0,
            army: 0.0,
            unemployed: 0.0,
            food_sat: 1.0,
            goods_sat: 1.0,
            lux_sat: 1.0,
            power_sat: 1.0,
            happiness: 60.0,
            stability: (md.base_stability + effects.get("stability")).clamp(5.0, 95.0),
            buildings: Vec::new(),
            construction: Vec::new(),
            hire_order: Vec::new(),
            stock: vec![0.0; goods],
            stockpile: vec![0.0; goods],
            price: ed.goods.iter().map(|g| g.price / exchange).collect(),
            plan_price: Vec::new(),
            wanted: vec![0.0; goods],
            got: vec![0.0; goods],
            use_rate: vec![0.0; goods],
            ledger: Ledger::new(goods),
            trade_policy: vec![TradePolicy::Free; goods],
            raw_short: vec![0.0; goods],
            home_power_bought: 0.0,
            wage: 0.0,
            wage0: 0.0,
            wage_anchor: 0.0,
            wage_w: 0.0,
            tech_start: 1.0,
            hdi_start: 0.5,
            population_start: 1.0,
            money_start: 0.0,
            goods_cpi: 1.0,
            goods_ring: Vec::new(),
            goods_inflation: 0.02,
            real_start: 0.0,
            real_supply: 0.0,
            money: 0.0,
            rate: 0.04,
            manual_rate: None,
            cpi: 1.0,
            cpi_ring: Vec::new(),
            inflation: 0.02,
            expected_inflation: 0.02,
            exchange,
            exchange_start: exchange,
            printed: 0.0,
            savings: 0.0,
            profits: 0.0,
            services_spending: 0.0,
            household_spending: 0.0,
            injection: 0.0,
            transfers: 0.0,
            projects: Vec::new(),
            income_tax: md.income_tax,
            corporate_tax: md.corporate_tax,
            sales_tax: md.sales_tax,
            tariff: md.tariff,
            welfare_rate: ed.welfare_rate,
            manual_taxes: false,
            treasury: 0.0,
            debt: 0.0,
            rating: 0,
            bond_rate: 0.05,
            corruption: 0.0,
            black_market: 0.0,
            embargo: Vec::new(),
            budget_today: Budget::default(),
            budget_month: Budget::default(),
            budget_last_month: Budget::default(),
            deficit_ema: 0.0,
            trade_capacity: 0.0,
            trade_used: 0.0,
            exports_w: 0.0,
            imports_w: 0.0,
            balance_ema: 0.0,
            exports_to: Vec::new(),
            imports_from: Vec::new(),
            exports_to_last: Vec::new(),
            imports_from_last: Vec::new(),
            transport_paid: 0.0,
            traded_value: 0.0,
            transport_share: 0.0,
            trade_margin: 0.0,
            border_provinces,
            gdp_local: 0.0,
            gdp: gdp_w,
            gdp_start: gdp_w,
            real_gdp: 0.0,
            real_trend: 0.0,
            real_growth: 0.0,
            money_growth: 0.0,
            printed_month: 0.0,
            history: History {
                prices: vec![Vec::new(); goods],
                ..History::default()
            },
            player: i == 0,
            state_autopilot: true,
        };
        n.skilled = (interpolate(&ed.skill_by_hdi, hdi) + effects.get("skill")).clamp(0.01, 0.9);
        n.refresh_fast_mods(ed);
        n.plan_price = n.price.clone();
        // Two day-1 fits, found together: productivity, so that at the wage
        // day 1's GDP needs, factories pay `labour_share` of what they add in
        // wages; and appetite, so that goods, power and fuel take what
        // households have left after services (see `calibrate`).
        for _ in 0..25 {
            n.buildings.clear();
            self.endow(data, ed, &mut n, gdp_w);
            let (va, bill, wants_target, wants) = self.calibrate(ed, &mut n, gdp_w);
            if va <= 0.0 || bill <= 0.0 || wants <= 0.0 {
                break;
            }
            let r = bill / (ed.labour_share * va);
            let a = wants_target / wants;
            if r.ln().abs() < 0.02 && a.ln().abs() < 0.02 {
                break;
            }
            n.tech_cal = (n.tech_cal * r.powf(0.6)).clamp(0.02, 50.0);
            n.need_cal = (n.need_cal * a.powf(0.6)).clamp(0.02, 50.0);
        }
        n
    }

    /// The buildings, stocks, wage and money a nation starts with: enough to
    /// feed its people, power its homes, make some of its goods and weapons,
    /// and sell its Main Resource abroad.
    fn endow(&mut self, data: &Data, ed: &EconData, n: &mut NationEcon, gdp_w: f64) {
        let _ = data;
        let goods = ed.goods.len();
        let t = tech(ed, n);
        let md = ed.models[n.model].clone();
        let p = n.population;
        let k = p / 1000.0;
        let labour = p * ed.participation;
        let army = (p * n.duty).min(labour * 0.5);
        let gov = labour * md.gov_share;
        let avail = (labour - army - gov).max(0.0);
        let u_nat = interpolate(&ed.natural_unemployment_by_hdi, n.hdi);
        let industrial = (0.3 + 0.8 * (n.hdi - 0.4)).clamp(0.3, 0.9);

        // What its people need each day, and how much of it is made at home.
        let b = &ed.basket;
        let mut target = vec![0.0; goods];
        target[ed.food] = k * b.food;
        let lv = living(ed, n);
        target[ed.consumer_goods] = k * (b.goods * lv.powf(b.goods_exp) + b.luxury * lv.powf(b.luxury_exp));
        target[ed.fuel] = k * b.fuel * lv + army / 1000.0 * 0.05;
        // Weapons: the army's yearly buying, a share made at home.
        let procurement = ed.state.procurement.get(&n.army_option).copied().unwrap_or(0.0) * n.mods.get("procurement");
        let military_w = procurement * gdp_w / ed.days_per_year as f64;
        let jets_ok = n.hdi >= 0.6;
        target[ed.ammo] += 0.5 * military_w / ed.goods[ed.ammo].price * industrial;
        target[ed.tanks] += (if jets_ok { 0.3 } else { 0.5 }) * military_w / ed.goods[ed.tanks].price * industrial;
        if jets_ok {
            target[ed.jets] += 0.2 * military_w / ed.goods[ed.jets].price * industrial;
        }
        let power_homes = k * b.power * lv;

        // Work backwards through the chains: what makes each target, and what that needs.
        let mut levels: Vec<f64> = vec![0.0; ed.buildings.len()];
        let mut need = target.clone();
        // Main Resource: a raw export worth about 4% of GDP, on top of home needs.
        if let Some(g) = n.main_resource {
            need[g] += 0.04 * gdp_w / ed.days_per_year as f64 / ed.goods[g].price;
        }
        // A month's stock that slowly goes bad has to be made up too.
        let spoil_extra = |g: usize| -> f64 {
            let sp = ed.goods[g].spoil;
            if sp >= 1.0 { 1.0 } else { 1.0 + ed.prices.days_of_cover * sp }
        };
        let maker = |g: usize, prefer: &dyn Fn(usize) -> f64| -> Option<usize> {
            ed.buildings
                .iter()
                .enumerate()
                .filter(|(_, b)| b.output == g)
                .max_by(|a, b| prefer(a.0).total_cmp(&prefer(b.0)))
                .map(|(i, _)| i)
        };
        // Coal or oil for power: whichever the nation has more of.
        let coal = ed.good_index("coal").unwrap_or(0);
        let oil = ed.good_index("oil").unwrap_or(0);
        let reserve = |g: usize| -> f64 { n.provinces.iter().map(|&q| self.provinces[q].deposits[g] * self.provinces[q].hexes).sum() };
        let (coal_reserve, oil_reserve) = (reserve(coal), reserve(oil));
        for tier in [4u8, 3, 2] {
            if tier == 2 {
                // Power: homes plus everything above.
                need[ed.electricity] += power_homes;
            }
            for g in 0..goods {
                let Some(bi) = maker(g, &|bi| {
                    if ed.buildings[bi].inputs.iter().any(|&(i, _)| i == coal) {
                        coal_reserve
                    } else if ed.buildings[bi].inputs.iter().any(|&(i, _)| i == oil) {
                        oil_reserve
                    } else {
                        1.0
                    }
                }) else {
                    continue;
                };
                let bd = &ed.buildings[bi];
                if bd.tier != tier || need[g] <= 0.0 {
                    continue;
                }
                let gross = need[g] * spoil_extra(g) / (1.0 - defect_rate(n, bd));
                levels[bi] += gross / (bd.amount * t * skill_factor(n, bd) * output_mod(ed, n, g));
                for &(i, per) in &bd.inputs {
                    need[i] += gross * per;
                }
                need[g] = 0.0;
            }
        }
        // Raw goods: dig what the land allows, richest provinces first.
        let mut raw_places: Vec<(usize, usize, f64)> = Vec::new(); // (building, province, levels)
        let mut raw_short = vec![0.0; goods];
        for (bi, bd) in ed.buildings.iter().enumerate() {
            if !bd.deposit {
                continue;
            }
            let g = bd.output;
            let mut want = need[g] * spoil_extra(g) / (1.0 - defect_rate(n, bd));
            let out_mod = output_mod(ed, n, g);
            let mut places: Vec<usize> = n.provinces.clone();
            places.sort_by(|&a, &b| self.provinces[b].deposits[g].total_cmp(&self.provinces[a].deposits[g]));
            for q in places {
                if want <= 0.0 {
                    break;
                }
                let pe = &self.provinces[q];
                if pe.deposits[g] < 0.2 {
                    continue;
                }
                let cap = pe.deposits[g].min(3.0) * pe.hexes * bd.cap_per_hex;
                let per_level = bd.amount * t * skill_factor(n, bd) * pe.deposits[g] * out_mod;
                let l = (want / per_level).min(cap);
                raw_places.push((bi, q, l));
                want -= l * per_level;
            }
            // Food comes first: short of grain, farm poorer land too, and
            // farm the good land harder (twice the usual levels at most).
            if g == ed.grain && want > 0.0 {
                let mut places: Vec<usize> = n.provinces.clone();
                places.sort_by(|&a, &b| self.provinces[b].deposits[g].total_cmp(&self.provinces[a].deposits[g]));
                for q in places {
                    if want <= 0.0 {
                        break;
                    }
                    let pe = &self.provinces[q];
                    if pe.deposits[g] < 0.05 {
                        continue;
                    }
                    let cap = pe.deposits[g].clamp(0.2, 3.0) * pe.hexes * bd.cap_per_hex;
                    let per_level = bd.amount * t * skill_factor(n, bd) * pe.deposits[g] * out_mod;
                    let l = (want / per_level).min(cap);
                    raw_places.push((bi, q, l));
                    want -= l * per_level;
                }
            }
            raw_short[g] = want.max(0.0);
        }

        // Not more workers than it has: shrink industry first, then everything.
        let wpl = ed.workers_per_level;
        let mut workers: f64 = levels.iter().sum::<f64>() * wpl + raw_places.iter().map(|r| r.2).sum::<f64>() * wpl;
        let room = avail * (1.0 - u_nat) * 0.85;
        if workers > room {
            let k = room / workers;
            levels.iter_mut().for_each(|l| *l *= k);
            raw_places.iter_mut().for_each(|r| r.2 *= k);
            workers = room;
        }

        // Lay the buildings: raw where the deposits are, the rest where the people are.
        let mut by_people = n.provinces.clone();
        by_people.sort_by(|&a, &b| self.provinces[b].people.total_cmp(&self.provinces[a].people));
        // Industry spreads over every province with a real share of the people.
        let mut main_places: Vec<usize> = by_people.iter().copied().filter(|&q| self.provinces[q].people >= 0.04).take(10).collect();
        if main_places.is_empty() {
            main_places = by_people.iter().copied().take(1).collect();
        }
        let shares: Vec<f64> = {
            let total: f64 = main_places.iter().map(|&q| self.provinces[q].people).sum::<f64>().max(1e-9);
            main_places.iter().map(|&q| self.provinces[q].people / total).collect()
        };
        for (bi, l) in levels.iter().enumerate() {
            if *l <= 0.0 || ed.buildings[bi].deposit {
                continue;
            }
            for (j, &q) in main_places.iter().enumerate() {
                push_building(&mut n.buildings, bi, q, l * shares[j]);
            }
        }
        for (bi, q, l) in raw_places {
            if l > 1e-6 {
                push_building(&mut n.buildings, bi, q, l);
            }
        }

        // What couldn't be dug at home, for the world's balance (see `balance_raw`).
        n.raw_short = raw_short;
        let _ = (workers, military_w, jets_ok, target, power_homes);
    }

    /// Day-1 money, calibrated so the first day's GDP is the wheel's.
    /// Returns the goods sector's value added and wage bill on day 1, what
    /// households can spend on goods, power and fuel, and what they cost.
    fn calibrate(&mut self, ed: &EconData, n: &mut NationEcon, gdp_w: f64) -> (f64, f64, f64, f64) {
        let goods = ed.goods.len();
        let t = tech(ed, n);
        let md = ed.models[n.model].clone();
        let k = n.population / 1000.0;
        let labour = n.population * ed.participation;
        let army = (n.population * n.duty).min(labour * 0.5);
        let gov = labour * md.gov_share;
        let avail = (labour - army - gov).max(0.0);
        let u_nat = interpolate(&ed.natural_unemployment_by_hdi, n.hdi);
        let b = &ed.basket;
        let lv = living(ed, n);
        let power_homes = k * b.power * lv;
        let wpl = ed.workers_per_level;
        let workers: f64 = n.buildings.iter().map(|x| x.levels).sum::<f64>() * wpl;
        let mut target = vec![0.0; goods];
        target[ed.food] = k * b.food;
        target[ed.consumer_goods] = k * (b.goods * lv.powf(b.goods_exp) + b.luxury * lv.powf(b.luxury_exp));
        target[ed.fuel] = k * b.fuel * lv;
        let procurement = ed.state.procurement.get(&n.army_option).copied().unwrap_or(0.0) * n.mods.get("procurement");
        let military_w = procurement * gdp_w / ed.days_per_year as f64;
        let jets_ok = n.hdi >= 0.6;
        // Day-1 money, calibrated so the first day's GDP is the wheel's.
        let gdp_day = gdp_w / ed.days_per_year as f64 / n.exchange;
        let mut va_goods = 0.0;
        let mut made = vec![0.0; goods];
        let mut used = vec![0.0; goods];
        for bld in &n.buildings {
            let bd = &ed.buildings[bld.kind];
            let rich = if bd.deposit { self.provinces[bld.province].deposits[bd.output] } else { 1.0 };
            let gross = bld.levels * bd.amount * t * rich * skill_factor(n, bd) * output_mod(ed, n, bd.output);
            va_goods += gross * (1.0 - defect_rate(n, bd)) * n.price[bd.output];
            made[bd.output] += gross;
            for &(i, per) in &bd.inputs {
                va_goods -= gross * per * n.price[i];
                used[i] += gross * per;
            }
        }
        let flow: Vec<f64> = (0..goods).map(|g| made[g].max(used[g])).collect();
        // Wage and service workers together: day 1's GDP is the wheel's, and
        // households can afford the services they buy.
        let mut goods_wages_per_w = 0.0;
        let mut goods_spend = 0.0;
        let mut wants_spend = 0.0;
        for bld in &n.buildings {
            goods_wages_per_w += bld.levels * wpl * (1.0 + ed.buildings[bld.kind].skill * ed.skill_pay);
        }
        for (g, need) in [
            (ed.food, k * b.food),
            (ed.electricity, power_homes),
            (ed.fuel, k * b.fuel * lv),
            (ed.consumer_goods, k * (b.goods * lv.powf(b.goods_exp) + b.luxury * lv.powf(b.luxury_exp))),
        ] {
            let spend = need * n.price[g] * (1.0 + md.sales_tax);
            goods_spend += spend;
            if g != ed.food {
                wants_spend += spend;
            }
        }
        // Day 1 adds up like this. Everyone not in the army, the state or a
        // building works in services, but for the natural unemployment. GDP
        // is goods value added, plus services, plus the state's wages: that
        // gives the wage. Households' wealth starts where what they save
        // and what they spend from it balance, so their budget is their
        // disposable income plus firms' retained profit; what that leaves
        // after services is what goods, power and fuel can take (the
        // appetite fit in `make_nation` aims there).
        let gdp_year_target = gdp_day * ed.days_per_year as f64;
        let debt_share = if md.planned { 0.2 } else { 0.35 };
        let debt0 = ((debt_share + n.mods.get("debt")).max(0.0)) * gdp_year_target;
        let rating0 = ed.rating_for(70.0 + n.mods.get("rating"));
        let interest = debt0 * (n.rate + ed.ratings[rating0].premium) / ed.days_per_year as f64;
        let mut building_upkeep = 0.0;
        for bld in &n.buildings {
            building_upkeep += bld.levels * ed.buildings[bld.kind].upkeep / n.exchange;
        }
        let mut upkeep = building_upkeep;
        for &q in &n.provinces {
            let p = &self.provinces[q];
            upkeep += (p.road + p.rail + p.port) * p.hexes * ed.state.infrastructure_upkeep / n.exchange;
        }
        let save = (ed.save_base + ed.save_rate_slope * (n.rate - 0.04)).clamp(0.02, 0.4);
        let m = ed.services_markup;
        let state_workers = gov + army * ed.state.army_pay;
        let room = (avail - workers).max(0.0);
        let unemployed = (u_nat * (labour - army)).min(room);
        let services = room - unemployed;
        let floor = 0.2 * gdp_day / labour.max(1.0);
        let wage = ((gdp_day - va_goods) / ((1.0 + m) * services + state_workers).max(1.0)).max(floor);
        let serv_spend = services * wage * (1.0 + m);
        let wage_bill = wage * (goods_wages_per_w + services + state_workers);
        let welfare = unemployed * wage * ed.welfare_rate + ed.pension_share * wage_bill;
        let profit = (va_goods - wage * goods_wages_per_w - building_upkeep) + serv_spend - services * wage;
        let profit_after = profit.max(0.0) * (1.0 - md.corporate_tax);
        let disposable = wage_bill * (1.0 - md.income_tax) + welfare + ed.payout * profit_after + interest;
        let retained = (1.0 - ed.payout) * profit_after;
        let budget = disposable + retained;
        let wealth = (save * disposable + retained) / 0.0005;
        // What households can spend on goods, power and fuel.
        let food_spend = goods_spend - wants_spend;
        let wants_target = (budget + upkeep - serv_spend - food_spend)
            .clamp(0.05 * budget, b.day_one_share * (b.goods_share + b.essentials_share) * budget);
        n.wage = wage;
        n.wage0 = n.wage;
        n.wage_anchor = n.wage * n.exchange;
        n.wage_w = n.wage_anchor;
        n.tech_start = t;
        n.hdi_start = n.hdi;
        n.population_start = n.population;
        n.services = services;
        n.services_spending = serv_spend;
        n.army = army;
        n.government = gov;
        n.unemployed = unemployed;

        // Stocks: a month of what flows; weapons: years of buying.
        for g in 0..goods {
            let daily = flow[g].max(target[g]).max(k * 0.01);
            n.stock[g] = if ed.goods[g].military { 0.0 } else { ed.prices.days_of_cover * daily };
            n.use_rate[g] = daily;
        }
        n.stock[ed.electricity] = 0.0;
        let years = ed.state.starting_stockpile_years;
        n.stockpile[ed.ammo] = years * 0.5 * military_w * ed.days_per_year as f64 / ed.goods[ed.ammo].price;
        n.stockpile[ed.tanks] = years * (if jets_ok { 0.3 } else { 0.5 }) * military_w * ed.days_per_year as f64 / ed.goods[ed.tanks].price;
        if jets_ok {
            n.stockpile[ed.jets] = years * 0.2 * military_w * ed.days_per_year as f64 / ed.goods[ed.jets].price;
        }
        n.stockpile = n.stockpile.iter().map(|x| x.floor()).collect();
        n.stockpile[ed.ammo] = n.stockpile[ed.ammo].max(0.0);

        let gdp_year = gdp_day * ed.days_per_year as f64;
        n.money = 0.7 * gdp_year;
        n.money_start = n.money;
        n.real_start = labour.max(1.0) * t;
        n.real_supply = n.real_start;
        n.savings = wealth;
        n.treasury = (0.02 + n.mods.get("treasury")).max(0.0) * gdp_year;
        let debt_share = if md.planned { 0.2 } else { 0.35 };
        n.debt = ((debt_share + n.mods.get("debt")).max(0.0)) * gdp_year;
        n.gdp_local = gdp_day;
        n.real_gdp = gdp_day;
        n.real_trend = gdp_day;
        n.cpi_ring = vec![1.0; ed.days_per_year as usize];
        n.goods_cpi = 1.0;
        n.goods_ring = vec![1.0; ed.days_per_year as usize];
        n.goods_inflation = ed.money.pi_target;
        n.corruption = (md.corruption + n.mods.get("corruption")).clamp(0.0, 0.6);
        n.rating = ed.rating_for(70.0 + n.mods.get("rating"));
        n.bond_rate = n.rate + ed.ratings[n.rating].premium;
        (va_goods, wage * goods_wages_per_w, wants_target, wants_spend)
    }

    /// The world starts able to dig what it needs: raw goods some nations
    /// can't produce are dug where there is spare land and labour.
    fn balance_raw(&mut self, ed: &EconData) {
        let wpl = ed.workers_per_level;
        // No nation digs for the world more than a tenth of its GDP's worth.
        let mut allowance: Vec<f64> = self.nations.iter().map(|n| 0.10 * n.gdp_start / ed.days_per_year as f64).collect();
        for (bi, bd) in ed.buildings.iter().enumerate() {
            if !bd.deposit {
                continue;
            }
            let g = bd.output;
            let mut short: f64 = self.nations.iter().map(|n| n.raw_short[g]).sum();
            if short <= 0.0 {
                continue;
            }
            // Every spare place, most productive first.
            let mut places: Vec<(f64, usize, usize, f64)> = Vec::new(); // (output per level, nation, province, spare levels)
            for (i, n) in self.nations.iter().enumerate() {
                let t = tech(ed, n);
                let out_mod = output_mod(ed, n, g);
                for &q in &n.provinces {
                    let pe = &self.provinces[q];
                    if pe.deposits[g] < 0.2 {
                        continue;
                    }
                    let have: f64 = n.buildings.iter().filter(|b| b.kind == bi && b.province == q).map(|b| b.levels).sum();
                    let spare = pe.deposits[g].min(3.0) * pe.hexes * bd.cap_per_hex - have;
                    if spare > 0.5 {
                        places.push((bd.amount * t * skill_factor(n, bd) * pe.deposits[g] * out_mod, i, q, spare));
                    }
                }
            }
            places.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
            for (per_level, i, q, spare) in places {
                if short <= 0.0 {
                    break;
                }
                let n = &mut self.nations[i];
                let room = labour_room(ed, n) / wpl;
                let by_value = allowance[i] / (per_level * ed.goods[g].price).max(1e-9);
                let add = spare.min(short / per_level.max(1e-9)).min(room.max(0.0)).min(by_value);
                if add > 0.01 {
                    push_building(&mut n.buildings, bi, q, add);
                    short -= add * per_level;
                    allowance[i] -= add * per_level * ed.goods[g].price;
                }
            }
        }
        for n in self.nations.iter_mut() {
            n.raw_short = Vec::new();
        }
    }

    /// Day 1's roads and rail carry what each province makes, with room to
    /// spare: rail is laid first, then roads, up to the most a province holds.
    fn fit_transport(&mut self, ed: &EconData) {
        let wpl = ed.workers_per_level;
        let _ = wpl;
        for n in &self.nations {
            let t = tech(ed, n);
            let mut load = vec![0.0; self.provinces.len()];
            for b in &n.buildings {
                let bd = &ed.buildings[b.kind];
                let rich = if bd.deposit { self.provinces[b.province].deposits[bd.output] } else { 1.0 };
                load[b.province] += b.levels * bd.amount * t * rich * skill_factor(n, bd) * output_mod(ed, n, bd.output) * ed.goods[bd.output].weight;
            }
            let mods = n.mods.get("transport");
            for &q in &n.provinces {
                let p = &mut self.provinces[q];
                let need = 1.3 * load[q] / (p.hexes * mods).max(1e-9);
                let cap = |p: &ProvinceEcon| ed.transport_base + ed.transport_road * p.road + ed.transport_rail * p.rail;
                if cap(p) < need {
                    p.rail = (p.rail + (need - cap(p)) / ed.transport_rail).min(MAX_INFRA);
                }
                if cap(p) < need {
                    p.road = (p.road + (need - cap(p)) / ed.transport_road).min(MAX_INFRA);
                }
            }
        }
    }

    /// Re-works out each nation's trade capacity (ports, or overland roads and rail).
    pub(super) fn refresh_trade_capacity(&mut self, ed: &EconData) {
        for n in self.nations.iter_mut() {
            let mut cap = 0.0;
            if !n.landlocked {
                for &q in &n.provinces {
                    cap += self.provinces[q].port * ed.port_capacity;
                }
                cap *= n.mods.get("port");
            } else {
                for &q in &n.border_provinces {
                    cap += (self.provinces[q].road + self.provinces[q].rail) * ed.land_capacity;
                }
            }
            n.trade_capacity = cap * n.mods.get("transport").max(0.1);
        }
    }
}

/// Workers a nation could still put in buildings on day 1.
fn labour_room(ed: &EconData, n: &NationEcon) -> f64 {
    let md = &ed.models[n.model];
    let labour = n.population * ed.participation;
    let army = (n.population * n.duty).min(labour * 0.5);
    let avail = (labour - army - labour * md.gov_share).max(0.0);
    let u_nat = interpolate(&ed.natural_unemployment_by_hdi, n.hdi);
    let used: f64 = n.buildings.iter().map(|b| b.levels).sum::<f64>() * ed.workers_per_level;
    avail * (1.0 - u_nat) * 0.85 - used
}

fn push_building(list: &mut Vec<Building>, kind: usize, province: usize, levels: f64) {
    if levels <= 1e-6 {
        return;
    }
    if let Some(b) = list.iter_mut().find(|b| b.kind == kind && b.province == province) {
        b.levels += levels;
        return;
    }
    list.push(Building {
        kind,
        province,
        levels,
        open: true,
        hired: 0.0,
        output: 0.0,
        input_factor: 1.0,
        profit: 0.0,
        losing_days: 0,
        utilization: 1.0,
        player_built: false,
    });
}

/// The heart of a nation's name: "Kingdom of Valmora" -> "Valmora".
pub fn base_name(name: &str) -> String {
    if let Some((_, rest)) = name.rsplit_once(" of ") {
        return rest.to_string();
    }
    name.split_whitespace().next().unwrap_or(name).to_string()
}

pub(super) fn add_building(list: &mut Vec<Building>, kind: usize, province: usize, levels: f64, by_player: bool) {
    push_building(list, kind, province, levels);
    if by_player {
        if let Some(b) = list.iter_mut().find(|b| b.kind == kind && b.province == province) {
            b.player_built = true;
        }
    }
}
