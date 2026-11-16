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
engaged defenders = min(soldiers, frontage)   (shared out by size)
engaged attackers = min(soldiers, frontage × flank)
flank = min(1 + 0.3 × ln(attackers / defenders), 2)   (1 if not outnumbering)
```

An attacker that outnumbers the defenders spreads round their flanks: at
20 to 1 its line is 1.9 times as wide. The defenders' line can't grow.

**Why the first Battle of Tolsai went wrong.** Tolsai is 8 steppe hexes,
so its frontage is about 34,000 a side. Before the balance pass, 4.58
million attackers (458 divisions, the whole army) piled into it against
219,000 defenders. Only 34,000 of the attackers could fire each hour (0.7%
of them), against 34,000 dug-in defenders (15%). The attackers were winning
by attrition (they lost 0.7% of their men, the defenders 10%), but the
"called off after 30 days whatever the score" rule ended it, and the report
said the defenders held. Three things were wrong, and are fixed: the
flanks above; an attack now sends at most `reserve_frontages` (3) ×
frontage × 2 soldiers (the rest go to other provinces, which opens new
fronts); and a long battle the attackers are winning ends with the
defenders pulling back (below). Tolsai in the same world now starts as
370,000 against 150,000.

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
              entrenchment: × (1 − 0.5 × entrenchment)
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
  morale −= shock (2) × casualties this hour / soldiers × (2 − general skill)
  morale −= 0.002 × (1 − equipped)          (shortages)
  morale −= 0.001 × war exhaustion           (section 9)
each day out of battle:
  morale += 0.06 × (target − morale);  target = 0.55 + 0.3 × training + 0.15 × equipped
a won battle: +0.1 for the winners · a lost battle: −0.15 for the losers
```

Past the **breaking point**:
- **break** (morale under 0.25): the division retreats to a friendly
  neighbour and regroups there (morale recovers out of battle).
  Stragglers are captured (1% to 3%, worse at lower morale). Broken or
  fuelless tanks are abandoned, and the enemy captures them into its
  stockpile.
- **breakout**: with no friendly neighbour, it breaks out through a
  neighbour the enemy holds but has no troops in (not across the sea).
- **pocket**: with nowhere at all to go, it fights on. It surrenders only
  when its morale is under `surrender_morale` (0.05) or it is out of ammo
  and food (equipped under `surrender_equipped`, 0.15). Then
  `pocket_escape` (40%) slip away in small groups and make their way home
  (counted as deserted), and the rest become prisoners; the enemy captures
  their equipment.
- **desertion**: below morale 0.4, soldiers desert, `desertion` (0.8%) ×
  (0.4 − morale) / 0.4 of them a day. Deserters go home: they leave the
  army but stay in the population.

Before the balance pass, 9.5 million soldiers were captured in 50-year
runs against 2.1 million killed: four in five prisoners came from whole
divisions surrendering the moment they broke with no friendly
neighbour. Now captured is about equal to killed (section 14).

### Who wins

A battle ends when one side has no soldiers in the province, or all its
divisions there have broken. An attack is also **called off** (the
attackers pull back in order, without stragglers, and the defenders hold):

```
the attackers' morale < call_off_morale (0.4), or
after attack_days (10): the attackers lost a bigger share of the soldiers they
  brought than the defenders did   (killed + wounded + captured + surrendered
  + deserted) / soldiers at the start, or
after 3 × attack_days (30), if the attackers are losing by that share
```

After 30 days with the attackers **winning** by attrition, the defenders
are worn down: those with a friendly neighbour pull back in order (no
stragglers), and the province falls when none are left.

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
   × (0.5 + training) × terrain. The side that started the war attacks at
   `initiative` (0.6) × the threshold: it has a plan. The level of the war
   limits targets (section 8); a border skirmish only attacks where the
   enemy stands. An attack takes at most `reserve_frontages` (3) ×
   frontage × `flank_max` soldiers, and a province already attacked by that
   many gets no more, so big armies attack on many fronts at once.
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
  `civil_war_stability` (20) for `civil_war_days` (60), each day has a
  `civil_war_chance` (0.6%) of rebellion. What pushes stability that low
  (the war names the biggest drag as its cause): war exhaustion and
  occupation (section 9), a lost war (−12 stability, × 1.5 for becoming
  a vassal or losing the government, × 0.5 for reparations only), famine
  and unemployment (through happiness, ECONOMY.md), a debt default (−10)
  and junk credit, a democracy's unpopular war, and an empire's vassals
  (section 10).
  - **Rebels** take the provinces with the worst unrest, a share of the
    army (up to 40% of the divisions, the least loyal ones) and their
    equipment. They fight with the same rules and are supplied from what
    they hold.
  - **They win** if they take the capital: the ideology changes (a new
    economic model), stability resets to 50, and the war ends.
  - **They lose** when their last division is gone.
  - **A settlement** after `civil_war_years` (3) of stalemate: the rebels
    lay down their arms for an amnesty and rejoin the army.
