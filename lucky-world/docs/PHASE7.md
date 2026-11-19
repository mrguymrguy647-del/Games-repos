# Phase 7: a finished game

What turned LUCKY WORLD from a simulation into a game you can start, save,
win and lose. The code is in `src/game/shell.rs` (menus, settings, help,
pause, results, notifications, music, tooltips), `src/game/tutorial.rs`,
`src/save.rs` and `src/save_codec.rs`, `src/sim.rs`, `src/victory.rs`,
`src/econ/underdog.rs`, `src/settings.rs`, `src/fullscreen.rs`,
`src/bot.rs` (the bot that plays), `src/player_test.rs` and
`src/balance.rs` (the balance and endurance tools).

## 1. The screens

```
Main menu ─┬─ Continue (the newest save)
           ├─ New Game: a world (or random), difficulty, length ─ the wheels ─ the map
           ├─ Tutorial: the same, on the Grand Continent at easy, with a guide
           ├─ Load ─ Settings ─ Help ─ Quit
The map ── Esc: the pause menu ─ Resume, Save (6 slots), Load, Settings,
           Help, Main menu, Quit (the last two save first)
           a result ─ the victory or defeat screen ─ Play on / Main menu
```

`--title` starts on the old title screen (as the tests do), `--spectate`
straight into a world of brains, `--saves DIR` keeps saves elsewhere,
`--years N` and `--difficulty NAME` set up the next game from the command
line (for trying the ending).

## 2. Saving

A save is everything that changes once the nations are on the map: the
wheels, the placed world, the economy, every brain (with its memory and
estimates), the wars (divisions, generals, battles, reports, prisoners),
diplomacy (treaties, relations and their reasons, the inbox, talks, spies)
and **every random number generator's exact state**, plus the game's own
setup (difficulty, length), its story so far, the clock's speed and the
tutorial's step.

The file (`saves/*.lws`):

| Bytes | |
|---|---|
| 12 | `LUCKYWORLD\n\0` |
| 4 | the save format's version (now 1) |
| 4 + n | a summary for the Load screen (nation, map, date, difficulty, when) |
| rest | the game, bincode, gzip-compressed |

- The version comes first. A save from a newer game, an older format the
  game no longer reads, a cut-short file or garbage is refused with a
  sentence (the Load screen lists it in red with the reason), never a
  crash. `save::READS` lists the formats this build reads.
- Writes go to a `.tmp` file first, then replace the old save, so a crash
  mid-save never ruins it.
- **Autosave** every game year (Settings can turn it off), keeping the one
  before (`Autosave previous`), and when leaving a game for the menu or the
  desktop. **Quick save** F6, **quick load** F9.
- `Rng` keeps its state private, so `save_codec::rng` reads it from its
  `Debug` text and rebuilds a generator by inverting Duck Forge's seed
  scramble (ISSUES.md item 48).

**The promise, tested** (`save::tests`): on all three maps, a world that
saves at day 400, loads, and runs 500 more days is byte-for-byte the world
that never saved, checked every 50 days. One `HashMap` (diplomacy's
refusals) had to become a `BTreeMap` for that: its order differed after a
load.

## 3. Winning and losing

`data/victory.toml` holds every number.

| | When |
|---|---|
| **Conquest** | you own half the map's provinces |
| **Economic power** | your GDP is 2.5 times the next biggest, for 10 years in a row |
| **Diplomatic** | your alliance (you, your allies by alliance treaty, your vassals) owns 60% of the map, with at least 2 allies, you its richest member, for 5 years in a row |
| **Score** | when the game's length (50, 100 or 150 years, or none) runs out, the best score wins; anyone else gets "Time's up" |

The **score**: 450 × your share of the world's GDP + 300 × your share of
its provinces + 150 × your share of its people + 100 × your technology
against the best + stability (0-100). A vassal's score counts half.

| Loss | When | Play on? |
|---|---|---|
| **Conquered** | you own no province (occupied land is still yours until a peace deal hands it over), or no people | no |
| **Overthrown** | rebels take your capital in a civil war (a new government) | yes |
| **Vassal** | a peace deal makes you one | yes: win independence and the defeat is cleared |

After a win you can play on; the other wins still count, each once.

**The story** (`victory::Fate`, saved with the game): each of your wars
(who started it, when it ended and how), your treaties, booms and bad
years, new best and worst ranks, your economy doubling, becoming the
world's leader, becoming a vassal and winning freedom, each a *moment*
with a weight. The result screen shows your 16 wheels, your wars, your
score year by year, and your four best and four worst moments.

