use bevy::prelude::*;

use super::{GestureShape, Rgb};

/// A short-lived glyph that is only shown (a drop of blood, a stroke of a
/// swing): anywhere between cells, off the map grid, blocking nothing. It
/// drifts at `velocity` (cells a second), slowed by `drag` (the share of
/// its speed lost a second, as a rate), turns from `color` to `end_color`
/// over its `life`, and shows fully for the share `hold` of it, then fades
/// out. It is drawn as `ch`, or, a stroke, as a line the way of `stroke`
/// (picked where it is drawn, so a turned view turns it).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    pub position: Vec2,
    pub velocity: Vec2,
    pub drag: f32,
    pub age: f32,
    pub life: f32,
    pub hold: f32,
    pub ch: char,
    pub stroke: Option<Vec2>,
    pub color: Rgb,
    pub end_color: Rgb,
}

impl Particle {
    /// How far through its life it is, 0 to 1.
    pub fn progress(&self) -> f32 {
        (self.age / self.life.max(f32::EPSILON)).clamp(0.0, 1.0)
    }

    /// How much of it shows now, from 1 down to 0 as it fades out.
    pub fn alpha(&self) -> f32 {
        let past = self.progress() - self.hold;
        if past <= 0.0 {
            1.0
        } else {
            (1.0 - past / (1.0 - self.hold).max(f32::EPSILON)).max(0.0)
        }
    }

    /// Its colour now.
    pub fn shown_color(&self) -> Rgb {
        let t = self.progress();
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgb::new(
            mix(self.color.r, self.end_color.r),
            mix(self.color.g, self.end_color.g),
            mix(self.color.b, self.end_color.b),
        )
    }
}

/// A particle that has not appeared yet: seconds until it does.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ParticleDelay(pub f32);

include!(concat!(env!("OUT_DIR"), "/effect_ids.rs"));

/// Play the effect `effect` (a file in `assets/effects`) from where
/// `entity` is shown, its way `toward`, `delay` seconds from now. A swing
/// gives its `swing`, which effects can follow over `secs`; `strength`
/// scales how much it throws off (the damage of a blow).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct PlayEffect {
    pub effect: EffectId,
    pub entity: Entity,
    pub toward: Vec2,
    pub swing: Option<GestureShape>,
    pub secs: f32,
    pub strength: i32,
    pub delay: f32,
}

/// What a blow knocks out of it (an effect: blood, splinters).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bleeds(pub EffectId);

/// One particle as the frame shows it: where (in frame cells, between
/// cells), what (`ch`, or a stroke the way of `stroke`, in frame cells),
/// and how much of it shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectCell {
    pub position: Vec2,
    pub ch: char,
    pub stroke: Option<Vec2>,
    pub color: Rgb,
    pub alpha: f32,
}

/// The line drawn for a stroke going `way` on screen, with y growing down
/// the screen (`-`, `\`, `|`, `/`).
pub fn stroke_glyph(way: Vec2) -> char {
    let angle = way.y.atan2(way.x).rem_euclid(std::f32::consts::PI);
    match (angle / std::f32::consts::FRAC_PI_4).round() as i32 {
        1 => '\\',
        2 => '|',
        3 => '/',
        _ => '-',
    }
}
