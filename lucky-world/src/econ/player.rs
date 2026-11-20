//! What the player can do to their economy (docs/ECONOMY.md, section 12):
//! set taxes, welfare, tariffs and the interest rate; build and close
//! buildings; choose what may be traded and with whom.
//!
//! Every action works on any nation (the tests and NPCs use the same rules),
//! takes effect through the normal daily simulation, and says what it did.

use super::init::MAX_INFRA;
use super::{Construction, EconData, Economy, TradePolicy};

const EPS: f64 = 1e-9;

/// The levers on the budget screen.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lever {
    IncomeTax,
    CorporateTax,
    SalesTax,
    Welfare,
    Tariff,
    /// The central bank's rate (from automatic to a fixed rate).
    InterestRate,
    /// Research and schools, a share of GDP a year.
    Research,
    /// Weapons bought for the stockpile, a share of GDP a year.
    Arms,
    /// The public works scheme's size, a share of the workforce (a job
    /// guarantee from a little over the natural rate, with retraining).
    PublicWorks,
}

impl Lever {
    pub const ALL: [Lever; 9] = [
        Lever::IncomeTax,
        Lever::CorporateTax,
        Lever::SalesTax,
        Lever::Welfare,
        Lever::Tariff,
        Lever::InterestRate,
        Lever::Research,
        Lever::Arms,
        Lever::PublicWorks,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Lever::IncomeTax => "Income tax",
            Lever::CorporateTax => "Corporate tax",
            Lever::SalesTax => "Sales tax",
            Lever::Welfare => "Welfare (share of a wage)",
            Lever::Tariff => "Tariff on imports",
            Lever::InterestRate => "Interest rate",
            Lever::Research => "Research and schools (of GDP)",
            Lever::Arms => "Arms purchases (of GDP)",
            Lever::PublicWorks => "Public works (of workers)",
        }
    }

    /// The step for one press, and the range it stays in.
    pub fn step(self) -> (f64, f64, f64) {
        match self {
            Lever::IncomeTax => (0.01, 0.0, 0.7),
            Lever::CorporateTax => (0.01, 0.0, 0.7),
            Lever::SalesTax => (0.01, 0.0, 0.4),
            Lever::Welfare => (0.02, 0.0, 0.8),
            Lever::Tariff => (0.01, 0.0, 1.0),
            Lever::InterestRate => (0.0025, 0.0, 0.4),
            Lever::Research => (0.0025, 0.0, 0.05),
            Lever::Arms => (0.001, 0.0, 0.06),
            Lever::PublicWorks => (0.01, 0.0, 0.25),
        }
    }
}

impl Economy {
    /// A lever's value now (the interest rate is today's rate, set or not).
    pub fn lever(&self, nation: usize, lever: Lever) -> f64 {
        self.lever_with(nation, lever, None)
    }

    /// The same, with the rules at hand for the levers that need them.
    pub fn lever_with(&self, nation: usize, lever: Lever, ed: Option<&EconData>) -> f64 {
        let n = &self.nations[nation];
        match lever {
            Lever::IncomeTax => n.income_tax,
            Lever::CorporateTax => n.corporate_tax,
            Lever::SalesTax => n.sales_tax,
            Lever::Welfare => n.welfare_rate,
            Lever::Tariff => n.tariff,
            Lever::InterestRate => n.manual_rate.unwrap_or(n.rate),
            Lever::Research => n.research,
            Lever::Arms => match ed {
                Some(ed) => n.arms_share(ed),
                None => n.arms.unwrap_or(0.0),
            },
            Lever::PublicWorks => n.works.unwrap_or_else(|| ed.map_or(0.0, |ed| ed.state.public_works_max)),
        }
    }

    /// How far one press moves a lever.
    pub fn lever_step(lever: Lever) -> f64 {
        lever.step().0
    }

    /// The lowest and highest a lever can go.
    pub fn lever_range(lever: Lever) -> (f64, f64) {
        let (_, lo, hi) = lever.step();
        (lo, hi)
    }

    /// Moves a lever by `steps` presses. Taxes and welfare set by hand stay
    /// as set (the fiscal autopilot leaves them alone from then on).
    pub fn pull(&mut self, nation: usize, lever: Lever, steps: i32) -> String {
        self.pull_with(None, nation, lever, steps)
    }

    /// `pull`, with the rules at hand (the arms lever starts from the wheel's share).
    pub fn pull_with(&mut self, ed: Option<&EconData>, nation: usize, lever: Lever, steps: i32) -> String {
        let (step, lo, hi) = lever.step();
        let value = (self.lever_with(nation, lever, ed) + step * steps as f64).clamp(lo, hi);
        let n = &mut self.nations[nation];
        match lever {
            Lever::IncomeTax => n.income_tax = value,
            Lever::CorporateTax => n.corporate_tax = value,
            Lever::SalesTax => n.sales_tax = value,
            Lever::Welfare => n.welfare_rate = value,
            Lever::Tariff => n.tariff = value,
            Lever::InterestRate => n.manual_rate = Some(value),
            Lever::Research => n.research = value,
            Lever::Arms => n.arms = Some(value),
            Lever::PublicWorks => n.works = Some(value),
        }
        if !matches!(lever, Lever::InterestRate | Lever::Tariff | Lever::Research | Lever::Arms | Lever::PublicWorks) {
            n.manual_taxes = true;
        }
        format!("{}: {:.1}%", lever.name(), value * 100.0)
    }

