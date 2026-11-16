# LUCKY WORLD: the economy

The economy is the backbone of every state: armies, wars and diplomacy
(later phases) all draw on it. It is deep underneath, and every screen shows
it as a few clear numbers.

This is the design with the actual math, as built. Every constant named here
lives in a data file under `data/economy/`, so the economy can be tuned
without a rebuild:

| File | What it holds |
|---|---|
| `goods.toml` | The 16 goods: tier, world starting price, weight, spoilage |
| `buildings.toml` | Farms, mines, wells, mills, plants, factories: output, inputs, skill, defects, deposits, build cost and time, upkeep |
| `effects.toml` | Every wheel result's economic job (and continent climates, land types) |
| `events.toml` | Booms, crashes, discoveries, bad harvests, fires, strikes, earthquakes, breakthroughs, oil shocks |
| `settings.toml` | Every other constant below, under the same names |
| `currencies.toml` | Currency names by naming style |

The simulation (`src/econ/`) never draws anything; the screens only read it.
It is **deterministic**: the same world and the same seed give the same
economy, day after day. Fifty years of 25 nations take about 4 seconds
headless (`--economy-report`).

---

## 1. Time

One **tick is one day**. A year is 360 days (12 months of 30) so months,
quarters and years line up. The clock starts on 1 January, Year 1. On the map
it can be paused (Space) or run at five speeds (keys 1 to 5):

| Speed | Days per second |
|---|---|
| 1 | 1 |
| 2 | 3 |
| 3 | 7 |
| 4 | 15 |
| 5 | 30 |

Daily things (production, trade, prices, money) run every tick. Monthly
things (budget books, ratings, tax autopilot, investment, closures,
infrastructure) run every 30 days. Charts record a sample every 7 days.
The game only advances whole days in its update (at most 30 a frame), never
while drawing.

---

## 2. Units

- **People** are counted one by one.
- **Workers** fill buildings in **levels**: one level is
  `workers_per_level` = 1,000 workers.
- **Goods** come in **units**. One unit of food is what 1,000 people eat in
  a day; the other goods are sized against it.
- **Money**: every nation has its own currency (the **local** unit, named
  from `currencies.toml`). The world market uses a reference money, the
  **world dollar** (W$). A nation's **exchange rate** `e` is how many W$ one
  local unit buys. On day 1 it is drawn at random between 0.002 and 2 (so
  currencies look different), and the local price of everything is the W$
  price over `e`.

### The 16 goods

The brief asked for 15 goods but listed 16 (6 raw, 5 refined, 5 final). All
16 are in the game.

| Tier | Good | World price (W$) | Weight | Spoils a day |
|---|---|---|---|---|
| Raw | grain | 500 | 1.0 | 0.2% |
| Raw | iron ore | 400 | 1.0 | |
| Raw | coal | 300 | 1.0 | |
| Raw | oil | 900 | 1.0 | |
| Raw | timber | 350 | 1.0 | |
| Raw | bauxite | 450 | 1.0 | |
| Refined | steel | 1,900 | 0.5 | |
| Refined | fuel | 1,500 | 0.5 | |
| Refined | electricity | 200 | 0 | all of it: made to order, never stored or traded |
| Refined | aluminium | 5,200 | 0.3 | |
| Refined | chemicals | 2,600 | 0.4 | |
| Final | food | 800 | 0.6 | 0.3% |
| Final | consumer goods | 2,400 | 0.3 | |
| Final | ammo | 4,000 | 0.3 | military |
| Final | tanks | 4,500,000 | 2.0 | military, per tank |
| Final | jets | 60,000,000 | 3.0 | military, per jet |

**Weight** is what a unit costs in transport and trade capacity. Military
goods go to the national **stockpile** and are not used in peace, except a
little ammo and fuel for training. **Luxuries** are more consumer goods,
bought once the basics are covered, and services.

---

## 3. Production chains

A building sits in a province and has a number of **levels**. Recipes are per
unit of output:

| Building | Output | Inputs per unit | Output per level per day (tech 1) |
|---|---|---|---|
| farm | grain | (land: grain deposit) | 67 |
| iron mine | iron ore | (deposit) | 86 |
| coal mine | coal | (deposit) | 114 |
| oil well | oil | (deposit) | 40 |
| lumber camp | timber | (forest) | 96 |
| bauxite mine | bauxite | (deposit) | 77 |
| coal power plant | electricity | 0.25 coal | 300 |
| oil power plant | electricity | 0.12 oil | 410 |
| steel mill | steel | 1.2 iron ore, 0.8 coal, 2 electricity | 51 |
| refinery | fuel | 1.1 oil, 0.5 electricity | 98 |
| aluminium smelter | aluminium | 2 bauxite, 10 electricity | 18 |
| chemical plant | chemicals | 0.8 oil, 0.8 coal, 1 electricity | 29 |
| food plant | food | 1.25 grain, 0.2 electricity | 305 |
| consumer goods factory | consumer goods | 1 timber, 0.5 steel, 1 electricity | 49 |
| ammo factory | ammo | 0.6 steel, 0.6 chemicals | 35 |
| tank factory | tanks | 400 steel, 200 electricity | 0.0115 |
| jet factory | jets | 2,000 aluminium, 2,000 electricity | 0.00092 |

The amounts are **balanced**: at base prices every building needs about the
same labour per W$ of value added (0.031 workers per W$ a day), so no chain
is born losing money. Electricity comes from coal or oil plants. Tanks and
jets will burn fuel once the war phase lets them move.

### One building's day

For a building `b` with `L` levels in province `p` of nation `n`:

```
tech             t  = exp(tech_slope · (HDI − 0.65)) · tech_cal · progress · tech modifiers
                      tech_slope = 7; tech_cal is fitted on day 1 (section 9);
                      progress grows tech_growth · catch-up a year (below)
crew             cr = fed, the share of its inputs it has been getting (smoothed
                      over about 20 days, at least 5%): it only keeps on the
                      crews it can feed
staffing         st = hired / (L · utilisation · 1000 · cr) · utilisation
skill factor     sk = min(1, skilled share / skill needed) ^ 0.5
richness         ef = deposit (raw only)
output modifier  om = output.all · output.<tier> · output.<good>   (wheels, events, land)
transport        tr = the province's transport factor (below)
potential        Q  = L · amount · t · st · sk · ef · om · tr
input factor     φ  = min over inputs of the share of the needed input it got
gross            G  = Q · φ
defect rate      d  = defect · (1.5 − 0.5·sk) · defect modifiers      (≤ 0.9)
output           O  = G · (1 − d)
inputs used      U_i = G · recipe_i
```

Defective output is scrapped: its inputs were used, but it never reaches the
stock. Goods are never created from nothing.

**Technical progress** converges on a frontier rather than compounding for
ever:

```
gap       = log10(frontier_per_head / GDP per head)        frontier_per_head = $200,000
catch-up  = min(gap, 1.5)              below the frontier (1.5× the base rate far below it)
          = max(frontier_above · gap, −8)  above it          frontier_above = 25
progress ×= exp(tech_growth · catch-up / 360) each day     tech_growth = 1.6%
```

A poor nation learns fast, a rich one slowly, and one above the frontier
slowly loses ground (the best wells and fields are taken, everyone is already
busy), so a windfall can lift a small nation past $200,000 a head but not keep
it there. (Before, every nation compounded 1.2% a year for ever, and tiny
rich ones ran off to millions a head.)

### Tiers each day

Buildings run in tiers, each drawing on the national stock the tier before
left:

1. Raw (farms, mines, wells, camps).
2. Power plants (coal or oil from tier 1).
3. Refined (steel, fuel, aluminium, chemicals: they need power).
4. Final (food, consumer goods, ammo, tanks, jets).

**If one link breaks, everything after it slows or stops.** With no bauxite,
smelters get `φ = 0`; jet factories run only until the aluminium stock is
gone (the test `a_broken_chain_stops_everything_after_it`).

### Sharing a scarce input

- **Food comes first**, in every model: what the food plants will need (grain,
  power) is held back from earlier tiers, and food plants are served first
  in their own tier. People must eat.
- **Households' power** is held back too (what they bought yesterday).
- **Market and mixed economies:** the rest is shared pro rata.
- **Planned economies:** in **plan priority order** (`plan_priority`:
  military first, then heavy industry, then consumer goods last). Earlier
  buildings fill completely; later ones get what is left. That is the planned
  economy's shortage risk.

### Electricity

Power can't be stored, so plants make it **to order**: at most
`1.15 · yesterday's use + a little`. Whatever is made and not used that day
is lost. Its price is still free to move (with a guard rail, section 6).

### Deposits

Every province has a **deposit** (richness) for each raw good: the terrain's
deposits (`data/terrain.toml`, per hex), plus grain for rivers (+0.3) and
lakes (+0.15), times a fixed random spread (0.6 to 1.4 from the seed). The
**Main Resource** wheel doubles that good in the nation's provinces and
guarantees one rich deposit. **Land Type** raises the matching raw good's
output (`[land.*]` in `effects.toml`). A raw building's levels in a province
are capped at `deposit · hexes · cap_per_hex`. **Discoveries** (events) add
rich deposits during play.

### Transport

Each province has **roads**, **rail** and, on the coast, **ports** (0 to 12).
What its buildings can move to the national market each day:

```
capacity_p = hexes_p · (transport_base + transport_road·road_p + transport_rail·rail_p) · transport modifiers
tonnage_p  = what its buildings moved recently (weight units; halves each day)
tr_p      += 0.2 · (min(1, capacity_p / tonnage_p) − tr_p)        (smoothed)
```

Day 1 lays enough rail and roads that every province moves 130% of what it
makes; after that the infrastructure autopilot (section 11) or the player
must keep up. Electricity has no weight (it moves on the grid).

---

## 4. People and jobs

```
labour force  L  = population · participation          participation = 0.48
army          A  = min(population · active duty (wheel), L/2)
government    Gv = L · gov_share(model)
skilled share    = skill_by_hdi(HDI) + skill modifiers   (5% at HDI 0.3 up to 55% at 0.95)
```

The workers left are hired in order:

1. **Buildings**, up to `levels · utilisation · 1000 · crew` (a building
   starved of inputs lets the crews it can't feed go). Market and mixed
   economies hire for food, grain and power first, then by profit. Planned
   economies hire food, grain and power first, then down the chains (raw
   before refined before final), in plan order within a tier.
2. **Services**: as many as services spending pays for,
   `services → spending / (wage · service_pay · (1 + services_markup))`,
   moving 2% a day.
3. Everyone else is **unemployed**: `u = unemployed / (L − A)`.

**Informal work.** When jobs are scarce, service work pays less than the
wage, so the same spending on services employs more people (street stalls,
odd jobs):

```
service_pay → clamp(1 − informal_slope · max(0, u − u_natural), informal_floor, 1)   (1% a day)
informal_slope = 2, informal_floor = 0.4
```

A wage cut alone can't do that: services are paid out of the same incomes it
cuts. Total services income doesn't change, so demand doesn't fall with it.
(The CPI still follows the formal wage.)

**Public works** (in a slump, section 11) employ people at
`public_works_pay` (0.6) of the wage; they count as government staff.

### The wage

Wages are **struck in world dollars**. Each nation's W$ wage `w$` moves 0.3%
a day of the way (in logs) towards

```
base       = day 1's W$ wage · tech / day 1's tech
can_pay    = pay_scale · labour_share · value added of all buildings at full output (W$)
             / worker-equivalents                       (pay_scale: day 1's wage over it,
                                                         so day 1 is a steady state)
anchor    += wage_anchor_speed · (can_pay − anchor)     wage_anchor_speed = 0.5% a day
mix        = base^(1 − wage_follows_pay) · anchor^wage_follows_pay      wage_follows_pay = 0.6
target     = mix · (1 + push)
push       = wage_shortage · tight     when labour is short (tight > 0)    wage_shortage = 0.08
           = wage_tightness · tight    in slack                            wage_tightness = 0.25
tight      = clamp((u_natural − u) / u_natural, −1, 1)
u_natural  = 12% at HDI 0.3 down to 5% at 0.95
w$ stays between 0.25 and 4 times base
local wage w = w$ / e;  when e moves, w$ moves wage_sticky (0.5) of the way with it
```

What all buildings could pay at full output has no selection in it (unlike
what today's survivors earn), and it reaches the wage slowly, so a price
spike doesn't become a wage jump and then a services-price jump.

Workers in skilled buildings earn `w · (1 + skill · skill_pay)`. Because
wages follow productivity in W$ and convert at the exchange rate, a tight
labour market raises **real** wages (squeezing profits until hiring slows)
instead of feeding a wage-price spiral. Wages follow mostly the tech level,
not only the firms still standing, so a slump can't price the rest out.

---

## 5. Households

Each day:

```
wage bill    = buildings + services + government + army (army pay 1.2×)
welfare      = unemployed · w · welfare_rate + pension_share · wage bill
dividends    = payout · firm profits after tax          payout = 0.7
transfers    = interest on state debt + half of what corruption skims
disposable   = wage bill + welfare + dividends + transfers − income tax
save rate s  = clamp(save_base + save_rate_slope · (r − 0.04), 0.02, 0.4)
budget       = (1 − s) · disposable · demand modifiers + 0.0005 · savings
savings     += disposable − budget   (plus firms' retained profits)
```

The budget buys, in order, at home prices plus sales tax:

| Need | Units per 1,000 people per day | Budget cap |
|---|---|---|
| food | 1.0 | 60% of the budget |
| electricity (homes) | `0.5 · lv` | power and fuel together: 12% |
| fuel | `0.1 · lv` | (same cap) |
| consumer goods (basic) | `0.6 · lv^0.9` | goods together: 25% |
| luxuries | `0.4 · lv^1.2` consumer goods | (same cap) |

`lv = tech · need_cal` is the **standard of living**: it rises with tech, from
a day-1 level fitted to what households can afford (section 9). What is
bought counts towards each need's **satisfaction** (0 to 1). What's left of
the budget buys **services**; when the shelves are bare, people save up to
10% of it instead.

Services are people's work: households can't buy more of them than the people
free to do it could deliver, at most `services_premium` (1.1) times their
cost with the markup. Past that, the money stays in savings. (Otherwise
profits paid out and spent again made small, fully employed nations "richer"
every year with nothing more made.)

### Happiness and stability

```
happiness* = 100 · ( 0.35 food_sat + 0.20 goods_sat + 0.10 lux_sat
                   + 0.20 (1 − min(1, u / 0.25))
                   + 0.15 (1 − min(1, max(0, π) / 0.30)) )
happiness moves 3% a day towards happiness*
stability* = 0.6 happiness + 0.4 base_stability(model) + stability modifiers
stability moves 1% a day towards stability*   (0 to 100)
```

### Population and HDI

```
growth a year = birth(HDI) − 0.2 · max(0, 0.85 − food_sat)/0.85
                − birth_unemployment · max(0, u − u_natural) + growth modifiers
                                                       birth_unemployment = 0.1
birth(HDI)    = 2.2% at HDI 0.3, 1.2% at 0.6, 0.2% at 0.95
HDI target    = day 1's HDI + 0.04 · ln(income per head / day 1's)
HDI moves 2% a year towards it
```

Hunger and joblessness both slow growth: where work is scarce, people have
fewer children and some leave. (Above 20% unemployment only, and at half the
rate, populations outgrew their jobs for decades.) HDI follows income gently from its
start: richer means more tech and more tech means richer, so a strong pull
would feed on itself.

---

## 6. Prices

Every good has a **home price** `p` in local money. Each day:

```
target    = days_of_cover (30) · use rate + export rate / 0.15
            (use rate: 33-day average of what was wanted; export rate: 10-day average
             of exports; an exporter holds about a week of exports for sale)
gap       = (target − stock) / max(target, stock)
shortage  = (wanted − got) / wanted
x         = gap + 0.5 · shortage           (power: shortage − 0.5 · share made but unused)
x        += cost_pull · ln(unit cost / p)  when p is below what it costs to make
            (not for what the nation mainly exports; not in planned economies)
Δ ln p    = clamp(price_speed · x, ±price_cap)      price_speed = 0.015, price_cap = 0.02
```

**Unit cost** is inputs, a full crew's wages and upkeep over output at full
output, smoothed over a month. Firms won't keep selling below it: without
the pull, a cheap glut of food closed the food plants and left a dear
shortage a season later. It is measured at full output because a starved
plant still paying its skeleton crew makes every unit look dear (that sent
power to three times its worth). Big exporters are left out because the world
sets their price; a dear producer pulling its home price above what it fetches
abroad stopped offering it.

**Trade parity** ties a traded good's home price to the world:

```
import parity ip = world price · (1 + trade cost + tariff) / e
export parity ep = world price · (1 − trade cost) / e
above ip: Δ ln p −= 0.08 · ln(p / ip);   below ep: Δ ln p += 0.08 · ln(ep / p)
and p always stays within [home_band[0] · ep, home_band[1] · ip] = [0.8 · ep, 1.25 · ip]
(traders step in; at 0.6 and 1.6 a local shortage could lift the home price
60% over the import price, and the CPI with it)
```

Every good (traded or not) also stays within `[0.25, 4] × its W$ worth`
(`base price / e`), a guard rail for power and for nations cut off from trade.
**Planned economies** move prices at `planned_price_speed` (0.004) and never
above 1.3× the plan price, which is each good's W$ worth in local money;
their shortages show up as unmet demand and black markets, not as prices.

### CPI and inflation

```
CPI         = 0.30 food + 0.25 consumer goods + 0.05 fuel + 0.05 power   (home prices against day 1's)
              + 0.35 services (the wage against day 1's)
inflation π = the change in CPI over the last 12 months (smoothed a little)
recent π    = the change over the last 3 months, at a yearly rate (smoothed)
```

The CPI's history starts at day 1's own CPI (day 1 may price some made goods
at their cost, section 9), so a level isn't counted as a year of inflation.

Wages are struck in W$, so the CPI rises when the currency falls or goods
get scarce at home, not because wages chase themselves.

---

## 7. Money and the state

### Money

Each nation has a **money supply** `M` (70% of GDP on day 1). It grows by:

- bank lending: `M · ((credit_growth_base + credit_slope · (r_neutral − r)) / 360 + real growth trend)`
  (at the neutral rate, the nation's own real growth plus 2% a year, the
  target inflation; less when rates are high);
- money the central bank **prints** to pay a deficit nobody will lend for.

(With a fixed 3% at the neutral rate, money outran output wherever growth was
slow, and rich nations near the frontier settled at 4-6% inflation.) Money
growing faster than that lifts the prices of what isn't traded
(`pass_through · (money growth − 2% − real growth)`, smoothed, capped).

### Exchange rates

The currency is worth what its money buys:

```
target e = day 1's e · world price index · capacity / capacity on day 1 · M_day1 / M · exp(tilt)
capacity = labour force · tech                      (what the nation can make)
tilt     = clamp(fx_trade · trade balance / GDP + fx_rate · (r − world r)
                 − fx_slump · max(0, u − slump line), ±0.3)
e moves fx_ppp = 1% of the way (in logs) a day, at most 1% a day
```

**Printing money or cheap credit weakens the currency**; a high interest rate
slows lending and strengthens it; a trade surplus helps; in a slump the
central bank lets it go (devalues, `fx_slump` = 1 per point of unemployment
over the slump line). Traded goods' home
prices follow `e` through parity, so a falling currency is inflation at home.

### The budget, each day (local money)

| | |
|---|---|
| **Revenue** | income tax · wages (less the black market share) + corporate tax · profits + sales tax · household purchases + tariffs + tribute in |
| less | corruption: `corruption · revenue` skimmed (half comes back as households' spending) |
| **Spending** | army pay (`army_pay` = 1.2 × wage) + training fuel and ammo + tank and jet upkeep |
| | welfare and pensions; government wages |
| | infrastructure upkeep (each road, rail and port level) and new infrastructure |
| | state building (planned and mixed economies, and the player's orders) |
| | weapons bought into the stockpile (`procurement` by Army Strength, a share of GDP a year) |
| | interest: `debt · bond_rate / 360` |
| | colony tribute: `tribute` (10%) of revenue to the overlord |

Upkeep, building and infrastructure money is spent at home (it becomes
services and construction work), so money is never lost from the economy.

### Deficit, debt and credit rating

When the treasury runs dry, the state **borrows** (bonds bought from
household savings) up to its rating's debt ceiling; past the ceiling the
central bank **prints** the rest. A treasury with more than 4% of GDP repays
debt. The rating is set monthly:

```
score = 100 − 60 · debt/GDP − 300 · max(0, deficit/GDP) − 80 · max(0, π − 0.05)
        + 0.2 · (stability − 50) + rating modifiers
AAA ≥ 85 (ceiling 120% of GDP), AA ≥ 75, A ≥ 65, BBB ≥ 55, BB ≥ 45, B ≥ 35,
CCC ≥ 20, else D (ceiling 5%)
bond rate = r + premium(rating)       (0.5% for AAA up to CCC's and D's much more)
```

**Default.** A state rated D that pays over 5% of GDP a year in interest stops
paying: half its debt is written off, it loses 10 stability, and the news
says so. Without it, a D rating's premium made the interest the deficit. The
deficit was printed, and the inflation and deficit kept the rating at D: a
spiral with no way out (one nation printed for 30 years).

### Central bank

```
π_seen = (1 − taylor_recent) · π + taylor_recent · recent π          taylor_recent = 0.5
slack  = max(0, u − u_natural) · clamp((taylor_u_ceiling − π_seen) / (taylor_u_ceiling − π_target), 0, 1)
r* = r_neutral + 1.5 · (π_seen − π_target) + 0.5 · output gap − taylor_u · slack
     r_neutral 3%, π_target 2%, taylor_u = 1, taylor_u_ceiling = 4%
r moves at most 0.05 points a day towards r*, between 0 and 40%
```

The bank cuts for joblessness as well as for low inflation. That easing fades
out as inflation climbs from 2% to 4%, so a long slump can't buy chronic
inflation. (With the full easing, nations that stayed slack settled at 6-8%,
and being big, lifted the world's average with them.) It watches the last
three months as well as the year: steering by the year's inflation alone, it
eased for half a year after a deflation had turned, then chased the
overshoot.

The player can fix the rate instead (the budget page).

### Fiscal autopilot (NPCs, and the player until they set taxes)

Monthly: a deficit over `deficit_target` (3% of GDP) raises income tax a step,
sales tax half a step and cuts welfare a point; a surplus over 2% cuts taxes;
debt over 90% of GDP and printing money both force austerity.

**In a slump** (unemployment over the **slump line**, `1.5 × natural + 5%`)
it does the opposite: it cuts taxes and lifts welfare back towards normal,
borrowing while its rating allows, whatever the deficit. Only printing that
has pushed inflation past `slump_inflation_limit` (8%) makes taxes rise, one
step a month. (Austerity in a slump deepened it for good: taxes went to
52-55% and stayed there, spending halved, and the services that employ most
people went with it.)

---

## 8. The world market and trade

One **world price** per traded good, in W$. Each day every nation works out
what it wants to trade:

```
home cover  = days_of_cover · use rate
offer       = (stock − home cover) · 0.15 · clamp(ep / p, 0.5, 2)    if stock > home cover (and exports allowed)
bid         = (home cover − stock) · 0.30 · clamp(p / ip, 0.5, 2)    if stock < home cover (and imports allowed)
              but at most 2 · (use rate − made today) + 5% of the use rate
```

A nation with a shortage always bids and one with a glut always offers;
prices set how keenly. A bid is capped by what the nation lacks a day, so a
short world is shared by need, not by how big a stock a nation wants to
rebuild. (By the refill, one giant topping up its month took the world's
food, and its tiny neighbours starved.)

**Trade capacity** caps the weight a nation can move:

- **Coastal:** `Σ port levels · port_capacity`;
- **Landlocked:** overland only, `Σ (road + rail) of border provinces · land_capacity`;
- plus any **old trade routes** day 1 needed (section 9).

What a nation needs comes in first; its surpluses share what capacity is
left. Before ports are shared out, each bid is clipped to what the rest of
the world offers, and each offer to what it bids. A bid for something nobody
sells mustn't take up port room that what is on sale could use: one nation's
huge unfilled bids for consumer goods squeezed its iron-ore bid to a fifth
while its port stood 97% empty.

The market clears pro rata over the offers each buyer may take
(**embargoes** block pairs, either way round). The world price moves with
the **world's stocks against the world's cover**, as home prices do, and
slowly back to its anchor (base × any shock):

```
pull   = clamp((Σ stock targets − Σ stocks) / max(Σ stock targets, Σ stocks), −1, 1)
Δ ln W = world_price_speed · pull + world_price_pull · ln(anchor / W)
         world_price_speed = 0.0008, world_price_pull = 0.01: at most about 8% from the anchor
W stays within world_price_band = [0.6, 1.7] × anchor
```

Bids against offers was biased: offers are a share of a stock, bids about a
day's need, so in a calm world every price sat 6-7% under its anchor and slid
there together at the start, a world-wide deflation. Holding the average of
all traded prices fixed is gone too: each price is already pulled to its own
anchor, and the renormalising made a glut of ore and coal lift food and
consumer goods, and the CPI with them.

**Capital flows.** A trade deficit is mostly financed from abroad:
`capital_flow` (80%) of it comes back as spending at home. A surplus nation
lends that share of its surplus out of its households' savings. (Without it,
money spent on imports was nobody's income: import-heavy nations' services
shrank day after day, and a third of their people were out of work for
decades.) The exchange rate still corrects imbalances slowly.

### Trade costs

| | Share of value | Per weight unit |
|---|---|---|
| Sea (coastal) | `sea_cost_share` (3%) | `sea_cost_weight` (W$15) |
| Overland (landlocked) | `land_cost_share` (9%) | `land_cost_weight` (W$40) |
| Landlocked transit fee | `transit_fee` (4%) | |

An importer pays `world price · (1 + transport)` plus its tariff (to its own
treasury); an exporter gets the world price less transport. **Landlocked
nations pay more** (the test `landlocked_nations_pay_more_to_trade`).

---

## 9. Every wheel result's economic job, and day 1

After day 1, **GDP is calculated every day** from the real economy:

```
GDP (local, a day) = Σ value added of buildings + services spending + state wages + trade margins
GDP (W$, a year)   = GDP_local · e · 360   (smoothed)
real GDP           = GDP / CPI
```

| Wheel | Economic job |
|---|---|
| Continent | Its provinces, terrain and deposits; continent climates (`[continent_tag]`) |
| Country Size | How many provinces: deposits, room for buildings, transport |
| Landlocked / Coastal / Peninsula / Island | Ports or overland trade; trade costs and capacity |
| Land Type | More output of its raw goods (`[land.*]`) |
| Population | The workforce and the demand |
| Human Development | Skilled share, tech, natural unemployment, birth rate |
| Ideology | The **economic model** (below), taxes, government size, corruption |
| History | Starting infrastructure, debt, damage and credit |
| GDP | The starting point: day 1 is fitted so its GDP is the wheel's. **Tiny but Rich!** is sized per person ($100k to $200k a head, like Monaco), not as a fixed sum: a town of 50,000 isn't handed a $300 billion economy |
| Main Resource | Doubled deposits of that good, its mines and wells, and an export of about 4% of GDP |
| Army Strength | The starting stockpile (5 years of buying) and how much the state buys a year |
| Active Duty | Soldiers: out of the workforce, paid by the state |
| Secret Service | Fights corruption and the black market |
| Leader | Builder: cheaper building; Genius: more tech, fewer defects; Warlord: more weapons; Diplomat: cheaper trade, better rating; and so on |
| Regional Impact | Trade capacity and costs, credit rating |
| Blessing or Curse | Fertile Lands boosts farms; Corrupt Court leaks money; Endless Drought hurts grain; and so on |

Every number is in `effects.toml` as **modifiers**: `output.<good>`,
`output.raw/refined/final/all`, `defect`, `build_cost`, `trade_cost`,
`transport`, `port`, `army_upkeep`, `procurement`, `demand`, `investment`,
`tech` and `event.<id>` multiply; `corruption`, `black_market`,
`stability`, `skill`, `rating`, `growth`, `infrastructure`, `debt`,
`treasury` add. All sources stack; events add timed modifiers on top.

### Economic models

| Model | Ideologies | What changes |
|---|---|---|
| market | Democracy | Free prices; private firms build where profit is; pro-rata inputs; small state |
| planned | Communism | The state builds what is short; plan priority for inputs; price controls; quotas follow stocks, not profit; shortage and black market risk |
| mixed | Monarchy, Military Rule, Theocracy, Imperialism, Authoritarianism, Tribalism | Private and state building; pro-rata inputs |
| colony | Colony | Market rules, and `tribute` (10%) of revenue to its overlord, the strongest non-colony on the map |

### Day 1

Day 1 must add up, or the first months are a crash. For each nation:

1. **Endowment.** Work back through the chains from what the nation needs:
   food for everyone, power, fuel and consumer goods for its standard of
   living, weapons for its army, and its Main Resource export. Each building
   is sized for its output (with skill, defects, modifiers and the stock that
   spoils). Raw buildings go on the richest deposits; farms may use poorer
   land if grain is short. Industry spreads over the provinces with people.
2. **Two fits, together** (up to 25 rounds): **tech** (`tech_cal`), so that at
   the wage day 1's GDP needs, buildings pay `labour_share` (60%) of what they
   add in wages; and **standard of living** (`need_cal`), so that goods,
   power and fuel take what households have left after services.
3. **The world's raw goods**, in three passes. Whatever nations can't dig at
   home goes, in order:
   - to others' spare good deposits (at most 10% of their GDP each);
   - if still short, to poorer deposits worked twice as hard, with twice the
     allowance;
   - if still short, back to the buyers, who dig it at home on any deposit
     they have.

   (Without the last two passes, a world could start a third short of its ore
   or oil. The buyers lived on a month's stock, then their power and steel
   failed and their prices soared.)
4. **Transport and ports.** Rail and roads so every province moves 130% of
   its output. Ports (or, landlocked, border rail then roads, each up to 12
   levels) carry `port_headroom` (1.6) times day 1's trade. Whatever no port
   or crossing could ever carry, a crowded city-state for instance, goes by
   **old trade routes** (`trade_routes`, kept for good). (A big grain buyer's
   port was 90% full on day 1; when grain imports grew, they crowded out the
   iron ore its steel needed.)
5. **Day 1 prices.**
   - A raw good a nation buys in starts at its import parity; one it digs for
     the world starts at its export parity.
   - No made good starts below what it costs to make on day 1.

   Both are where trade and the cost pull would take them anyway, and getting
   there showed up as a year of inflation in every big importer.
6. **The accounts.** Everyone not in the army, the state or a building works
   in services, but for natural unemployment. The wage then follows from GDP:
   `w = (GDP − goods value added) / ((1 + markup) · services workers + state workers)`,
   but never more than 80% of what the buildings add (`can_pay`). Household
   wealth starts where saving and spending from wealth balance. Five years of
   weapons go in the stockpile. Stocks are a month of home use, plus:
   - for an exporter, its exports over the offer share (a week's worth);
   - for an importer, less its imports over the bid share (about three days'
     worth).

   So trade flows from day 1. (With full stocks nobody bid for months, the
   exporters' stocks piled up, they dug less, and when the bids came there
   was too little ore for steel.)

---

## 10. Luck and rot

### Events

Each nation draws events each day: `chance a year / 360 · event.<id>
modifiers`. Booms (10% a year), crashes (6%), discoveries (8%, a rich new
deposit), bad harvests (12%, a fifth of the grain lost for five months), factory fires (20%, 20 to 50% of a factory's
levels), strikes (10%), earthquakes (2%, roads and rail damaged), technical
breakthroughs (4%), and the world's oil shocks (4%, the oil price × 1.6 for a
year). News of the player's nation and the world pops up on the map with a
sting.

### Corruption and the black market

```
corruption c = base(model) + modifiers (Secret Service, Leader, Blessing...)   (0 to 0.6)
black market b = clamp(0.3 · worst shortage + 0.5 · max(0, tariff − 0.2) + 0.3 · c + modifiers, 0, 0.6)
```

`c` of state revenue is skimmed (half of it comes back as spending by the
people who took it); `b` of income and sales tax goes unpaid. Smuggling goods
across borders is left for a later phase.

---

## 11. Autopilot (every NPC, and the player's defaults)

NPC brains come later. Until then every economy runs on an autopilot, which
also keeps the world market alive:

- **Central bank:** the Taylor rule. **Taxes:** the fiscal autopilot.
- **Utilisation** (daily) follows a damped **supply signal** for each
  storable good:

  ```
  signal = clamp(gap + supply_damping · (share of demand the stock lost today), −1, 1)
           gap against the stock target (section 6); supply_damping = 8
  firm earning money, or short:    utilisation += 0.5% · signal
  firm losing money, stocks full:  utilisation −= 0.5% · min(1, 0.2 − signal)
  planned firm:                    utilisation += 1% · signal
  power (made to order):           by profit, as before
  ```

  (Profit alone, with prices following stocks, made farms and food plants
  run a cycle of a year or so, and food prices and the CPI with them. A
  thin loss with stocks near target only trims output: full cuts there
  starved the next link of the chain.)
- **Investment** (monthly):
  - private firms (market and mixed) spend up to 15% of savings (less when
    rates are high) on the best returns above `r + 8%`;
  - the state (planned and mixed) builds what is short, basic goods first;
  - nobody builds where the stock is already a glut, or more of a kind whose
    buildings already stand idle for want of inputs;
  - nobody builds a good the nation could already make (counting what is
    being built) more than `invest_margin` (1.15) times what it uses and
    sells abroad, unless it is short. Building costs are small next to a
    level's profit, so without this, firms chased every price spike: a
    nation grew one province's consumer-goods plants ninefold in a year,
    then its steel mills twentyfold, and both chains starved;
  - building materials come out of what the nation holds beyond three
    quarters of its usual stock, half of that at a time. (Half the whole
    stock at a time let a building boom eat the steel factories needed.)
- **Closures:** a building losing money for 90 days loses 10% of its levels,
  but only surplus capacity closes: never what the nation is short of, nor
  what it makes no more than it uses and sells, with a fifth to spare. (One
  colony shut its only food plant, then starved while exporting its grain.)
- **Infrastructure:** roads, then rail, in the three most choked provinces; a
  port when trade capacity is 85% used; for a landlocked nation, a level of
  border rail in up to three border provinces when its crossings are 85%
  used, or when damage has worn them below 80% of day 1's (whatever its
  budget: cut off, one micro-state had no trade for decades). Not in a deep
  deficit otherwise.
- **Trade:** surpluses offered and shortages bid at world prices.
- **Fighting a slump** (unemployment over the slump line, `1.5 × natural +
  5%`). Bad luck can hurt, but it shouldn't be permanent, so every tool works
  at once:
  - **rates come down** (the Taylor rule's `taylor_u`, section 7);
  - **the currency is let go** (`fx_slump`, section 7);
  - **taxes come down and welfare goes back up**, borrowed for (section 7);
  - **public works** take on `public_works` (1.0) times the jobless over 5
    points under the slump line (those already on the works counted as
    jobless, so the scheme doesn't shrink because it works). They are capped
    at `public_works_max` (10%) of the workforce, pay `public_works_pay`
    (0.6) of the wage, ramp 1% a day, and wind down as market jobs come
    back. (Uncapped and at a full wage, they once took a third of one
    nation's workers, paid for by printing.);
  - **retraining** adds `retraining` (2 points) a year to the skilled share,
    up to `retraining_max` (6 points), at a tenth of a wage per jobless
    person, fading at half that rate after the slump;
  - **informal work** absorbs the rest (section 4), and populations stop
    outgrowing their jobs (section 5).

The player's economy runs on the same autopilot until the player takes a
lever (their taxes, the rate, buildings, trade).

---

## 12. What the player can do

On the map, **O** (or the ECONOMY button) opens the economy window:

| Page | Shows | Does |
|---|---|---|
| Budget | Taxes, welfare, tariff, interest rate; currency, debt, rating, treasury, money printed; last month's revenue and spending as bars | Left/Right or −/+ change a lever; **A** hands taxes back to the autopilot and the rate back to the bank |
| Chains | The 16 goods in four columns (raw, power, refined, final), lines from inputs to outputs, each box green / amber / red by how much of what was wanted was got; the chosen good's makers, workers, inputs and profit; what is being built | **B** builds a block of its maker (10% more, at least 2 levels) at the best place, paid by the treasury; **X** closes a block |
| Market | Every good: home price in W$ against the world price, stock in days, made, used, bought and sold | **T**: free trade → no exports → no imports |
| Trade | Exports, imports, balance, capacity used, transport costs, the top goods; every partner by trade | **X** embargoes the chosen nation, or lifts it |
| Stockpile | Ammo, tanks and jets in store and their worth; army size; spending on weapons and upkeep | |
| Charts | GDP, inflation, unemployment and a chosen good's price against the world, weekly since day 1 | Up/Down chooses the good |

**V** colours the map by **resources** (each province's richest deposit) or
**industry** (what its buildings make a day). Clicking a province shows its
people, buildings, roads, rail and port, and deposits.

---

## 13. One daily tick, step by step

1. **World events** (an oil shock moves a world price's anchor).
2. For each nation:
   1. **Events**: old ones wear off; construction goes on and finishes; new
      events are drawn; modifiers are rebuilt if events changed.
   2. **Labour**: army, government, buildings (in hire order), services; the
      rest are unemployed.
   3. **Transport factors** from recent tonnage.
   4. **Production by tier**: food's needs held back, power made to order,
      inputs shared (pro rata or by plan), defects scrapped.
   5. **Households**: income, taxes, saving; buy food, power, fuel, goods and
      luxuries; services spending.
   6. **The state buys**: training fuel and ammo, weapons for the stockpile.
   7. **Spoilage**; **home prices** move.
3. **World market**: offers and bids (bids capped by need, both clipped to
   what the world offers and bids), capacity, pro-rata clearing, embargoes,
   transport and tariffs; world prices move with world stocks; capital flows
   finance trade deficits.
4. For each nation again:
   1. **Firms' accounts**: profits, tax, retained profits, unit costs,
      supply signals and utilisation.
   2. **Wage** (in W$), service pay; **fighting a slump** (public works,
      retraining).
   3. **Budget**: corruption, upkeep, interest, tribute; borrowing, printing
      or repaying.
   4. **Money**: lending, printing, drift; **CPI** and inflation.
   5. **Central bank**, **exchange rate**, the local wage.
   6. **Technical progress**, **happiness, stability, population, HDI**.
   7. **GDP**; a chart sample every 7 days.
5. Every 30 days: the month's books, **ratings (and default), fiscal
   autopilot, investment, closures, infrastructure, hiring order**.

---

## 14. What the tests hold the economy to

`cargo test --no-default-features econ::`:

- **The same seed gives the same economy**, every number, two years in, on
  all three maps (and another seed gives another one).
- **No NaN and no negative stock**, price, treasury or debt, every nation,
  every day, for five years on all three maps.
- **Goods are never made from nothing**: every day, every nation and good,
  `stock = stock before + produced + imported − used − eaten − exported −
  spoiled − stockpiled`, to rounding; the world's exports equal its imports.
- **A broken chain stops everything after it**: no bauxite, no aluminium, no
  jets; power still flows.
- **Landlocked nations pay more to trade**, by the rules and in practice.
- **Default nations neither spiral nor collapse in 50 years**, on all three
  maps: prices neither rise nor fall over 60% in year 50, no GDP below a
  fifth of its start, no more than 60% out of work, most people still there,
  and the world richer than it started.
- **The balance rules hold for 50 years on six seeds**
  (`the_balance_rules_hold_for_fifty_years_on_six_seeds`): every map with
  seeds 1 to 6 (18 worlds, one thread each), watched every month:
  - no nation above **$300,000 GDP per head**;
  - no nation above **25% unemployment for more than 5 years** straight;
  - **world inflation** (GDP-weighted) **between −3% and 8%** at every year's
    end;
  - **never more than 2 nations above 15% inflation** at once;
  - and no nation below a fifth of its starting GDP at the end.

The same rules run from the command line on any seeds:

```
cargo run --release --no-default-features -- --economy-health 50 1 2 3 4 5 6 7 42 [--why]
```

It prints one line per world and `N of M worlds broke a rule`; `--why` names
who did it (the nations over 15% at the worst moment, and those that weighed
most in the worst year of world inflation). `--economy-world MAP SEED MONTHS`
prints the world month by month (world and median inflation, unemployment,
rates, money growth, world prices); `--economy-trace ... --cpi` takes one
nation's CPI apart.

### The balance pass, before and after

`--economy-health 50` on every map with seeds 1 to 7 and 42 (24 worlds, 600
nations, 50 years each), before this pass and after it:

| | Before | After | Unseen seeds 8-15, after |
|---|---|---|---|
| Worlds breaking a rule | 24 of 24 | **0 of 24** | **0 of 24** |
| Highest GDP per head | $62.1M (median world $674k) | **$262k** (median $129k) | $280k |
| Worlds with a nation over $300k a head | 21 | **0** | 0 |
| Longest spell over 25% unemployment | 49.7 years | **0.8 years** | 1.7 years |
| World inflation, every year's end | −53.9% to +37.4% | **−2.3% to +7.7%** | −1.8% to +6.8% |
| Most nations over 15% inflation at once | 25 (all of them) | **1** | 1 |
| Nations collapsed below a fifth of their GDP | 5 | **0** | 0 |

The cases that started it, at year 50:

| Map, seed | Nation | Before | After |
|---|---|---|---|
| five_shores 7 | Zanada Federation | 87k people, $9.04M a head | 122k people, $170k a head, 0.6% unemployed |
| isles 7 | the world | 23 of 25 nations at 10-24% inflation, world 15% | every nation between 0.9% and 6.0%; world −0.9% to 5.3% in every year |
| grand 7 | State of Iskoheim | 40-46% unemployed, CCC | 0% unemployed, AAA, 3.5% growth a year |
| grand 42 | Holy Skarmark | 49.6 years over 25% unemployment, CCC | 0.2% unemployed, AAA, GDP $34B to $314B |

What drove them, and the fixes (each is described in its section above):

1. **Micro-states at millions a head.**
   - Tiny but Rich was a fixed sum, not per person. It is now $100k to $200k
     a head (section 9).
   - Technical progress compounded for ever. It now converges on a $200k
     frontier and runs backwards above it (section 3).
   - Services profits paid out and spent again inflated small economies with
     nothing more made. Services are now capped at what the people free to
     do them could deliver (section 5).
2. **World inflation waves.**
   - World prices sat biased low and moved all together: a weak pull, bids
     set against stocks, and the whole basket renormalised. They now follow
     world stocks, pulled to their own anchors (section 8).
   - Day 1 wasn't a steady state. Ore and oil were short world-wide, ports
     too small, importers' stocks full, prices and wages off their
     equilibrium. Day 1 now balances raw goods, ports, stocks and prices
     (section 9).
   - Production and investment ran hog cycles: profit-only steering, building
     chasing price spikes, construction eating the steel buffer. They now
     follow damped supply signals and a demand-gated build rule (section 11).
   - Monetary policy overshot. Lagging inflation, fixed credit growth and a
     slump easing that bought chronic inflation now give way to a blended
     inflation gauge, credit with real growth and a capped easing
     (section 7).
   - Debt spirals came from a D rating's premium. Default now ends them
     (section 7).
   - A 40% bad harvest in a giant: now 20% (section 10).
3. **Permanent collapse.**
   - Imports were nobody's income. Capital flows now finance deficits
     (section 8).
   - Spending leaked as services shrank. Informal work and public works now
     absorb the jobless (sections 4 and 11).
   - Austerity in slumps: the fiscal autopilot now cuts taxes in a slump
     (section 7).
   - Plants were closed until a nation starved. Only surplus capacity closes
     now (section 11).
   - Giants' restocking bids took the world's food. Bids are now capped by
     need (section 8).
   - A landlocked micro-state was cut off by damaged crossings. They are now
     rebuilt (section 11).
   - Populations outgrew their jobs. Growth now responds to unemployment
     (section 5).
4. **Tighter tests**: the balance test above.

`--economy-report 50` prints the 50-year table for all three maps
([docs/economy_report_50y.txt](economy_report_50y.txt) has seed 1). Known
weak spot: big poor nations can carry 15-20% unemployment for years (under
the 25% line). Informal work and public works keep it there, rather than
markets clearing it (see ISSUES.md).
