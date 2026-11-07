use bevy::prelude::*;

use super::{GestureShape, Rgb};

/// A short-lived glyph that is only shown (a drop of blood, a streak of a
/// swing): anywhere between cells, off the map grid, blocking nothing. It
/// drifts at `velocity` (cells a second), slowed by `drag` (the share of
/// its speed lost a second, as a rate), and fades out over the last `fade`
/// seconds of its `life`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    pub position: Vec2,
    pub velocity: Vec2,
    pub drag: f32,
    pub age: f32,
    pub life: f32,
    pub fade: f32,
    pub ch: char,
    pub color: Rgb,
}

impl Particle {
    /// How much of it shows now, from 1 down to 0 as it fades out.
    pub fn alpha(&self) -> f32 {
        let left = self.life - self.age;
        if self.fade <= 0.0 {
            return if left > 0.0 { 1.0 } else { 0.0 };
        }
        (left / self.fade).clamp(0.0, 1.0)
    }
}

/// A particle that has not appeared yet: seconds until it does.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ParticleDelay(pub f32);

/// Blood splashes off `entity` from where it is shown, driven `toward` (the
/// way the blow went), `delay` seconds from now; `strength` scales how
/// much (the damage of the blow).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct Splash {
    pub entity: Entity,
    pub toward: Vec2,
    pub strength: i32,
    pub delay: f32,
}

/// The streak `entity`'s weapon leaves as it swings along `shape` (the
/// swing's gesture), starting `delay` seconds from now and taking `secs`.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct Swoosh {
    pub entity: Entity,
    pub shape: GestureShape,
    pub delay: f32,
    pub secs: f32,
}

/// One particle as the frame shows it: where (in frame cells, between
/// cells), what, and how much of it shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectCell {
    pub position: Vec2,
    pub ch: char,
    pub color: Rgb,
    pub alpha: f32,
}
