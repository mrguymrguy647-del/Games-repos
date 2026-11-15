# Problems and missing features found in the Duck tools

Found while building LUCKY WORLD's wheel screen (November 2026), against:

| Tool | Commit |
|---|---|
| Duck Forge | `9ebb6ca` |
| Duck Quill | default branch |
| Duck Echo | default branch |
| Duck Mold | default branch |

Each entry says what happened, where, and what would fix it. Entries are
ordered by how much they cost.

## Duck Forge (engine)

### Bugs and traps

1. **3D vanishes unless `draw` starts with `canvas.clear()`.** The depth
   buffer is only reset by `clear()` (`src/canvas.rs:537`), not at the start
   of each frame. LUCKY WORLD began `draw` by blitting a full-screen backdrop
   picture instead of clearing. Last frame's depths then hid this frame's 3D,
   in patches that moved with the camera: half the wheel and most of the
   stage were missing.
   - This took the longest to find in the whole project.
   - Fix: reset depth when the engine starts each frame (where `reset_view`
     is called, `src/simulation.rs:1206`), or warn in the docs next to
     `blit_sprite`/`draw_background`.
2. **The skill's clone URL is wrong.** `skills/duckforge/SKILL.md:18` clones
   `.../game-engine-rust-language`; the repository is `Duck-Forge`.
3. **`LoadError` isn't in the prelude**, although `Sprite::load_png`,
   `Wav::load`, `Font::load` and `Mesh::load_glb` all return it. An assets
   loader needs `duckforge::LoadError` spelled out.

### Games outside the engine's repository

4. **The tools assume every game lives in the engine's `examples/`.**
   `tools/verify.sh`, `tools/new_game.sh`, the default tune-file path
   (`examples/<game>.tune.json`) and the Studio all expect it. LUCKY WORLD is
   its own repository with Duck Forge as a git dependency, which works well
   for building, but:
   - `verify.sh` can't run on it (I used `cargo test`, `--headless` and
     `--fuzz` by hand);
   - `new_game.sh` can't start a game in its own folder.

   Fix: a `--manifest-path` or `--game-dir` option, or a `new_game.sh --outside DIR`.
5. **A game can't have its own data files watched or recorded.**
   `files::read_text` is `pub(crate)` (`src/files.rs:68`), so only tile maps
   get hot reload, `reload()` calls and replays that keep the file's text.
   LUCKY WORLD's wheel tuning lives in TOML, so editing `wheels.toml` needs
   a restart, and a recording doesn't keep the odds it was made with.
   Fix: a public `duckforge::read_text_file(path)` that notes the file the
   way tile maps do.

### Drawing

6. **The 2D canvas has no arcs, wedges or rings, and no anti-aliasing.**
   `fill_circle` and `fill_triangle` are hard-edged; supersampling is 3D only
   (`src/lights.rs:322`). A prize wheel is all wedges, so I built it as a 3D
   mesh to get smooth edges.
   Fix: `fill_wedge(center, r_in, r_out, from, to, color)`, plus an
   anti-aliased option for 2D shapes.
7. **Text can't be drawn onto a texture directly.** Slice labels had to be
   drawn on a spare `Canvas`, copied out with `get_pixel` one pixel at a
   time, then made into a `Sprite::from_pixels`.
   Fix: `canvas.to_sprite()` or `Sprite::from_canvas(&canvas)`.
8. **Mesh textures cut alpha at 50%** (`src/frame3d.rs:1453`), so
   anti-aliased text on a see-through texture gets hard, jaggy edges. I
   worked round it by painting each label on an opaque patch of its slice's
   colour.
   Fix: blended (or alpha-to-coverage) textures, at least with supersampling.
9. **Turned and scaled sprites use nearest-neighbour sampling only**
   (`draw_placed`). The spinning hub, the flapping pointer and the scaled
   glows and sparkles look stair-stepped next to the smooth 3D.
   Fix: a smooth (bilinear) option in `Placed`.
10. **No additive blending for 2D.** Glows are plain alpha (`opacity`), so
    they grey things over instead of lighting them up.
    Fix: `Placed { blend: Add, .. }`.
