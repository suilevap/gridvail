//! Tile-rule loaders. Faithful ports of `TileRule.Load` and
//! `DirectionTileRule.Load` (rectangular-input asserts kept as documented
//! behavior; symbol widths counted in `char`s, never bytes).

/// 16-entry wall autotile table indexed by neighbour mask.
///
/// Bit layout mirrors `TileSystem.UpdateMask`: bit0 = +X neighbour,
/// bit1 = +Y neighbour, bit2 = -X, bit3 = -Y.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileRule {
    pub symbols: [char; 16],
}

/// 5-entry direction table indexed by [`crate::model::Direction`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectionTileRule {
    /// [None, Right, Up, Left, Down]
    pub symbols: [char; 5],
}

impl TileRule {
    /// Parse a rule file: non-`.` cells linked right/down accumulate mask
    /// bits, exactly like the C# loader.
    pub fn parse(text: &str) -> Self {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let lines: Vec<Vec<char>> = text.lines().map(|l| l.chars().collect()).collect();
        let mut mask = [[0u8; 32]; 32];
        let mut result = [' '; 16];
        if lines.is_empty() {
            return Self { symbols: result };
        }
        for y in 0..lines.len().saturating_sub(1) {
            for x in 0..lines[y].len().saturating_sub(1) {
                if lines[y][x] == '.' {
                    continue;
                }
                if lines[y][x + 1] != '.' {
                    mask[x][y] |= 1 << 0;
                    mask[x + 1][y] |= 1 << 2;
                }
                if lines[y + 1][x] != '.' {
                    mask[x][y] |= 1 << 1;
                    mask[x][y + 1] |= 1 << 3;
                }
                result[mask[x][y] as usize] = lines[y][x];
            }
        }
        Self { symbols: result }
    }

    pub fn symbol(&self, mask: u8) -> char {
        self.symbols[(mask & 15) as usize]
    }

    /// The symbol a wall drawn as `symbol` shows in a view turned
    /// `quarters` quarter turns counter-clockwise: the symbol of its mask
    /// with every neighbour turned along. `None` if `symbol` is not a wall.
    pub fn turned(&self, symbol: char, quarters: i32) -> Option<char> {
        let mask = self.symbols.iter().position(|s| *s == symbol)? as u8;
        Some(self.symbol(turn_mask(mask, quarters)))
    }
}

/// Turns a neighbour mask counter-clockwise on screen (y down) by quarter
/// turns: the +X neighbour (bit 0) shows where -Y (bit 3) was, +Y where +X
/// was, and so on.
fn turn_mask(mask: u8, quarters: i32) -> u8 {
    (0..quarters.rem_euclid(4)).fold(mask & 15, |m, _| ((m >> 1) | (m << 3)) & 15)
}

