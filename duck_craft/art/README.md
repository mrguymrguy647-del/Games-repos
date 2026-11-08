# Duck Craft's art and sounds

The block pictures and the duck's skin come from Duck Quill (`quill`). The files the game loads
are in `../assets/`. Run the commands in `examples/duck_craft`.

| Source | Makes | With |
|---|---|---|
| `make_blocks.py` | `blocks.json`, the Duck Quill script: one 16 x 16 pixel-art tile per animation (grass, dirt, stone, cobblestone, sand, logs, leaves with holes, planks, glass, brick, coal and gold ore, water, bedrock, cloud, lamp, four stages of cracks; tall grass and three flowers; and the items: apple, seeds, bread, gold, coal, the stone blade) | `python3 art/make_blocks.py` |
| `blocks.json` | the atlas `assets/blocks.png` and `assets/blocks.json` | `quill export art/blocks.json --sheet --out assets` |
| `make_skin.py` | `skin.json`: the duck's skin, each body part's six faces unwrapped onto one 64 x 32 picture, like a Minecraft skin (the layout is in the script's docstring; `main.rs`'s `Duck::load` uses the same numbers) | `python3 art/make_skin.py` |
| `skin.json` | `assets/skin.png` | `quill render art/skin.json --out assets` |
| `make_hud.py` | `hud.json`: the 9 x 9 icons over the hotbar: hearts, slices of bread (hunger) and bubbles (air), full, half and empty | `python3 art/make_hud.py` |
| `hud.json` | `assets/hud.png` and `assets/hud.json` | `quill export art/hud.json --sheet --out assets` |
| `make_monsters.py` | `hollow.json` and `watcher.json`: the night's shapes, in the duck's skin layout (a Hollow: ash-grey, eyes gone to holes, beak broken off, the suit rotted; the Watcher: black, two points of light) | `python3 art/make_monsters.py` |
| `hollow.json`, `watcher.json` | `assets/hollow.png`, `assets/watcher.png` | `quill render art/hollow.json --out assets` (and the same for `watcher`) |
| `make_duckling.py` | `duckling.json`: the ducklings' skin, the same kind of layout (body with tail, head with beak, legs; `ducklings.rs` uses the same numbers) | `python3 art/make_duckling.py` |
| `duckling.json` | `assets/duckling.png` | `quill render art/duckling.json --out assets` |

To change a block, edit its function in `make_blocks.py` (a grid of palette
names, `.` for see-through), then run both commands. Every tile is in one
picture, so all the blocks of a chunk share one texture and draw as one mesh.
The digging, building, footstep, crafting, thud and quack sounds are made in code (`Sound { .. }`).

## Music and sounds

`../audio/make_sounds.py` makes every `.wav` in `../assets`, from nothing
but sine waves, noise and arithmetic (numpy and scipy), because Duck Echo
isn't ready yet. Everything is at 44100 samples a second, the engine's own
rate, so nothing is resampled on loading; music, wind, bells and the sting
are stereo; the Hollows' sounds are mono, because the game pans them toward
the Hollow:

| File | What |
|---|---|
| `music_day.wav` | "Still Water": a calm piano song on a warped tape that isn't calm: notes a quarter-tone flat, an out-of-tune music box lullaby, a far choir that sinks in pitch, a chord rising in backwards, and two moments where the song just stops and the room rings on; 64 s, stereo, loops with no seam |
| `music_night.wav` | "Nothing Waits": a low drone and a throb under the ground, a tone that falls forever (a Shepard tone), whispers that move from ear to ear, strings piling up on wrong notes and cut off dead, bowed metal, a choir sinking on a tritone, a far bell; 64 s, stereo |
| `music_title.wav` | "The Island Doesn't Remember", in the style of creepypasta music: an original children's tune on an old games-console chip, slowed down and played off a worn VHS tape (wobble, hiss, crinkles), a thin high note a half-step wrong, a skip, the tune backwards, then a broadcast tone; 32 s, stereo |
| `wind.wav` | the island's wind, gusting, a little different in each ear; 24 s, stereo |
| `groan.wav`, `groan_far.wav` | a Hollow, near and far (the engine plays a file at one loudness, so distance is a second file); panned toward it |
| `hollow_hurt.wav`, `hollow_die.wav` | a Hollow hit, and coming apart into ash; panned toward it |
| `sting.wav` | the Watcher, seen |
| `heartbeat.wav` | one beat, looped when the duck is badly hurt |
| `bell_dusk.wav`, `bell_dawn.wav` | nightfall and morning |

Run it in `duck_craft/`: `python3 audio/make_sounds.py` (about 30 s). The
script's docstring says what makes it sound clean (the reverb, the piano's
strings, dither). The music is about 30 MB of `.wav`: big, but the engine
reads only `.wav` (see ISSUES.md).
The game fades the loops itself: the title's on the title screen, day and
night by the light, the heartbeat by how hurt you are.

## The bread

The bread isn't drawn like everything else: it's a real photo of a loaf,
crushed small, saved as a bad JPEG twice, faded yellow, date-stamped and
printed on old photo paper, with "BREAD?" pencilled on the back, as if
someone forced it into the game. `make_bread_photo.py path/to/photo.jpg`
makes `../assets/bread_photo.png` (front and back side by side); the
source photo isn't kept here, only the crushed print. In the game it's a
paper-thin card: it turns slowly in its slot, a little crooked, and goes
edge-on; in the hand you hold up the print.

Every other item is drawn in 3D too: blocks as small turning cubes seen
from a little above, and things like the apple and the blade as their
picture made solid, with a one-pixel edge (`item_model` in `main.rs`).

