#!/usr/bin/env bash
# Builds LUCKY WORLD as double-click apps and zips each with its data and art:
#   dist/LuckyWorld-Windows.zip  (LuckyWorld.exe: unzip, double-click)
#   dist/LuckyWorld-Linux.zip    (LuckyWorld: unzip, double-click or ./LuckyWorld)
# Windows is cross-built from Linux; it needs `rustup target add
# x86_64-pc-windows-gnu` and mingw-w64 (`apt-get install mingw-w64`).
set -euo pipefail
cd "$(dirname "$0")/.."
rm -rf dist && mkdir -p dist

cargo build --release
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
  cargo build --release --target x86_64-pc-windows-gnu

pack() { # pack NAME BINARY APP
  local dir="dist/$1"
  mkdir -p "$dir"
  cp "$2" "$dir/$3"
  cp -r data assets "$dir/"
  cp tools/HOW_TO_PLAY.txt "$dir/"
  (cd dist && zip -qr "$1.zip" "$1")
  rm -rf "$dir"
}
pack LuckyWorld-Windows target/x86_64-pc-windows-gnu/release/lucky-world.exe LuckyWorld.exe
pack LuckyWorld-Linux target/release/lucky-world LuckyWorld
ls -lh dist
