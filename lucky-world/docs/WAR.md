# LUCKY WORLD: war

War is where the wheels, the economy and the brains meet. Every soldier is
a person taken from the workforce, and every rifle, shell, tank, jet and
ship was made in a real factory (with that factory's defects) and bought
with real money. Every litre of fuel was refined at home or imported. A
battle is fought hour by hour with Lanchester's laws and dice. Its losses
come off the population, the stockpile and the treasury, and the province
it was fought over is left damaged.

This is the design, with the math, as built. Every number lives in
`data/war/war.toml`, so war can be tuned without a rebuild. The code is in
`src/war/`; nothing there draws except `src/game/war_screen.rs`.

The phase was built in three layers, each tested before the next:

| Layer | What | Section |
|---|---|---|
| 5a | armies, land combat, supply | 1 to 5 |
| 5b | air and sea | 6 and 7 |
| 5c | kinds of war, escalation, peace, consequences, war brains | 8 to 11 |

---

## 1. Units and scales

The economy's goods are counted in units (docs/ECONOMY.md, section 2).
War reads them as:

| Good | One unit is | World price (W$) | Notes |
|---|---|---|---|
| `rifles` (new) | 1 rifle | 400 | one per soldier; jams with its factory's defects |
| `ammo` | 1,000 rounds, or 40 shells | 4,000 | a shell weighs as much as 25 rounds |
| `tanks` | 1 tank | 4,500,000 | breaks down; needs fuel and spare parts |
| `jets` | 1 jet | 60,000,000 | needs fuel, a pilot and an airbase |
| `warships` (new) | 1 warship | 350,000,000 | coastal shipyards only |
| `transports` (new) | 1 transport ship | 40,000,000 | carries 1,000 soldiers or 10 tanks |
| `fuel` | 1 tonne | 1,500 | tanks, jets and ships burn it |
| `food` | a day's food for 500 people | 800 | every soldier eats |
| `steel` | spare parts | 1,900 | 0.01 a tank a day at war |

Rifles, ammo, tanks, jets, warships and transports are military goods: the
state buys them with its arms budget (ECONOMY.md, section 7) into the
**stockpile**. Food, fuel and steel for the army are bought from the
nation's ordinary stocks at its own prices, as the army's upkeep.

**Warships and transports** are made at two new buildings, the **Naval
Shipyard** (steel 2,000 + fuel 300 + electricity a ship) and the **Cargo
Shipyard** (steel 300 + fuel 40 a ship), which can only stand in a
**coastal province**. A landlocked nation can't build them, can't hold
them (it has no port), and so has no navy. A **Rifle Works** makes rifles
from steel and timber.

### Defects: weapons remember their factory

Every factory has a defect rate (ECONOMY.md, section 4):

```
defect = building defect × (1.5 − 0.5 × skill factor) × the nation's defect modifier
```

For ordinary goods the defects are scrapped. For weapons, quality control
catches only part of them: a share `latent` (60%) of the defects leaves the
factory unnoticed. The stockpile keeps, for each weapon, the share of
units that are faulty, mixed as weapons come in:

```
faulty' = (faulty × stock + latent × defect × made) / (stock + made)
```

Imports carry `import_defect` (3%). Faulty weapons fail in battle:
- a faulty **rifle** jams: its soldier fires nothing that hour, with
  chance `jam_per_hour` (0.5) for a faulty rifle;
- a faulty **round** misfires and a faulty **shell** is a **dud**: fired,
  paid for, and harmless;
- a faulty **tank** breaks down `tank_fault_breakdown` (8) times as often;
- a faulty **jet** is grounded for repairs more often.

**Cheap mass production means more defects.** Total war switches civilian
factories to weapons (section 8), with a `rush_defect` (2×) defect
modifier on everything made that way. A nation with few skilled workers
makes worse weapons than one with many.

---

## 2. Armies

### Soldiers

The army is made of **divisions** of up to `division_size` (10,000)
soldiers. The economy's soldiers (`army`) are the sum of every division
plus the wounded in hospital. In peace that is the Active Duty wheel's
share of the population. At war, mobilisation multiplies it (section 8):

```
wanted soldiers = population × active duty × mobilisation(level)
                  (never more than 15% of the population)
```

Recruits leave the workforce and join the weakest divisions with
`recruit_training` (0.2). The dead leave the population for good.

### A division

| | Meaning |
|---|---|
| soldiers | alive and in the line |
| rifles, tanks | its equipment, each with the stockpile's faulty share it came with |
| ammo, fuel, food | what it carries (in rounds, tonnes, soldier-days) |
| training | 0 to 1: drill. Starts at `0.35 + 0.5 × HDI` plus the Army Strength wheel's bonus (`[training]`), and creeps towards that cap in peace |
| experience | 0 to 1: combat. Grows `experience_gain` (1.5%) a day of battle |
| morale | 0 to 1, section 3 |
| general | its commander (below) |
| entrenchment | 0 to 1: grows 10% a day it holds still, 0 when it moves |
| kind | infantry, or armoured (with tanks) |

A division's **equipped %**:

```
equipped = 0.45 × min(1, rifles / soldiers)
         + 0.25 × min(1, ammo / (3 battle days of ammo))
         + 0.15 × min(1, food / (20 days))
         + 0.15 × (armoured: min(1, tanks / authorised tanks) × min(1, fuel / 10 days); else 1)
```

### Generals

Each division has a **general**, rolled when it is raised:
- skill: 0.6 to 1.4, around the nation's mean (the Leader wheel:
  genius +0.15, warlord +0.05, coward −0.1);
