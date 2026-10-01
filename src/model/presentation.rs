use bevy::prelude::*;

use super::LightCell;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyph {
    pub ch: char,
    pub depth: u8,
    pub color: u8,
}

impl Glyph {
    pub const fn new(ch: char, depth: u8, color: u8) -> Self {
        Self { ch, depth, color }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderCell {
    pub ch: char,
    pub color: u8,
    pub depth: u8,
}

/// A visible glyph entity (wall, lamp, actor...), drawn by renderers on its
/// own, above the ground, where the animation step shows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectCell {
    pub entity: Entity,
    /// Logical cell.
    pub pos: IVec2,
    /// Shown position in fractional cells; `pos` when nothing animates it.
    pub position: Vec2,
    /// Height above the ground in cells.
    pub lift: f32,
    pub cell: RenderCell,
    /// Drawn in its glyph's color rather than the light's (`OwnColor`).
    pub own_color: bool,
}

/// Composed frame.
///
/// `current` is the flat cell view with every visible glyph. Renderers draw
/// it as two layers instead: `ground` holds only what belongs to cells (the
/// floor and the fog edge, blank under objects) and `objects` lists every
/// visible glyph entity, each of which may be anywhere between cells.
#[derive(Resource, Debug)]
pub struct RenderBuffers {
    pub width: i32,
    pub height: i32,
    pub current: Vec<RenderCell>,
    pub previous: Vec<RenderCell>,
    pub ground: Vec<RenderCell>,
    pub previous_ground: Vec<RenderCell>,
    pub objects: Vec<ObjectCell>,
}

impl RenderBuffers {
    pub fn new(width: i32, height: i32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let n = (width * height) as usize;
        Self {
            width,
            height,
            current: vec![RenderCell::default(); n],
            previous: vec![RenderCell::default(); n],
            ground: vec![RenderCell::default(); n],
            previous_ground: vec![RenderCell::default(); n],
            objects: Vec::with_capacity(n),
        }
    }

    pub fn idx(&self, p: IVec2) -> Option<usize> {
        (p.x >= 0 && p.y >= 0 && p.x < self.width && p.y < self.height)
            .then_some((p.y * self.width + p.x) as usize)
    }

    pub fn swap(&mut self) {
        std::mem::swap(&mut self.current, &mut self.previous);
        std::mem::swap(&mut self.ground, &mut self.previous_ground);
    }
}

#[derive(Resource, Debug)]
pub struct StaticLight {
    pub width: i32,
    pub height: i32,
    pub data: Vec<LightCell>,
    pub dirty: bool,
    pub last_seen_revisions: Vec<(Entity, u64)>,
    pub current_revisions: Vec<(Entity, u64)>,
}

impl StaticLight {
    pub fn new() -> Self {
        Self {
            width: 1,
            height: 1,
            data: vec![LightCell::default()],
            dirty: true,
            last_seen_revisions: Vec::new(),
            current_revisions: Vec::new(),
        }
    }

    pub fn resize(&mut self, width: i32, height: i32) {
        self.width = width.max(1);
        self.height = height.max(1);
        self.data = vec![LightCell::default(); (self.width * self.height) as usize];
        self.dirty = true;
        self.last_seen_revisions.clear();
        self.current_revisions.clear();
    }
}

impl Default for StaticLight {
    fn default() -> Self {
        Self::new()
    }
}

pub fn is_hex_pos(p: IVec2) -> bool {
    (p.x + p.y % 2) % 2 == 0
}