- **War of independence**: a vassal fights its overlord (a military
  operation; section 10).
- **Blockade**: a navy controls an enemy's coastal zones (from a
  military operation up) and its sea trade is cut (section 7).

---

## 9. War exhaustion and consequences

Each side's **war exhaustion** E grows every day:

```
dE = 10 × (killed + wounded + captured that day) / population
   + level rate: skirmish 0.0015, operation 0.001, declared 0.0006, total 0.0006
   + 0.0025 × (share of its provinces occupied)
   + 0.0015 if blockaded
E falls 1% a day in peace.
```

(Before the balance pass the level term was 0.002 × level a day, so a
declared war reached the 0.5 peace-offer line in about 80 days and wars
averaged 0.1-0.3 years. Now how much exhaustion a side bears before
suing for peace grows with the stakes: section 10.)

It drains the country:
- stability −0.25 × E (through the economy's stability target);
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

**When peace comes.** The deeper the war, the more a side bears before
it gives up: `stake` by level is skirmish 1, operation 1.5, declared war
2.5, total war 9. A side **offers peace** when:
- its exhaustion is over `offer_exhaustion` (0.5) × stake × (1 + risk),
- or the war is hopeless for it: its score under −`hopeless` (operation
  40, declared 60, total 85) after `min_days` (120) × stake,
- or it is 40 ahead and the war has gone on `won_years` (operation 0.3,
  declared 1, total 3),
- or the goal is reached,
- or the war has lasted `max_war_years` (6).

A border skirmish grows or fades after `skirmish_days` (45).

A side **accepts** when the terms cost it less than fighting on:
`exhaustion + war score / 100 > 0.6 × stake`. No side offers peace in the
first `min_days` (120) unless it is exhausted.

**Vassals.** A vassal pays tribute (ECONOMY.md, section 10), and costs its
overlord `vassal_output` (1%) of output and `vassal_stability` (3) of
stability each (garrisons, governors, resentment). No empire snowballs:
an overlord holds at most `max_vassals` (3), a vassal can't take
vassals, and when an overlord becomes someone's vassal its own vassals go
free. Each month a vassal weighs its freedom:

```
liberty = 0.2 + 0.5 × min(years as a vassal / 20, 1)
        + 0.8 × min(its soldiers / its overlord's, 1.5)
        + 0.4 if the overlord is at war (military operation up)
        + 0.5 × min(overlord's exhaustion, 1)
        + 0.2 × (overlord's other vassals)
        + 0.3 if their ideologies differ
        + 0.3 × its resentment (−regard)
```

A vassal stronger than 1.3 × its overlord simply leaves. One whose
liberty passes `liberty_threshold` (1.0), with an army at least a third of
its overlord's (or the overlord busy at war), no truce and no war of its
own in `cooldown_years`, fights a **war of independence** (a military
operation), and the overlord's other restless vassals (liberty over 0.7)
join it. If the overlord doesn't win the peace, they are free.

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

For each possible target `j` (an opportunity, its counter target, a
hostile nation, a holder of its lost land, a neighbour, another ideology),
the desire is the sum of its **motives**, less the costs. The biggest
motive is the war's cause, and goes in the report:

```
weakness   = 0.5 × aggression × min(power ratio − 1, 2) + 0.4 × weakness of j
             (its opportunity reasons; without any, half of the first term: "strength")
grudge     = 1.0 × (max(0, −regard) + 0.3 × times it attacked us, up to 3)   (memory)
revanche   = 1.3 if j holds a province taken from it in a peace deal in the last 40 years
ideology   = zeal × max(aggression, 0.2), if their ideologies differ:
             communism 1.0 ("to spread the revolution"), theocracy 1.1 ("a holy war"),
             imperialism 0.7, military rule 0.5, democracy 0.35
resources  = 0.9 × deposit × import share, for a raw good it imports (15%+ of its use)
             found in one of j's border provinces (deposit 0.4+)
costs      = 0.8 × own war exhaustion + 0.3 × own debt need
           + 5 × the share of the world's nations already at declared or total war
aggression = 0.3 + leader (warlord +0.4, coward −0.3, genius 0, builder −0.1, diplomat −0.2)
           + ideology (military rule +0.3, imperialism +0.25, authoritarianism +0.15,
             democracy −0.25, colony −0.2, others 0)
           + risk
```

Being the stronger is never a reason on its own: at least one other
motive must be there. If `desire > war_threshold` (1.2), its power is at
least 0.7 of the target's, and the target can be reached (a land border,
or the sea with transports and warships), the brain starts:

```
declared war (3)        if aggression + 0.2 × (desire − 0.9) > declare_aggression (1.0)
military operation (2)  else if aggression > operation_aggression (0.6)
border skirmish (1)     otherwise
```

