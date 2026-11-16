# LUCKY WORLD: the NPC brains

Every nation the player doesn't run has a **brain**. It reads its own country
(fully), the world market (public) and the other nations (through its
secret service, with errors). It decides what its government does, using
**exactly the levers the player has**. If the NPCs are dumb the game isn't
worth playing, so the brains have to be smart and fair, and the numbers
below show that they beat the phase 3 autopilot.

This is the design, with the math, as built. Every weight and threshold
lives in a data file, so the brains can be tuned without a rebuild:

| File | What it holds |
|---|---|
| `data/ai/brain.toml` | Timing, the considerations (needs), goals, every action's effects, scoring, flip-flop guard, difficulty, intel, power, threat, memory and opportunity numbers |
| `data/ai/personality.toml` | What each wheel result does to a brain: leader (style), ideology (goals and limits), history, regional impact, blessing or curse |

The brains live in `src/ai/`; nothing there draws. They are
**deterministic**: the same world and seed give the same decisions.

---

## 1. Three layers

```
every day        economy.tick()  (the same rules for every nation)
                 brains.day()    ┌─ strategy  every strategy_days (90), staggered by nation
                                 ├─ planning  every planning_days (7), staggered by nation
                                 └─ battles   every day: a hook, no orders yet
```

| Layer | Asks | Produces |
|---|---|---|
| **Strategy** (every 3 months) | Am I safe? Who is my biggest threat? Is anyone too strong? Who is weak? What is my goal? | the goal (grow, get rich, dominate, survive), threat, a counterweight target, opportunities, alliance wishes |
| **Planning** (weekly) | What helps my goal most right now? | up to `max_actions` actions on the budget, taxes, rate, building and closing, roads and ports, trade |
| **Battles** (daily) | (war phase) | `BattleOrder`s; today always none |

Nations are staggered (nation `i` thinks on days where `(day + i) % period ==
0`), so 25 brains never all think on the same day.

---

## 2. What a brain may know (fair play)

| Known exactly | Known with errors (the secret service) | Never known |
|---|---|---|
| Everything about its own country | Others' GDP, soldiers, weapons stockpile, unemployment, stability, inflation, growth | Others' treasury, savings, buildings, stocks, plans and brains |
| World prices, the news, credit ratings, embargoes (public) | | |
| Its own trade with each nation | | |

Every three months each brain re-estimates every other nation (`strategy::observe`):

```
estimate = truth · exp(ε),   ε ~ Normal(0, error(secret service))
error:  none 0.30 · amateur 0.20 · competent 0.12 · feared 0.07 · legendary 0.04
```

`ε` comes from the brain's own random stream (seeded from the world seed
and the nation), so a replay is the same. Strategy reads only these
estimates and public facts; planning reads only the brain's own nation and
world prices. The test `npcs_never_use_information_they_shouldnt_have`
changes every hidden number of the other nations and checks that nothing
the brain thinks or does changes.

**Same rules.** A brain acts only through the functions behind the
player's buttons:
- `pull_with` (a lever), `free_rate`;
- `player_build`, `player_close`;
- `player_road` on `most_choked`, `player_port` on `smallest_port`;
- `set_trade_policy`, `toggle_embargo`.

A brain's nation is marked `brain`: the autopilot then leaves its taxes,
state building and roads to the brain (as it does for the player). Public
works, retraining and private firms carry on as for everyone.

They cost the same, have the same limits, and take effect through the same
daily simulation. A brain has no bonuses. **Difficulty** is only how well
it plans: how much noise is in its judgement and how far ahead it looks
(section 8).

### New levers (for the player too)

Phase 4 gives the player (and so every brain) four new buttons:
- **Research and schools** (budget page): a share of GDP a year. It raises
  technical progress.
- **Arms purchases** (budget page): a share of GDP a year spent on weapons
  for the stockpile. By default it follows the Army Strength wheel.
- **R: a road** (chains page): a level of road, or rail where roads are
  full, in the province where goods move worst.
- **P: a port** (chains page): a level of port on the coast with the
  smallest one.

Research works with a lag:

