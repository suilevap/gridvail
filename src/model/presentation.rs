use bevy::prelude::*;

use super::LightCell;
use crate::foundation::portal::CellTransform;

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
///
/// Through portals one entity can be seen in several places; each is its
/// own instance, numbered from 0, and `(entity, instance)` identifies it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectCell {
    pub entity: Entity,
    pub instance: u16,
    /// Frame cell it is drawn in (its logical cell as seen by the player).
    pub pos: IVec2,
    /// Shown position in fractional frame cells; `pos` when nothing
    /// animates it.
    pub position: Vec2,
    /// Height above the ground in cells.
    pub lift: f32,
    pub cell: RenderCell,
}

/// When present, every frame cell the player does not see is drawn as seen
/// directly, so the whole map is in sight: for recordings of what happens
/// out of sight (`--reveal`).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct RevealAll;

/// What a frame cell shows while the player sees it: a map cell, seen
/// through `transform` (the identity when seen directly).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeenCell {
    pub world: IVec2,
    pub transform: CellTransform,
}

/// Composed frame.
///
/// Frame cells are laid out in the player's map coordinates, so renderers
/// place them like map cells. A frame cell the player sees shows the map
/// cell it looks onto (`seen`), which differs from its own position behind
/// a portal; one it does not see shows the map there as remembered.
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
    /// What each frame cell shows while seen; `None` when not seen now.
    pub seen: Vec<Option<SeenCell>>,
    /// For each map cell, the first frame cell seen showing it, then
    /// `seen_next` links the others (`NO_CELL` ends a list).
    pub seen_first: Vec<u32>,
    pub seen_next: Vec<u32>,
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
            seen: vec![None; n],
            seen_first: vec![Self::NO_CELL; n],
            seen_next: vec![Self::NO_CELL; n],
        }
    }

    /// Ends a `seen_first` / `seen_next` list.
    pub const NO_CELL: u32 = u32::MAX;

    /// The frame cell at `index`.
    pub fn pos_of(&self, index: usize) -> IVec2 {
        IVec2::new(index as i32 % self.width, index as i32 / self.width)
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
