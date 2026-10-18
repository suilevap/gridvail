use super::*;
use crate::model::{BounceLight, LightCell, LightKind, SkyLight, SunTime};

fn context(kind: LightKind, value: u8) -> LightContext {
    LightContext::new((0, 0), 4, value, kind)
}

#[test]
fn falloff_is_full_at_center_and_fades() {
    let mut center = LightCell::default();
    merge_light(&mut center, 1.0, 0, 0, &context(LightKind::Fire, 100));
    assert_eq!((center.value, center.kind), (100, LightKind::Fire));
    let mut edge = LightCell::default();
    merge_light(&mut edge, 1.0, 4, 0, &context(LightKind::Fire, 100));
    assert_eq!(edge.value, 36);
}

#[test]
fn same_kind_saturates_at_255() {
    let mut cell = LightCell::default();
    let context = context(LightKind::Fire, 200);
    merge_light(&mut cell, 1.0, 0, 0, &context);
    merge_light(&mut cell, 1.0, 0, 0, &context);
    assert_eq!(cell.value, 255);
}

#[test]
fn different_kind_wins_only_when_brighter() {
    let mut cell = LightCell::default();
    merge_light(&mut cell, 1.0, 0, 0, &context(LightKind::Fire, 100));
    merge_light(&mut cell, 1.0, 0, 0, &context(LightKind::Acid, 50));
    assert_eq!((cell.value, cell.kind), (100, LightKind::Fire));
    merge_light(&mut cell, 1.0, 0, 0, &context(LightKind::Acid, 150));
    assert_eq!((cell.value, cell.kind), (150, LightKind::Acid));
}

#[test]
fn palette_lookup_matches_original_tables() {
    assert_eq!(light_to_palette(&LightCell::default()), BLACK);
    assert_eq!(
        light_to_palette(&LightCell {
            value: 255,
            kind: LightKind::Fire
        }),
        WHITE
    );
    assert_eq!(
        light_to_palette(&LightCell {
            value: 1,
            kind: LightKind::Fire
        }),
        DARK_YELLOW
    );
    assert_eq!(
        light_to_palette(&LightCell {
            value: 1,
            kind: LightKind::None
        }),
        DARK_GRAY
    );
}

#[test]
fn remembered_colours_are_darker() {
    use super::palette::*;
    assert_eq!(dimmed(YELLOW), DARK_YELLOW);
    assert_eq!(dimmed(CYAN), DARK_CYAN);
    assert_eq!(dimmed(WHITE), GRAY);
    assert_eq!(dimmed(GRAY), DARK_GRAY);
    assert_eq!(dimmed(DARK_YELLOW), DARK_GRAY);
    assert_eq!(dimmed(BLACK), BLACK);
}

/// A topology from rows of `#` (walls) and `.` (open cells), and a layer
/// lit by `light` on the cells marked `*` (open) or `@` (walls).
fn bounce_map(rows: &[&str], light: LightCell) -> (BounceTopology, Vec<LightCell>, usize) {
    let width = rows[0].len();
    let mut topology = BounceTopology::default();
    let mut layer = Vec::new();
    for (y, row) in rows.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            topology.open.push(matches!(ch, '.' | '*'));
            layer.push(if matches!(ch, '*' | '@') {
                light
            } else {
                LightCell {
                    value: 1,
                    kind: LightKind::None,
                }
            });
            let mut neighbours = [NO_NEIGHBOUR; 4];
            let steps = [(0, -1), (1, 0), (0, 1), (-1, 0)];
            for (slot, (dx, dy)) in steps.into_iter().enumerate() {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx >= 0 && ny >= 0 && (nx as usize) < width && (ny as usize) < rows.len() {
                    neighbours[slot] = (ny as usize * width + nx as usize) as u32;
                }
            }
            topology.neighbours.push(neighbours);
        }
    }
    (topology, layer, width)
}

fn bounced() -> BounceLight {
    BounceLight {
        enabled: true,
        ..BounceLight::default()
    }
}

#[test]
fn bounced_light_reaches_round_a_corner() {
    let fire = LightCell {
        value: 200,
        kind: LightKind::Fire,
    };
    let (topology, mut layer, width) = bounce_map(
        &[
            "#@@@@#", //
            "@****@", //
            "#@@@*@", //
            "####.#", //
            "####.#", //
        ],
        fire,
    );
    Bounce::default().apply(&mut layer, &topology, &bounced());
    let around = layer[4 * width + 4];
    assert_eq!(around.kind, LightKind::Fire);
    assert!(around.value > 1, "the corridor gets bounced light");
    assert!(
        layer[3 * width + 4].value > around.value,
        "fading on the way"
    );
    assert_eq!(layer[width + 1], fire, "lit cells keep their own light");
}