```
knowledge += (research share − knowledge) / (research_lag · 360)   each day   (research_lag = 2 years)
extra tech growth = research_effect · ln(1 + knowledge / research_ref) · clamp(gap / 0.5, 0, 1)
                    research_effect = 0.008, research_ref = 0.01
gap = log10(frontier / GDP per head)     (section 3 of ECONOMY.md: none at or above the frontier)
```

So 1% of GDP gives about +0.55% a year of productivity growth, 2% about
+0.9% and 4% about +1.3%. The money is spent at home (teachers, labs), so
it is also demand. The phase 3 autopilot never uses these levers (they
didn't exist), and the player's nation starts at 0% research.

---

## 3. Needs (the considerations)

Planning starts from the nation's **needs**, each between 0 (fine) and 1
(urgent), worked out from its own numbers:

| Need | Formula | Data |
|---|---|---|
| `jobs` | `clamp((u − u_natural) / 0.07, 0, 1)` | `jobs_span` |
| `heat` (prices rising) | `clamp((π − 0.04) / 0.08, 0, 1)` | `heat_from`, `heat_span` |
| `chill` (prices falling) | `clamp((0 − π) / 0.03, 0, 1)` | `chill_from`, `chill_span` |
| `debt` | the largest of: deficit over 2% of GDP (`/0.06`); debt over half its rating's ceiling; how near the rating agencies' score is to junk, `clamp((70 − score) / 25, 0, 1)` with the agencies' own formula; 0.8 if printing money; 0.6 if rated junk (BB or worse). **Emergency**: printing money with a deficit over 8% makes it 1 + 1.5 = 2.5, so it outweighs everything else | `deficit_ok`, `rating_safe`, `emergency`, … |
| `supply` | the worst shortage of food, power, consumer goods, fuel and every input in use, weighted by importance (food 1, power 0.9, inputs 0.7, goods 0.6, fuel 0.5) | `importance` |
| `growth` | `clamp(1 − real growth / 0.03, growth_floor, 1)`: there is always some wish to grow | `growth_ok`, `growth_floor` |
| `support` | `clamp((60 − stability) / 30, 0, 1)` | `support_ok`, `support_span` |
| `security` | the strategy layer's threat level (section 5), or how far arms are under the personality's `arms_floor`; at least `dominate_floor` with the goal *dominate* | |
| `trade` | `clamp((trade used / capacity − 0.8) / 0.2, 0, 1)`, and the worst choked province | |

Each need is multiplied by the personality's weight for it (`P`, section
8). `need_smoothing` can average them over a few weeks; tuning found that
fresh numbers work better, so it is 1 (no smoothing).

---

## 4. Utility: scoring every action

Each week the brain lists every action it could take now (section 6). Each
action `a` has an **effect** on each need `c`, between −1 (makes it worse)
and +1 (relieves it fully), from `[action.*]` in `brain.toml`. Some are
scaled by the situation (a building's effect on supply is the shortage of
what it makes). The score is

```
score(a) = Σ_c  W_goal[c] · weight(c, a) · effect[a][c]      (debt harm × (1 − risk))
           + like[category(a)]                     (personality)
           − flipflop(a)                           (section 7)

weight(c, a) = need[c]                          if effect[a][c] > 0   (relief only counts if needed)
             = max(need[c], harm_floor)         if effect[a][c] < 0   (harm always counts a little)

rank(a)  = score(a) + noise · Normal(0, 1)      noise = difficulty + personality
```

- `W_goal` is the goal's weight on each need (section 5); the needs already
  carry the personality's weights.
- The options are tried in order of **rank**: the brain's own judgement,
  with its misjudgement. An option is taken only if its **score** beats
  **holding still** (`hold_score`, 0.03): noise can make a brain pick the
  second-best of two good options, never act on a bad one. (Tuning found
  that letting noise decide whether to act made brains busier and
  luckier, and made easy brains as good as hard ones.)
- The best of each category (fiscal, money, building, infrastructure,
  trade, military, research) is taken, at most `max_actions` (3) a week.
  Holding still is always an option, so a calm nation does nothing.

Every week's thinking is kept for the spectator panel:
- the goal and why;
- the needs;
- the three best options, each with its score and the two needs that
  weighed most in it ("helps jobs (18% out of work (normal 9%))").

