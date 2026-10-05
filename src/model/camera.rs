use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

/// What the renderers show: a renderer-neutral view of the map.
///
/// View coordinates are cells relative to the centre of the screen, x to
/// the right and y down like the map. A map point `p` is shown at
/// `(turn(p - position, rotation) + offset) * zoom`, so `position` is the
/// map point the view is centred on (before `offset`).
///
/// Something else decides where the camera goes (the camera operator);
/// renderers only read it.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct ViewCamera {
    /// Map point the view is centred on, in fractional cells.
    pub position: Vec2,
    /// How far the map is turned on screen, in radians, counter-clockwise.
    pub rotation: f32,
    /// Size of a cell on screen; 1 is the renderer's normal cell size.
    pub zoom: f32,
    /// Where `position` appears relative to the screen centre, in view
    /// cells: (0, 2) shows the followed object two cells below the centre.
    pub offset: Vec2,
}

impl Default for ViewCamera {
    fn default() -> Self {
        Self {
            position: Vec2::ZERO,
            rotation: 0.0,
            zoom: 1.0,
            offset: Vec2::ZERO,
        }
    }
}

impl ViewCamera {
    /// Where a map point is shown, in view cells from the screen centre.
    pub fn to_view(&self, map: Vec2) -> Vec2 {
        (turn(map - self.position, self.rotation) + self.offset) * self.zoom
    }

    /// The map point shown at a place in the view.
    pub fn to_map(&self, view: Vec2) -> Vec2 {
        turn(view / self.zoom - self.offset, -self.rotation) + self.position
    }

    /// The rotation rounded to whole quarter turns, in `0..4`.
    pub fn quarter_turns(&self) -> i32 {
        ((self.rotation / FRAC_PI_2).round() as i32).rem_euclid(4)
    }

    /// How far the rotation is past its nearest quarter turn, in radians,
    /// within ±π/4. Zero whenever the view rests on a quarter turn.
    pub fn quarter_remainder(&self) -> f32 {
        let quarters = (self.rotation / FRAC_PI_2).round();
        self.rotation - quarters * FRAC_PI_2
    }

    /// The map direction that points `screen` (such as up) on screen, so
    /// controls follow the view however it is turned. Mid-turn, the
    /// nearest quarter turn decides.
    pub fn map_direction(&self, screen: IVec2) -> IVec2 {
        (0..self.quarter_turns()).fold(screen, |v, _| IVec2::new(-v.y, v.x))
    }
}

/// Turns `v` counter-clockwise on screen by `angle` radians (y points down).
fn turn(v: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(v.x * cos + v.y * sin, -v.x * sin + v.y * cos)
}
