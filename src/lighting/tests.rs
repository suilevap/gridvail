use super::*;
use crate::model::{LightCell, LightKind, Rgb};

fn context(kind: LightKind, value: u8) -> LightContext {
    LightContext::new((0, 0), 4, value, kind)
}

#[test]
fn falloff_is_full_at_center_and_fades() {
    let mut center = LightCell::default();
    merge_light(&mut center, 1.0, 0, 0, &context(LightKind::Fire, 100));
    assert_eq!(center, LightCell::of(LightKind::Fire, 100));
    let mut edge = LightCell::default();
    merge_light(&mut edge, 1.0, 4, 0, &context(LightKind::Fire, 100));
    assert_eq!(edge.level(LightKind::Fire), 36);
}

#[test]
fn same_kind_saturates_at_255() {
    let mut cell = LightCell::default();
    let context = context(LightKind::Fire, 200);
    merge_light(&mut cell, 1.0, 0, 0, &context);
    merge_light(&mut cell, 1.0, 0, 0, &context);
    assert_eq!(cell.level(LightKind::Fire), 255);
}

#[test]
fn every_kind_keeps_its_own_light() {
    let mut cell = LightCell::default();
    merge_light(&mut cell, 1.0, 0, 0, &context(LightKind::Fire, 100));
    merge_light(&mut cell, 1.0, 0, 0, &context(LightKind::Acid, 50));
    merge_light(&mut cell, 1.0, 0, 0, &context(LightKind::None, 20));
    merge_light(&mut cell, 1.0, 0, 0, &context(LightKind::None, 20));
    assert_eq!(cell.levels, [40, 100, 0, 50]);
    assert_eq!(cell.total(), 190);
    merge_light(&mut cell, 1.0, 0, 0, &context(LightKind::Electricity, 100));
    assert_eq!(cell.total(), 255, "saturates");
}

#[test]
fn one_kind_alone_is_its_gradient() {
    for kind in LightKind::ALL {
        for value in [1, 60, 200, 255] {
            assert_eq!(
                light_color(&LightCell::of(kind, value)),
                light_lut(kind).get(value)
            );
        }
    }
}

#[test]
fn kinds_mix_by_their_share_at_their_combined_brightness() {
    let mut cell = LightCell::of(LightKind::Fire, 60);
    cell.add(LightKind::Electricity, 60);
    let fire = FIRE.get(120);
    let electricity = ELECTRICITY.get(120);
    let half = |a: u8, b: u8| (a as u16 + b as u16).div_ceil(2) as u8;
    assert_eq!(
        light_color(&cell),
        Rgb::new(
            half(fire.r, electricity.r),
            half(fire.g, electricity.g),
            half(fire.b, electricity.b)
        ),
        "an even mix of both gradients at 120"
    );
    // A faint light barely tints a bright one.
    let mut lamp = LightCell::of(LightKind::Fire, 200);
    lamp.add(LightKind::Acid, 2);
    let color = light_color(&lamp);
    let lit = FIRE.get(202);
    assert!(color.r.abs_diff(lit.r) <= 3 && color.g.abs_diff(lit.g) <= 3);
    assert!(color.b.abs_diff(lit.b) <= 3);
}

#[test]
fn no_light_is_black() {
    for kind in LightKind::ALL {
        assert_eq!(light_color(&LightCell::of(kind, 0)), BLACK);
    }
}

#[test]
fn light_brightens_smoothly_with_its_value() {
    let luma = |c: Rgb| c.r as u32 * 299 + c.g as u32 * 587 + c.b as u32 * 114;
    for kind in LightKind::ALL {
        let mut previous = light_color(&LightCell::of(kind, 1));
        assert_ne!(previous, BLACK, "{kind:?}: the least light shows");
        for value in 2..=255 {
            let color = light_color(&LightCell::of(kind, value));
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
    assert_eq!(light_color(&LightCell::of(LightKind::Fire, 255)), WHITE);
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
    let faint = light_color(&LightCell::of(LightKind::None, 1));
    assert!(dimmed(faint).r >= faint.r, "still visible");
    let yellow = dimmed(YELLOW);
    assert!(yellow.r < YELLOW.r && yellow.g < YELLOW.g);
    assert!(yellow.r.abs_diff(yellow.b) < YELLOW.r.abs_diff(YELLOW.b));
}
