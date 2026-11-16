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
                    offers[i][g] = ((stock - target) * super::OFFER_SHARE * push).min(stock - 0.5 * target).max(0.0);
                }
                if n.trade_policy[g] != TradePolicy::NoImports && stock < target {
                    let pull = (p / ip).clamp(0.5, 2.0);
                    // No more than twice what it lacks a day (use beyond what
                    // it makes), and a little: a short world is shared by
                    // need, not by how big a stock a nation wants to rebuild.
                    // (By the refill, one giant topping up its month took a
                    // world's food and its tiny neighbours starved.)
                    let lacks = (n.use_rate[g] - n.ledger.produced[g]).max(0.0);
                    bids[i][g] = ((target - stock) * super::BID_SHARE * pull).min(2.0 * lacks + 0.05 * n.use_rate[g]);
                }
            }
        }
        // Nobody plans to ship more than the world offers (or bids): a bid
        // for something nobody sells mustn't take up port room that what is
        // on sale could use.
        let total_offer: Vec<f64> = (0..goods).map(|g| offers.iter().map(|o| o[g]).sum()).collect();
        let total_bid: Vec<f64> = (0..goods).map(|g| bids.iter().map(|b| b[g]).sum()).collect();
        for (i, n) in self.nations.iter().enumerate() {
            let (mut bid_weight, mut offer_weight) = (0.0, 0.0);
            for g in 0..goods {
                let others_offer = total_offer[g] - offers[i][g];
                let others_bid = total_bid[g] - bids[i][g];
                let (b, o) = (bids[i][g].min(others_offer), offers[i][g].min(others_bid));
                bid_weight += b * ed.goods[g].weight;
                offer_weight += o * ed.goods[g].weight;
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
        // A trade agreement: the buyer weighs its partner's offers more.
        let pacts = &self.trade_pact;
        let pref = self.trade_pref;
        let weight = |i: usize, j: usize| if pacts.is_empty() || !pacts[i][j] { 1.0 } else { pref };
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
                    let e: f64 = (0..count).filter(|&j| j != i && !blocked(i, j, &self.nations)).map(|j| offers[j][g] * weight(i, j)).sum();
                    eligible[i] = e;
                    if e <= EPS {
                        continue;
                    }
                    for j in 0..count {
                        if j != i && offers[j][g] > EPS && !blocked(i, j, &self.nations) {
                            asked[j] += bids[i][g] * offers[j][g] * weight(i, j) / e;
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
                        flows[i * count + j] = bids[i][g] * offers[j][g] * weight(i, j) / eligible[i] * scale;
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
                            im.stock_faulty[g] = super::mix_faulty(im.stock_faulty[g], im.stock[g], ed.import_defect, f);
                            im.stock[g] += f;
                            im.ledger.imported[g] += f;
                            let partner = !self.trade_pact.is_empty() && self.trade_pact[i][j];
                            let tariff = if partner { 0.0 } else { value * im.tariff / im.exchange };
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
            // The world price moves with the world's stocks against a month of
            // what the world uses (and the exporters sell), as home prices do,
            // and slowly back towards its cost. (Bids against offers was
            // biased: offers are a share of a stock, bids about a day's need,
            // so in a calm world every price sat 6-7% under its anchor and
            // slid there together at the start, a world-wide deflation.)
            let _ = (total_bid, total_offer);
            let (stock, cover): (f64, f64) = self
                .nations
                .iter()
                .fold((0.0, 0.0), |(s, c), n| (s + n.stock[g], c + n.stock_target(ed, g)));
            let pull = if stock.max(cover) > EPS { ((cover - stock) / stock.max(cover)).clamp(-1.0, 1.0) } else { 0.0 };
            let shock: f64 = self.world_shocks.iter().filter(|s| s.0 == g).map(|s| s.1).product();
            let anchor = self.base_world_price[g] * shock;
            let w = &mut self.world_price[g];
            *w *= (pr.world_price_speed * pull + pr.world_price_pull * (anchor / *w).ln()).exp();
            *w = w.clamp(pr.world_price_band[0] * anchor, pr.world_price_band[1] * anchor);
        }
        // (No renormalising of the whole basket: each price is already pulled
        // to its own anchor, and holding their average fixed made a glut of
        // ore and coal lift food and consumer goods, and the CPI with them.)
        for n in self.nations.iter_mut() {
            n.balance_ema = 0.97 * n.balance_ema + 0.03 * (n.exports_w - n.imports_w);
            // Capital flows: a trade deficit is mostly financed from abroad
            // (`capital_flow` of it comes back as spending at home), and a
            // surplus nation lends that share of its surplus out of its
            // savings. (Without it, money spent on imports was nobody's
            // income: import-heavy nations' services shrank day after day and
            // a third of their people were out of work for decades.)
            let net = (n.exports_w - n.imports_w) / n.exchange.max(1e-12);
            let flow = ed.trade.capital_flow * net;
            if flow < 0.0 {
                n.injection -= flow;
            } else {
                n.savings = (n.savings - flow).max(0.0);
            }
        }
    }
}
