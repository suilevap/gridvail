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

#[derive(Resource, Debug)]
pub struct RenderBuffers {
    pub width: i32,
    pub height: i32,
    pub current: Vec<RenderCell>,
    pub previous: Vec<RenderCell>,
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
        }
    }

    pub fn idx(&self, p: IVec2) -> Option<usize> {
        (p.x >= 0 && p.y >= 0 && p.x < self.width && p.y < self.height)
            .then_some((p.y * self.width + p.x) as usize)
    }

    pub fn swap(&mut self) {
        std::mem::swap(&mut self.current, &mut self.previous);
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
