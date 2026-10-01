use bevy::prelude::*;

/// Color shared by a key and the doors it opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyColor {
    Red,
    Green,
    Blue,
    Yellow,
}

impl KeyColor {
    /// Map letter of a key of this color; its door is the uppercase letter.
    pub fn from_letter(letter: char) -> Option<Self> {
        match letter.to_ascii_lowercase() {
            'r' => Some(Self::Red),
            'g' => Some(Self::Green),
            'b' => Some(Self::Blue),
            'y' => Some(Self::Yellow),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Yellow => "yellow",
        }
    }
}

/// An object an actor picks up by stepping onto its cell. A carried item has
/// no `Pos`; it is listed in its carrier's `Inventory` instead.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Item;

/// An item that opens doors of its color. It is kept after opening one.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key(pub KeyColor);

/// A door that opens when an actor carrying a key of its color walks into
/// it. A closed door is a `Collider` that blocks vision; an open one is
/// neither.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Door {
    pub color: KeyColor,
    pub open: bool,
}

impl Door {
    pub const CLOSED_GLYPH: char = '+';
    pub const OPEN_GLYPH: char = '\'';

    pub const fn closed(color: KeyColor) -> Self {
        Self { color, open: false }
    }
}

/// Items an actor carries. They are dropped on its cell when it is destroyed.
#[derive(Component, Clone, Debug, Default)]
pub struct Inventory(pub Vec<Entity>);

/// The glyph keeps its own color instead of taking the light's, so keys and
/// doors show which color they are.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct OwnColor;