- a style:
  - aggressive: attacks at a 1.15:1 strength ratio, gains morale on the attack;
  - cautious: attacks only at 1.8:1, digs in faster;
  - balanced: 1.4:1.

A general's skill multiplies the division's hits, and a bad one costs
morale (section 3). When a division is destroyed or surrenders, its
general is lost.

**Armies fight on their own.** The generals' AI (section 4) moves,
attacks, digs in and retreats. The player *may* give a division orders
(attack a province, hold, dig in, retreat, reinforce a province). An order
stands until it is done or impossible, then the general takes over again.

---

## 3. Land combat (every hour)

A **battle** is fought in a province when divisions of two sides at war
are in it, or attacking into it from a neighbour. It is resolved every
hour of game time (24 rounds a day). The game clock can be paused, so a
battle can be watched round by round.

### Frontage

Not everyone can fight at once:

```
frontage(P) = frontage_base (12,000) × sqrt(hexes of P) × terrain frontage
engaged soldiers of a side = min(soldiers, frontage)  (shared out by size)
```

### Fire

For each side, each hour:

```
rifle rounds R  = engaged × rounds_per_hour (12) × stance × (1 − jammed share) × ammo limit
                  stance: attack 1.0, defend 0.8
                  jammed share = rifles' faulty share × jam_per_hour
shells S        = engaged × shells_per_soldier_hour (0.015)          (mortars, guns)
                + engaged tanks × shells_per_tank_hour (3)
duds            = S × ammo faulty share;  misfires = R × ammo faulty share
```

Each round or shell fired comes out of the division's ammo. A division
out of ammo fires at `no_ammo_fire` (5%): bayonets and captured rifles.

### Hits

```
M = accuracy × terrain × weather × general × air × supply × morale_fire × luck

accuracy    = (0.5 + training) × (1 + 0.5 × experience)
terrain     = the TARGET province's defence (defender only; attackers stand in the open)
              plains 1.0, steppe 1.0, desert 1.0, tundra 0.85, forest 0.75, hills 0.7,
              marsh 0.7, jungle 0.65, glacier 0.6, mountains 0.5; city ×0.7 more;
              entrenchment: × (1 − 0.35 × entrenchment)
river       = attacking across a river: the attacker's hits × 0.75
weather     = clear 1.0, rain 0.85, snow 0.75, storm 0.6
general     = skill (0.6 to 1.4)
air         = 1 + 0.35 × superiority (section 6), and the enemy's × (1 − 0.25 × superiority)
supply      = 0.4 + 0.6 × equipped
morale_fire = 0.5 + 0.5 × morale
luck        = exp(0.25 × Normal(0, 1))       (one roll per side per hour: the dice)

hits on soldiers = (R_good × p_round + S_good × p_shell + tank shells × p_tank_vs_men) × M
                   p_round 0.00007, p_shell 0.010, p_tank_vs_men 0.006
hits on tanks    = (tank shells × p_tank_vs_tank (0.01) + engaged × p_at (0.00002)) × M × tank exposure
```

