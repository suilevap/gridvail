use bevy::prelude::*;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Pos(pub IVec2);

/// Cells moved per simulation pass, not metres per second.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Speed(pub IVec2);

#[derive(Component, Clone, Copy, Debug)]
pub struct Friction(pub i32);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PendingPos {
    #[default]
    None,
    MoveTo(IVec2),
    RemoveFromMap,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrevPos(pub IVec2);

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Facing(pub IVec2);

#[derive(Component, Clone, Copy, Debug)]
pub struct BoundTo {
    pub parent: Entity,
    pub offset: IVec2,
    pub offset_dir: IVec2,
}

pub fn rotate(v: IVec2, dir: IVec2) -> IVec2 {
    if dir == IVec2::ZERO {
        return v;
    }
    debug_assert_eq!(
        dir.length_squared(),
        1,
        "only normalized direction supported"
    );
    IVec2::new(v.x * dir.x + v.y * dir.y, -v.y * dir.x + v.x * dir.y)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    None,
    Right,
    Up,
    Left,
    Down,
}

impl Direction {
    pub fn of(v: IVec2) -> Self {
        if v == IVec2::ZERO {
            Self::None
        } else if v.x.abs() > v.y.abs() {
            if v.x > 0 {
                Self::Right
            } else {
                Self::Left
            }
        } else if v.y > 0 {
            Self::Up
        } else {
            Self::Down
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::None => 0,
            Self::Right => 1,
            Self::Up => 2,
            Self::Left => 3,
            Self::Down => 4,
        }
    }
}