#[test]
fn a_wall_gives_light_back_only_to_its_lit_side() {
    let fire = LightCell {
        value: 200,
        kind: LightKind::Fire,
    };
    let (topology, mut layer, width) = bounce_map(
        &[
            "**@..", //
            "**@..", //
            "**@..", //
        ],
        fire,
    );
    Bounce::default().apply(&mut layer, &topology, &bounced());
    assert_eq!(layer[width + 3].value, 1, "nothing shines through the wall");
}

#[test]
fn bounced_light_tints_a_dimmer_light_of_another_kind() {
    let (topology, mut layer, width) = bounce_map(
        &[
            "@@@@@", //
            "*****", //
            ".....", //
        ],
        LightCell {
            value: 255,
            kind: LightKind::Fire,
        },
    );
    let acid = 2 * width + 2;
    layer[acid] = LightCell {
        value: 10,
        kind: LightKind::Acid,
    };
    Bounce::default().apply(&mut layer, &topology, &bounced());
    assert_eq!(layer[acid].kind, LightKind::Fire);
    assert!(layer[acid].value > 10);
}

#[test]
fn smooth_light_starts_unlit_and_never_darkens() {
    let brightness = |[r, g, b]: [u8; 3]| 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32;
    for kind in [
        LightKind::None,
        LightKind::Fire,
        LightKind::Electricity,
        LightKind::Acid,
        LightKind::Sky,
        LightKind::Sun,
    ] {
        let rgb = |value| light_to_rgb(&LightCell { value, kind });
        assert_eq!(rgb(1), [64, 64, 64], "{kind:?} fades into the unlit grey");
        for value in 2..=255 {
            assert!(
                brightness(rgb(value)) >= brightness(rgb(value - 1)) - 0.5,
                "{kind:?} gets darker at {value}"
            );
        }
    }
}

/// Daylight on rows of `X` (walls), `.` (open sky), `#` (roofed walls)
/// and `,` (roofed floor), at `time`: the layer and its width.
fn daylight(rows: &[&str], time: SunTime) -> (Vec<LightCell>, usize) {
    let width = rows[0].len();
    let cells: Vec<char> = rows.iter().flat_map(|row| row.chars()).collect();
    let blocks: Vec<bool> = cells.iter().map(|&c| matches!(c, 'X' | '#')).collect();
    let roofed: Vec<bool> = cells.iter().map(|&c| matches!(c, '#' | ',')).collect();
    let mut layer = vec![
        LightCell {
            value: 1,
            kind: LightKind::None,
        };
        cells.len()
    ];
    let sky = SkyLight {
        time,
        ..SkyLight::default()
    };
    Daylight::default().apply(&mut layer, width as i32, &blocks, &roofed, &sky);
    (layer, width)
}

#[test]
fn walls_cast_shadows_away_from_the_sun() {
    // At dusk the sun is in the west: the shadow falls east of the wall.
    let (layer, _) = daylight(&["......X......"], SunTime::Dusk);
    assert_eq!(
        layer[5].kind,
        LightKind::Sun,
        "west of the wall, in the sun"
    );
    for east in 7..=10 {
        assert_eq!(layer[east].kind, LightKind::Sky, "{east} is in its shadow");
        assert!(layer[east].value < layer[5].value);
    }
    assert_eq!(layer[11].kind, LightKind::Sun, "past the shadow's reach");
    let (noon, _) = daylight(&["......X......"], SunTime::Noon);
    assert!(noon.iter().all(|cell| cell.kind == LightKind::Sun));
}

#[test]
fn daylight_goes_under_a_roof_only_through_an_opening_and_fades() {
    let (layer, width) = daylight(
        &[
            "...........",
            ".#####.###.",
            ".#,,,,,,,#.",
            ".#########.",
            ".#,,,,,,,#.",
            ".#########.",
        ],
        SunTime::Noon,
    );
    let row = |y: usize| &layer[y * width..(y + 1) * width];
    assert_eq!(row(2)[6].kind, LightKind::Sun);
    assert!(row(2)[5].value > row(2)[2].value, "darker further in");
    assert!(row(2)[2].value > 1, "but still lit");
    assert!(
        row(4)[2..9].iter().all(|cell| cell.value == 1),
        "a closed room stays dark"
    );
}

#[test]
fn low_sun_shines_in_through_a_doorway() {
    // At dusk, from the west: a patch on the floor in line with the door.
    let (layer, width) = daylight(
        &["..#########", "...,,,,,,,#", "..#########"],
        SunTime::Dusk,
    );
    // The sun's 150 at 85% for the four cells of the patch, where daylight
    // spreading in would fade at once.
    for x in 3..=6 {
        assert_eq!(
            layer[x + width],
            LightCell {
                value: 128,
                kind: LightKind::Sun
            },
            "{x} is in the patch"
        );
    }
    assert!(layer[7 + width].value < 128, "past the patch it fades");
}