A war about a province (lost land, a resource) aims at that province
first. A border skirmish lasts at most 45 days: then it grows (if its leader's
aggression is over 0.75) or the border goes quiet. Each nation may
start a war at most once every `war_cooldown_years` (5), and no war
against a nation it made peace with in the last 8 years.

**Gang up.** When a hegemon exists (AI.md, section 5), brains whose
`alliance_wishes` share it join any war against it on the other side, at
that war's level, if their desire (with the gang-up bonus) passes 0.6.

**Coalitions** (the balance of power). Once a month, when at least
`coalition_min` (3) nations see the same hegemon (the player included),
are at peace, can reach it and have no truce with it, the strongest of
them declares war on it and the others join: a declared war "against X,
grown too strong", at most once in `coalition_years` (12) per hegemon,
never in the first two years or while over 10% of the world is already
at war. No side of any war has more than `max_side` (4) nations.

### During a war

- **Escalate** when losing a war it cares about (score under −30, or −55
  to go from declared to total war) and its escalation cost × (1 − risk)
  < the stake, with a 5% chance a week.
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
seeds 1-6), 450 nations, 50 years each, in about 140 s on 4 cores. The
whole report, with every war and each world's bloodiest battle, is
[war_report_50y.txt](war_report_50y.txt). The report also prints where
prisoners came from, why peace was offered, and what drove exhaustion.

### The war balance pass: before and after

| | Before | After | Target |
|---|---|---|---|
| Border skirmish, years (longest) | 0.24 (0.25) | **0.11 (0.12)** | weeks |
| Military operation | 0.27 (0.50) | **0.79 (2.31)** | months |
| Declared war | 0.10 (0.34) | **1.24 (5.83)** | 1-3 years |
| Total war | 0.12 (0.19) | **3.18 (4.84)** | 2-6 years |
| Civil war | none | 1.51 (3.00) | |
| War of independence | none | 0.42 (1.95) | |
| Captured ÷ killed | 4.5 (9.55M ÷ 2.13M) | **1.0 (62.1M ÷ 60.3M)** | not far above 1 |
| Civil wars | 0 | **63** (3.5 a world) | a few a world |
| Nations whose stability fell under 20 | 0 | 102 | |
| Causes | weakness 463, grudge 12 | **weakness 225, ideology 163, grudge 157, resources 133, civil war 63, independence 62, coalition 42, revanche 2** | many |
| Vassals made / broke free | 87 / 0 | **27 / 20** (1 walked away, 62 wars of independence) | |
| Biggest bloc (overlord + vassals) | 39% of a world | **32%** (the world that starts at 39%: its colonies) | no snowball |
| Wars (a nation per decade) | 475 (0.21) | 848 (0.38), incl. civil and independence | |
| Most nations at declared or total war at once | 16% | 32% | at most a third |
| Soldiers killed | 2.1M | 60.3M | |

Wars are now ten times as long and fought by mass armies, so far more
soldiers die: 60 million over 18 worlds and 50 years (about 3.3 million a
world). Desertion is high too (43 million, 18.5 million of them slipping
out of collapsing pockets to go home): mass armies in long wars bleed
deserters.

What did it:
1. **Morale and surrender** (section 3): morale shock halved (4 → 2), a
   broken division retreats and regroups, breaks out of an encirclement
   through empty enemy land, and a pocket fights on until its morale or
   supplies are gone; then 40% slip away. Defenders dig in better (cover
   0.35 → 0.5 × entrenchment).
2. **Exhaustion and stakes** (sections 9-10): the daily cost of being at
   war is a third of what it was, and the exhaustion a side bears before
   suing for peace grows with the level (stake 1 to 9). A losing side
   holds out longer in a deeper war (hopeless at −60 / −85), and a winner
   presses on (1 / 3 years).
3. **Frontage, flanks and reserves** (sections 3-4): see Tolsai. Armies
   now attack on many fronts instead of piling into one province.
4. **Civil wars** (section 8): lost wars, the empire's burden and war
   exhaustion join famine, unemployment and debt in pulling stability
   down; rebellion needs 60 days under 20, then 0.6% a day; a three-year
   stalemate ends in an amnesty.
5. **Motives and coalitions** (section 11), **vassals** (section 10).
6. **Speed**: battles look only at their own divisions each hour (the same
   result as looking at all, checked on all three maps), so a year of the
   heaviest total war with 25 nations takes about 2.0 s of CPU for the
   economy, brains and war together (top speed allows 4).

### Weak spots, honestly

- Only 2 wars were for lost land: few provinces change hands in peace
  deals (most end in reparations or white peace), so there is little to
  win back.
- The one world whose 39% bloc comes from its colonies at the start ends
  at 32%: its colonies break away slowly.
- One world in six peaks at 32% of its nations at declared or total war,
  just under the one-third rule.

### Tuning in phase 5 (before the balance pass)

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
