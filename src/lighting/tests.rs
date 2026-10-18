use super::*;
use crate::model::{BounceLight, LightCell, LightKind};

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

#[test]
fn shadow_edges_light_only_the_points_a_light_sees() {
    use bevy::prelude::IVec2;
    let open = |_: IVec2| false;
    assert_eq!(lit_points(IVec2::ZERO, IVec2::new(4, 2), &open), FULLY_LIT);
    let wall = |p: IVec2| p == IVec2::new(2, 0);
    assert_eq!(
        lit_points(IVec2::ZERO, IVec2::new(4, 0), &wall),
        0,
        "straight behind the wall"
    );
    let corner = |p: IVec2| p == IVec2::new(2, 1);
    // Rays to the cell's left points pass beside the wall, to its right
    // points through it.
    let lit = lit_points(IVec2::ZERO, IVec2::new(3, 3), &corner);
    assert!(
        lit != 0 && lit != FULLY_LIT,
        "half behind the wall: {lit:#010b}"
    );
}

#[test]
fn shadow_edge_glyphs_show_the_lit_part() {
    assert_eq!(lit_blocks(FULLY_LIT), '█');
    assert_eq!(lit_blocks(0x03), '▘', "dots 1 and 2: the top left");
    assert_eq!(lit_blocks(0xA0), '▗', "dots 6 and 8: the bottom right");
    assert_eq!(lit_dot(0x01), '⠁');
    assert_eq!(
        lit_dot(0x08 | 0x10 | 0x80),
        '⠐',
        "dots 4, 5 and 8: their middle is nearest dot 5"
    );
    assert_eq!(lit_dots(FULLY_LIT), '⢕', "a checkerboard of four");
}
