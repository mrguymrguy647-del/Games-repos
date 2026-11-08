"""The bread: not drawn like everything else, but a real photo of a loaf,
crushed small, saved as a JPEG too many times, faded, date-stamped and
printed on old photo paper, as if someone forced it into the game.

    python3 art/make_bread_photo.py path/to/bread_photo.jpg

writes assets/bread_photo.png: 128 x 64, the print's front on the left
and its back (yellowed paper, a pencilled note) on the right. The source
photo isn't kept in the repo; only this crushed print is.
"""

import io
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageEnhance

OUT = Path(__file__).resolve().parent.parent / "assets" / "bread_photo.png"
rng = np.random.default_rng(7)

# 3 x 5 digits, for the date stamp.
DIGITS = {
    "0": ["###", "#.#", "#.#", "#.#", "###"], "1": [".#.", "##.", ".#.", ".#.", "###"],
    "2": ["###", "..#", "###", "#..", "###"], "3": ["###", "..#", ".##", "..#", "###"],
    "4": ["#.#", "#.#", "###", "..#", "..#"], "5": ["###", "#..", "###", "..#", "###"],
    "6": ["###", "#..", "###", "#.#", "###"], "7": ["###", "..#", ".#.", ".#.", ".#."],
    "8": ["###", "#.#", "###", "#.#", "###"], "9": ["###", "#.#", "###", "..#", "###"],
    "'": [".#.", ".#.", "...", "...", "..."], " ": ["...", "...", "...", "...", "..."],
    "B": ["##.", "#.#", "##.", "#.#", "##."], "R": ["##.", "#.#", "##.", "#.#", "#.#"],
    "E": ["###", "#..", "##.", "#..", "###"], "A": [".#.", "#.#", "###", "#.#", "#.#"],
    "D": ["##.", "#.#", "#.#", "#.#", "##."], "?": ["##.", "..#", ".#.", "...", ".#."],
}


def jpeg(image, quality):
    buffer = io.BytesIO()
    image.save(buffer, "JPEG", quality=quality)
    return Image.open(io.BytesIO(buffer.getvalue())).convert("RGB")


def stamp(pixels, text, x, y, color):
    for ch in text:
        for row, line in enumerate(DIGITS[ch]):
            for col, mark in enumerate(line):
                if mark == "#":
                    pixels[y + row, x + col] = color
        x += 4


def main(source):
    photo = Image.open(source).convert("RGB")
    w, h = photo.size
    # The loaf, a little off-centre, as if the print was cut by hand.
    photo = photo.crop((int(w * 0.02), int(h * 0.04), int(w * 0.99), int(h * 0.98)))
    # Crushed: small, then saved as a bad JPEG twice (the blocks show).
    photo = jpeg(photo.resize((56, 42), Image.LANCZOS), 9)
    photo = jpeg(photo, 14)
    # Faded: less colour, a yellow cast, blacks gone grey-brown.
    photo = ImageEnhance.Color(photo).enhance(0.7)
    px = np.asarray(photo).astype(float)
    px = px * np.array([1.0, 0.93, 0.78]) + np.array([26, 20, 8])
    # Darker towards the corners, as old prints go.
    yy, xx = np.mgrid[0:42, 0:56]
    vignette = 1 - 0.35 * (((xx - 28) / 28) ** 2 + ((yy - 21) / 21) ** 2)
    px = px * vignette[..., None]
    px = np.clip(px, 0, 255)
    # A crease across it, and dust.
    for x in range(56):
        y = int(9 + x * 0.28)
        if 0 <= y < 42:
            px[y, x] = np.minimum(px[y, x] * 1.12 + 8, 255)
    for _ in range(14):
        px[rng.integers(42), rng.integers(56)] = rng.choice([235, 30])
    stamp(px, "'98 7 14", 22, 35, np.array([255, 120, 40]))

    # The print: yellowed paper, a thicker margin at the bottom.
    paper = np.zeros((64, 64, 3))
    paper[:] = [226, 214, 182]
    paper += rng.normal(0, 5, (64, 64, 1))
    stain = np.exp(-(((np.mgrid[0:64, 0:64][1] - 50) / 10) ** 2 + ((np.mgrid[0:64, 0:64][0] - 56) / 6) ** 2))
    paper -= stain[..., None] * np.array([30, 40, 60])
    front = paper.copy()
    front[4:46, 4:60] = px
    # The back: plain old paper, and "bread?" in pencil, faint.
    back = paper.copy()[:, ::-1]
    stamp(back, "BREAD?", 20, 28, np.array([150, 140, 125]))
    sheet = np.concatenate([front, back], axis=1)
    Image.fromarray(np.clip(sheet, 0, 255).astype(np.uint8)).save(OUT)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main(sys.argv[1])