## 4. The underdog rule

`src/econ/underdog.rs`, `[underdog]` in `data/victory.toml`. A world
rule, for every nation (the player's or a brain's) whose start is below
half of the median nation's (65% GDP share, 35% soldiers):

- development aid of up to 6% of its GDP a year from a world fund (the
  budget shows it), for 12 years, fading over the last third;
- 5 years in which no brain starts a war on it;
- a floor of up to 25 under its stability.

The help is strongest for the weakest (its strength is
`sqrt(1 - start / (0.5 × median))`). A strong start gets nothing extra.

## 5. Difficulty

`[difficulty.*]` in `data/ai/brain.toml`. Difficulty only changes how
well the brains think; it never gives them money, soldiers or sight.

| | noise | horizon | moves a week | rethinks | "too strong" at | misjudges wars | looks for war |
|---|---|---|---|---|---|---|---|
| Easy | 0.12 | 0.6 | 1 | 1.6× slower | 1.25× the share | ±35% | every 45 days |
| Normal | 0.05 | 1.0 | 3 | as built | as built | no | every 30 days |
| Hard | 0.02 | 1.3 | 3 | 0.8× | 0.9× | no | every 20 days |
| Brutal | 0 | 1.6 | 4 | 0.6× | 0.8× | no | every 14 days |

Normal is exactly the brains of phases 4-6, so their balance numbers hold.

## 6. Settings

`lucky_world.settings.toml` beside the game (written by the Settings
screen; a hand-edited file with nonsense in it is cleaned, not refused):

- **Graphics quality**: Low, Medium (all on the graphics card, no
  smoothing), High (the wheels smoothed 2×), Ultra (3× and a glow). The
  world map is always drawn by the graphics card, whatever the quality
  (ISSUES.md item 15). `lucky_world.tune.json` can still override.
- **Window size** 1280 × 720 or 2560 × 1440 at the next start (the engine
  only scales by whole numbers, item 47; drag the window for any size).
- **Fullscreen**, borderless, asked of the window system directly (item 46).
- **Music** and **effects** volumes, **UI scale** (90-115%: the windows
  and menus, drawn through a 2D camera), **autosave**, **notifications**
  (only what matters most, important news, everything), **tooltips**.
- **Keys**: the 25 map actions (windows, camera, pause, quick save and
  load, help, the log) can each be rebound; binding a key that's taken
  swaps the two; the engine's own keys (F1, F3, F10, F12) and Esc, Enter,
  the arrows and 1-6 are kept.

## 7. Learning the game

- **Tutorial** (`src/game/tutorial.rs`): 15 steps that each wait for the
  player to do the thing: spin, skip, the country card, the rivals, the
  map, the clock, the budget and a tax, chains, the war room's supply
  page, a treaty proposal, how to win. Enter skips a step, Backspace ends
  it; the game goes on afterwards.
- **Tooltips** (`data/tips.toml`): every number on the top bar, the budget
  (each lever and each line), the market's columns, the stockpile, the
  war room's ladder, score and losses, diplomacy's relations and
  reputation, the spies' chances, the province panel, the score and how
  close each victory is.
- **Help** (F2, or the menus; `data/help.toml`): the goal, the wheels,
  the map, the economy, war and supply, diplomacy and spies, difficulty,
  saving.
- **Notifications** (`notify`): one at a time with the rest queued (at
  most three; the least important go first), the same words within 30
  seconds dropped, a sound at most every two seconds, war on you and peace
  offers cut in, and the settings choose which levels pop up. L shows the
  last 300.

## 8. Sound and music

Music follows the game: a menu theme, a quiet peace theme on the map, and
the war drums while you're at war, crossfading. Every important action has
its sound: saving (a chime), a refusal (a muted buzz), a treaty signed (a
quill and a seal), war declared (a horn), peace (a bell), building (a
hammer), a spy sent, menu moves, victory and defeat. The effects volume
scales them all.

## 9. Tools

```sh
cargo run --release --no-default-features -- --balance 30 20     # thousands of starts, with and without the underdog rule
cargo run --release --no-default-features -- --wheel-effects 1 2 3  # what each wheel result changes on day one
cargo run --release --no-default-features -- --long-game 100 1     # 100 years on each map: speed, memory, save size by year
cargo run --release --no-default-features -- --tour out/tour 25    # the bot plays and takes the screenshots
```

Their outputs from this phase are in `docs/phase7_balance.txt`,
`docs/phase7_wheel_effects.txt` and `docs/phase7_long_game.txt`.
