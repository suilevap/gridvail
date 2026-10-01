use bevy::prelude::*;

/// An object an actor picks up by stepping onto its cell. A carried item has
/// no `Pos`; it is listed in its carrier's `Inventory` instead.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Item;

/// An item that opens one door and is used up doing so.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Key;

/// A door that opens when an actor carrying a key walks into it. A closed
/// door is a `Collider` that blocks vision; an open one is neither.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Door {
    pub open: bool,
}

impl Door {
    pub const CLOSED_GLYPH: char = '+';
    pub const OPEN_GLYPH: char = '\'';
}

/// Items an actor carries. They are dropped on its cell when it is destroyed.
#[derive(Component, Clone, Debug, Default)]
pub struct Inventory(pub Vec<Entity>);
