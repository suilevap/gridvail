use bevy::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LightKind {
    #[default]
    None = 0,
    Fire = 1,
    Electricity = 2,
    Acid = 4,
    /// Daylight from the open sky, in shadow.
    Sky = 8,
    /// Sunlight.
    Sun = 16,
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

/// Which cells of the map are under a roof (row by row), from the map's
/// `@roof` block. A map without one has no sky.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct Roofs(pub Vec<bool>);

/// Where the sun is. Its light comes in slanting, so walls cast shadows away
/// from it, longer when it is low, and it shines through doorways onto the
/// floor inside. At noon it is overhead: no shadows, no patches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SunTime {
    Night,
    Dawn,
    #[default]
    Morning,
    Noon,
    Afternoon,
    Dusk,
}

impl SunTime {
    const ALL: [Self; 6] = [
        Self::Night,
        Self::Dawn,
        Self::Morning,
        Self::Noon,
        Self::Afternoon,
        Self::Dusk,
    ];

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|time| time.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Night => "night",
            Self::Dawn => "dawn",
            Self::Morning => "morning",
            Self::Noon => "noon",
            Self::Afternoon => "afternoon",
            Self::Dusk => "dusk",
        }
    }

    pub fn next(self) -> Self {
        Self::ALL[(self as usize + 1) % Self::ALL.len()]
    }

    /// The sun's light, if it is up: the cell step it travels (on the map,
    /// y down, south being +y), how many cells a wall's shadow and a
    /// doorway's sun patch reach, and how bright it is.
    pub fn sun(self) -> Option<(IVec2, i32, u8)> {
        match self {
            Self::Night => None,
            // From the east, low.
            Self::Dawn => Some((IVec2::NEG_X, 4, 150)),
            // From the south-east.
            Self::Morning => Some((IVec2::new(-1, -1), 2, 200)),
            Self::Noon => Some((IVec2::ZERO, 0, 235)),
            // From the south-west.
            Self::Afternoon => Some((IVec2::new(1, -1), 2, 200)),
            // From the west, low.
            Self::Dusk => Some((IVec2::X, 4, 150)),
        }
    }
}

/// Daylight on maps with a roof: open cells get the sun (or, in a wall's
/// shadow, the sky's dimmer light), and it reaches under the roofs only
/// through openings, fading with each cell further in.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct SkyLight {
    pub time: SunTime,
    /// The sky's light in shadow, as a share of the sun's.
    pub shade: f32,
    /// The share of daylight that reaches each next cell under a roof.
    pub falloff: f32,
    /// How many cells daylight goes in under a roof.
    pub steps: u8,
}

impl Default for SkyLight {
    fn default() -> Self {
        Self {
            time: SunTime::default(),
            shade: 0.55,
            falloff: 0.82,
            steps: 12,
        }
    }
}