Hits fall on the enemy's engaged soldiers. Doubling one side's shooters
doubles the other side's losses, so concentrated fire follows
**Lanchester's square law**. Forests, jungles and cities lower the hit
chance, which moves the fight towards the linear law: numbers matter less
in cover.

Tanks fight worse in mountains (×0.3), marsh (×0.4), jungle (×0.5),
forest and cities (×0.7) and hills (×0.8).

### What a hit does

```
killed = 25% · wounded = 65% · captured = 10%   (shares of each hit; they add up to 1)
```

Wounded go to hospital: 70% return in `heal_days` (30), 10% die there and
20% are sent home. Captured soldiers become **prisoners** of the enemy.
Destroyed tanks are gone. Every hour, each tank also has a chance to
**break down**:

```
breakdown = 0.0008 × (1 + 8 × faulty share) × (3 if no spare parts)
```

A broken tank is repaired in 2 days with spare parts.

### Morale

```
each hour in battle:
  morale −= shock (4) × casualties this hour / soldiers × (2 − general skill)
  morale −= 0.002 × (1 − equipped)          (shortages)
  morale −= 0.001 × war exhaustion           (section 9)
each day out of battle:
  morale += 0.06 × (target − morale);  target = 0.55 + 0.3 × training + 0.15 × equipped
a won battle: +0.1 for the winners · a lost battle: −0.15 for the losers
```

Past the **breaking point**:
- **break** (morale under 0.25): the division retreats to a friendly
  neighbour. Stragglers are captured (2% to 6%, worse at lower morale).
  Broken or fuelless tanks are abandoned, and the enemy captures them
  into its stockpile.
- **surrender**: a division that breaks with nowhere to retreat (all
  neighbours enemy or sea) surrenders. All its soldiers become prisoners,
  and the enemy captures its equipment.
- **desertion**: below morale 0.4, soldiers desert, `desertion` (2%) ×
  (0.4 − morale) / 0.4 of them a day. Deserters go home: they leave the
  army but stay in the population.

### Who wins

A battle ends when one side has no soldiers in the province, or all its
divisions there have broken. An attack is also **called off** (the
attackers pull back in order, without stragglers, and the defenders hold):

```
the attackers' morale < call_off_morale (0.4), or
after attack_days (10): the attackers lost a bigger share of the soldiers they
  brought than the defenders did   (killed + wounded + captured + surrendered
  + deserted) / soldiers at the start, or
after 3 × attack_days (30), whatever the score
```

So numbers count the way attrition counts: a 20-to-1 attacker that trades
two of its men for each defender is still winning, and keeps grinding.

If the attacker holds the field, the province is **occupied** (in a
military operation, only a province the war's goal names; a border
skirmish never takes land): its controller becomes the attacker. An
occupied province:
- makes `occupied_output` (40%) of its output for its owner (the rest is
  lost to the occupation);
- yields its stores of food and fuel to the occupier;
- lets the occupier pass through it.

Each battle damages the province: `battle_damage` (0.5%) of its building
levels and 0.1 levels of road per battle day. The rebuilding is left to
the economy.

### Soldier accounting

Each division counts its own:

```
start + recruits = alive + killed + wounded + captured + deserted   (every hour, exactly)
```

A test checks this identity in every battle (section 12).

---

## 4. The generals' AI (land)

Every day, for each nation at war, the generals:
1. **Fronts.** They group the nation's border with each enemy into fronts
   by compass direction from the capital ("Northern front"). Each front
   gets divisions in proportion to the enemy strength seen there:
   `need = estimated enemy soldiers × 1.2`. They see neighbouring
   provinces through scouts with the secret service's error (AI.md,
   section 2).
2. **Attack.** A division attacks a neighbouring enemy province when the
   strength ratio there beats its general's threshold:
   `own power / enemy power ≥ threshold`, with power = soldiers × equipped
   × (0.5 + training) × terrain. The level of the war limits targets
   (section 8).