impl DirectionTileRule {
    /// Parse a 3x3 rule file; each non-`.` cell maps to its direction
    /// from the center.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let lines: Vec<Vec<char>> = text.lines().map(|l| l.chars().collect()).collect();
        if lines.len() != 3 || lines.iter().any(|l| l.len() != 3) {
            return None;
        }
        let mut result = [' '; 5];
        for (y, row) in lines.iter().enumerate().take(3) {
            for (x, c) in row.iter().enumerate().take(3) {
                if *c == '.' {
                    continue;
                }
                let c = *c;
                let dx = x as i32 - 1;
                let dy = y as i32 - 1;
                let idx = if dx == 0 && dy == 0 {
                    0
                } else if dx.abs() > dy.abs() {
                    if dx > 0 {
                        1
                    } else {
                        3
                    }
                } else if dy > 0 {
                    2
                } else {
                    4
                };
                result[idx] = c;
            }
        }
        Some(Self { symbols: result })
    }

    pub fn symbol(&self, dir: crate::model::Direction) -> char {
        self.symbols[dir.index()]
    }

    /// The symbol a direction drawn as `symbol` shows in a view turned
    /// `quarters` quarter turns counter-clockwise. `None` if `symbol` is not
    /// one of this rule's.
    pub fn turned(&self, symbol: char, quarters: i32) -> Option<char> {
        use crate::model::Direction;
        use bevy::math::IVec2;
        // Map vectors of the slots in `symbols` order.
        const SLOTS: [IVec2; 5] = [IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::NEG_X, IVec2::NEG_Y];
        let slot = self.symbols.iter().position(|s| *s == symbol)?;
        let turned = (0..quarters.rem_euclid(4)).fold(SLOTS[slot], |v, _| IVec2::new(v.y, -v.x));
        Some(self.symbol(Direction::of(turned)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Direction;

    const WALL_RULE: &str = include_str!("../../assets/rules/wall_rule.txt");
    const ARROW_RULE: &str = include_str!("../../assets/rules/direction_arrow_rule.txt");
    const TRIANGLE_RULE: &str = include_str!("../../assets/rules/direction_triangle_rule.txt");
    const V_RULE: &str = include_str!("../../assets/rules/direction_v_rule.txt");

    #[test]
    fn wall_rule_covers_all_16_masks() {
        let rule = TileRule::parse(WALL_RULE);
        // Every mask resolves to a non-placeholder wall glyph.
        for (i, s) in rule.symbols.iter().enumerate() {
            assert!(*s != ' ' && *s != '.', "mask {i} has no glyph");
        }
        // Isolated wall (mask 0) is the lone glyph from the file.
        assert_eq!(rule.symbol(0), '≡');
        // Fully connected cross is the center glyph.
        assert_eq!(rule.symbol(0b1111), '╬');
    }

    #[test]
    fn direction_rules_map_five_slots() {
        // NOTE: the original maps file rows top-to-bottom while game Y grows
        // downward, so visually-up glyphs land on `Down` and vice versa.
        // This inversion is inherited verbatim from `ToDirection`.
        let arrow = DirectionTileRule::parse(ARROW_RULE).expect("arrow rule");
        assert_eq!(arrow.symbol(Direction::Right), '→');
        assert_eq!(arrow.symbol(Direction::Up), '↓');
        assert_eq!(arrow.symbol(Direction::Left), '←');
        assert_eq!(arrow.symbol(Direction::Down), '↑');

        let tri = DirectionTileRule::parse(TRIANGLE_RULE).expect("triangle rule");
        assert_eq!(tri.symbol(Direction::Right), '►');
        assert_eq!(tri.symbol(Direction::Up), '▼');
        assert_eq!(tri.symbol(Direction::Left), '◄');
        assert_eq!(tri.symbol(Direction::Down), '▲');
        assert_eq!(tri.symbol(Direction::None), '●');

        let v = DirectionTileRule::parse(V_RULE).expect("v rule");
        assert_eq!(v.symbol(Direction::Right), '>');
        assert_eq!(v.symbol(Direction::Up), 'V');
        assert_eq!(v.symbol(Direction::Left), '<');
        assert_eq!(v.symbol(Direction::Down), '^');
        assert_eq!(v.symbol(Direction::None), 'x');
    }

    #[test]
    fn turned_walls_keep_their_connections_on_screen() {
        let rule = TileRule::parse(WALL_RULE);
        // Every wall shape has its own glyph, so a glyph names one mask.
        let mut distinct = rule.symbols.to_vec();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), 16);
        // A quarter turn counter-clockwise stands a horizontal wall up.
        assert_eq!(rule.turned('═', 1), Some('║'));
        // A corner joined right and down (map +X, +Y) is joined up and
        // right after a counter-clockwise quarter turn.
        assert_eq!(rule.turned('╔', 1), Some('╚'));
        assert_eq!(rule.turned('╔', -1), Some('╗'));
        // A dead end pointing right (joined to +X) points up.
        assert_eq!(rule.turned('╞', 1), Some('╨'));
        for symbol in rule.symbols {
            assert_eq!(rule.turned(symbol, 4), Some(symbol));
            assert_eq!(rule.turned(symbol, 0), Some(symbol));
        }
        assert_eq!(rule.turned('@', 1), None);
    }

    #[test]
    fn turned_directions_point_the_same_way_on_the_map() {
        let tri = DirectionTileRule::parse(TRIANGLE_RULE).expect("triangle rule");
        // East shows at the top of a view turned counter-clockwise.
        assert_eq!(tri.turned('►', 1), Some('▲'));
        assert_eq!(tri.turned('►', 2), Some('◄'));
        assert_eq!(tri.turned('►', 3), Some('▼'));
        assert_eq!(tri.turned('▲', 1), Some('◄'));
        assert_eq!(tri.turned('●', 1), Some('●'));
        assert_eq!(tri.turned('x', 1), None);
    }

    #[test]
    fn rejects_non_3x3_direction_input() {
        assert!(DirectionTileRule::parse("ab\ncd").is_none());
    }
}
