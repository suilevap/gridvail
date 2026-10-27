//! Floor tiles (`assets/floor_tiles.txt`): 8 by 8 glyphs, and styles that
//! lay them out on the floor. The file's own comments describe the format.

/// The glyphs and the styles that use them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FloorTiles {
    /// Each glyph's rows, top first; bit 7 is the leftmost pixel.
    pub glyphs: Vec<[u8; 8]>,
    pub glyph_names: Vec<String>,
    pub styles: Vec<TileStyle>,
}

/// How a style lays glyphs out.
#[derive(Clone, Debug, PartialEq)]
pub struct TileStyle {
    pub name: String,
    /// Glyphs along a cell's side.
    pub slots: u32,
    /// How many slots every other row is moved along.
    pub shift: f32,
    /// How many of its pixels a glyph may stray.
    pub jitter: u32,
    /// The share of slots left empty.
    pub empty: f32,
    /// How bright lit glyphs are.
    pub gain: f32,
    pub pick: Pick,
    /// The glyphs it uses, indices into `FloorTiles::glyphs`.
    pub glyphs: Vec<usize>,
}

/// How a style chooses a slot's glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// At random, the same at the same place.
    Random,
    /// From a block this many slots wide and high, row by row, repeated.
    Block(u32, u32),
    /// By how bright it is there, darkest first.
    Ramp,
}

impl FloorTiles {
    /// The style called `name`, by index.
    pub fn style(&self, name: &str) -> Option<usize> {
        self.styles.iter().position(|style| style.name == name)
    }
}

/// The most glyphs a style may use.
pub const MAX_STYLE_GLYPHS: usize = 64;

pub fn parse_floor_tiles(text: &str) -> Result<FloorTiles, String> {
    let mut tiles = FloorTiles::default();
    let mut lines = text
        .lines()
        .map(|line| line.trim_end())
        // A comment is `#` and a space (glyph rows start with `#` too).
        .filter(|line| !line.is_empty() && *line != "#" && !line.starts_with("# "))
        .peekable();
    while let Some(line) = lines.next() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("glyph") => {
                let name = words.next().ok_or("a glyph needs a name")?;
                let mut rows = [0u8; 8];
                for row in &mut rows {
                    let pixels = lines
                        .next_if(|line| line.chars().all(|c| matches!(c, '#' | '.')))
                        .ok_or_else(|| format!("glyph {name} needs 8 rows of # and ."))?;
                    if pixels.len() != 8 {
                        return Err(format!("glyph {name}'s rows are 8 wide: `{pixels}`"));
                    }
                    *row = pixels
                        .chars()
                        .fold(0, |bits, c| bits << 1 | u8::from(c == '#'));
                }
                tiles.glyphs.push(rows);
                tiles.glyph_names.push(name.to_string());
            }
            Some("style") => {
                let name = words.next().ok_or("a style needs a name")?;
                let style = parse_style(name, words, &tiles)?;
                tiles.styles.push(style);
            }
            _ => return Err(format!("expected `glyph` or `style`: `{line}`")),
        }
    }
    Ok(tiles)
}

fn parse_style<'a>(
    name: &str,
    words: impl Iterator<Item = &'a str>,
    tiles: &FloorTiles,
) -> Result<TileStyle, String> {
    let mut style = TileStyle {
        name: name.to_string(),
        slots: 1,
        shift: 0.0,
        jitter: 0,
        empty: 0.0,
        gain: 1.7,
        pick: Pick::Random,
        glyphs: Vec::new(),
    };
    let bad = |word: &str| format!("style {name}: bad `{word}`");
    for word in words {
        let (key, value) = word.split_once('=').unwrap_or((word, ""));
        match key {
            "slots" => {
                style.slots = value.parse().map_err(|_| bad(word))?;
                if !matches!(style.slots, 1 | 2 | 4) {
                    return Err(format!("style {name}: slots are 1, 2 or 4"));
                }
            }
            "shift" => style.shift = value.parse().map_err(|_| bad(word))?,
            "jitter" => style.jitter = value.parse().map_err(|_| bad(word))?,
            "empty" => style.empty = value.parse().map_err(|_| bad(word))?,
            "gain" => style.gain = value.parse().map_err(|_| bad(word))?,
            "ramp" => style.pick = Pick::Ramp,
            "block" => {
                let (width, height) = value.split_once('x').ok_or_else(|| bad(word))?;
                style.pick = Pick::Block(
                    width.parse().map_err(|_| bad(word))?,
                    height.parse().map_err(|_| bad(word))?,
                );
            }
            "pick" => {
                for glyph in value.split(',') {
                    let index = tiles
                        .glyph_names
                        .iter()
                        .position(|known| known == glyph)
                        .ok_or_else(|| format!("style {name}: no glyph `{glyph}`"))?;
                    style.glyphs.push(index);
                }
            }
            _ => return Err(bad(word)),
        }
    }
    if style.glyphs.is_empty() || style.glyphs.len() > MAX_STYLE_GLYPHS {
        return Err(format!("style {name} picks 1 to {MAX_STYLE_GLYPHS} glyphs"));
    }
    if let Pick::Block(width, height) = style.pick {
        if (width * height) as usize != style.glyphs.len() {
            return Err(format!(
                "style {name}: a {width}x{height} block picks {} glyphs",
                width * height
            ));
        }
    }
    Ok(style)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOT: &str = "glyph dot\n........\n........\n...##...\n...##...\n........\n........\n........\n........\n";

    #[test]
    fn glyphs_are_rows_of_bits_and_styles_name_them() {
        let text =
            format!("# a comment\n#\n{DOT}\nstyle dots slots=2 jitter=1 empty=0.5 pick=dot,dot\n");
        let tiles = parse_floor_tiles(&text).unwrap();
        assert_eq!(
            tiles.glyphs,
            vec![[0, 0, 0b0001_1000, 0b0001_1000, 0, 0, 0, 0]]
        );
        let style = &tiles.styles[0];
        assert_eq!((style.slots, style.jitter, style.empty), (2, 1, 0.5));
        assert_eq!(style.glyphs, vec![0, 0]);
        assert_eq!(style.pick, Pick::Random);
        assert_eq!(tiles.style("dots"), Some(0));
    }

    #[test]
    fn a_block_picks_as_many_glyphs_as_it_has_slots() {
        let ok = format!("{DOT}style b slots=1 block=2x1 pick=dot,dot\n");
        assert_eq!(
            parse_floor_tiles(&ok).unwrap().styles[0].pick,
            Pick::Block(2, 1)
        );
        let short = format!("{DOT}style b slots=1 block=2x2 pick=dot\n");
        assert!(parse_floor_tiles(&short).unwrap_err().contains("picks 4"));
        let unknown = format!("{DOT}style b pick=nothing\n");
        assert!(parse_floor_tiles(&unknown)
            .unwrap_err()
            .contains("no glyph"));
    }

    #[test]
    fn the_bundled_tiles_parse() {
        let tiles = parse_floor_tiles(include_str!("../../assets/floor_tiles.txt")).unwrap();
        assert!(tiles.style("points").is_some());
        assert!(tiles.styles.len() >= 5);
    }
}
