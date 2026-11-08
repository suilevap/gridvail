//! Effects, one file each in `assets/effects` (`<name>.ron`): what an
//! effect throws off, as emitters of particles. Plain data, read with the
//! generic `data::read`; a new effect is a new file, which `build.rs` makes
//! an `EffectId` variant (`blood.ron` is `Blood`) that code and data name
//! it by (a weapon's `streak`, an actor's `bleeds`).

use serde::Deserialize;

/// One effect: every emitter plays when it is asked for.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct EffectSpec {
    pub emitters: Vec<EmitterSpec>,
}

/// Particles of one look, thrown off together.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct EmitterSpec {
    /// How many, and how many more for each point of the request's
    /// strength (the damage of a blow).
    pub count: u32,
    #[serde(default)]
    pub per_strength: f32,
    /// Where they start and which way they go.
    pub from: SourceSpec,
    /// How fast they go that way (cells a second, picked in the range), and
    /// the share of their speed they lose a second (a rate).
    #[serde(default)]
    pub speed: (f32, f32),
    #[serde(default)]
    pub drag: f32,
    /// Seconds after the request's own delay before they appear.
    #[serde(default)]
    pub delay: f32,
    /// How long each lives, in seconds (picked in the range), and the share
    /// of that it shows fully before it fades out.
    pub life: (f32, f32),
    #[serde(default)]
    pub hold: f32,
    /// What each is drawn as.
    pub glyphs: GlyphSpec,
    /// The colours each starts in (one picked), and the colour it turns
    /// to over its life (its own when not given).
    pub colors: Vec<(u8, u8, u8)>,
    #[serde(default)]
    pub fade_to: Option<(u8, u8, u8)>,
}

/// Where an emitter's particles start, from the request's place and way.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum SourceSpec {
    /// `ahead` cells along the way (negative: back toward where it came
    /// from), each going off within `spread` radians either side of it.
    Point { ahead: f32, spread: f32 },
    /// Along the request's swing, one after another over its time: a
    /// thrust's line from `near` to `far` cells out, a sweep's arc `far`
    /// out, `arc` radians either side of straight ahead. Each goes the way
    /// the swing does there. Nothing without a swing.
    Along { near: f32, far: f32, arc: f32 },
}

/// What a particle is drawn as.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub enum GlyphSpec {
    /// One of these, picked.
    Pick(Vec<char>),
    /// A stroke the way it goes (`-`, `\`, `|`, `/`).
    Along,
}
