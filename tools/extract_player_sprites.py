#!/usr/bin/env python3
"""Extract the four original player views without redrawing or resampling.

Run from the repository root with Pillow installed. Source coordinates refer
only to the first row of the approved generated roster. Connected dark background pixels
become transparent; enclosed details such as the mask eyes stay opaque; all retained RGB pixels are copied unchanged. Equal cells
and a common foot baseline keep facing changes from moving the character.
"""
from collections import deque
from pathlib import Path
from PIL import Image

SOURCE = Path('docs/art/gridvail-sprite-roster.png')
OUTPUT = Path('assets/characters/trespasser.png')
CELL = 192
BASELINE = 175
# Fixed body pivots (not bounding-box centres, which shift with the sword).
PIVOTS = (162, 425, 695, 960)


def extract():
    source = Image.open(SOURCE).convert('RGBA')
    atlas = Image.new('RGBA', (CELL * 4, CELL))
    for column, pivot in enumerate(PIVOTS):
        top = 199 - BASELINE
        cell = source.crop((pivot - CELL // 2, top, pivot + CELL // 2, top + CELL))
        # Flood only the outside background, preserving dark eyes/hood details.
        outside, pending = set(), deque([(0, 0)])
        while pending:
            x, y = pending.popleft()
            if not (0 <= x < CELL and 0 <= y < CELL) or (x, y) in outside:
                continue
            r, g, b, _ = cell.getpixel((x, y))
            if max(r, g, b) >= 40:
                continue
            outside.add((x, y))
            pending.extend(((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)))
        for x, y in outside:
            cell.putpixel((x, y), (0, 0, 0, 0))
        atlas.paste(cell, (column * CELL, 0))
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    atlas.save(OUTPUT)
    print(f'{OUTPUT}: {atlas.width}x{atlas.height}, left/right/back/front')


if __name__ == '__main__':
    extract()
