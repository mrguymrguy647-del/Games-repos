# Games-repos

Games made with **Duck Forge** (the engine, in
[game-engine-rust-language](https://github.com/mrguymrguy647-del/game-engine-rust-language)),
with art from **Duck Quill**. One folder per game; the engine comes from its
own repo, so the engine and the games change separately.

## The games

| Game | What it is |
|---|---|
| [`duck_craft`](duck_craft) | A Minecraft-style island of blocks: dig, build, craft, survive the night or play in creative. Nothing lasts. Build anyway. |

## Play

You need Rust ([rustup.rs](https://rustup.rs)). In this folder:

```
cargo run --release -p duck_craft
```

The first build takes a few minutes (it fetches and builds the engine);
after that it starts quickly. It runs from any folder: each game finds its
pictures and sounds next to its own `Cargo.toml`.

**Duck Craft's controls**: W A S D walk, Shift run, Space jump, mouse look;
left click dig or hit, right click build (or eat, holding food); 1-9 or the
wheel pick a slot; E the bag and crafting; V view from behind the duck;
Escape pauses (mode, music, save and quit). Creative: Space twice to fly.

It saves every 30 seconds and on **Save and quit** (closing the window with
its X can lose the last 30 seconds). The save is `duck_craft_save.dat` in
the folder you ran it from; delete it for a new island.

## Check a game

```
tools/verify.sh duck_craft
```

builds it without a window, runs its play tests, replays its recorded
playthroughs (`<game>/playthroughs/`), fuzzes it 50 times and prints one
report. Run it after any change.

## How it's laid out

```
Cargo.toml            the workspace: the games, and the engine they use
tools/verify.sh       the whole check for one game
duck_craft/
  Cargo.toml          the game, and its switches (window, gpu, audio)
  src/                main.rs and the rest; tests.rs and bot.rs are its tests
  assets/             what the game loads: pictures, sounds, music
  art/                the Duck Quill scripts the pictures come from
  audio/              the script the music and sounds come from
  playthroughs/       recorded play, replayed by verify
  ISSUES.md           open issues met in Duck Forge, Duck Quill, Duck Echo
```

## The engine

Every game uses the same Duck Forge, from `[workspace.dependencies]` in
`Cargo.toml`, pinned to one commit, so a change to the engine never breaks
a game by surprise. To move to a newer engine, change the `rev` (or use
`branch = "main"`), then run `tools/verify.sh` for each game.

## Add a game

1. Make a folder, like `my_game/`, with a `Cargo.toml` like
   `duck_craft/Cargo.toml` (change the name) and `src/main.rs`.
2. Add it to `members` in the top `Cargo.toml`.
3. Keep its assets in `my_game/assets/` and load them from
   `concat!(env!("CARGO_MANIFEST_DIR"), "/assets")`.
4. `cargo run -p my_game`, and `tools/verify.sh my_game`.