3. **Dig in** when not attacking. **Retreat** when broken. **Reinforce**
   threatened provinces from the rear.

Movement takes `move_days` (terrain cost / (1 + 0.15 × road)). Plains take 1
day, mountains 3, marsh 2.5.

---

## 5. Supply

Supplies leave the **stockpile** (weapons) and the **stocks** (food, fuel,
steel) at the capital and travel to the divisions. Each day, for each
division:

```
path      = the shortest chain of provinces the nation controls (or friends do),
            base → division; by sea through its own ports if needed (section 7).
            The base is the capital, or (the capital lost) the most populous
            province the nation still holds
hop       = min(0.97 + 0.004 × (road + rail), 0.995) × (0.95 + 0.05 × transport)
lost      = 1 − Π hop over the path
interdict = the enemy's air superiority over that front × 0.5   (section 6)
arrives   = sent × (1 − lost) × (1 − interdict)
capacity  = supply_capacity (4,000 tonnes a day) × (1 + 0.5 × the weakest road + rail on the path)
```

What is lost on the way is gone (the economy's `destroyed`). A division
cut off from every friendly province gets nothing.

### Requests, and the player's job

Routine supplies (food, ammo top-ups) go out automatically. Big things
(tanks, rifles for new recruits, weeks of fuel, jets for the air wing) are
**requests** from the generals, one per front, in words:

> Northern front needs 200 tanks and 3 weeks of fuel

The player decides which requests are met. Each front has a priority:
hold back, low, normal or high. Requests wait for an answer and grow if
ignored. **Auto** (on by default) answers them by priority. NPC nations'
quartermasters answer theirs by the same rule.

When the stockpile is short, it is shared out by priority × need:

```
share(front) = priority weight (0, 0.5, 1, 2) × need / Σ (weight × need)
```

### Shortages hurt

- no rifles: a soldier fights at 10%;
- no ammo: fire at `no_ammo_fire` (5%);
- no food: morale −0.02 a day, and 0.05% of soldiers die a day;
- no fuel: tanks don't move or fight; abandoned if the division retreats.

All of this works through equipped % (section 2), morale and fire (section 3).

---

## 6. Air (layer 5b)

Jets in the stockpile form each nation's **air force**. Pilots train from
the skilled workforce: `pilot_training_days` (120) each, up to
`pilots_per_jet` (1.5) per jet. A jet flies from an **airbase**: the
capital, or any province the nation controls with road + rail ≥ 3.

Each day, the air force is shared out over the fronts in proportion to
the land forces there. On each front where both sides fly:

```
air power  = ready jets × pilot quality (0.5 + training) × fuel limit × weather_air
ready jets = jets × (1 − faulty share × grounding) × pilot cover
superiority s = (A − B) / (A + B)                  (−1: theirs, +1: ours)
losses     = air_kill (0.002) × B × B / (A + B) × luck   our jets lost a day (and the same the other way)
```

The air war is fought only from a military operation up (a border
skirmish stays on the ground).

Each sortie burns `jet_fuel_per_day` (2 t) a jet. The side with the upper
hand (s > 0) uses its spare air power:
- **close support**: its ground hits × (1 + 0.35 s), the enemy's × (1 − 0.25 s);
- **interdiction**: cuts the enemy's supply on that front by 0.5 s;
- **strategic bombing** (declared war and above): destroys
  `bomb_factories` (0.4%) × s of building levels a day in the enemy's
  provinces near the front, and `bomb_rail` (0.05 levels) × s of rail and
  road. In total war, cities too (stability −0.05 a day for the bombed).

Anti-aircraft fire costs bombers `aa_loss` (0.1%) × the bombers flying ×
min(1, enemy soldiers / 1,000,000) jets a day.

---

## 7. Sea (layer 5b)

### Sea zones

The sea is cut into **zones**. Each connected body of water is split
around seed hexes on a grid (about 120 sea hexes a zone), and every sea
hex goes to the nearest seed in its own body. Every coastal province
touches the zones its sea edges face. Zones next to each other are linked.

### Navies

A nation's **warships** form one fleet per sea zone it chooses. Each
fleet sails from a port (a coastal province with a port). Each day, in
each zone where enemies meet:

```
naval power = warships × (0.5 + training) × fuel limit × weather_sea × (1 − faulty share × 0.5)
losses      = enemy naval power × sea_kill (0.005) × luck   warships sunk a day
```

A side whose enemies outgun it and its friends by more than
`control_ratio` (1.5) times loses the zone to them (or it is contested).
Neutral fleets in a zone don't count: they neither control it nor break a
blockade.

- **Trade.** A nation's sea trade (its port capacity) is cut by the share
  of its coastal zones the enemy controls. Its trade routes are safe only
  where its own ships, or no enemy's, sail.
- **Blockade.** An enemy that controls every zone touching a nation's
  coasts cuts its port trade by `blockade_cut` (90%). Its imports and
  exports dry up through the economy's own market.
- **Invasions.** Transports carry divisions (1,000 soldiers or 10 tanks
  each) from a home port to an enemy coast. The route must not cross an
  enemy-controlled zone. Landing into a contested zone costs `convoy_loss`
  (5%) of the transports, with their troops. The transports come home
  free when the troops are ashore. The landing is a battle with the
  attacker's hits × `amphibious` (0.6) for its first two days.

**The Thousand Isles must be fully playable**: there, every war is a naval
war first. The generals' AI plans invasions when it controls the sea next
to an enemy island.

---

## 8. Kinds of war: the escalation ladder (layer 5c)

| Level | Name | Who may fight where | Mobilisation | Economy |
|---|---|---|---|---|
| 0 | **Cold war** | nobody: an arms race and threats | 1.0 | both raise arms |
| 1 | **Border skirmish** | up to 2 divisions a side, border provinces only, no occupation | 1.0 | normal |
| 2 | **Military operation** | one goal: take the target province(s); no bombing of factories | 1.4 | normal |
| 3 | **Declared war** | anywhere; war goals; strategic bombing; blockade | 2.0 | arms 6% of GDP |
| 4 | **Total war** | everything, cities included | 3.0 (conscription) | 40% of consumer factories make weapons; arms 15% of GDP; defects × 2 |

**Total war costs fast.** Stability −0.15 a day, happiness −0.1 a day, and
`total_war_output` (−10%) on everything else the economy makes.

### Escalating

- Either side can **escalate** one level.
- The other must **match** within `match_days` (14), or **back down**.
- Backing down ends the war on the escalator's terms (its goal, at the
  level before).

