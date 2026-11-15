# Tool issues found while building LUCKY WORLD

DRAFT (filled in as work goes on)

## Duck Quill (from survey)
- export overwrites source script when --out is the script's folder (names output <script>.json). docs/sprite-sheet-format.md:3 no warning.
- no arc/wedge/pie/ring primitive (crates/engine/src/script.rs:437-469; docs/script-format.md:267-288). Wedge points had to be generated in Python.
- no conic/sweep gradient (docs/script-format.md:306-325).
- text: Inter only, no weight; rotated text in repeat comes out upside down in bottom half.
- export refuses still pictures ("this script has no animations"); still UI art needs render/atlas. Duck Forge SKILL.md says `quill export art/hero.json --sheet` generally.
- render always writes .quill next to PNG; no flag to skip. --bg also writes -view.png.
- atlas --padding defaults to 0 (glows bleed).
- README layout stale (missing filters, noise, critique, senses, study, animate, reuse, cli views...). README build steps Windows-only.
- undocumented: $param inside use's colors map works.

## Duck Forge
- skills/duckforge/SKILL.md:18 clones github.com/.../game-engine-rust-language, repo is Duck-Forge.
- files::read_text is pub(crate): an outside game can't have its own data files (TOML) watched for reload or kept in recordings.
- tools (verify.sh, new_game.sh, tune file path examples/<game>.tune.json) assume the game lives in the engine repo's examples/.
- 2D canvas: no arc/wedge/ring, no anti-aliasing for 2D shapes; supersample is 3D only.

## Duck Echo (from survey)
- README.md:121 says Forge mixes stereo to mono; current Forge keeps stereo (sound.rs:387-390, 445-517, 883-926).
- .mcp.json runs cargo run --release inside repo: writes target/ into the clone; cold build ~2 min > 30 s MCP start timeout; README says "about a minute".
- Studio needs libasound2-dev (not documented as a Linux prerequisite?) - CLI/MCP fine.
- analyze calls a hard-attack fanfare "slow attack" (attack_seconds = time to loudest moment) - confusing wording.
- No pitch variants helper for repeated sfx (had to render tick variants by hand) [check]

## Duck Mold (from survey)
- README "first build takes under a minute": plain cargo build --release also builds Studio (eframe/wgpu), 7m35s.
- `edit --help` first line lists 15 commands, list below has 40+.
- quick start `duck-mold export examples/robot.toml` writes into the repo; suggest -o.
- pointer hanging down needs socket with facing="down"; "roll over" with two sockets same way - documented but hard to find.
