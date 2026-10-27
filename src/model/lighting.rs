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

/// How the floor is drawn: as the text renderer's dot per cell, or by a
/// floor shader from [`FloorLight`]. Most styles are tiles: a pattern that
/// is the floor's material, lit a pixel at a time as glyphs are lit. The
/// others draw the light itself as a pattern.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FloorStyle {
    #[default]
    Text = 0,
    /// Light as four dots a cell, larger where it is brighter.
    Dots = 1,
    /// Light as ordered dithering.
    Dither = 2,
    /// Light only on every third pixel row, with a slow rolling band.
    Scanlines = 3,
    /// Rows of lines raised by the light under them, swaying.
    Ridges = 4,
    /// Tiles: a dot in each cell, as the text renderer draws it.
    Points = 5,
    /// Tiles: square tiles with grout between.
    Squares = 6,
    /// Tiles: staggered boards with grain.
    Planks = 7,
    /// Tiles: rounded stones of different sizes.
    Cobble = 8,
    /// Tiles: flowing lines across the floor, like a map's contours.
    Contours = 9,
    /// Tiles: scattered specks.
    Gravel = 10,
    /// Tiles: a basket weave of short strokes.
    Weave = 11,
}

impl FloorStyle {
    const ALL: [Self; 12] = [
        Self::Text,
        Self::Points,
        Self::Squares,
        Self::Planks,
        Self::Cobble,
        Self::Contours,
        Self::Gravel,
        Self::Weave,
        Self::Dots,
        Self::Dither,
        Self::Scanlines,
        Self::Ridges,
    ];

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|style| style.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Dots => "dots",
            Self::Dither => "dither",
            Self::Scanlines => "scanlines",
            Self::Ridges => "ridges",
            Self::Points => "points",
            Self::Squares => "squares",
            Self::Planks => "planks",
            Self::Cobble => "cobble",
            Self::Contours => "contours",
            Self::Gravel => "gravel",
            Self::Weave => "weave",
        }
    }

    pub fn next(self) -> Self {
        let at = Self::ALL
            .iter()
            .position(|&style| style == self)
            .unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }
}
