//! ASCII map parsing. Mirrors `LoadMapSystem.TryGetSpawnRequest`:
//! `X`/`x` wall, `p` player, `e` enemy, `~` electricity, `i` light,
//! `%` acid; anything else is empty floor.

use bevy::prelude::*;

/// One parsed cell that spawns an entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnKind {
    Wall,
    Player,
    Enemy,
    Electricity,
    Light,
    Acid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnCell {
    pub pos: IVec2,
    pub kind: SpawnKind,
}

pub fn spawn_kind_of(c: char) -> Option<SpawnKind> {
    match c {
        'X' | 'x' => Some(SpawnKind::Wall),
        'p' => Some(SpawnKind::Player),
        'e' => Some(SpawnKind::Enemy),
        '~' => Some(SpawnKind::Electricity),
        'i' => Some(SpawnKind::Light),
        '%' => Some(SpawnKind::Acid),
        _ => None,
    }
}

pub fn parse_map(text: &str) -> (i32, i32, Vec<SpawnCell>) {
    // Strip a UTF-8 BOM like `File.ReadAllLines` effectively does.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // Accept CRLF or LF.
    let lines: Vec<&str> = text.lines().map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    let width = lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0) as i32;
    let height = lines.len() as i32;
    let mut cells = Vec::new();
    for (y, line) in lines.iter().enumerate() {
        for (x, c) in line.chars().enumerate() {
            if let Some(kind) = spawn_kind_of(c) {
                cells.push(SpawnCell {
                    pos: IVec2::new(x as i32, y as i32),
                    kind,
                });
            }
        }
    }
    (width, height, cells)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_six_entity_types() {
        let (w, h, cells) = parse_map("Xpx\n~i%\ne..");
        assert_eq!((w, h), (3, 3));
        for (pos, kind) in [
            (IVec2::new(0, 0), SpawnKind::Wall),
            (IVec2::new(1, 0), SpawnKind::Player),
            (IVec2::new(2, 0), SpawnKind::Wall),
            (IVec2::new(0, 1), SpawnKind::Electricity),
            (IVec2::new(1, 1), SpawnKind::Light),
            (IVec2::new(2, 1), SpawnKind::Acid),
            (IVec2::new(0, 2), SpawnKind::Enemy),
        ] {
            assert!(
                cells.contains(&SpawnCell { pos, kind }),
                "missing {kind:?} at {pos}"
            );
        }
        // '.' and unknown chars spawn nothing
        assert_eq!(cells.len(), 7);
    }

    #[test]
    fn handles_bom_and_crlf() {
        let (w, h, cells) = parse_map("\u{feff}Xp\r\neX");
        assert_eq!((w, h), (2, 2));
        assert_eq!(cells.len(), 4);
    }

    #[test]
    fn empty_input_yields_empty_world() {
        let (w, h, cells) = parse_map("");
        assert_eq!((w, h), (0, 0));
        assert!(cells.is_empty());
    }
}
