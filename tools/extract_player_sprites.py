#!/usr/bin/env python3
"""Extract the complete approved roster without redrawing or resampling.

Run from the repository root with Pillow installed. Each row contains four
views: left, right, back, front. Flood only the connected dark background;
enclosed details (mask eyes, machinery) and every retained RGB pixel survive.
Fixed body pivots and a shared foot baseline align the directional images.
"""
from collections import deque
from pathlib import Path
from PIL import Image

SOURCE = Path('docs/art/gridvail-sprite-roster.png')
OUTPUT = Path('assets/characters/roster.png')
CELL = 256
BASELINE = 239
# Name, source feet baseline, isolated row bounds, body pivot for each view.
ROSTER = (
    ('trespasser', 199, (25, 220), (162, 425, 695, 960)),
    ('watchman', 453, (240, 480), (162, 425, 695, 960)),
    ('scavenger', 664, (500, 690), (162, 425, 695, 960)),
    ('furnace_guard', 909, (700, 940), (162, 425, 695, 960)),
    ('hound', 1090, (970, 1120), (162, 425, 695, 960)),
    ('surveyor', 1356, (1135, 1380), (162, 425, 695, 960)),
)


def extract():
    source = Image.open(SOURCE).convert('RGBA')
    atlas = Image.new('RGBA', (CELL * 4, CELL * len(ROSTER)))
    for row, (name, feet, band, pivots) in enumerate(ROSTER):
        for column, pivot in enumerate(pivots):
            top = feet - BASELINE
            cell = Image.new('RGBA', (CELL, CELL))
            # Isolate the row so a tall empty cell cannot pick up another actor.
            crop = source.crop((pivot - CELL // 2, band[0], pivot + CELL // 2, band[1]))
            cell.paste(crop, (0, band[0] - top))
            outside, pending = set(), deque([(0, 0)])
            while pending:
                x, y = pending.popleft()
                if not (0 <= x < CELL and 0 <= y < CELL) or (x, y) in outside:
                    continue
                r, g, b, a = cell.getpixel((x, y))
                if a and max(r, g, b) >= 40:
                    continue
                outside.add((x, y))
                pending.extend(((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)))
            for x, y in outside:
                cell.putpixel((x, y), (0, 0, 0, 0))
            atlas.paste(cell, (column * CELL, row * CELL))
        print(f'{name}: row {row}')
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    atlas.save(OUTPUT)
    print(f'{OUTPUT}: {atlas.width}x{atlas.height}')


if __name__ == '__main__':
    extract()