    /// Hands the interest rate back to the central bank.
    pub fn free_rate(&mut self, nation: usize) -> String {
        self.nations[nation].manual_rate = None;
        "The central bank sets the rate again".to_string()
    }

    /// Hands taxes and welfare back to the fiscal autopilot.
    pub fn free_taxes(&mut self, nation: usize) -> String {
        self.nations[nation].manual_taxes = false;
        self.nations[nation].works = None;
        "The treasury sets taxes, welfare and public works again".to_string()
    }

    /// Where a new building of a kind would go: the richest deposit with room
    /// for a mine, farm or well; where the most people live for the rest.
    pub fn best_place(&self, ed: &EconData, nation: usize, kind: usize) -> Option<usize> {
        let n = &self.nations[nation];
        let bd = &ed.buildings[kind];
        let mut best: Option<(usize, f64)> = None;
        for &q in &n.provinces {
            let p = &self.provinces[q];
            if bd.coastal && !p.coastal {
                continue;
            }
            let value = if bd.deposit {
                let d = p.deposits[bd.output];
                if d < 0.2 {
                    continue;
                }
                let have: f64 = n.buildings.iter().filter(|b| b.kind == kind && b.province == q).map(|b| b.levels).sum::<f64>()
                    + n.construction.iter().filter(|c| c.kind == kind && c.province == q).map(|c| c.levels).sum::<f64>();
                if d.min(3.0) * p.hexes * bd.cap_per_hex - have < 1.0 {
                    continue;
                }
                d
            } else {
                p.people + if q == n.capital { 0.2 } else { 0.0 }
            };
            if best.is_none_or(|(_, v)| value > v) {
                best = Some((q, value));
            }
        }
        best.map(|(q, _)| q)
    }

    /// How many levels one "build" order adds, and what it costs (local money).
    pub fn build_block(&self, ed: &EconData, nation: usize, kind: usize) -> (f64, f64) {
        let n = &self.nations[nation];
        let bd = &ed.buildings[kind];
        let have: f64 = n.buildings.iter().filter(|b| b.kind == kind).map(|b| b.levels).sum();
        let levels = (0.1 * have).max(2.0);
        let cost = bd.cost * n.mods.get("build_cost") / n.exchange * levels;
        (levels, cost)
    }

    /// The player orders new levels of a building, paid by the treasury (and
    /// borrowed if the treasury is short). Building goods come out of stock.
    pub fn player_build(&mut self, ed: &EconData, nation: usize, kind: usize) -> Result<String, String> {
        let bd = &ed.buildings[kind];
        if bd.output == ed.jets && self.nations[nation].hdi < 0.6 {
            return Err("Jets need an HDI of at least 0.6".to_string());
        }
        let Some(q) = self.best_place(ed, nation, kind) else {
            return Err(format!("No province has room for a {}", bd.name));
        };
        let (mut levels, _) = self.build_block(ed, nation, kind);
        let n = &mut self.nations[nation];
        for &(g, per) in &bd.build_goods {
            if per > 0.0 {
                levels = levels.min(n.stock[g] / per);
            }
        }
        if levels < 0.5 {
            let short: Vec<&str> = bd.build_goods.iter().filter(|&&(g, per)| n.stock[g] < per * 0.5).map(|&(g, _)| ed.goods[g].name.as_str()).collect();
            return Err(format!("Not enough {} in store to build", short.join(" or ")));
        }
        for &(g, per) in &bd.build_goods {
            n.stock[g] -= per * levels;
            n.ledger.used[g] += per * levels;
        }
        let cost = bd.cost * n.mods.get("build_cost") / n.exchange * levels;
        n.treasury -= cost;
        if n.treasury < 0.0 {
            let gap = -n.treasury;
            n.debt += gap;
            n.savings -= gap;
            n.treasury = 0.0;
        }
        n.budget_month.construction += cost;
        n.projects.push((cost / bd.build_days.max(1) as f64, bd.build_days.max(1)));
        n.construction.push(Construction {
            kind,
            province: q,
            levels,
            days_left: bd.build_days.max(1),
            by_state: true,
            by_player: true,
        });
        Ok(format!("Building {:.0} levels of {} (ready in {} days)", levels, bd.name, bd.build_days))
    }