How easily a nation escalates depends on its ideology (`[escalation]`):

| Ideology | Escalation cost | Note |
|---|---|---|
| Military Rule | 0.5 | escalates fast |
| Imperialism, Authoritarianism | 0.7 | |
| Monarchy, Theocracy, Tribalism, Communism | 1.0 | |
| Colony | 1.2 | needs its overlord's leave (section 10) |
| Democracy | 1.6 | needs **public support** |

Public support is the stability and the threat the people feel. A
democracy at war with stability under 40 loses 0.3 stability a day more,
and must make peace within 90 days or its government falls (a civil war
check, below).

### Other kinds

- **Proxy war**: a nation secretly sends weapons (ammo, rifles, tanks) to
  one side of someone else's war, or to rebels: `proxy_share` (5%) of its
  stockpile a month. Only the receiver knows. A victim with a feared or
  legendary secret service finds out, and its brain remembers it.
- **Cold war**: two great powers (among the 8 richest) with regard under
  −0.4 each way, one of them the other's biggest threat, start an arms
  race: no shooting, the arms floor of a declared war. The same pair
  can't start another for 10 years. It ends when regard recovers or a
  real war starts.
- **Civil war**: a luck event. When a nation's stability has stayed under
  `civil_war_stability` (20) for `civil_war_days` (45), each day has a
  `civil_war_chance` (1%) of rebellion:
  - **Rebels** take the provinces with the worst unrest, a share of the
    army (up to 40% of the divisions, the least loyal ones) and their
    equipment. They fight with the same rules and are supplied from what
    they hold.
  - **They win** if they take the capital: the ideology changes (a new
    economic model), stability resets to 50, and the war ends.
  - **They lose** when their last division is gone.
- **Blockade**: a navy controls an enemy's coastal zones (from a
  military operation up) and its sea trade is cut (section 7).