11. **`Camera3D::project` needs a `&Canvas`**, which `update` doesn't have.
    To put sparkles where the 3D pointer is, the game keeps a spare
    1280 x 720 canvas just for its size.
    Fix: `camera.project_to(point, width, height)`.
12. **No rounded rectangles.** All panels and buttons had to be pictures
    (made in Quill), so every card size is its own picture. See Quill item 5.

### Sound

13. **One-off sounds can't change pitch** (documented in AI_GUIDE section 12).
    A spinning wheel's ticks want to rise and fall a little, so I rendered
    two tick pitches in Echo and alternated them.
    Fix: `ctx.play_with(&wav, volume, pan)` plus a `pitch` (playback-rate)
    argument.

### Docs

14. **The first look at the API is hard to find.** For "how do I draw a
    textured quad from my own vertices", the answer (`add_quad_uv` and its
    corner order) is only in section 7 of a 2,000-line guide.
    `cargo doc` covers it, but the skill doesn't point there.

### Speed

15. **Bloom and supersampling are very costly, and they turn off the GPU.**
    At 1280 x 720 on this 4-core cloud machine, drawing cost per frame:

    | Setting | Cost per frame |
    |---|---|
    | Bloom (`Bloom::default()` radius) | about 67 ms |
    | `set_supersample(2)` | about 25 ms |
    | One big floor disc (`Mesh::cylinder` scaled to 16 m, mostly off screen or behind the near plane) | about 13 ms |

    Any lit effect also moves all 3D off the graphics card onto the CPU
    (`src/render3d.rs:1280`). LUCKY WORLD went from 108 ms to 30 ms by
    painting the floor into the backdrop, faking the glow with a 2D sprite,
    and making supersample and bloom tunable (`lucky_world.tune.json`).
    Fix:
    - bloom only around glowing pixels' bounding boxes;
    - supersample only where 3D was drawn;
    - GPU versions of both;
    - `--time` could say which effect costs what.
16. **Headless runs don't show drawing cost.** A normal `--headless` run
    draws only the frames it saves, so the game looked like 190 fps until
    `--time` showed 9 fps. The run summary could print the drawing time of
    the frames it did draw.

## Duck Quill (art)

1. **`quill export` can overwrite the source script.** It names its JSON
   `<script-name>.json`, so with `--out` pointing at the script's own folder
   it silently replaces the script with sprite-sheet JSON. The next run fails
   with `unknown field 'image'`. `docs/sprite-sheet-format.md:3` doesn't warn
   about this.
   Fix: refuse to write over the input, or name the output `<name>.sheet.json`.
2. **No arc, wedge, pie or ring primitive** (command list at
   `crates/engine/src/script.rs:437-469`). Wedges need their arc points
   worked out outside Quill.
   Fix: `{ "cmd": "wedge", "center", "radius", "inner", "from", "to" }`.
3. **No conic (sweep) gradient**: only linear and radial
   (`docs/script-format.md:306-325`).
4. **Text that runs off the canvas is clipped silently.** The first logo
   (`LUCKY WORLD` at size 132 on a 900 px canvas) lost its first and last
   letters, and `quill check` said nothing. `fit` fixes it once you know.
   Fix: a `check` warning when text or shapes leave the canvas.
5. **No 9-slice or stretchable frames.** Each card size is its own script
   (7 card and panel scripts, differing only in size and border colour).
   Fix: export a 9-slice (the corner and edge sizes) that the engine can
   stretch, or parameters on the command line (`--set width=400`).
6. **`export` refuses still pictures** ("this script has no animations").
   Duck Forge's skill says `quill export art/x.json --sheet` in general, but
   still UI art needs `render` (or `atlas`).
7. **`render` always writes a `.quill` file** next to the PNG, and there's
   no flag to skip it; `--bg` also writes `<name>-view.png`.
   `tools/make_assets.sh` deletes both.
8. **The font is Inter only, with no weight option.** Text rotated by
   `repeat` comes out upside down on the bottom half of a circle.
9. **`atlas` defaults to `--padding 0`**, so soft glows packed side by side
   bleed into each other when filtered.
10. **Stale docs.**
    - The README's project layout is missing `filters`, `noise`,
      `critique`, `senses`, `study`, `animate`, `reuse` and several CLI
      modules.
    - Its build steps are Windows-only (`.bat`); the Linux path is only in
      the skill and QUICKSTART.
