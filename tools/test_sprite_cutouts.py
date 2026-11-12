"""Cutout regressions: opaque interiors, real gaps, and original RGB pixels."""
import unittest
from pathlib import Path
from PIL import Image, ImageDraw
from extract_player_sprites import CELL, ROSTER, SOURCE, cutout_alpha


class CutoutTests(unittest.TestCase):
    def test_enclosed_black_eye_is_opaque(self):
        image = Image.new('RGBA', (40, 40), (9, 8, 12, 255))
        ImageDraw.Draw(image).rectangle((8, 8, 31, 31), outline='white', width=3)
        alpha = cutout_alpha(image)
        self.assertEqual(alpha.getpixel((20, 20)), 255)
        self.assertEqual(alpha.getpixel((0, 0)), 0)

    def test_hairline_leak_does_not_erase_the_face(self):
        image = Image.new('RGBA', (40, 40), (9, 8, 12, 255))
        draw = ImageDraw.Draw(image)
        draw.rectangle((8, 8, 31, 31), outline='white', width=3)
        draw.rectangle((19, 8, 19, 10), fill=(9, 8, 12, 255))
        self.assertEqual(cutout_alpha(image).getpixel((20, 20)), 255)

    def test_real_open_gap_between_legs_stays_transparent(self):
        image = Image.new('RGBA', (40, 40), (9, 8, 12, 255))
        draw = ImageDraw.Draw(image)
        draw.rectangle((6, 5, 33, 11), fill='white')
        draw.rectangle((6, 5, 12, 34), fill='white')
        draw.rectangle((27, 5, 33, 34), fill='white')
        self.assertEqual(cutout_alpha(image).getpixel((20, 27)), 0)

    def test_part_guide_protects_dark_coat_open_to_background(self):
        image = Image.new('RGBA', (40, 40), (9, 8, 12, 255))
        draw = ImageDraw.Draw(image)
        draw.rectangle((10, 8, 12, 30), fill='white')
        draw.rectangle((27, 8, 29, 30), fill='white')
        self.assertEqual(cutout_alpha(image).getpixel((20, 20)), 0)
        alpha = cutout_alpha(image, [(10, 8, 30, 31)])
        self.assertEqual(alpha.getpixel((20, 20)), 255)
        self.assertEqual(alpha.getpixel((5, 20)), 0)

    def test_every_actor_file_preserves_original_rgb_and_padding(self):
        source = Image.open(SOURCE).convert('RGBA')
        for name, feet, _, pivots in ROSTER:
            image = Image.open(Path('assets/characters') / (name + '.png')).convert('RGBA')
            self.assertEqual(image.size, (CELL * 4, CELL))
            self.assertEqual(set(image.getchannel('A').getextrema()), {0, 255})
            for column, pivot in enumerate(pivots):
                for y in range(CELL):
                    for x in range(CELL):
                        r, g, b, a = image.getpixel((column * CELL + x, y))
                        if a:
                            original = source.getpixel((pivot + x - CELL // 2, y + feet - 239))
                            self.assertEqual((r, g, b), original[:3])
                        if x in (0, CELL - 1) or y in (0, CELL - 1):
                            self.assertEqual(a, 0)

    def test_real_dark_material_is_not_a_hole(self):
        # Actual source coordinates: coat, armour, gear and robe interiors.
        samples = {'watchman': (3, 960, 382), 'furnace_guard': (3, 949, 822),
                   'hound': (0, 177, 1035), 'surveyor': (3, 962, 1264)}
        for name, feet, _, pivots in ROSTER:
            if name not in samples:
                continue
            column, x, y = samples[name]
            pixel = Image.open(f'assets/characters/{name}.png').getpixel(
                (column * CELL + x - pivots[column] + CELL // 2, y - feet + 239))
            self.assertLess(max(pixel[:3]), 40)
            self.assertEqual(pixel[3], 255)


if __name__ == '__main__':
    unittest.main()
