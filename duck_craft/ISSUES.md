# Open issues met while making Duck Craft

A log for improving Duck Forge, Duck Quill and Duck Echo, kept while
building the game. Only open issues are here: resolved ones are taken out
(git history has them). Duck Forge's numbers stay as they were, so a gap
is an issue that's been resolved, or folded into another that's the same
gap (26 into 41, 39 into 23, 44 into 19). "The game works round it" means the
game copes; the tool still has the gap.

## Duck Forge

16. **A game can't tell it's running headless.** Saving must never happen
    in a replay or fuzz run (or the next replay would start from a saved
    island and stop being repeatable), but `Context` doesn't say. `main`
    looks for `--headless` in the command line itself. A
    `ctx.is_headless()` (or a `Config` flag the CLI fills in) would do.
17. **No hook when the window closes.** Closing the window with its X
    button ends the program without calling the game, so the last moments
    aren't saved. The pause menu saves when you quit from it, and the game
    autosaves every 30 seconds, but the close button still loses up to
    30 s. A `Game::quit(&mut self, ctx)` called on any way out would fix it.
19. **The fuzzer cries "freeze" on menus.** It presses every key the game
    reads, and a menu ignores most of them, so ten seconds on a menu
    pressing keys it ignores looks like a frozen game. It got worse as the
    game read more keys (the inventory's Q, R, Tab, Backspace). Duck Craft
    gets round it: every menu takes many keys (W/S, A/D, arrows, Tab) and
    reports its state. A way to say "paused" (`ctx.report_paused(true)`),
    or the fuzzer pressing only the keys the current screen reads (a game
    could report them), would be fairer.
20. **`Simulation` tests have no way to step until something is true.**
    The bots and the showcase write `for _ in 0..N { if done { break } sim.step() }`.
    A `sim.run_until(limit, |game| ...)` would read better and stop
    off-by-one waits.
22. **The fuzzer never uses the mouse.** It finds the inventory screen (E)
    but never clicks in it, and it can't aim, so it never fights. The
    game's own `random_clicks_never_lose_or_copy_things` test does 5,000
    random clicks instead. A `--fuzz-mouse` (random moves, clicks and
    drags, within the canvas) would cover mouse-driven screens.
23. **No way to draw 3D into part of the screen.** The duck in the
    inventory is drawn with `draw_mesh_in_view` at a depth worked out so
    it lands in its window (`draw_duck_preview`), and made small enough
    never to poke out, because nothing can clip it. A
    `canvas.draw_mesh_in_rect(mesh, transform, rect)` (its own little
    camera and viewport), or a `canvas.clip(rect)` push and pop for 2D
    and 3D drawing, would make previews, character screens and scrolling
    lists easy.
24. **`draw_mesh_in_view` is lit by the scene's light**, so the preview
    duck is dark at night unless the game swaps the light just before
    drawing it (it does). Fine, but worth a sentence in its docs.
25. **No way to ask for an atlas tile's texture points without the
    `Tileset`.** The plants keep their own copy of `atlas.uv_rect(tile)`
    for each kind, taken when the art loads. `BlockPalette` could hand
    them out, since it has the atlas.
27. **The windowed game shows the system's pointer, recorded videos
    don't.** The showcase draws its own arrow (`draw_pointer`), off in
    the real game. A video option to draw the pointer would help every
    game with menus.
28. **`play_wav` has no volume.** A far-off Hollow should groan quietly,
    but a `.wav` plays at one loudness; the game ships a second, quieter,
    muffled file (`groan_far.wav`). `ctx.play_wav_panned` now puts a
    Hollow's groan on the side it's on, but the side is fixed when it
    starts: turn while it groans and it doesn't move. A
    `ctx.play_wav_at(&wav, volume, pan)` (or sounds placed in the world,
    followed as the listener turns) would do.
29. **A `Wav` can't be made from samples in code.** The music had to be
    made in Python and shipped as files, where a
    `Wav::from_samples(&[f32], rate)` would let a game make sounds at
    start.
30. **Loading all the sound in every play test** (each `arena` loads the
    art, now 39 MB of `.wav`) takes the tests from 0.5 s to about 9 s. A
    load cache in `Wav::load` (or the engine sharing loaded files) would
    bring it back.
31. **Replays don't use the tune values they were recorded with.** The
    night showcase starts at dusk (`start_time_of_day`). A replay now
    warns when its tune values differ from the recording's, but it still
    needs `--set start_time_of_day=0.47` to play the same game. An option
    to replay with the recorded values would finish it.
32. **Labels under a menu make `verify` fail.** The title and pause
    screens are drawn over the world, and a duckling's label under a
    button counts as "spawned on top of each other". The game labels
    world things only while playing; a way to mark a menu as a layer
    above the world would be cleaner.
34. **Fuzz runs all start at the same time of day.** The night (and the
    Hollows) only got fuzzed by adding a tune value for the starting
    time and fuzzing with `--set start_time_of_day=0.6`. That works well;
    worth a line in the skill ("fuzz the hard parts with --set").
35. **`set_loop_volume` every frame fills the log.** Fading five loops
    for a whole run would write thousands of volume changes; the game
    only sends a change when it's bigger than 0.004. The engine could
    skip changes too small to hear.
36. **`Simulation` has no `frame()`;** `sim.context().frame()` works.
37. **Only `.wav` loads, so the sound is big.** At full quality (44.1 kHz,
    stereo, 16-bit) 64 s of music is 11 MB; the game's sounds are 39 MB,
    nearly all music, rain and wind, where `.ogg` would be about 2 MB.
    Reading Ogg Vorbis (or FLAC, lossless at about half) in `Wav::load`
    would keep repos and downloads small.
38. **No gradient or pattern fill.** The inventory's PlayStation-style
    panels are shaded in bands that meet in a dithered checkerboard, which
    takes hundreds of one-pixel `fill_rect`s a frame. A
    `fill_rect_gradient(rect, top, bottom, dither: bool)` (or an ordered
    dither option on `fill_rect`) would do it in one call, and look more
    like the real thing.
40. **No way to start a fresh depth buffer for 3D drawn over 2D.** The
    items in the hotbar are small 3D models drawn in view; the hand, drawn
    in view just before, left its depths behind, so it hid the items it
    overlapped. Only a world draw makes the next in-view draw forget old
    depths, so the game draws an invisible mesh to get one
    (`fresh_depth`). A `canvas.clear_depth()` would say what it means.
41. **Every block is a full cube.** `BlockWorld` draws only cubes, so
    anything else is faked or kept outside the grid. Tall grass, flowers
    and torches are crossed pictures in their own map and mesh
    (`plants.rs`), rebuilt whole when one changes (a torch's light is kept
    by the game too). An open door is a frame drawn on a see-through cube.
    A block shape in `BlockKind` (cube, slab, panel on one side, crossed
    pictures) would let them all sit in the grid, chunk by chunk, and let
    a door be a door.
42. **Block kinds need unique names, but one thing can be two kinds.** A
    door's halves, open and shut, are four kinds of block, and
    `BlockPalette::new` panics on two kinds both called "Door". They had
    to be called "Door (top)" and so on. A kind's key could be separate
    from the name shown, or the panic could say which blocks clash.
43. **No pathfinding.** The Hollows' way round walls is a flow field the
    game builds itself (`hunt.rs`, a breadth-first search over the
    blocks). Any block game needs one: a `BlockWorld::flow_field(goal,
    can_stand, can_step)` would save every game writing its own.
45. **`BlockWorld::draw` draws every chunk out to the horizon.**
    `draw_mesh` skips a chunk that's off the screen, but not one past
    the fog's far edge, where it would only be fog: a world 192 blocks
    across is 144 chunks, most of them sea. The game culls by distance
    itself (`draw_chunks`, with `chunk_mesh`); `draw` could skip chunks
    past the fog (or a distance it's given).
46. **A test can't call game code that needs a `Context`.** `Simulation`
    only steps the whole game, so the Other's tricks had to take a bare
    `Rng` instead of the `Context` they use in play, to be tested one
    by one. Something like `sim.with_context(|game, ctx| ..)` would do.
47. **`verify` only replays `<game>.json` and `<game>-*.json`.** The
    night showcase (`duck_craft_night.json`, an underscore) is never
    replayed, so it went stale unnoticed; and it needs a tune value
    (`start_time_of_day`) that a replay can't carry (see 31).

## Duck Quill

1. **First build is slow (about 8 minutes)**, because `cargo build
   --release -p quill-cli` also builds the app's GUI crates. A leaner
   CLI-only build (or a prebuilt binary in the release zip) would help
   agents on servers.
2. **`export` needs animations.** A script of still tiles (one `define`
   each) is refused: "this script has no animations". Every tile needs a
   one-frame animation that `use`s it. `quill atlas` is the other route,
   but one script per tile.
3. **No noise or speckle fill for pixel art.** Grass, dirt, stone and
   leaves were generated pixel by pixel in Python (`art/make_blocks.py`).
   A `speckle`/`noise` command on a pixel canvas, seeded, would let
   agents do it in the script.
4. **A skin needs the layout twice.** The skin's face layout lives in
   `make_skin.py` and again in the game (`skin_box` calls); Duck Quill
   could export a JSON of named regions (like the atlas's `tiles`) that
   the engine reads, or the engine could take a Minecraft-style skin
   layout directly.
5. **`check` warned that ~26 palette colours were "never used"** though
   they are used inside `define`d symbols (via `pixels` `colors`). Worth
   a look: either a false warning or symbols aren't scanned.
6. **More than 52 colours in one pixel script.** The atlas has 70
   colours, and `pixels` rows use one character a colour, so
   `make_blocks.py` ran out of letters and uses digits and symbols too.
   A `colors` map of short names (two characters) would read better.

## Duck Echo

- Not used: it isn't ready. The music, wind, rain, thunder and the
  Hollows' sounds come from `audio/make_sounds.py` (numpy and scipy,
  written for this game); short sounds (dig, place, step, quack, click,
  doors, bangs) are `Sound { .. }` in code. What it took, for Duck Echo's
  sake: a felt piano (harmonics, detuned strings, two-stage decays),
  inharmonic bells, a music box, band-passed saws for voices and a
  choir, whispers (breath through vowel formants), a Shepard tone, tape
  wow and flutter, a VHS treatment, a convolution reverb with a
  different room per ear, dither, and loops made seamless by rendering
  three passes and keeping the middle one (drones and LFOs tuned to
  whole cycles per loop, noise made periodic in the frequency domain).
