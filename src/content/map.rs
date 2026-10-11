//! ASCII map parsing. Mirrors `LoadMapSystem.TryGetSpawnRequest`:
//! `X`/`x` wall, `p` player, `e` enemy, `~` electricity, `i` light,
//! `%` acid; anything else is empty floor. Additions: `k` key, `D` door,
//! `h` hunter, `_` pressure plate, and digits `1`-`9` for portal walls (the
//! two cells with the same digit lead to each other).
//!
//! After the map, `@room <x> <y>` blocks give other layouts of a room: the
//! block's lines are the walls (`X`/`x`; anything else is floor) of the
//! rectangle they cover, with its top-left corner at `x`, `y`. The map
//! itself holds the room's first layout; blocks for the same corner follow
//! it in order.

use bevy::prelude::*;

/// One parsed cell that spawns an entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnKind {
    Wall,
    Player,
    Enemy,
    Hunter,
    Electricity,
    Light,
    Acid,
    Key,
    Door,
    Plate,
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
        'h' => Some(SpawnKind::Hunter),
        '~' => Some(SpawnKind::Electricity),
        'i' => Some(SpawnKind::Light),
        '%' => Some(SpawnKind::Acid),
        'k' => Some(SpawnKind::Key),
        'D' => Some(SpawnKind::Door),
        '_' => Some(SpawnKind::Plate),
        '1'..='9' => Some(SpawnKind::Portal(c as u8 - b'0')),
        _ => None,
    }
}

/// The lines of a map file, without a BOM or carriage returns.
fn lines_of(text: &str) -> impl Iterator<Item = &str> {
    // Strip a UTF-8 BOM like `File.ReadAllLines` effectively does.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // Accept CRLF or LF.
    text.lines().map(|l| l.strip_suffix('\r').unwrap_or(l))
}

/// Parses the map, up to its first `@` block.
pub fn parse_map(text: &str) -> (i32, i32, Vec<SpawnCell>) {
    let lines: Vec<&str> = lines_of(text)
        .take_while(|line| !line.starts_with('@'))
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

/// A room whose walls change between layouts. Each layout lists its wall
/// cells, in map coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomSpec {
    pub origin: IVec2,
    pub size: IVec2,
    pub layouts: Vec<Vec<IVec2>>,
}

/// Reads the `@room` blocks after the map. The map's `cells` (from
/// [`parse_map`]) give each room its first layout: the walls in its
/// rectangle. A layout may not put a wall on a door, portal or plate.
pub fn parse_rooms(text: &str, cells: &[SpawnCell]) -> Result<Vec<RoomSpec>, String> {
    let mut rooms: Vec<RoomSpec> = Vec::new();
    let mut lines = lines_of(text)
        .skip_while(|line| !line.starts_with('@'))
        .peekable();
    while let Some(header) = lines.next() {
        let fields: Vec<&str> = header.split_whitespace().collect();
        let origin = match fields[..] {
            ["@room", x, y] => x
                .parse()
                .and_then(|x| Ok(IVec2::new(x, y.parse()?)))
                .map_err(|_| format!("bad room corner in `{header}`"))?,
            _ => {
                return Err(format!(
                    "unknown block `{header}`; expected `@room <x> <y>`"
                ))
            }
        };
        let mut rows = Vec::new();
        while let Some(line) = lines.next_if(|line| !line.starts_with('@')) {
            rows.push(line);
        }
        while rows.last().is_some_and(|row| row.trim().is_empty()) {
            rows.pop();
        }
        let size = IVec2::new(
            rows.iter()
                .map(|row| row.chars().count())
                .max()
                .unwrap_or(0) as i32,
            rows.len() as i32,
        );
        if size.min_element() == 0 {
            return Err(format!("room at {origin} has an empty layout"));
        }
        let walls: Vec<IVec2> = rows
            .iter()
            .enumerate()
            .flat_map(|(y, row)| {
                row.chars().enumerate().filter_map(move |(x, c)| {
                    matches!(c, 'X' | 'x').then_some(origin + IVec2::new(x as i32, y as i32))
                })
            })
            .collect();
        if let Some(cell) = cells.iter().find(|cell| {
            walls.contains(&cell.pos)
                && matches!(
                    cell.kind,
                    SpawnKind::Door | SpawnKind::Portal(_) | SpawnKind::Plate
                )
        }) {
            return Err(format!(
                "room at {origin} puts a wall on the {:?} at {}",
                cell.kind, cell.pos
            ));
        }
        match rooms.iter_mut().find(|room| room.origin == origin) {
            Some(room) if room.size != size => {
                return Err(format!(
                    "room at {origin} is {} in one layout and {size} in another",
                    room.size
                ))
            }
            Some(room) => room.layouts.push(walls),
            None => {
                let inside = |p: IVec2| p.cmpge(origin).all() && p.cmplt(origin + size).all();
                let first = cells
                    .iter()
                    .filter(|cell| cell.kind == SpawnKind::Wall && inside(cell.pos))
                    .map(|cell| cell.pos)
                    .collect();
                rooms.push(RoomSpec {
                    origin,
                    size,
                    layouts: vec![first, walls],
                });
            }
        }
    }
    Ok(rooms)
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
    fn room_blocks_follow_the_map_and_give_more_layouts() {
        let text = "XXXX\nX._X\nXXXX\n@room 1 0\n.X\nX.\n@room 1 0\n..\n..\n";
        let (w, h, cells) = parse_map(text);
        assert_eq!((w, h), (4, 3));
        assert!(cells.contains(&SpawnCell {
            pos: IVec2::new(2, 1),
            kind: SpawnKind::Plate,
        }));
        let rooms = parse_rooms(text, &cells).unwrap();
        assert_eq!(
            rooms,
            [RoomSpec {
                origin: IVec2::new(1, 0),
                size: IVec2::new(2, 2),
                layouts: vec![
                    vec![IVec2::new(1, 0), IVec2::new(2, 0)],
                    vec![IVec2::new(2, 0), IVec2::new(1, 1)],
                    vec![],
                ],
            }]
        );
    }

    #[test]
    fn bad_rooms_are_reported() {
        let rooms = |text| parse_rooms(text, &parse_map(text).2);
        assert!(rooms("X_\n@room 0 0\n.X").unwrap_err().contains("Plate"));
        assert!(rooms("X.\n@room 0 0\nX\n@room 0 0\nXX")
            .unwrap_err()
            .contains("in another"));
        assert!(rooms("X.\n@door 0 0")
            .unwrap_err()
            .contains("unknown block"));
        assert!(rooms("X.\n@room a 0\nX").unwrap_err().contains("corner"));
        assert_eq!(rooms("X.\n").unwrap(), []);
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