11. **Works but isn't documented:** `$param` inside a `use`'s `colors` map.

## Duck Echo (sound)

1. **`README.md:121` is out of date.** It says Duck Forge mixes stereo down
   to mono; current Forge keeps stereo (`sound.rs:387-390`).
2. **`.mcp.json` runs `cargo run --release` inside the repository.**
   - It writes `target/` into the clone.
   - A cold build (about 2 minutes) is longer than the 30 s MCP start
     timeout, and the README says "about a minute".

   Fix: point at a built binary, or build first in the instructions.
3. **`render` has no `--quiet`.** Its one-line report goes to stderr, so
   `>/dev/null` doesn't silence it in a build script.
4. **No folder render.** `duck-echo render sound/*.toml --out assets/sound`
   would save the loop in every game's build script.
5. **`analyze` calls a hard-hitting fanfare "slow attack".**
   `attack_seconds` measures the time to the loudest moment, not the first
   onset; the guide explains this, but the summary line misleads.
6. **The Studio needs `libasound2-dev` on Linux.** The README doesn't list
   it; the CLI and MCP server don't need it.

## Duck Mold (3D models)

1. **The first build is much slower than the README says.** The README says
   "under a minute", but a plain `cargo build --release` also builds the
   Studio (eframe, wgpu) and took 7.5 minutes. The CLI alone
   (`-p duck-mold-cli`) took under 2.
2. **`edit --help` lists 15 commands on its first line**; the list under it
   has 40+ (`pull`, `inset`, `set-pose`, `mirror-pose`...).
3. **The quick start writes into the repository.**
   `duck-mold export examples/robot.toml` writes `examples/robot.glb`; show
   `-o` there.
4. **A part hanging down needs a socket with `facing = "down"`**, and two
   sockets facing the same way "roll over". It's documented under "Joining",
   but hard to find when you need it.
5. **Layering parts is hard to guess.** For a thin gold band under a stage,
   I first attached the trim to `stage.top` with a bigger radius, which
   covered the whole stage. Ordering parts needs a mental model of the
   sockets. A `duck-mold render` before export catches it; the Forge game
   was where I saw it.

---

# Phase 2 (the 3D world map)

Found while building the world map, against the same commits. Numbering
carries on from Phase 1 for each tool. Nothing here blocked the work, so
Duck Forge itself is unchanged.

## Duck Forge (engine)

17. **Canvas effects are sticky, so one screen's settings silently move the
    next screen onto the CPU.** `set_supersample`, `set_bloom`, `set_lamps`
    and the rest stay on the canvas until changed. The wheel screen sets
    supersample 2. Without an explicit `set_supersample(1)` and
    `set_bloom(None)` at the start of every map frame, the map would be drawn
    by the CPU lit renderer even on a real GPU, with nothing saying so.
    - There's no `canvas.reset_effects()`.
    - There's no way to ask which renderer drew the last frame.
    - The "can't use the GPU" line prints only once, at start.
    - Fix: reset effects every frame (like the camera stack), or add
      `reset_effects()` plus a `renderer_used()` the debug overlay shows.
18. **The fuzzer only finds keys the first screens read.** `--fuzz`'s
    practice runs never reach the world map, so the map's keys (Tab, M, Q/E,
    R/F, T/G, C, H) weren't pressed: coverage said `camera_yaw always 0`.
    `--fuzz-keys` fixes it by hand.
    Fix: grow the key list during the fuzz runs themselves (keys read on any
    frame), or let a game declare its keys.
19. **The `Key` enum has no `+`, `-`, `=`, Home, End, PageUp, PageDown or
    Delete**, so zoom couldn't be the usual +/- and is on R/F instead.
20. **No text in 3D.** Nation names and flags are projected by hand each
    frame (`camera.project`), with the game's own overlap test so names don't
    sit on each other. A `canvas.draw_font_text_at(&font, text, world_point,
    &camera, ..)` with optional collision culling would save every map game
    the same work.
21. **`draw_line` is always 1 pixel**, with no width. The camera's box on the
    minimap is drawn twice, one pixel apart, with a dark copy to be seen.
