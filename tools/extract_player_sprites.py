#!/usr/bin/env python3
"""Extract the complete approved roster without redrawing or resampling.

Run from the repository root with Pillow installed. Each character file has
four views: left, right, back, front. Flood only connected dark background;
enclosed details (mask eyes, machinery) and every retained RGB pixel survive.
Fixed body pivots and a shared foot baseline align the directional images.
"""
from collections import deque
from pathlib import Path
from PIL import Image, ImageChops, ImageDraw, ImageFilter

SOURCE = Path('docs/art/gridvail-sprite-roster.png')
OUTPUT = Path('assets/characters')
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

# Anatomical part windows, not whole-figure hulls. Within each part we fill
# between ORIGINAL visible edge pixels on the same row/column. This restores
# dark material without extrapolating triangles or rectangular patches into
# the surrounding background. Windows keep limb/weapon gaps independent.
SOLID_PARTS = {
    'trespasser': [[(-50, 45, 60, 106), (-38, 38, 100, 175)]] * 4,
    'watchman': [[(-40, 40, 305, 335), (-30, 30, 330, 425)]] * 4,
    'scavenger': [
        [(-48, 15, 530, 580), (-18, 30, 575, 638)],
        [(-12, 35, 530, 580), (-30, 18, 575, 638)],
        [(-35, 35, 530, 567), (-30, 30, 570, 638)],
        [(-35, 35, 530, 580), (-30, 30, 570, 638)],
    ],
    'furnace_guard': [
        [(-55, 8, 730, 789), (-12, 35, 785, 865), (-32, -15, 860, 908), (28, 49, 860, 908)],
        [(7, 60, 730, 789), (-35, 12, 785, 865), (-40, -20, 860, 908), (25, 45, 860, 908)],
        [(-24, 20, 717, 777), (-35, 35, 770, 865), (-45, -14, 859, 908), (14, 35, 859, 908)],
        [(-45, 24, 730, 792), (-35, 30, 790, 865), (-65, -27, 861, 908), (6, 37, 861, 908)],
    ],
    'hound': [
        [(-90, -22, 998, 1062), (-26, 45, 1004, 1045)],
        [(35, 105, 998, 1062), (-40, 20, 1001, 1044)],
        [(-24, 20, 1000, 1040)],
        [(-24, 20, 1000, 1048)],
    ],
    'surveyor': [[(-30, 22, 1197, 1228), (-25, 27, 1225, 1320)]] * 4,
}


def cutout_alpha(cell, solid_regions=()):
    """Use silhouette connectivity, sealing hairline leaks before the flood.

    Black is allowed inside the subject. A small morphological closing seals
    narrow cracks in its bright outline; only border-connected background is
    removed. Eroding the filled guide keeps the closing from thickening the
    outer artwork. Larger gaps between limbs stay connected to the outside.
    RGB is never changed, and alpha remains binary for crisp pixel art.
    """
    ink = Image.new('L', cell.size)
    ink.putdata([255 if a and max(r, g, b) >= 40 else 0
                 for r, g, b, a in zip(*[iter(cell.tobytes())] * 4)])
    barrier = ink.filter(ImageFilter.MaxFilter(5)).filter(ImageFilter.MinFilter(5))
    outside = Image.new('L', cell.size)
    pending = deque([(0, 0)])
    while pending:
        x, y = pending.popleft()
        if not (0 <= x < cell.width and 0 <= y < cell.height):
            continue
        if outside.getpixel((x, y)) or barrier.getpixel((x, y)):
            continue
        outside.putpixel((x, y), 255)
        pending.extend(((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)))
    inside = ImageChops.invert(outside).filter(ImageFilter.MinFilter(3))
    alpha = ImageChops.lighter(ink, inside)
    protected = ImageDraw.Draw(alpha)
    for left, top, right, bottom in solid_regions:
        left, top = max(0, left), max(0, top)
        right, bottom = min(cell.width, right), min(cell.height, bottom)
        for y in range(top, bottom):
            edges = [x for x in range(left, right) if ink.getpixel((x, y))]
            if len(edges) > 1:
                protected.line((edges[0], y, edges[-1], y), fill=255)
        for x in range(left, right):
            edges = [y for y in range(top, bottom) if ink.getpixel((x, y))]
            if len(edges) > 1:
                protected.line((x, edges[0], x, edges[-1]), fill=255)
    return alpha


def extract():
    source = Image.open(SOURCE).convert('RGBA')
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for name, feet, band, pivots in ROSTER:
        sheet = Image.new('RGBA', (CELL * 4, CELL))
        for column, pivot in enumerate(pivots):
            top = feet - BASELINE
            cell = Image.new('RGBA', (CELL, CELL))
            crop = source.crop((pivot - CELL // 2, band[0], pivot + CELL // 2, band[1]))
            cell.paste(crop, (0, band[0] - top))
            regions = [(left + CELL // 2, y0 - top, right + CELL // 2, y1 - top)
                       for left, right, y0, y1 in SOLID_PARTS[name][column]]
            cell.putalpha(cutout_alpha(cell, regions))
            sheet.paste(cell, (column * CELL, 0))
        path = OUTPUT / (name + '.png')
        sheet.save(path)
        print(f'{path}: {sheet.width}x{sheet.height}')


if __name__ == '__main__':
    extract()
