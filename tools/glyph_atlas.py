#!/usr/bin/env python3
"""Make assets/fonts/unscii-8-atlas.png and .txt: the game font's glyphs as
8 by 8 white pixels on transparent, 16 to a row, for the voxel renderer.

The glyphs are printable ASCII and every other character the game's code
and rules use that the font has, in the order of the .txt file.

    python3 tools/glyph_atlas.py   (needs fontTools and Pillow)
"""
from pathlib import Path

from fontTools.ttLib import TTFont
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
FONT = ROOT / "assets/fonts/unscii-8.ttf"
OUT = ROOT / "assets/fonts/unscii-8-atlas"
SIZE = 8
PER_ROW = 16


def used_chars():
    sources = list((ROOT / "src").rglob("*.rs"))
    sources += list((ROOT / "assets/rules").glob("*"))
    sources.append(ROOT / "assets/wall_rule.txt")
    chars = set()
    for path in sources:
        if path.is_file():
            chars |= {c for c in path.read_text(encoding="utf-8") if ord(c) > 126}
    return chars


def main():
    cmap = TTFont(FONT).getBestCmap()
    chars = [chr(code) for code in range(32, 127)]
    chars += sorted(c for c in used_chars() if ord(c) in cmap)
    # Drawn at twice its size and sampled at each pixel's middle, as at
    # its own size the rasterizer blurs some glyphs (the shades, say).
    font = ImageFont.truetype(str(FONT), SIZE * 2)
    rows = (len(chars) + PER_ROW - 1) // PER_ROW
    atlas = Image.new("RGBA", (PER_ROW * SIZE, rows * SIZE), (0, 0, 0, 0))
    for index, char in enumerate(chars):
        big = Image.new("L", (SIZE * 2, SIZE * 2), 0)
        ImageDraw.Draw(big).text((0, 0), char, font=font, fill=255)
        glyph = Image.new("L", (SIZE, SIZE), 0)
        for y in range(SIZE):
            for x in range(SIZE):
                if big.getpixel((x * 2 + 1, y * 2 + 1)) > 127:
                    glyph.putpixel((x, y), 255)
        white = Image.new("RGBA", (SIZE, SIZE), (255, 255, 255, 255))
        x, y = index % PER_ROW * SIZE, index // PER_ROW * SIZE
        atlas.paste(white, (x, y), glyph)
    atlas.save(OUT.with_suffix(".png"))
    OUT.with_suffix(".txt").write_text("".join(chars), encoding="utf-8")


if __name__ == "__main__":
    main()
