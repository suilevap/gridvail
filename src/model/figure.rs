use bevy::{platform::collections::HashMap, prelude::*};

use super::Rgb;

/// Drawn as a stack of glyph parts (`Figures`, by name) by renderers that
/// show height, in place of its glyph; others draw its glyph as before.
///
/// A figure is drawn as seen from above, facing north: its parts turn with
/// the object's `Facing`, so which way it faces shows from any side.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Figure(pub String);

impl Figure {
    pub fn named(name: &str) -> Self {
        Self(name.to_string())
    }
}

/// What a figure part draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartGlyph {
    Char(char),
    /// The object's own glyph (as its tile rules turn it), so what changes
    /// it (a mood, a door opening) shows on the figure too.
    Own,
}

/// One slice of a figure: a glyph lying flat, standing from `bottom` to
/// `top` (cells above the floor), its ink extruded between them.
#[derive(Clone, Debug, PartialEq)]
pub struct FigurePart {
    pub glyph: PartGlyph,
    pub bottom: f32,
    pub top: f32,
    /// Where the glyph's cell is centred, from the object's centre, in
    /// cells, as drawn facing north (x to the right, y toward the back).
    pub offset: Vec2,
    /// The glyph's size, 1 for a whole cell.
    pub scale: f32,
    /// Multiplies the light it is in.
    pub tint: Rgb,
    /// Its glyph keeps to the map instead of turning with the figure (its
    /// offset still turns): for a glyph that shows direction itself.
    pub upright: bool,
    /// Drawn only while the object's glyph is one of these; always if
    /// empty. Shows an object's state, such as a mood.
    pub only: Vec<char>,
}

impl FigurePart {
    pub fn shown_for(&self, glyph: char) -> bool {
        self.only.is_empty() || self.only.contains(&glyph)
    }
}

/// The parts of a figure, bottom to top in the order listed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FigureShape {
    pub parts: Vec<FigurePart>,
}

impl FigureShape {
    /// The highest top of its parts, in cells.
    pub fn top(&self) -> f32 {
        self.parts.iter().map(|part| part.top).fold(0.0, f32::max)
    }
}

/// Every figure by name.
#[derive(Resource, Clone, Debug, Default)]
pub struct Figures(pub HashMap<String, FigureShape>);

impl Figures {
    pub fn get(&self, figure: &Figure) -> Option<&FigureShape> {
        self.0.get(&figure.0)
    }
}
