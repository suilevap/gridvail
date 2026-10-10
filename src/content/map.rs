//! ASCII map parsing. Mirrors `LoadMapSystem.TryGetSpawnRequest`:
//! `X`/`x` wall, `p` player, `e` enemy, `~` electricity, `i` light,
//! `%` acid; anything else is empty floor. Additions: `k` key, `D` door,
//! and digits `1`-`9` for portal walls (the two cells with the same digit
//! lead to each other).

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
    Key,
    Door,
    /// A wall whose open face is a portal to the other cell with the same
    /// digit.
    Portal(u8),
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
        'k' => Some(SpawnKind::Key),
        'D' => Some(SpawnKind::Door),
        '1'..='9' => Some(SpawnKind::Portal(c as u8 - b'0')),
        _ => None,
    }
}

pub fn parse_map(text: &str) -> (i32, i32, Vec<SpawnCell>) {
    // Strip a UTF-8 BOM like `File.ReadAllLines` effectively does.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // Accept CRLF or LF.
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as i32;
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

/// One portal wall: its cell, its open side (towards the only floor cell
/// next to it), and the cell of the portal wall it leads to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortalLink {
    pub pos: IVec2,
    pub side: IVec2,
    pub exit: IVec2,
}

/// Pairs the portal walls of a parsed map: each digit must appear exactly
/// twice, and each portal wall must have exactly one open (non-wall,
/// in-map) neighbour, which is the side its face opens to.
pub fn portal_links(
    width: i32,
    height: i32,
    cells: &[SpawnCell],
) -> Result<Vec<PortalLink>, String> {
    let solid = |p: IVec2| {
        cells.iter().any(|cell| {
            cell.pos == p
                && matches!(
                    cell.kind,
                    SpawnKind::Wall | SpawnKind::Door | SpawnKind::Portal(_)
                )
        })
    };
    let inside = |p: IVec2| p.x >= 0 && p.y >= 0 && p.x < width && p.y < height;
    let mut links = Vec::new();
    for cell in cells {
        let SpawnKind::Portal(digit) = cell.kind else {
            continue;
        };
        let pair: Vec<IVec2> = cells
            .iter()
            .filter(|other| other.kind == cell.kind)
            .map(|other| other.pos)
            .collect();
        let [a, b] = pair[..] else {
            return Err(format!(
                "portal {digit} appears {} times; it needs exactly two cells",
                pair.len()
            ));
        };
        let exit = if a == cell.pos { b } else { a };
        let open: Vec<IVec2> = [IVec2::X, IVec2::Y, IVec2::NEG_X, IVec2::NEG_Y]
            .into_iter()
            .filter(|&side| inside(cell.pos + side) && !solid(cell.pos + side))
            .collect();
        let [side] = open[..] else {
            return Err(format!(
                "portal {digit} at {} has {} open sides; it needs exactly one",
                cell.pos,
                open.len()
            ));
        };
        links.push(PortalLink {
            pos: cell.pos,
            side,
            exit,
        });
    }
    Ok(links)
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
    fn parses_keys_and_doors() {
        let (_, _, cells) = parse_map("kD.d");
        assert_eq!(
            cells.iter().map(|cell| cell.kind).collect::<Vec<_>>(),
            [SpawnKind::Key, SpawnKind::Door]
        );
    }

    #[test]
    fn digits_are_portal_walls() {
        let (_, _, cells) = parse_map("1X0\n9.");
        assert_eq!(
            cells.iter().map(|cell| cell.kind).collect::<Vec<_>>(),
            [SpawnKind::Portal(1), SpawnKind::Wall, SpawnKind::Portal(9)]
        );
    }

    #[test]
    fn portal_walls_pair_and_open_towards_their_floor() {
        let text = "XXXXXXX\nX.1X1.X\nXXXXXXX";
        let (w, h, cells) = parse_map(text);
        let links = portal_links(w, h, &cells).unwrap();
        assert_eq!(
            links,
            [
                PortalLink {
                    pos: IVec2::new(2, 1),
                    side: IVec2::NEG_X,
                    exit: IVec2::new(4, 1),
                },
                PortalLink {
                    pos: IVec2::new(4, 1),
                    side: IVec2::X,
                    exit: IVec2::new(2, 1),
                },
            ]
        );
    }

    #[test]
    fn bad_portals_are_reported() {
        let (w, h, cells) = parse_map("X1.\nXXX");
        assert!(portal_links(w, h, &cells)
            .unwrap_err()
            .contains("exactly two"));
        let (w, h, cells) = parse_map(".1.\nX1X");
        assert!(portal_links(w, h, &cells)
            .unwrap_err()
            .contains("open sides"));
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
