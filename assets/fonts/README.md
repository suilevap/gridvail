# Unscii-8

Bundled unmodified from https://github.com/viznut/unscii (commit
257b4df, `fontfiles/unscii-8.ttf`). Public Domain / CC0; see
`LICENSE_UNSCII`.

An 8×8 pixel font drawn for a square character grid, with the Unicode
box-drawing, block and direction symbols the game uses. Every glyph's
advance equals its height, so the text renderer's cells are square and
wall glyphs join on all four sides. It is drawn at 16 px (2× its pixel
size).

`unscii-8-atlas.png` is the font's glyphs as 8×8 white pixels, 16 to a row,
in the order of the characters in `unscii-8-atlas.txt`: printable ASCII and
the other characters the game uses. The voxel renderer paints them on its
cubes and tiles. `python3 tools/glyph_atlas.py` remakes both.
