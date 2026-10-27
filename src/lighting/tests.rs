use super::*;
use crate::model::{LightCell, LightKind, Rgb};

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
fn no_light_is_black() {
    for kind in [
        LightKind::None,
        LightKind::Fire,
        LightKind::Electricity,
        LightKind::Acid,
    ] {
        assert_eq!(light_color(&LightCell { value: 0, kind }), BLACK);
    }
}

#[test]
fn light_brightens_smoothly_with_its_value() {
    let luma = |c: Rgb| c.r as u32 * 299 + c.g as u32 * 587 + c.b as u32 * 114;
    for kind in [
        LightKind::None,
        LightKind::Fire,
        LightKind::Electricity,
        LightKind::Acid,
    ] {
        let mut previous = light_color(&LightCell { value: 1, kind });
        assert_ne!(previous, BLACK, "{kind:?}: the least light shows");
        for value in 2..=255 {
            let color = light_color(&LightCell { value, kind });
            assert!(luma(color) >= luma(previous), "{kind:?} {value}");
            for (a, b) in [
                (color.r, previous.r),
                (color.g, previous.g),
                (color.b, previous.b),
            ] {
                assert!(a.abs_diff(b) <= 16, "{kind:?} jumps at {value}");
            }
            previous = color;
        }
    }
    assert_eq!(
        light_color(&LightCell {
            value: 255,
            kind: LightKind::Fire
        }),
        WHITE
    );
}

#[test]
fn lut_passes_through_its_stops() {
    let lut = LightLut::bake(&[(0, BLACK), (100, Rgb::new(200, 100, 0)), (255, WHITE)]);
    assert_eq!(lut.get(0), BLACK);
    assert_eq!(lut.get(50), Rgb::new(100, 50, 0));
    assert_eq!(lut.get(100), Rgb::new(200, 100, 0));
    assert_eq!(lut.get(255), WHITE);
}

#[test]
fn remembered_colours_are_darker_and_greyer() {
    assert_eq!(dimmed(BLACK), BLACK);
    assert!(dimmed(WHITE).r < WHITE.r);
    let faint = light_color(&LightCell {
        value: 1,
        kind: LightKind::None,
    });
    assert!(dimmed(faint).r >= faint.r, "still visible");
    let yellow = dimmed(YELLOW);
    assert!(yellow.r < YELLOW.r && yellow.g < YELLOW.g);
    assert!(yellow.r.abs_diff(yellow.b) < YELLOW.r.abs_diff(YELLOW.b));
}
