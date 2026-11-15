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
                      progress grows tech_growth = 1.2% a year
staffing         st = hired / (L · utilisation · 1000) · utilisation
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

1. **Buildings**, up to `levels · utilisation · 1000`. Market and mixed
   economies hire for food, grain and power first, then by profit. Planned
   economies hire food, grain and power first, then down the chains (raw
   before refined before final), in plan order within a tier.
2. **Services**: as many as services spending pays for,
   `services → spending / (wage · (1 + services_markup))`, moving 2% a day.
3. Everyone else is **unemployed**: `u = unemployed / (L − A)`.

### The wage

Wages are **struck in world dollars**. Each nation's W$ wage `w$` moves 0.3%
a day of the way (in logs) towards

```
base       = day 1's W$ wage · tech / day 1's tech
can_pay    = labour_share · value added of buildings (W$) / worker-equivalents   (smoothed)
target     = base^0.75 · can_pay^0.25 · (1 + wage_tightness · tight)
tight      = clamp((u_natural − u) / u_natural, −1, 1)       wage_tightness = 0.25
u_natural  = 12% at HDI 0.3 down to 5% at 0.95
w$ stays between 0.25 and 4 times base
local wage w = w$ / e
```

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
                − birth_unemployment · max(0, u − 0.2) + growth modifiers
