# LUCKY WORLD: diplomacy and spies

Phase 6. Every pair of nations has relations you can read the reasons
for; nations sign treaties with terms and durations, and can break them;
breaking one costs a reputation everyone sees; allies are called to war
and decide whether to come; wars end in negotiated peace; vassals weigh
their loyalty; and every nation runs spies, whose missions succeed or fail
by real odds. The NPC brains use all of it, with the same rules and the
same fog of war as the player.

Every number here is in `data/diplomacy/diplomacy.toml` (a typo is one
clear sentence). The code is `src/diplo/`. Nothing in it draws.

Contents: 1 relations · 2 reputation · 3 treaties · 4 proposals and
answers · 5 breaking · 6 alliances in war · 7 peace talks · 8 vassals and
colonies · 9 spies · 10 the brains · 11 the war and the economy · 12 on
screen · 13 tools and tests · 14 results

---

## 1. Relations

Relations are `i`'s view of `j`, from −100 (hatred) to +100 (trust). They
are not a number that drifts: they are **rebuilt every month from their
reasons**, so the game can always say why. Each reason is a line, like

> +12 trade partners · +25 allies · −30 they invaded us in Year 14

```
relations(i, j) = Σ reasons, clamped to ±100

ideology     same ideology +10; a different one −5; opposed pairs −15
             (communism and democracy, communism and monarchy, theocracy and
             democracy, military rule and democracy); for communism and
             theocracy ("ideological friends") the same ideology is +20
border       a shared land border −5 (friction), unless they are allies
trade        +40 × j's share of i's trade (imports from + exports to), at most +20
treaties     each active treaty: trade +8, non-aggression +5, military access +5,
             defensive pact +15, alliance +25, guarantee +15 (for the guaranteed),
             loan +5, sanction against i −15
reputation   (reputation of j − 50) / 5             (section 2: −10 to +10)
power        j is i's biggest threat −10; j is the hegemon (AI.md) −10
history      each remembered event, its weight halving every `half_life` (10) years
```

**History** is a list per pair of events with a day and a weight. The war,
the brains and diplomacy all add to it:

| Event | Weight (for the one it happened to) |
|---|---|
| they invaded us | −40 |
| we fought each other | −20 (for the attacker) |
| we fought side by side | +25 |
| they made peace with us | +5 |
| they broke a treaty with us | −35 |
| they broke a treaty with our ally | −15 |
| they refused our call to arms | −30 |
| they came to our aid | +30 |
| we caught their spy | −25 |
| they embargoed us / lifted it | −20 / +5 |
| they armed our enemy | −20 |
| they sold us arms / lent us money | +8 / +10 |
| they defaulted on our loan | −25 |
| they protected us (overlord) / neglected us | +20 / −30 |

The brains' Phase 4 regard (`relations[j].regard`, used for grudges,
threats and coalitions) is set to `relations / 100` every month, so every
earlier rule now sees diplomacy.

## 2. Reputation

Each nation has a **reputation** from 0 to 100. It starts at 50, plus the
leader's wheel (diplomat +15, genius +5, builder +5, coward 0, warlord −10).

```
breaking a treaty     −break_cost[kind]: non-aggression 15, defensive pact 20,
                      alliance 25, guarantee 20, loan default 15, trade 3, access 3
honouring a call      +5
every year            moves 5% of the way back towards 50
```

Breaking a deal hurts with **everyone**, not just the victim: reputation
is in every nation's relations with the liar (section 1), and in every
answer it gets (section 4), so liars get fewer deals.

## 3. Treaties

| Kind | Who | Years | What it does |
|---|---|---|---|
| Trade agreement | both | 10 | no tariffs between them; each prefers the other's goods (offers weigh `trade_pref` × 1.5 in the market) |
| Non-aggression pact | both | 10 | neither may attack the other |
| Defensive pact | both | 10 | if one is attacked, the other is called to arms |
| Alliance | both | 15 | defensive, and called to each other's wars; military access |
| Military access | both | 5 | armies may pass and retreat through (they count as friendly land) |
| Guarantee | one-sided | 10 | the guarantor is called to arms if the guaranteed is attacked |
| Arms deal | buyer, seller | once | tanks, jets, ammo or warships from one stockpile to the other, at world price × `arms_markup` (1.2) |
| Loan | lender, borrower | 5-10 | W$ now; repaid monthly with interest `loan_rate` + `(1 − trust) × loan_risk` |
| Sanctions | several, against one | 5 | every member embargoes the target |