22. **`Mesh::bounds()` returns a bare `(Vec3, Vec3)`**, while Duck Mold's
    `inspect --json` says `bounds.min`/`bounds.max`. `b.min` doesn't compile
    (found by the helper checking the models).
23. **No `Mesh` part ranges after `merge`.** The capital's "roofs" are
    recoloured per nation before merging. After a merge there is no way to
    find a model's parts again: to recolour one nation's capital later
    (a captured city), the whole cities mesh must be rebuilt.
    Fix: `merge` could return the triangle range it added.

Worth keeping as they are:
- The GPU path works: with a Vulkan driver (lavapipe here), the map is drawn
  by wgpu and looks the same as on the CPU.
- The GPU mesh cache (address plus fingerprint) means static meshes upload
  once.
- `--time` shows the drawing cost.

## Duck Quill (art)

12. **Field names differ between shape commands.** `rect`, `ellipse` and
    `polygon` take `outline` for a border, but `path` takes `color` and
    `size` and refuses `outline`
    (`command 4 (path): unknown field 'outline'`). The error is clear, but
    the names should match.
13. **9-slice again (Phase 1 item 5).** The map needed 4 more fixed-size
    card and panel scripts (`card_small`, `card_small_on`, `card_small_new`,
    `map_panel`): 11 card scripts in all, which differ only in size and
    border colour.

## Duck Echo (sound)

7. **Song notes cut one-shot sounds short without a warning.** A note's
   `length` defaults to 1 beat, and the sound stops 5 ms after the note
   ends. 4.5 s wave sounds came out about 1 s long, and neither `validate`
   nor `render` warned. This is documented in RECIPE_SPEC ("How a note plays
   an instrument"), but it needs a warning when an instrument's envelope is
   longer than its note.
8. **Plain recipes can't loop.** Only songs have `loop = true`, so the
   ambience loop had to be a song with tempo, bars and notes, every noise
   note carrying a dummy `note = "C4"`.
9. **False "ENDS WITH A CLICK" on loops.**
   - `analyze` on the looping ambience WAV says it ends with a click, and
     `loop_seam` is null.
   - There's no `--loop` flag to check a WAV's seam.
   - `analyze --per-track` says the same of a loop song's stems.

   Only the song render's own `loop_seam` ("joins cleanly") looked at the
   join.
10. **Noise notes are checked as pitched.** `analyze --per-track` reports
    `wrong_pitch` for noise-only notes (written C4, "heard" A2/F3/B3), and the
    summary says "8 of 11 notes heard".
11. **Onsets on a steady pad.** "sharp attack, 88 onsets" for a sustained
    chord, plus an `extra_onsets` entry just before the loop end.
12. **`render --json` points at keys that aren't there.** Its summary says
    "per_track describes each" (or "per_layer") when neither `--per-track`
    nor `--per-layer` was given, and the JSON has no such key.

## Duck Mold (3D models)

6. **No top view for still models.** `duck-mold render x.toml --view top`
   fails ("required arguments were not provided: --animation"), and the
   default sheet has no top view. Map props are mostly seen from above.
7. **`move` on an assembly places its zero, not its middle.**
   `docs/recipe-format.md:102` says `move` is "where the part's middle
   goes". For a `use` of an assembly it places the assembly's zero (the
   centre of its first, unmoved part), so the town's houses floated by half
   a roof.
8. **"touches nothing" when the real problem is that the group floats.** The
   palm's five leaves overlapped each other but sat 0.01 above the trunk.
   Each got "the part "leafN" touches nothing", while `overlapping` listed
   them inside each other. It should say the group floats.
9. **`inset` is the same distance from every edge**, so a door on the
   bottom edge of a wall is impossible ("must be less than 0.085 m"). The
   door had to be a separate box.
10. **No way to scale a whole recipe.** Shrinking the capital by 7% meant
    editing every `move` by hand.
11. **`origin = "bottom"` can hide a buried part.** A tower attached to the
    wrong socket sank half into the ground. The whole model was silently
    lifted, still "rests on the ground", and nothing warned.
12. **Stale and missing docs.**
    - `docs/AI_GUIDE.md:1115` and the `reserved` error (`AI_GUIDE.md:500`)
      still call `colliders` reserved, though it works.
    - Sphere triangle counts are only given for even segment counts
      (`recipe-format.md:266`).
