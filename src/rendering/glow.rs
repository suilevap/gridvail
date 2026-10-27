//! Glow: bright glyphs bloom. Where `GlowStyle` is set, the text camera
//! renders in HDR and spreads its brightest glyphs into a soft halo of
//! their own colour (bloom), and light sources are drawn brighter than
//! white, so they burn white at the core. B switches it, as does the
//! settings menu.

use bevy::{
    post_process::bloom::{Bloom, BloomCompositeMode, BloomPrefilter},
    prelude::*,
};

use crate::model::{GlowStyle, SettingsMenu};

/// A light source's glyph as glow draws it: this many times brighter, for
/// soft glow, and twice as much more for strong.
const SOURCE_GAIN: f32 = 0.8;

/// A colour as glow draws it: a light source's brighter than its colour.
/// Other glyphs keep theirs; past white, the screen would clip them to
/// another hue (orange to yellow).
pub(super) fn glow_color(color: Color, glow: Option<GlowStyle>, source: bool) -> Color {
    let strength = match glow {
        Some(GlowStyle::Soft) => 1.0,
        Some(GlowStyle::Strong) => 2.0,
        None | Some(GlowStyle::Off) => return color,
    };
    if !source {
        return color;
    }
    let linear = color.to_linear();
    let gain = 1.0 + strength * SOURCE_GAIN;
    Color::LinearRgba(LinearRgba {
        red: linear.red * gain,
        green: linear.green * gain,
        blue: linear.blue * gain,
        alpha: linear.alpha,
    })
}

/// The bloom for a glow style: glyphs brighter than the threshold (in
/// linear light) spread, light sources most.
fn bloom(glow: GlowStyle) -> Option<Bloom> {
    let (intensity, threshold) = match glow {
        GlowStyle::Off => return None,
        GlowStyle::Soft => (0.2, 0.4),
        GlowStyle::Strong => (0.3, 0.3),
    };
    Some(Bloom {
        intensity,
        prefilter: BloomPrefilter {
            threshold,
            threshold_softness: 0.4,
        },
        composite_mode: BloomCompositeMode::Additive,
        ..Bloom::NATURAL
    })
}

/// Blooms the text camera as `GlowStyle` says, when it changes.
pub(super) fn apply_glow(
    mut commands: Commands,
    glow: Option<Res<GlowStyle>>,
    cameras: Query<Entity, With<Camera2d>>,
) {
    let Some(glow) = glow.filter(|glow| glow.is_changed()) else {
        return;
    };
    for camera in &cameras {
        match bloom(*glow) {
            Some(bloom) => commands.entity(camera).insert(bloom),
            None => commands.entity(camera).remove::<Bloom>(),
        };
    }
}

/// B switches glow: off, soft, strong.
pub(super) fn cycle_glow(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    menu: Res<SettingsMenu>,
    glow: Option<ResMut<GlowStyle>>,
) {
    if menu.open {
        return;
    }
    if let Some(mut glow) = glow {
        if keys.is_some_and(|keys| keys.just_pressed(KeyCode::KeyB)) {
            *glow = glow.step(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brightest(color: Color) -> f32 {
        let linear = color.to_linear();
        linear.red.max(linear.green).max(linear.blue)
    }

    #[test]
    fn light_sources_go_past_white_and_other_glyphs_keep_their_colour() {
        let fire = Color::srgb(1.0, 0.55, 0.2);
        let soft = Some(GlowStyle::Soft);
        assert_eq!(glow_color(fire, soft, false), fire);
        assert_eq!(glow_color(fire, Some(GlowStyle::Off), true), fire);
        assert_eq!(glow_color(fire, None, true), fire);
        let source = brightest(glow_color(fire, soft, true));
        let strong = brightest(glow_color(fire, Some(GlowStyle::Strong), true));
        assert!(1.0 < source && source < strong);
    }

    #[test]
    fn glow_styles_go_round_by_name() {
        for style in GlowStyle::ALL {
            assert_eq!(GlowStyle::from_name(style.name()), Some(style));
        }
        assert_eq!(GlowStyle::Strong.step(1), GlowStyle::Off);
        assert_eq!(GlowStyle::Off.step(-1), GlowStyle::Strong);
        assert!(bloom(GlowStyle::Off).is_none());
    }
}