---

## 5. Strategy (every 3 months)

### Power

From the estimates, every nation's power:

```
power_j = gdp_j + army_value · soldiers_j + stockpile_j        (W$; army_value = 60,000 a soldier)
share_j = power_j / Σ power
```

### Threat

How dangerous nation `j` looks to nation `i`:

```
threat_ij = sqrt(share_j / share_i) · proximity_ij · (calm + hostility_ij) · arming_j    calm 0.5
proximity:  land border 1.0 · same continent 0.6 · elsewhere 0.25
hostility_ij = max(0, −regard_ij)                 (memory, section 7)
arming_j = 1 + clamp(growth of j's estimated stockpile over the last year, 0, 1)
threat level_i = clamp((max_j threat_ij − threat_ok) / threat_span, 0, 1)    threat_ok 0.8, span 1.2
```

The `j` with the highest `threat_ij` is the **biggest threat**.

### Balance of power

A nation whose share is over `hegemon_share` (30%), and over `hegemon_lead`
(1.5) times the next one, is a **hegemon**. It may be the player. Every
other brain then:
- adds `hegemon_pressure · share` to its security need, so it arms;
- may shift trade away from it (an embargo is scored like any action,
  weighed against the trade it would lose);
- lists the others threatened by the same hegemon as **alliance wishes**,
  a hook for diplomacy.

### Opportunities

A nation `j` within reach (proximity at least 0.6) is a **weak neighbour**
when the estimates show at least one of:
- unemployment over 20%;
- stability under 35;
- a junk rating;
- GDP 15% down on a year ago;
- a collapse (under a fifth of its start).

If the brain is also stronger (`power_i > power_j`), it records an
**opportunity**: target, reasons, power ratio and day. It does nothing with
it now; the war phase reads them (`Brain::opportunities`, `war_targets`).

### The goal

```
grow      = bias + 0.5 + 0.5 · growth need
get_rich  = bias + 0.6 · (1 − debt need) + 0.4 · trade openness
dominate  = bias + 1.5 · max(0, my share − best other's share) / my share + 0.3 · (power rank is 1)
survive   = bias + 1.2 · max(jobs, debt, support, threat level)
```

The biases come from the personality (section 8). The brain keeps its goal
unless another scores `goal_stickiness` (0.15) more. Each goal weighs the
needs differently (`[goal.*]`):

| Need | grow | get rich | dominate | survive |
|---|---|---|---|---|
| jobs | 1.0 | 0.6 | 0.6 | 1.2 |
| heat / chill | 0.6 | 0.9 | 0.5 | 0.8 |
| debt | 0.6 | 1.2 | 0.5 | 1.2 |
| supply | 0.9 | 0.8 | 0.9 | 1.2 |
| growth | 1.2 | 1.0 | 0.8 | 0.3 |
| support | 0.5 | 0.5 | 0.4 | 1.0 |
| security | 0.3 | 0.2 | 1.3 | 0.9 |
| trade | 0.6 | 1.1 | 0.4 | 0.4 |

---

## 6. Planning (weekly): the actions