---

## 9. War exhaustion and consequences

Each side's **war exhaustion** E grows every day:

```
dE = 20 × (killed + wounded + captured that day) / population
   + 0.002 × level
   + 0.004 × (share of its provinces occupied)
   + 0.003 if blockaded
E falls 1% a day in peace.
```

It drains the country:
- stability −0.5 × E a day (through the economy's stability target);
- output × (1 − 0.1 × min(E, 1));
- morale −0.001 × E an hour in battle.

**Consequences that stay after the war:**
- **Deaths.** The dead (killed, and died of wounds or hunger) leave the
  population and the workforce for good.
- **Damage.** Bombed and fought-over buildings, roads and rail must be
  rebuilt by the economy.
- **Prisoners.** They are held until a peace deal returns them, or
  released `prisoner_release_days` (720) after it.
- **Equipment.** Captured weapons go to the captor's stockpile.

---

## 10. Peace

A war ends with a **peace deal**. The terms come from the **war score**:

```
war score (attacker's view, −100 to +100)
  = 40 × (enemy provinces occupied / enemy provinces) − 40 × (own provinces occupied / own provinces)
  + 30 × (enemy losses − own losses) / (enemy losses + own losses)
  + 15 × (enemy exhaustion − own exhaustion)
```

| Score | Terms |
|---|---|
| under 15 either way | **white peace**: everything back as it was; prisoners returned |
| 15 to 40 | the loser pays **reparations**: 2% of its GDP a year for 10 years |
| 40 to 70 | the winner keeps the **provinces** it occupies that its goal asked for, plus reparations |
| over 70 | the goal itself: provinces, or a **vassal** (it pays tribute, ECONOMY.md section 10), or a **change of ideology** |

Prisoners are returned in every deal unless the winner keeps them as a
bargaining chip (the hook: `PeaceTerms::prisoners`).

**When peace comes.** A side **offers peace** when:
- its exhaustion is over its personality's limit,
- or its war score is under −40,
- or the goal is reached,
- or the war has lasted `max_war_years` (6).

A side **accepts** when the terms cost it less than fighting on:
`exhaustion + war score / 100 > 0.6`. No side offers peace in the first
`min_days` (120) unless it is exhausted.

The losses term of the score counts against the size of both armies:
`30 × clamp((enemy lost − own lost) / (0.1 × both armies), −1, 1)` (lost
= killed + captured), so a tenth of the armies lost one way is the whole
30 points. A deal never takes the loser's capital or its last province.

**The hook for diplomacy (phase 6).** Every offer goes through
`war::peace::PeaceTable`: who offers, the terms, and the answer. In phase
5 the brains answer it with the rule above. The player answers offers to
them in the war room, and can make offers.

A province changing hands moves everything in it: its share of the
people (they become the new owner's), its buildings, roads, rail and
port, and its deposits. The economy re-balances both countries.

---

## 11. The brains go to war (AI.md hooks)

Strategy (every 3 months) reads the hooks Phase 4 left:
`opportunities`, `war_targets`, `counter`, `hegemon`, `alliance_wishes`,
`threat`, memory (`regard`) and the personality.

### Starting a war

For each possible target `j` (an opportunity, its counter target, or a
hostile neighbour):

```
desire = aggression × (power ratio − 1)          (my power / theirs, from estimates)
       + 0.4 × weakness of j                     (its opportunity reasons)
       + 0.5 × max(0, −regard_ij)                (grudges)
       + 0.6 if j is the hegemon and others are fighting it (gang up)
       − 0.8 × own war exhaustion − 0.3 × own debt need − 0.5 if already at war
aggression = 0.3 + leader (warlord +0.4, coward −0.3, genius 0, builder −0.1, diplomat −0.2)
           + ideology (military rule +0.3, imperialism +0.25, authoritarianism +0.15,
             democracy −0.25, colony −0.2, others 0)
           + risk
```

If `desire > war_threshold` (0.9), and the target can be reached (a land
border, or the sea with transports and warships), the brain starts:

```
declared war (3)        if aggression + 0.2 × (desire − 0.9) > declare_aggression (1.0)
military operation (2)  else if aggression > operation_aggression (0.6)
border skirmish (1)     otherwise
```

A border skirmish lasts at most 90 days: then it grows (if its leader's
aggression is over 0.75) or the border goes quiet. Each nation may
start a war at most once every `war_cooldown_years` (5), and no war
against a nation it made peace with in the last 8 years.

**Gang up.** When a hegemon exists (AI.md, section 5), brains whose
`alliance_wishes` share it join any war against it on the other side, at
that war's level, if their desire (with the gang-up bonus) passes 0.6.

### During a war

- **Escalate** when losing a war it cares about (score under −20, goal
  important) and its escalation cost × (1 − risk) < the stake.
- **Match** an enemy's escalation when its power ratio is above 0.7, or
  **back down**.
- **Make peace** as in section 10.

### Same rules, same fog

The war brains see the enemy through their estimates (with their secret
service's errors), give orders through the same functions as the
player's buttons, and pay the same costs.

---

## 12. On screen

All of it is in `src/game/war_screen.rs` (map layer in `src/map_view.rs`).

- **Armies on the map** (Duck Mold models, recoloured in each nation's
  colour): one soldier figure for about **1,000 soldiers** (up to 16 a
  province and nation), one tank for about **100 tanks** (up to 6), jets
  circling the capitals of nations at war (one for about 40 jets),
  warships in their sea zones (one for about 8), transports at sea
  carrying an invasion. In peace only the armies of nations at war show;
  the war overlay (V) shows every army.
- **Front lines**: the hex edges between provinces held by enemies at war,
  in glowing red. Occupied land is dotted in the holder's colour.
- **Arrows** (orange) for every attack and march under way, and a pulsing
  crossed-swords marker on every battle, sized by the soldiers in it.
  Click a marker (or J: the biggest battle) to zoom in: a panel shows the
  battle live, hour by hour: soldiers and tanks left, morale, rounds,
  misfires, jams, shells, duds, killed, wounded, captured, surrendered,
  deserted, tanks lost, ammo and fuel used, and a bar of the soldiers left.
- **War overlay** (V, after resources and industry): each province in the
  colour of whoever holds it (occupied land bright), battles in red.
- **War room** (K or the button under the economy's):
  - **Wars**: every war (yours first): why, its goal, the ladder with its
    level and the highest reached, a pending escalation and its deadline,
    the war score bar, losses on both sides, the peace the score buys and
    whether the enemy would sign today. E escalate · M match · B back down
    · P offer peace · Y / N answer their offer · D declare on the nation
    whose province is selected · L the level to declare at.
  - **Army**: totals (divisions, soldiers, tanks, jets and pilots,
    warships, transports, hospital, prisoners), each front's divisions,
    soldiers, tanks, equipped %, morale and orders, and the chosen
    front's divisions with their generals (skill, style), training,
    experience, morale and what they are doing. Orders, all optional: A
    attack, H hold, G dig in, R retreat, F reinforce the selected
    province, U hand back to the general.
  - **Supply**: each front's priority (Left / Right) and how much of
    what's sent arrives (or "cut off!"), the stockpile with what has been
    used or lost, and the generals' requests in their own words ("Western
    front needs 200 tanks and 3 weeks of fuel"): Y approve, N deny, T
    Auto on or off.
  - **Reports**: every battle report, newest first, on paper.
- **Battle report** after each of your battles: a card (victory or
  defeat, losses both ways); Enter opens the full report on paper.
- **Sound** (Duck Echo): gunfire when a battle starts, an explosion when
  one ends, and a war-drum loop while you (or, watching, the world) are at
  war. News of your wars, and peace offers, come up as toasts.

---

## 13. Tools and tests

```
cargo run --release --no-default-features -- --war-report [YEARS] [SEEDS...] [--battles]
cargo run --release --no-default-features -- --war-battle MAP SEED [sea]   (one battle, hour by hour)
cargo run --release --no-default-features -- --war-army MAP SEED            (an army's books, day by day)
cargo run --release --no-default-features -- --war-world MAP SEED YEARS     (one world's wars)
cargo run --release --no-default-features -- --spectate-war [MAP]           (watch a war from day one)
```

`--war-report` runs NPC-only worlds and summarises every war: who, why,
how far it escalated, losses on each side, prisoners, and how it ended.

`src/war/tests.rs` holds the war to:
- the same seed gives the same battle, hour by hour;
- every soldier accounted for, every hour: start + recruits = alive +
  killed + wounded + captured + deserted;
- every shell and round accounted for: stockpile at start + made + bought
  + captured = stockpile now + fired + in the field + lost in transit +
  given away;
- no weapons from nothing: a nation's weapons only come from factories,
  imports or capture;
- supply shortages really weaken armies (the same battle with no ammo or
  no food loses);
- landlocked nations can't build or hold navies;
- 50 years on all 3 maps and 6 seeds: wars at a believable rate, the
  world doesn't burn down (no year with over a third of nations at
  declared war), and wars end in peace;
- spectator mode shows a whole war from the first shot to the peace deal
  (`a_whole_war_from_first_shot_to_peace`, and on screen the
  `playthroughs/war.json` playthrough with `--spectate-war`);
- the player's war screens work (`playthroughs/war_player.json`: declare,
  orders, supply with Auto off, a report read, a peace offered);
- a year of total war for 25 nations with brains takes under 1.5 s
  (about 0.8 s: top speed gives it 4 s).

---

## 14. Results

From `--war-report 50 1 2 3 4 5 6 --battles`: 18 NPC worlds (every map,
seeds 1-6), 450 nations, 50 years each, in 114 s on 4 cores. The whole
report, with every war and each world's bloodiest battle, is
[war_report_50y.txt](war_report_50y.txt).

| | Grand Continent | Thousand Isles | Five Shores | All 18 worlds |
|---|---|---|---|---|
| Wars (cold wars not counted) | 195 | 118 | 162 | **475** (0.21 a nation per decade) |
| Battles | 1,854 | 495 | 1,483 | **3,832** |
| Soldiers killed | 987,675 | 231,730 | 911,183 | **2.13 million** |

- **How far they went**: 239 stayed border skirmishes, 45 military
  operations, 162 declared wars, 29 total wars; also 18 cold wars and 24
  wars with proxy suppliers.
- **How they ended**: all 475 in peace. 207 skirmishes went quiet, 101
  surrenders, 83 white peaces, 53 peace deals, 27 operations reached their
  goal, 4 ended with one side backing down from an escalation.
- **The world doesn't burn down**: a nation spends 0.4 of 50 years at war
  on average, and at worst 16% of a world's nations were at declared or
  total war at once.
- **Weak spots, honestly**: 136 of the 475 wars (29%) never had a battle:
  79 skirmishes that stayed a stand-off, and most of the rest wars across
  the sea that neither side had the ships to fight. There were no civil
  wars in these 18 worlds (stability rarely stays under 20 for 45 days);
  the civil-war code is covered by its tests and showed up in other runs.

**Tuning that got here** (each change checked on this report):

| Change | Why |
|---|---|
| Hit shares killed / wounded / captured .25 / .65 / .10; stragglers 2-6% | prisoner counts in the millions |
| Morale shock 5 → 4, recovery 0.03 → 0.06 | divisions broke and never recovered |
| Generals keep a third as a garrison; attacks called off after 10 days | endless battles, empty provinces walked into |
| Called off by **attrition share**, not raw losses | a 20-to-1 attacker gave up after trading 2-to-1 |
| `initiative` 0.6: the side that started the war attacks at 0.6 × its ratio | declared wars with no battle |
| A border skirmish attacks only where the enemy stands | skirmishes marched into empty provinces and nothing happened (zero-battle wars 193 → 136) |
| Air: `air_kill` 0.002 × B × B/(A+B); the air war from level 2; AA scales with bombers | air forces wiped out in weeks |
| Sea: `sea_kill` 0.005; landing losses only into contested zones; neutral fleets don't count | navies gone in a month; blockades never held |
| Occupied provinces make 40% for their owner; the supply base moves when the capital falls | famines and armies with no supply |
| Cold wars only between great powers (the 8 richest), 10 years apart | 415 cold wars in one run |
| War score's losses term against the size of both armies; no peace offers before 120 days | wars decided by tiny casualty ratios |
| A peace never takes a capital or a nation's last province | a landless nation broke the economy |
