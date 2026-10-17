//! What the text renderer shares with the 3D wall backend, kept apart from
//! it so a build without 3D rendering (the web build) still compiles.

use bevy::{ecs::entity::EntityHashMap, prelude::*};

/// Wall objects drawn as meshes, with the glyph each mesh replaces.
#[derive(Resource)]
pub(super) struct ExtrudedWalls {
    pub(super) symbols: EntityHashMap<char>,
    /// The mesh entity of each wall object. Only the 3D renderer reads it,
    /// and the web build never creates this resource.
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    pub(super) meshes: EntityHashMap<Entity>,
}

impl ExtrudedWalls {
    pub(super) fn symbol_of(&self, object: Entity) -> Option<char> {
        self.symbols.get(&object).copied()
    }
}

pub(super) fn billboard_pattern(symbol: char) -> Option<&'static str> {
    match symbol {
        // A trailing hair space optically centers these asymmetric glyphs
        // without moving the body or changing the billboard anchor.
        '@' => Some("@ \n/|\\\n/ \\"),
        'x' => Some("x \n/|\\\n/ \\"),
        '>' => Some("> \n/|\\\n/ \\"),
        '<' => Some("< \n/|\\\n/ \\"),
        '^' => Some("^ \n/|\\\n/ \\"),
        'V' => Some("V \n/|\\\n/ \\"),
        'e' | 'S' | 'G' | 'B' => Some("o\n/|\\\n/ \\"),
        _ => None,
    }
}
