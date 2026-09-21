use super::*;
use crate::model::{LightCell, LightKind};

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