| Category | Actions | Taken through |
|---|---|---|
| fiscal | income tax, corporate tax, sales tax, welfare, tariff: a point up or down | `pull_with` |
| research | research and schools: a quarter point up or down | `pull_with` |
| money | fix the rate half a point lower (a slump the bank isn't fighting: jobs need over 0.5, no inflation) or higher (heat need over 0.5); hand it back to the central bank | `pull_with`, `free_rate` |
| building | build more of the maker of a good that is short at home, or that sells abroad above its cost; close idle state-built buildings | `player_build`, `player_close` |
| infrastructure | a road or rail level in the most choked province; a port where trade is full | `player_road`, `player_port` |
| trade | ban exports of a good short at home; lift a ban once the store is full again; embargo the counterweight target (only with a threat level over 0.5 and an embargo-minded personality); lift an embargo | `set_trade_policy`, `toggle_embargo` |
| military | arms purchases 0.2 points up (only with a security need) or down (not under the personality's floor) | `pull_with` |

Situational effects, on top of `[action.*]`:
- **Taxes and welfare.**
  - A point's help with the debt is what a point of that tax brought in
    last month (its own books), over `yield_ref` (0.4% of GDP), between
    0.1× and 2.5×: the brain raises the taxes that pay.
  - The higher a tax already is, the more another point hurts and a cut
    helps: harms × `0.5 + tax / 0.35` (welfare: `0.5 + welfare / 0.25`).
  - Ceilings as the treasury's: income 55%, corporate 45%, sales 25%,
    welfare between 10% and 50%; the personality's `tax_floor` below.
- **Build a good's maker.**
  - Supply effect: the good's shortage × its importance × 1.5.
  - Growth effect: its kind's return on cost (`roi / roi_full`) × horizon.
  - Debt harm: cost over a year's GDP × `cost_harm`.
  - Only while the nation can make less than `build_margin` (1.1) times what
    it uses and sells; when not short, only a kind that already earns
    `roi_min` (12%) a year and sells abroad for over `export_margin` × its
    cost. Never a kind starved of inputs, never without the building goods
    in store, never with debt near the ceiling, and **one order of a kind at
    a time** (wait to see what the last one did).
- **Close.** Only the brain's own buildings standing idle (running at 30%
  or less, losing money for 90 days), making nothing short.
- **Research.**
  - Growth effect: × `horizon_years · horizon / 10` × `research_belief`
    × the room left under `research_max` (5% of GDP; the personality may
    lower it). Jobs: the money is spent at home.
- **Arms.** Security effect × how far under `arms.max` (3%) it is.
- **Embargo.** Security effect: the threat level plus the personality's
  taste for embargoes. Growth and trade harm: the share of its trade it
  would cut.

---

## 7. Memory

Every brain keeps a **relation** with every other nation:
- `regard`, from −1 (hates) to +1 (trusts);
- a log of what happened.

It is built from what the brain can see: its own trade, embargoes and the
news.

| Event | When | Regard |
|---|---|---|
| embargoed us | their embargo on us starts | −0.4 |
| lifted an embargo | it ends | +0.15 |
| trade kept | a full year of steady trade with us | +0.05 |
| trade broken | a partner (at least 5% of what we buy) cuts its sales to us by over 80% in a month while ours to them didn't, and nobody embargoed anyone; at most once a year per partner (trade swings month to month) | −0.1 |
| helped | they sold us a good we were short of | +0.03 (at most once a quarter) |

Regard decays `memory_decay` (5%) towards 0 each quarter. The log keeps the
last 32 events per nation. Strategy uses regard for hostility. Diplomacy
and war will read the log.

**No flip-flopping.** Each lever, building kind, trade ban and embargo
remembers its last move.
- A move is never undone within `undo_days` (90): its effect hasn't had
  time to show.
- Undoing it later, within `flipflop_days` (180), costs
  `flipflop_penalty · (1 − days since / flipflop_days)`.
- `patience_days` can make a lever wait after any move; tuning found that
  slowed the brains for nothing, so it is 0.

---

## 8. Personality from the wheels

Each brain's personality is the sum of what its wheels say
(`personality.toml`):

| Wheel | Sets |
|---|---|
| **Leader** | style: warlord takes risks (weighs debt less, security and arms more), coward is careful (debt and security more, embargoes less), genius plans further ahead (horizon, less noise, research), builder loves industry (building), diplomat loves trade (trade, embargoes much less) |
| **Ideology** | goals and limits: Democracy needs public support (support weight, taxes cost support); Military Rule loves its army (an arms floor, the *dominate* goal); Communism plans (building, supply, no tax cuts below the plan's levels); Colony wants independence (security against its overlord, *get rich*); Monarchy, Theocracy, Imperialism and the rest in the file |
| **History** | Former Empire leans to *dominate*; Ex-Colony distrusts the strong (more hegemon pressure); Post-War is careful; Revolution and Breakaway are restless (support matters more) |
| **Regional impact** | a Regional Power looks further (bigger reach for threat and opportunities); isolated nations weigh security less |
| **Blessing or curse** | Loyal Army: arms cost less support; Corrupt Court: a little more noise; Rebel Province: support matters more; Golden Generation: research pays off more (in the brain's eyes too) |

A personality is:
- a weight per need (`P`);
- a liking per action category (`like`);
- goal biases;
- `risk` (multiplies debt harm by `1 − risk`);
- `horizon_years`;
- `noise` (added to the difficulty's);
- `research_max`;
- `arms_floor`.

The final value is the default plus every wheel's change, clamped to its
range.

### Difficulty

| | noise (rank sd) | horizon × | plans |
|---|---|---|---|
| easy | 0.12 | 0.6 | 1 action a week |
| normal | 0.05 | 1.0 | up to 3 |
| hard | 0.02 | 1.3 | up to 3 |

Difficulty never touches the economy or the brain's information. The game
plays at `default_difficulty` (normal).

---

## 9. Battles (hook)

`src/ai/battle.rs` defines `BattleView` (what a brain may see of a front),
`BattleOrder` and the trait `BattlePlanner`. `Brains::day` calls the
planner for every brain every day; the default (`NoBattles`) gives no
orders. Strategy already fills `war_targets` (opportunities on weak
neighbours, and the counterweight target when the goal is *dominate*) and
`alliance_wishes`, for the war and diplomacy phases.

---

## 10. Spectator mode

On the title screen, **W** (or WATCH THE BRAINS) builds a world of NPCs
only (every nation, including the first, has a brain). It opens on the map
at speed 6 (90 days a second, this mode only). The top bar shows
SPECTATING and the world's GDP, growth and how many nations are over 15%
unemployment. Click any nation's name, any province, or press Tab, and its
**brain panel** shows:
- the goal and why;
- the needs as bars;
- the three best options this week, each with its score and its two
  reasons;
- what it did that week (or that it held still);
- the biggest threat;
- opportunities;
- the counterweight target, if any.

`--spectate [MAP]` starts straight there (`--seed N` picks the world).
In a normal game the rivals have brains too, but their thinking is
private: the panel is for spectators only.

---

## 11. Tools

```
cargo run --release --no-default-features -- --ai-compare [YEARS] [SEEDS...]
cargo run --release --no-default-features -- --ai-tune [WORLDS] [YEARS]
cargo run --release --no-default-features -- --ai-trace MAP SEED NATION DAYS
```

- **`--ai-compare`** runs every map with each seed twice, in parallel:
  once with the phase 3 autopilot and once with brains. It prints the three
  yardsticks per world and in total:
  - world growth (W$ GDP, start to end, % a year);
  - nations above 15% unemployment at the end;
  - junk ratings (BB or worse) at the end.

  `--why` lists every junk or jobless nation under the brains with its
  taxes, welfare, research, arms and budget.
- **`--ai-tune`** runs many NPC-only worlds (seeds 100 on, the maps in
  turn, in parallel) and reports dumb behaviour, with the worst nations:
  - **bankruptcies**: defaults, and nations ending near default while
    printing money;
  - **building nobody needs**: builds whose buildings run at under half
    their capacity a year later;
  - **flip-flops**: moves undone within 90 days, and those adjusted back
    within half a year;
  - every decision by action, and actions that couldn't be done.
- **`--ai-trace MAP SEED NATION DAYS`** prints one brain's week: goal and
  why, needs, the top three options with reasons, and what it did.

---

## 12. What the tests hold the brains to

`src/ai/tests.rs`:
- **`the_same_seed_gives_the_same_decisions`**: two runs of two years, every
  decision on every day equal.
- **`npcs_never_use_information_they_shouldnt_have`**: everything hidden
  about the other nations (buildings, stores, treasury, debt, taxes,
  research, savings) is changed, and the brain's goal, threat, estimates,
  needs, options and actions stay the same; a change its spies can see
  (GDP) does move its estimates.
- **`intel_is_sharper_with_a_better_secret_service`**: errors shrink level
  by level, a brain sees itself exactly, and others within its error.
- **`brains_use_only_the_players_levers`**: two years in, the autopilot
  built nothing for a brain, every state building was a build-button order,
  every lever is in the buttons' range, and roads and ports grew only by
  the brain's own orders.
- **`the_brains_are_fast_enough_for_25_nations_at_top_speed`**: a year of
  25 brains' thinking under 1 s (a few hundredths in practice; top speed
  needs a year in 4 s).
- **`the_brain_files_load_and_name_real_wheel_results`** and
  **`a_mistake_in_brain_toml_is_one_clear_sentence`**.

Beating the autopilot is measured by `--ai-compare` (section 13): a
50-year, 36-world run takes a few minutes, too long for every test run.

---

## 13. Brains against the autopilot

`--ai-compare 50 1 2 3 4 5 6 7 8 9 10 11 12`: every map with twelve seeds
(36 worlds, 900 nations), 50 years, once with the phase 3 autopilot and
once with every nation run by a brain. The full table is in
[ai_compare_50y.txt](ai_compare_50y.txt).

| 50 years | Autopilot (phase 3) | NPC brains (phase 4) |
|---|---|---|
| World growth a year (W$ GDP) | 0.84% | **2.09%** |
| Nations above 15% unemployment | 426 | **300** |
| Junk credit ratings (BB or worse) | 241 | **123** |

- Seeds 1-6, the worlds the weights were tuned on: 0.93% → 2.18% growth,
  217 → 161 over 15%, 124 → 65 junk.
- Seeds 7-12, never used for tuning: 0.75% → 2.00%, 209 → 139, 117 → 58.
- World by world, growth is better in all 36 (0.4% to 3.0% a year).
  Fewer nations are over 15% unemployment in 31 (as many in 3), and fewer
  are junk in 28 (as many in 2).

### Dumb behaviour, before and after tuning

`--ai-tune 40 30` (40 NPC-only worlds of 30 years, seeds 100-139; final
output in [ai_tune.txt](ai_tune.txt)):

| | First brains | Tuned |
|---|---|---|
| Defaults | 99 | 43 |
| Nations ending near default and printing money | 23 | 10 |
| Builds still mostly idle a year later | 50.7% | 27.2% |
| Moves undone within 90 days | 0.9% of decisions | 0 |
| Orders that couldn't be carried out | 0.5% | 0 |
| World growth in these worlds | 1.15% | 1.57% |

What the tuning tool found, and what fixed it:
- **Overbuilding in a boom, factories nobody fed.** Builds are now only
  for a shortage the nation can't already cover, or for a kind that
  already earns well abroad; never without the goods in store; one order
  of a kind at a time.
- **Taxes ratcheted to the ceiling while the deficit grew.** The brains
  raised the taxes that bring in least (corporate, sales) and cut welfare
  to 0. Now a point's help with the debt is what it really brought in
  last month, higher taxes hurt more, and welfare has the treasury's
  floor and a ceiling.
- **Welfare raised to 80% of a wage** (13-19% of GDP): the deficits behind
  most junk ratings. Welfare now costs more to raise the higher it is.
- **Paralysis in a crisis.** With every need at 1, helps and harms
  cancelled, and one colony printed money for years at a 17% deficit. A
  debt emergency now outweighs everything (section 3).
- **Hyperactivity from noise.** Noise added to the scores pushed weak
  options over "hold still": 2.5 times as many decisions and twice the
  defaults. Noise now only ranks the options (section 4).
- **Paranoia.** Month-to-month trade swings counted as "cut its trade
  with us": regard fell to −1, neighbours became 100% threats, and
  brains armed and turned to *survive*. A broken deal now needs a real
  partner and counts once a year at most. That one fix lifted growth by
  0.1-0.15% a year on both seed sets and put 5-11 fewer nations over 15%
  unemployment.
- **Export bans flip-flopping** as a shortage came and went. A ban is now
  lifted only once the store is full again.
- **Embargo cascades.** A warlord embargoed four neighbours in four weeks
  (each embargo made the next nation hostile). Embargoes are now only for
  the nation a brain plans against, with a real threat.

The settings that mattered most were the research ceiling (raising it from
3% to 5% of GDP lifted the brains' growth from about 1.9% to 2.1% a year)
and the jobs span (with unemployment 7 points over natural fully urgent,
not 10, about 40 fewer nations ended over 15% on seeds 1-6).
