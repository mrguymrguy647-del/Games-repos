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