A treaty has terms (who, the amount, the rate, the goods), a start, an end
and a state: **active**, **expired** (ran its course), **ended** (by
agreement), or **broken** (by whom, on what day, how).

Weapons in arms deals move with the war's flow books (`given`, `received`),
so "every shell accounted for" still holds. Loan money moves treasury to
treasury (`lent`, `repaid` in the budget books).

## 4. Proposals and answers

Anyone proposes (the player in the diplomacy screen, a brain monthly). The
other side weighs it:

```
value(x, kind, y) = gain(x, kind, y) − cost(x, kind, y)
                  + relations(x, y) / 100 × relations_weight (0.6)
                  + (reputation(y) − 50) / 50 × trust_weight (0.5)
                  + personality (treaty_love; ideology friends)
accept if value > accept_threshold (0.25)
```

| Kind | Gain | Cost |
|---|---|---|
| trade | 2 × trade potential (what y sells that x imports, its share of x's trade) | planned economies prefer few deals (−0.2) |
| non-aggression | x's fear of y (y is its threat: `threat_level`; y's power over x's) | x has designs on y (an opportunity or war target): −0.8 |
| defensive / alliance | protection: y's power / x's, against a shared threat (the same threat) +0.5; ideological friends +0.3 | being dragged in: y's enemies' power / x's (alliance × 1.5) |
| access | y is an ally | −0.1 |
| guarantee | for the guaranteed: its threat level × 1.5 | for the guarantor: the threat's power / its own |
| arms deal | the buyer: at war or threatened, its stockpile low; the seller: money, a surplus | the seller: the buyer's enemies are its friends: −1 |
| loan | the borrower: its debt need (AI.md); the lender: interest, influence | the lender: the borrower's default risk (junk rating, low reputation) |
| sanctions | the target broke treaties or attacked a friend; x's relations with it < −30 | x's trade with the target |

Every answer says why, from the biggest term:

> **No**: you broke a pact with our ally · **No**: we have plans for your
> land · **Yes**: we fear the same enemy · **No**: you are not to be trusted

## 5. Breaking treaties

Treaties break by the rules, never at random:

- **Attacking a partner**: declaring war on a nation it has a
  non-aggression pact, defensive pact, alliance or guarantee with breaks
  that treaty (the attacker breaks it). Every other treaty between them
  ends.
- **Refusing a call to arms** (section 6) breaks the defensive pact,
  alliance or guarantee.
- **A loan default**: a borrower that can't pay for three months running
  defaults.
- **Ending early**: a treaty can be ended before its time; trade and access
  then count as broken (a small cost), the rest as broken in full.

Before a brain declares war, the cost of every treaty it would break is in
its desire (section 11). A warlord, whose honour is low, pays less of it.

## 6. Alliances in war

When a war starts (and when a new member joins it), every nation bound to
a side is **called to arms**: defensive pact and guarantee partners of the
attacked, alliance partners of either side, and the overlord of an
attacked vassal (section 8). Each decides:

```
honour = honor (personality) + relations(x, ally) / 100 − relations(x, enemy) / 200
       + 0.5 × (power of the side it would join, with it) / (enemy side's power) − 0.5
       − 0.6 × exhaustion − 0.3 if already at war elsewhere
join if honour > 0.5 (an alliance's offensive war: > 0.8, and refusing it breaks nothing)
```

A nation that comes gains +30 with its ally; one that refuses breaks the
treaty (section 5). The player is asked (the inbox); after 14 days without
an answer, the player honours it.

**Coalitions become alliances.** When the Phase 5 coalition rule goes to
war on a hegemon, its members sign alliances with each other (10 years),
and brains that fear the same hegemon (`alliance_wishes`) propose
defensive pacts to each other in peace (section 10).

## 7. Peace talks

The Phase 5 peace deal (one side offers at the war score, the other takes
it or not) becomes a negotiation. Each week, for every war:

```
demand(s) = score(s) + greed(s) + allies' claims − concession × weeks talking
reserve(s) = score(s) − give(s);  give = 30 × exhaustion / stake + 10 × weeks talking / 52
```

`score(s)` is side s's war score (WAR.md section 10), as its brain sees it:
its own losses exactly, the enemy's through its secret service (a bad one
is optimistic or pessimistic by up to `intel_error` × 100 points).
`greed` = 10 × (aggression − 0.5). A side opens talks when it would make
peace in Phase 5 (worn out, hopeless or won). The other answers:

- **accept** if the offer is at least its reserve;
- otherwise **counteroffer** at its own demand, with the reason ("No: we
  hold three of your provinces").

Each round both demands move by `concession` (4 points a week). They meet
when one side's offer reaches the other's reserve. The terms are those the
agreed score buys (WAR.md section 10). **Allies on the winning side add
their own demands**: each ally that holds occupied provinces of the loser
next to its own land claims them (if the score passes the provinces line),
and each takes `ally_reparations` (half the lead's share) for itself.

Talks are logged round by round, for the spectator and the player.

## 8. Vassals and colonies

A vassal pays tribute (ECONOMY.md section 10) and has a **loyalty** from
0 to 100:

```
loyalty target = 50 + relations(vassal, overlord) / 2
               − 300 × tribute share of its revenue (10% → −30)
               + 20 if the overlord came when it was attacked (protection)
               − 30 if the overlord refused (neglect)
loyalty moves 5% of the way to its target each month
```

The overlord is called to arms when a vassal is attacked (section 6);
coming is **protection**, refusing **neglect**. Loyalty enters the Phase 5
liberty desire (WAR.md section 10): `liberty −= (loyalty − 50) / 100`. A
vassal under 25 has an **independence movement**: its stability falls 0.5
a month (and at that loyalty its liberty is already 0.25 higher).

## 9. Spies

Every nation has a **secret service**: agents, their skill, and a share
of them on counter-intelligence. The Secret service wheel sets the start:

| Wheel | Agents | Skill | Counter-intel base |
|---|---|---|---|
| None | 0 | 0.20 | 0.10 |
| Amateur | 2 | 0.35 | 0.25 |
| Competent | 5 | 0.50 | 0.40 |
| Feared | 10 | 0.65 | 0.55 |
| Legendary | 16 | 0.80 | 0.70 |

**Training** an agent takes `train_days` (180) and `train_cost` (0.01% of
GDP), then `upkeep` (0.005% of GDP) a year, from the treasury. A nation can hold at
most `max_agents` for its wheel (2× the start, at least 4). New agents
start at the service's skill; each finished mission adds +0.02 (up to 0.95).

**Counter-intelligence**:

```
C(j) = ci_base(j) + 0.15 × ln(1 + agents on counter-intel) × their skill
```

**Missions.** A mission has a kind, a target, 1-3 agents and a time.
At its end:

```
S = mean skill of the agents + 0.08 × (agents − 1)
p(success) = 1 / (1 + exp(−k × (S − C(target) + ease[kind])))       (k = 6)
p(caught)  = 1 / (1 + exp(−k × (C(target) − S + exposure[kind])))
luck: two independent rolls, one for each
```

Against an even match (skill = the target's counter-intelligence) the
odds are: intel 55% / caught 15%, fund 50 / 20, unrest 40 / 25, sabotage
35 / 30, meddle 30 / 30, steal 25 / 25, bribe 15 / 35, revolt 10 / 40,
kill 8 / 50.

| Mission | Ease | Exposure | Days | On success |
|---|---|---|---|---|
| Gather intel | +0.03 | −0.29 | 60 | the spymaster's estimates of the target are `intel_error × 0.25` wrong (instead of × 1) for 2 years, in the economy and on the battlefield |
| Sabotage | −0.10 | −0.14 | 90 | the target's most industrial province loses 8% of its building levels and 0.5 rail (or its power plants 15%) |
| Steal research | −0.18 | −0.18 | 120 | the spymaster gains 30% of the target's lead in knowledge |
| Stir unrest | −0.07 | −0.18 | 90 | target stability −5 |
| Start a revolt | −0.37 | −0.07 | 120 | if the target's stability is under 20 (where it could rise anyway), a civil war (WAR.md section 8); else stability −12 |
| Fund rebels / proxy war | 0.00 | −0.23 | 30 | 5% of the spymaster's ammo and rifles go to the target's rebels, or to its enemy in a war; with neither, dissidents are paid (stability −3) |
| Meddle in politics | −0.14 | −0.14 | 120 | a democracy only: stability −10, and its war-weariness clock +30 days |
| Kill a general | −0.41 | 0.00 | 90 | the target's best general dies (its divisions get a new one) |
| Bribe a general | −0.29 | −0.10 | 90 | the target's best general secretly fights at 60% of his skill for a year |

**Caught.** Each caught agent is lost. The target learns who sent them: a
**diplomatic incident** (relations −25 "we caught their spy", reputation
−2, news). A mission can succeed and still be caught.

**The fog.** Nobody sees another nation's exact counter-intelligence: the
spy screen shows success chances from the spymaster's **estimate** of
`C`, with its own intel error (narrowed by an intel mission). Spies never
reveal more than their intel buys: an estimate is never closer than
`intel_floor` (0.03) and gathering intel on one nation tells nothing about
another.

## 10. The brains

Monthly (on staggered days), each brain:

1. **Proposes** the treaty it values most (section 4, from its own side)
   to the nation it values it with, if the value passes `propose_threshold`
   (0.45). A proposal it was refused waits a year. To the player, only a
   deal worth `propose_threshold` + 0.3, and one a month from the whole
   world; an offer left in the player's inbox lapses after 90 days.
2. **Answers** proposals (section 4) and calls to arms (section 6).
3. **Arms deals and loans**: a brain at war or threatened, with a low
   stockpile, buys from the friendliest nation with a surplus; a rich one
   lends to friends in debt.
4. **Sanctions**: it proposes sanctions on a treaty-breaker or an
   aggressor that attacked a friend, to those who dislike the target.
5. **Spies**: trains agents if it can afford them, keeps
   `ci_share` (0.4, more if threatened) on counter-intelligence, and, one
   mission at a time and in `launch_chance` (25%) of months (+ its spy
   trait, + 20% at war), sends free agents on the mission with the best
   expected value, `p(success) × gain − p(caught) × incident cost × honour`,
   as its own service estimates the odds. Unrest, revolts, sabotage at full
   value, killing and bribing generals are for enemies at war.

Personality (`[personality]`, by wheel option):

| | honour | treaty love | wants protection | ideology friends | spy use |
|---|---|---|---|---|---|
| diplomat | 0.9 | +0.3 | | | |
| warlord | 0.3 | −0.2 | | | +0.2 |
| coward | 0.7 | | +0.4 | | |
| genius | 0.6 | +0.1 | | | +0.1 |
| builder | 0.7 | +0.1 | | | |
| communism, theocracy | | | | ×2 relations with their own | |
| democracy | +0.1 | +0.1 | | | −0.1 |
| military rule | −0.1 | | | | +0.1 |

The same rules and the same fog apply: brains see others through their
estimates (with spies' intel), and act through the same functions as the
player's buttons.

## 11. The war and the economy

- **Deterrence and honour in war decisions.** A brain's war desire on
  `j` (WAR.md section 11) has a new cost: `diplo_cost(i, j)` =
  honour × break_cost[treaties with j] / 20 + 1.5 × (power of j's
  defensive partners and guarantors) / (i's power). A war whose desire
  passes the threshold without it, but not with it, is counted as
  **prevented by diplomacy**.
- **Friends**: allies with military access count as friendly land for
  retreat and supply.
- **Peace**: when diplomacy runs, it takes over the Phase 5 offer rule
  (section 7); escalation, surrenders and the six-year cap stay.
- **Trade agreements** change the market: no tariffs between partners,
  and a buyer's bids prefer partners' offers × 1.5.
- **Intel** from spies narrows the brains' estimates and the generals'
  view of enemy armies.

## 12. On screen

- **Diplomacy** (P): pages **Nations** (every nation's relations with
  you, a bar, and the reasons; their treaties with you), **Treaties**
  (yours, and the world's latest; B: break the chosen one), **Propose**
  (to the chosen nation: Left / Right the kind, Enter to propose; the
  answer and its reason), **Inbox** (proposals and calls to arms: Y / N),
  **Talks** (peace talks round by round; your wars: Left / Right your
  demand, Enter to send it).
- **Spies** (I): **Agents** (agents, skill, training: T train one, C the
  counter-intel share), **Missions** (Left / Right the target, Up / Down
  the mission, the success and caught chances as your service estimates
  them, Enter to send), **Reports** (every mission's result).
- **Map overlays** (V): **Alliances** (each bloc of alliances and
  defensive pacts in its own colour) and **Relations** (green to red, with
  you, or watching, with the chosen nation).

## 13. Tools and tests

```
cargo run --release --no-default-features -- --diplomacy-report [YEARS] [SEEDS...] [--compare]
```

It runs full NPC worlds (economy, brains, war, diplomacy, spies) on every
map and seed and sums up the treaties signed and broken by kind,
alliances, calls to arms honoured and refused, peace talks, spy missions
and incidents, wars prevented, and the world's GDP growth. `--compare`
runs the same worlds again without diplomacy, to show what it changes.

`src/diplo/tests.rs`:
- the same seed gives the same diplomacy;
- treaties are honoured or broken by the rules (attacking a partner
  breaks the pact; refusing a call breaks it; a default breaks a loan;
  breaking costs reputation with everyone);
- spies never reveal more than they should (estimates keep at least the
  floor's error; intel on one nation doesn't change another's);
- every arms deal and loan is accounted for;
- a treaty in force is never signed twice (the index of active treaties);
- 50 years of the full game on all maps and six seeds: world GDP grows
  1.5-3% a year with war on, diplomacy holds wars back and ends others at
  the table, allies are called and come;
- the diplomacy playthrough (`playthroughs/diplomacy.json`) opens every
  page, gets answers with reasons, signs from the inbox, trains an agent,
  sends a mission, and shows both overlays;
- the full game for 25 nations at top speed stays fast: a year of the
  heaviest total war, with economy, brains, war and diplomacy, takes about
  2.4 s of CPU (diplomacy about 0.03 s of it; top speed allows 4).

## 14. Results

From `--diplomacy-report 50 1 2 3 4 5 6 --compare` (about 6 minutes on 4
cores; the whole report, world by world, with the broken treaties, talks
and news, is [diplomacy_report_50y.txt](diplomacy_report_50y.txt)).

Over 50 years on all three maps and six seeds (18 worlds, 450 nations), the
full game with diplomacy and spies against the same worlds without them:

| | Without diplomacy | With diplomacy |
|---|---|---|
| World GDP growth a year | 2.47% (worlds 1.17% to 3.44%) | **2.55%** (1.61% to 3.31%) |
| Wars (a nation per decade) | 848 (0.38) | **631** (0.28) |
| Soldiers killed | 60.3M | **27.6M** |
| Total wars (average length) | 48 (3.09 years) | **20 (2.55 years)** |
| Declared wars (average length) | 330 (1.24 years) | 253 (1.18 years) |
| Civil wars | 63 | 74 |
| Wars held back by treaties or deterrence (pair-years) | | 2,111 |
| Wars ended by negotiation | | 315 |

- **Treaties**: 22,182 signed (5,224 non-aggression, 5,022 trade, 4,828
  military access, 2,152 guarantees, 1,911 defensive pacts, 1,635
  alliances, 800 loans, 548 arms deals, 62 sanctions) and 111 broken (29
  non-aggression pacts by attacks, 26 alliances and 22 guarantees by
  refused calls, 29 loans by defaults). 153 alliances came out of
  coalitions against a hegemon.
- **Calls to arms**: 869 honoured, 18 refused (an alliance's offensive
  call declined 7 more times).
- **Peace talks**: 317 opened, 374 rounds, 315 deals; allies took their
  share of reparations 301 times.
- **Spies**: 34,396 missions (13,745 to steal research, 9,111 for intel,
  8,563 sabotage, 2,035 meddling, 870 funding rebels or a proxy, 72
  revolts), 26,347 succeeded; 4,557 agents caught in 4,097 diplomatic
  incidents; 38 revolts started.
- **Vassals**: 69 independence movements.
- **Money and arms**: $417B lent (658 loans repaid), $1.14T of arms sold.

**What diplomacy changes.** Fewer wars (−26%), less than half the dead,
fewer and shorter total wars: treaties and their defenders hold brains
back (a war whose desire passes the line only without the treaty cost
counts as held back), allies come when called (a war against a nation with
friends is a bigger war, so fewer are started), and talks end wars when
both sides' offers meet. World growth stays inside 1.5-3% with war on:
2.55% against 2.47% without diplomacy (trade agreements lift trade a
little; fewer wars destroy less).

**Weak spots, honestly.**
- Spies are busy: about 1.9 missions a nation a year, most of them
  stealing research; 4,097 incidents over 900 nation-decades. They add a
  few civil wars (74 against 63), through revolts and unrest at war.
- Few provinces change hands in talks (allies claimed 3): most deals are
  reparations or a white peace.
- Many proposals (144,000) for each deal signed (22,000): brains propose
  monthly and are often refused; nothing is lost but log space.
- Relations can saturate at ±100 for close allies after a shared war (each
  shared war adds +25).
