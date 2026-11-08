"""The bread: not drawn like everything else, but a real photo of a loaf,
unedited, simply put in the game as it is, as if someone forced it in.

    python3 art/make_bread_photo.py path/to/bread_photo.jpg

writes, only made smaller (no other change):
  assets/bread_photo.png  256 pixels wide, held in the hand
  assets/bread_icon.png   16 pixels wide, smoothly shrunk, for the slots
"""

import sys
from pathlib import Path

from PIL import Image

ASSETS = Path(__file__).resolve().parent.parent / "assets"


def main(source):
    photo = Image.open(source).convert("RGB")
    w, h = photo.size
    for name, width in [("bread_photo.png", 256), ("bread_icon.png", 16)]:
        size = (width, max(1, round(h * width / w)))
        photo.resize(size, Image.LANCZOS).save(ASSETS / name)
        print(f"wrote {ASSETS / name} ({size[0]} x {size[1]})")


if __name__ == "__main__":
    main(sys.argv[1])