    /// The player closes a block of a building kind (the least profitable
    /// first). Its workers look for other jobs; nothing is refunded.
    pub fn player_close(&mut self, ed: &EconData, nation: usize, kind: usize) -> Result<String, String> {
        let (block, _) = self.build_block(ed, nation, kind);
        let n = &mut self.nations[nation];
        let mut owned: Vec<usize> = (0..n.buildings.len()).filter(|&b| n.buildings[b].kind == kind && n.buildings[b].levels > EPS).collect();
        if owned.is_empty() {
            return Err(format!("You have no {} to close", ed.buildings[kind].name));
        }
        owned.sort_by(|&a, &b| n.buildings[a].profit.total_cmp(&n.buildings[b].profit).then(a.cmp(&b)));
        let mut left = block;
        for b in owned {
            if left <= EPS {
                break;
            }
            let cut = n.buildings[b].levels.min(left);
            n.buildings[b].levels -= cut;
            left -= cut;
        }
        n.buildings.retain(|b| b.levels > 0.01);
        n.hire_order = (0..n.buildings.len()).collect();
        Ok(format!("Closed {:.0} levels of {}", block - left, ed.buildings[kind].name))
    }

    /// Free trade, then no exports, then no imports, then free again.
    pub fn cycle_trade_policy(&mut self, ed: &EconData, nation: usize, good: usize) -> String {
        let next = match self.nations[nation].trade_policy[good] {
            TradePolicy::Free => TradePolicy::NoExports,
            TradePolicy::NoExports => TradePolicy::NoImports,
            TradePolicy::NoImports => TradePolicy::Free,
        };
        self.set_trade_policy(ed, nation, good, next)
    }

    /// Sets what may cross the border of one good.
    pub fn set_trade_policy(&mut self, ed: &EconData, nation: usize, good: usize, policy: TradePolicy) -> String {
        let p = &mut self.nations[nation].trade_policy[good];
        *p = policy;
        let what = match *p {
            TradePolicy::Free => "trades freely",
            TradePolicy::NoExports => "may not be sold abroad",
            TradePolicy::NoImports => "may not be bought abroad",
        };
        format!("{} {what}", ed.goods[good].name)
    }

    /// Stops (or starts again) all trade with another nation.
    pub fn toggle_embargo(&mut self, nation: usize, other: usize) -> String {
        if nation == other {
            return "You can't embargo yourself".to_string();
        }
        let e = &mut self.nations[nation].embargo[other];
        *e = !*e;
        format!("{} {}", if *e { "Embargo on" } else { "Trade reopened with" }, self.nations[other].name)
    }

    /// The province where goods move worst (where a road helps most), if any
    /// moves less than all of its goods.
    pub fn most_choked(&self, nation: usize) -> Option<usize> {
        self.nations[nation]
            .provinces
            .iter()
            .copied()
            .filter(|&q| self.provinces[q].transport < 1.0 - EPS)
            .min_by(|&a, &b| self.provinces[a].transport.total_cmp(&self.provinces[b].transport).then(a.cmp(&b)))
    }

    /// The coastal province with the smallest port (where a port level goes).
    pub fn smallest_port(&self, nation: usize) -> Option<usize> {
        self.nations[nation]
            .provinces
            .iter()
            .copied()
            .filter(|&q| self.provinces[q].coastal && self.provinces[q].port < 10.0)
            .min_by(|&a, &b| self.provinces[a].port.total_cmp(&self.provinces[b].port).then(a.cmp(&b)))
    }

    /// Lays a level of road (or rail where roads are full) in a province.
    pub fn player_road(&mut self, ed: &EconData, nation: usize, province: usize) -> Result<String, String> {
        if !self.nations[nation].provinces.contains(&province) {
            return Err("That province isn't yours".to_string());
        }
        let p = &mut self.provinces[province];
        let n = &mut self.nations[nation];
        let (what, cost) = if p.road < 6.0 {
            p.road += 1.0;
            ("road", ed.state.road_cost * p.hexes / n.exchange)
        } else if p.rail < MAX_INFRA {
            p.rail += 1.0;
            ("rail", ed.state.rail_cost * p.hexes / n.exchange)
        } else {
            return Err("Its roads and rail are already full".to_string());
        };
        n.treasury -= cost;
        if n.treasury < 0.0 {
            let gap = -n.treasury;
            n.debt += gap;
            n.savings -= gap;
            n.treasury = 0.0;
        }
        n.budget_month.infrastructure += cost;
        n.projects.push((cost / 60.0, 60));
        Ok(format!("A level of {what} laid"))
    }

    /// Builds a level of port in one of its coastal provinces (the one with
    /// the smallest port), paid by the treasury.
    pub fn player_port(&mut self, ed: &EconData, nation: usize, province: usize) -> Result<String, String> {
        if !self.nations[nation].provinces.contains(&province) {
            return Err("That province isn't yours".to_string());
        }
        if !self.provinces[province].coastal {
            return Err("That province has no coast".to_string());
        }
        if self.provinces[province].port >= 10.0 {
            return Err("Its port is already as big as a port gets".to_string());
        }
        self.provinces[province].port += 1.0;
        let n = &mut self.nations[nation];
        let cost = ed.state.port_cost / n.exchange;
        n.treasury -= cost;
        if n.treasury < 0.0 {
            let gap = -n.treasury;
            n.debt += gap;
            n.savings -= gap;
            n.treasury = 0.0;
        }
        n.budget_month.infrastructure += cost;
        n.projects.push((cost / 60.0, 60));
        self.refresh_trade_capacity(ed);
        Ok("A level of port built".to_string())
    }
}