birth(HDI)    = 2.2% at HDI 0.3, 1.2% at 0.6, 0.2% at 0.95
HDI target    = day 1's HDI + 0.04 · ln(income per head / day 1's)
HDI moves 2% a year towards it
```

Hunger and joblessness both slow births. HDI follows income gently from its
start: richer means more tech and more tech means richer, so a strong pull
would feed on itself.

---

## 6. Prices

Every good has a **home price** `p` in local money. Each day:

```
target    = days_of_cover (30) · use rate      (use rate: 33-day average of what was wanted)
gap       = (target − stock) / max(target, stock)
shortage  = (wanted − got) / wanted
x         = gap + 0.5 · shortage           (power: shortage − 0.5 · share made but unused)
Δ ln p    = clamp(price_speed · x, ±price_cap)      price_speed = 0.015, price_cap = 0.02
```

**Trade parity** ties a traded good's home price to the world:

```
import parity ip = world price · (1 + trade cost + tariff) / e
export parity ep = world price · (1 − trade cost) / e
above ip: Δ ln p −= 0.08 · ln(p / ip);   below ep: Δ ln p += 0.08 · ln(ep / p)
and p always stays within [0.6 · ep, 1.6 · ip]  (traders step in)
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
```

Wages are struck in W$, so the CPI rises when the currency falls or goods
get scarce at home, not because wages chase themselves.

---

## 7. Money and the state

### Money

Each nation has a **money supply** `M` (70% of GDP on day 1). It grows by:

- bank lending: `M · (credit_growth_base + credit_slope · (r_neutral − r)) / 360`
  (3% a year at the neutral rate; less when rates are high);
- money the central bank **prints** to pay a deficit nobody will lend for.

Money growing faster than real output lifts the prices of what isn't traded
(`pass_through · (money growth − 3% − real growth)`, smoothed, capped).

### Exchange rates

The currency is worth what its money buys:

```
target e = day 1's e · world price index · capacity / capacity on day 1 · M_day1 / M · exp(tilt)
capacity = labour force · tech                      (what the nation can make)
tilt     = clamp(fx_trade · trade balance / GDP + fx_rate · (r − world r), ±0.3)
e moves fx_ppp = 1% of the way (in logs) a day, at most 1% a day
```

**Printing money or cheap credit weakens the currency**; a high interest rate
slows lending and strengthens it; a trade surplus helps. Traded goods' home
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

### Central bank

```
r* = r_neutral + 1.5 · (π − π_target) + 0.5 · output gap      r_neutral 3%, π_target 2%
r moves at most 0.05 points a day towards r*, between 0 and 40%
```

The player can fix the rate instead (the budget page).

### Fiscal autopilot (NPCs, and the player until they set taxes)

Monthly: a deficit over `deficit_target` (3% of GDP) raises income tax a step,
sales tax half a step and cuts welfare a point, **unless the nation is in a
slump** (unemployment above 1.8 × natural + 3%): then the deficit is borrowed
and taxes may even come down. A surplus over 2% cuts taxes; debt over 90% of
GDP and printing money both force austerity.

---

## 8. The world market and trade

One **world price** per traded good, in W$. Each day every nation works out
what it wants to trade:

```
target      = days_of_cover · use rate
offer       = (stock − target) · 0.15 · clamp(ep / p, 0.5, 2)    if stock > target (and exports allowed)
bid         = (target − stock) · 0.30 · clamp(p / ip, 0.5, 2)    if stock < target (and imports allowed)
```

A nation with a shortage always bids and one with a glut always offers;
prices set how keenly. **Trade capacity** caps the weight it can move:

- **Coastal:** `Σ port levels · port_capacity`;
- **Landlocked:** overland only, `Σ (road + rail) of border provinces · land_capacity`.

What a nation needs comes in first; its surpluses share what capacity is
left. The market clears pro rata over the offers each buyer may take
(**embargoes** block pairs, either way round). The world price moves with
the imbalance and slowly back to its anchor (base × any shock), and the
average world price level is held steady (the W$ is the world's unit):

```
Δ ln W = world_price_speed · (bids − offers) / max(bids, offers) + 0.004 · ln(anchor / W)
then all traded world prices are scaled so their average (in logs) against the anchors is 0
```

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
| GDP | The starting point: day 1 is fitted so its GDP is the wheel's |
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
3. **The world's raw goods.** What nations can't dig, others with spare
   deposits and workers dig for the world (at most 10% of their GDP each).
4. **Transport.** Rail and roads so every province moves 130% of its output.
5. **The accounts.** Everyone not in the army, the state or a building works
   in services, but for natural unemployment. The wage then follows from GDP:
   `w = (GDP − goods value added) / ((1 + markup) · services workers + state workers)`.
   Household wealth starts where saving and spending from wealth balance.
   A month of stock of everything, and five years of weapons in the stockpile.

---

## 10. Luck and rot

### Events

Each nation draws events each day: `chance a year / 360 · event.<id>
modifiers`. Booms (10% a year), crashes (6%), discoveries (8%, a rich new
deposit), bad harvests (12%), factory fires (20%, 20 to 50% of a factory's
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
- **Utilisation** (daily): a market firm losing money runs 0.5% less a day
  (down to 20%), unless the nation is short of what it makes; one earning
  money runs 0.5% more, unless its good is piling up. Planned firms follow
  stocks: less when stocks are over twice the target, more when below it.
- **Investment** (monthly):
  - private firms (market and mixed) spend up to 15% of savings (less when
    rates are high) on the best returns above `r + 8%`;
  - the state (planned and mixed) builds what is short, basic goods first;
  - nobody builds where the stock is already a glut, and a kind grows by
    about what is missing, not by leaps.
- **Closures:** a building losing money for 90 days loses 10% of its levels,
  unless the nation is short of what it makes.
- **Infrastructure:** roads, then rail, in the three most choked provinces;
  a port when trade capacity is 85% used; not in a deep deficit.
- **Trade:** surpluses offered and shortages bid at world prices.

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
3. **World market**: offers and bids, capacity, pro-rata clearing, embargoes,
   transport and tariffs; world prices move and the average is held.
4. For each nation again:
   1. **Firms' accounts**: profits, tax, retained profits, utilisation.
   2. **Wage** (in W$).
   3. **Budget**: corruption, upkeep, interest, tribute; borrowing, printing
      or repaying.
   4. **Money**: lending, printing, drift; **CPI** and inflation.
   5. **Central bank**, **exchange rate**, the local wage.
   6. **Technical progress**, **happiness, stability, population, HDI**.
   7. **GDP**; a chart sample every 7 days.
5. Every 30 days: the month's books, **ratings, fiscal autopilot,
   investment, closures, infrastructure, hiring order**.

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

`--economy-report 50` prints the 50-year table for all three maps. Across
four seeds and three maps (300 nations), the world grows 1.3% to 2.8% a year
with no hyperinflation. Known weak spots: a few microstates (a few thousand
people) can stall, and big poor nations with fast-growing populations can
carry high unemployment for years (see ISSUES.md).
