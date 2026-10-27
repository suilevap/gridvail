use bevy::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LightKind {
    #[default]
    None = 0,
    Fire = 1,
    Electricity = 2,
    Acid = 4,
}

impl LightKind {
    pub fn overlaps(self, other: LightKind) -> bool {
        (self as u8) & (other as u8) != 0
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct LightSource {
    pub radius: i32,
    pub kind: LightKind,
    pub value: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightCell {
    pub value: u8,
    pub kind: LightKind,
}

/// Light bounced off lit walls and floors, filling the shadows the direct
/// light leaves: it reaches round corners and tints what is near a coloured
/// light. Off by default: the original game lights only what a light sees.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct BounceLight {
    pub enabled: bool,
    /// The share of the light falling on a wall it gives back.
    pub wall: f32,
    /// The share of the light falling on a floor it gives back.
    pub floor: f32,
    /// The share of bounced light that reaches the next cell.
    pub spread: f32,
    /// How many cells bounced light travels.
    pub steps: u8,
}

impl Default for BounceLight {
    fn default() -> Self {
        Self {
            enabled: false,
            wall: 0.4,
            floor: 0.1,
            spread: 0.7,
            steps: 6,
        }
    }
}

/// How light colours cells: in the original game's palette steps, or
/// smoothly, each kind's colours blended along a gradient.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LightStyle {
    #[default]
    Palette,
    Smooth,
}

impl LightStyle {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "palette" => Some(Self::Palette),
            "smooth" => Some(Self::Smooth),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Palette => "palette",
            Self::Smooth => "smooth",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Palette => Self::Smooth,
            Self::Smooth => Self::Palette,
        }
    }
}

/// How the floor is drawn: as the text renderer's dot per cell (`None`),
/// or by a floor shader from [`FloorLight`] in one of the floor tile
/// styles (`assets/floor_tiles.txt`), by its index.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FloorStyle(pub Option<usize>);

impl FloorStyle {
    /// The next of `styles` floor tile styles, then the text renderer's.
    pub fn next(self, styles: usize) -> Self {
        Self(match self.0 {
            None => (styles > 0).then_some(0),
            Some(style) => (style + 1 < styles).then_some(style + 1),
        })
    }
}
