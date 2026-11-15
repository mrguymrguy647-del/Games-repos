//! The world market (docs/ECONOMY.md, section 8): every nation offers its
//! surpluses and bids for its shortages; the market clears pro rata, round
//! embargoes; transport costs and tariffs are paid; world prices move.

use super::tick::{parity, trade_cost};
use super::{EconData, Economy, TradePolicy};

const EPS: f64 = 1e-9;

impl Economy {
    pub(super) fn world_market(&mut self, ed: &EconData) {
        let goods = ed.goods.len();
        let count = self.nations.len();
        let pr = &ed.prices;
        for n in self.nations.iter_mut() {
            n.trade_margin = 0.0;
            n.trade_used = 0.0;
            n.exports_w = 0.0;
            n.imports_w = 0.0;
        }

        // What each nation would sell and buy today.
        let mut offers = vec![vec![0.0; goods]; count];
        let mut bids = vec![vec![0.0; goods]; count];
        for (i, n) in self.nations.iter().enumerate() {
            if n.trade_capacity <= 0.0 {
                continue;
            }
            let (mut bid_weight, mut offer_weight) = (0.0, 0.0);
            for g in 0..goods {
                if !ed.goods[g].trade {
                    continue;
                }
                let (ip, ep) = parity(ed, n, g, self.world_price[g]);
                let target = pr.days_of_cover * n.use_rate[g];
                let p = n.price[g];
                let stock = n.stock[g];
                // Above a month's cover, the rest is for sale; below it, the
                // gap is bought in. Prices set how hard: a good dear at home
                // is bought keenly and sold reluctantly, and the other way round.
                if n.trade_policy[g] != TradePolicy::NoExports && stock > target {
                    let push = (ep / p).clamp(0.5, 2.0);
                    offers[i][g] = ((stock - target) * 0.15 * push).min(stock - 0.5 * target).max(0.0);
                }
                if n.trade_policy[g] != TradePolicy::NoImports && stock < target {
                    let pull = (p / ip).clamp(0.5, 2.0);
                    bids[i][g] = (target - stock) * 0.3 * pull;
                }
                bid_weight += bids[i][g] * ed.goods[g].weight;
                offer_weight += offers[i][g] * ed.goods[g].weight;
            }
            // Ports (or roads and rail at the border) limit it all: what the
            // nation needs comes in first, and its surpluses share the rest.
            let cap = n.trade_capacity;
            if bid_weight > cap {
                let k = cap / bid_weight;
                bids[i].iter_mut().for_each(|x| *x *= k);
            }
            let left = (cap - bid_weight.min(cap)).max(0.0);
            if offer_weight > left {
                let k = left / offer_weight;
                offers[i].iter_mut().for_each(|x| *x *= k);
            }
        }

        // Clear each good: bids are shared over the offers each buyer may take.
        let blocked = |a: usize, b: usize, nations: &[super::NationEcon]| nations[a].embargo[b] || nations[b].embargo[a];
        let mut flows = vec![0.0; count * count];
        for g in 0..goods {
            if !ed.goods[g].trade {
                continue;
            }
            let total_bid: f64 = bids.iter().map(|b| b[g]).sum();
            let total_offer: f64 = offers.iter().map(|o| o[g]).sum();
            if total_bid > EPS && total_offer > EPS {
                flows.iter_mut().for_each(|f| *f = 0.0);
                let mut asked = vec![0.0; count];
                let mut eligible = vec![0.0; count];
                for i in 0..count {
                    if bids[i][g] <= EPS {
                        continue;
                    }
                    let e: f64 = (0..count).filter(|&j| j != i && !blocked(i, j, &self.nations)).map(|j| offers[j][g]).sum();
                    eligible[i] = e;
                    if e <= EPS {
                        continue;
                    }
                    for j in 0..count {
                        if j != i && offers[j][g] > EPS && !blocked(i, j, &self.nations) {
                            asked[j] += bids[i][g] * offers[j][g] / e;
                        }
                    }
                }
                for i in 0..count {
                    if bids[i][g] <= EPS || eligible[i] <= EPS {
                        continue;
                    }
                    for j in 0..count {
                        if j == i || offers[j][g] <= EPS || blocked(i, j, &self.nations) {
                            continue;
                        }
                        let scale = if asked[j] > offers[j][g] { offers[j][g] / asked[j] } else { 1.0 };
                        flows[i * count + j] = bids[i][g] * offers[j][g] / eligible[i] * scale;
                    }
                }
                // Settle every flow: importer i buys from exporter j.
                let w = self.world_price[g];
                for i in 0..count {
                    for j in 0..count {
                        let f = flows[i * count + j];
                        if f <= EPS {
                            continue;
                        }
                        let f = f.min(self.nations[j].stock[g]);
                        if f <= EPS {
                            continue;
                        }
                        let value = f * w;
                        // The exporter.
                        {
                            let ex = &mut self.nations[j];
                            let c = trade_cost(ed, ex, g, w);
                            ex.stock[g] -= f;
                            ex.ledger.exported[g] += f;
                            let receive = value * (1.0 - c).max(0.0) / ex.exchange;
                            ex.trade_margin += receive - f * ex.price[g];
                            ex.exports_w += value;
                            ex.exports_to[i] += value;
                            ex.trade_used += f * ed.goods[g].weight;
                            ex.transport_paid += value * c;
                            ex.traded_value += value;
                        }
                        // The importer.
                        {
                            let im = &mut self.nations[i];
                            let c = trade_cost(ed, im, g, w);
                            im.stock[g] += f;
                            im.ledger.imported[g] += f;
                            let tariff = value * im.tariff / im.exchange;
                            let landed = value * (1.0 + c) / im.exchange + tariff;
                            im.budget_today.tariffs += tariff;
                            im.trade_margin += f * im.price[g] - landed;
                            im.imports_w += value * (1.0 + c);
                            im.imports_from[j] += value;
                            im.trade_used += f * ed.goods[g].weight;
                            im.transport_paid += value * c;
                            im.traded_value += value;
                        }
                    }
                }
            }
            // The world price moves with the imbalance, and slowly back towards its cost.
            let pull = if total_bid.max(total_offer) > EPS { (total_bid - total_offer) / total_bid.max(total_offer) } else { 0.0 };
            let shock: f64 = self.world_shocks.iter().filter(|s| s.0 == g).map(|s| s.1).product();
            let anchor = self.base_world_price[g] * shock;
            let w = &mut self.world_price[g];
            *w *= (pr.world_price_speed * pull + 0.004 * (anchor / *w).ln()).exp();
        }
        // The W$ is the world's unit: goods get dearer or cheaper against
        // each other, but the average world price stays where shocks put it.
        let traded: Vec<usize> = (0..goods).filter(|&g| ed.goods[g].trade).collect();
        if !traded.is_empty() {
            let mean = traded
                .iter()
                .map(|&g| {
                    let shock: f64 = self.world_shocks.iter().filter(|s| s.0 == g).map(|s| s.1).product();
                    (self.world_price[g] / (self.base_world_price[g] * shock)).ln()
                })
                .sum::<f64>()
                / traded.len() as f64;
            let f = (-mean).exp();
            for &g in &traded {
                self.world_price[g] *= f;
            }
        }
        for n in self.nations.iter_mut() {
            n.balance_ema = 0.97 * n.balance_ema + 0.03 * (n.exports_w - n.imports_w);
        }
    }
}
