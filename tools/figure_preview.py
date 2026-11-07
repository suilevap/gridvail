#!/usr/bin/env python3
"""Draw figures from assets/rules/figures.txt, every view side by side.

Usage: python3 tools/figure_preview.py OUT.png [NAME ...] [--figures FILE] [--zoom N]

For designing figures without running the game: each part is drawn as the
text renderer draws it (the cell font at its own 8 px, scaled, tinted,
turned and mirrored like the game), in white light, over a faint outline of
the object's cell and the font-pixel grid. Requires Pillow.
"""
import argparse
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
FONT = ROOT / 'assets/fonts/unscii-8.ttf'
PX = 16  # font pixels a cell is wide at the default scale
VIEWS = ['front', 'back', 'left', 'right']


def length(value):
    return float(value[:-2]) / PX if value.endswith('px') else float(value)


def parse(text):
    """Figures by name: view -> list of parts, as `content::figures` reads them."""
    figures, name, view = {}, None, 'front'
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith('#'):
            continue
        if line.startswith('figure '):
            name, view = line[7:].strip(), 'front'
            figures[name] = {}
            continue
        if line.startswith('view '):
            view = line[5:].strip()
            continue
        words = line.split()
        part = {'glyph': words[1], 'x': 0.0, 'y': 0.0, 'scale': 0.5, 'turn': 0.0,
                'tint': (255, 255, 255), 'only': '', 'noflip': False}
        for word in words[2:]:
            key, _, value = word.partition('=')
            if key in ('x', 'y', 'scale'):
                part[key] = length(value)
            elif key == 'turn':
                part['turn'] = float(value)
            elif key == 'shade':
                grey = round(min(max(float(value), 0.0), 1.0) * 255)
                part['tint'] = (grey, grey, grey)
            elif key == 'tint':
                part['tint'] = tuple(int(value[i:i + 2], 16) for i in (1, 3, 5))
            elif key == 'only':
                part['only'] = value
            elif key == 'noflip':
                part['noflip'] = True
        figures[name].setdefault(view, []).append(part)
    return figures


def parts_for(figure, view):
    """The parts drawn for `view` and whether mirrored (`FigureShape::parts`)."""
    mirror = {'left': 'right', 'right': 'left'}.get(view)
    if view in figure:
        return figure[view], False
    if mirror in figure:
        return figure[mirror], True
    return figure.get('front', []), False


def glyph_mask(font, ch):
    """The glyph's ink at the font's own size, 8 x 8, placed as the game's
    atlas does (top at the ascent)."""
    mask = Image.new('L', (8, 8), 0)
    ImageDraw.Draw(mask).text((0, 0), ch, font=font, fill=255, anchor='la')
    return mask.point(lambda v: 255 if v >= 128 else 0)


def draw_view(font, parts, mirrored, zoom, own):
    cells = 3
    size = cells * PX
    image = Image.new('RGB', (size * zoom, size * zoom), (14, 16, 22))
    draw = ImageDraw.Draw(image)
    for i in range(size + 1):
        colour = (40, 44, 56) if i % PX == 0 else (22, 25, 32)
        draw.line([(i * zoom, 0), (i * zoom, size * zoom)], fill=colour)
        draw.line([(0, i * zoom), (size * zoom, i * zoom)], fill=colour)
    centre = size / 2
    flip = -1 if mirrored else 1
    for part in parts:
        if part['only'] and own not in part['only']:
            continue
        ch = own if part['glyph'] == 'own' else part['glyph']
        side = max(round(part['scale'] * PX * zoom), 1)
        mask = glyph_mask(font, ch).resize((side, side), Image.NEAREST)
        glyph_flip = 1 if part['noflip'] else flip
        if glyph_flip < 0:
            mask = mask.transpose(Image.FLIP_LEFT_RIGHT)
        if part['turn']:
            mask = mask.rotate(part['turn'] * glyph_flip, Image.NEAREST, expand=True)
        x = (centre + part['x'] * flip * PX) * zoom - mask.width / 2
        y = (centre - part['y'] * PX) * zoom - mask.height / 2
        image.paste(Image.new('RGB', mask.size, part['tint']), (round(x), round(y)), mask)
    return image


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('out')
    parser.add_argument('names', nargs='*')
    parser.add_argument('--figures', default=str(ROOT / 'assets/rules/figures.txt'))
    parser.add_argument('--zoom', type=int, default=8, help='screen pixels a font pixel')
    parser.add_argument('--own', default='e', help='the object glyph `own` parts draw')
    args = parser.parse_args()
    figures = parse(Path(args.figures).read_text(encoding='utf-8-sig'))
    font = ImageFont.truetype(str(FONT), 8)
    names = args.names or list(figures)
    rows = []
    for name in names:
        views = [draw_view(font, *parts_for(figures[name], view), args.zoom, args.own)
                 for view in VIEWS]
        row = Image.new('RGB', (sum(v.width for v in views) + 8 * (len(views) - 1),
                                views[0].height + 14), (0, 0, 0))
        draw = ImageDraw.Draw(row)
        x = 0
        for view, image in zip(VIEWS, views):
            row.paste(image, (x, 14))
            draw.text((x + 2, 1), f'{name} {view}', fill=(200, 200, 200))
            x += image.width + 8
        rows.append(row)
    sheet = Image.new('RGB', (max(r.width for r in rows), sum(r.height + 8 for r in rows)))
    y = 0
    for row in rows:
        sheet.paste(row, (0, y))
        y += row.height + 8
    sheet.save(args.out)


if __name__ == '__main__':
    main()
