#!/usr/bin/env bash
# Rebuilds everything in assets/ from its source:
#   art/*.json       Duck Quill scripts   -> assets/art/*.png
#   sound/*.toml     Duck Echo recipes    -> assets/sound/*.wav
#   models/*.toml    Duck Mold recipes    -> assets/models/*.glb
#
# Point these at your built tools (or have them on PATH):
#   QUILL=path/to/quill  DUCK_ECHO=path/to/duck-echo  DUCK_MOLD=path/to/duck-mold
set -euo pipefail
cd "$(dirname "$0")/.."
QUILL=${QUILL:-quill}
DUCK_ECHO=${DUCK_ECHO:-duck-echo}
DUCK_MOLD=${DUCK_MOLD:-duck-mold}
mkdir -p assets/art assets/sound assets/models

for f in art/*.json; do
  "$QUILL" render "$f" --transparent --out assets/art >/dev/null
done
# quill render always writes a .quill file beside the picture (see ISSUES.md).
rm -f assets/art/*.quill assets/art/*-view.png

for f in sound/*.toml; do
  name=$(basename "$f" .toml)
  "$DUCK_ECHO" render "$f" -o "assets/sound/$name.wav" >/dev/null
done

for f in models/*.toml; do
  name=$(basename "$f" .toml)
  "$DUCK_MOLD" export "$f" -o "assets/models/$name.glb" >/dev/null
done
echo "assets rebuilt: $(ls assets/art | wc -l) pictures, $(ls assets/sound | wc -l) sounds, $(ls assets/models | wc -l) models"
