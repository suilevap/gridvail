//! Children shown relative to their animated parent.
//!
//! A child (`BoundTo`, such as the player's direction marker) is drawn at
//! its parent's shown position plus its own offset, expressed in the
//! parent's frame. Everything the parent does on screen (gliding,
//! overshooting, hopping, bumping, teleporting across the map edge) carries
//! over in the same frame, through chains of any depth. On top of that the
//! child has two motions of its own:
//!
//! - its heading follows the parent's facing, sweeping around the parent
//!   along the shorter arc instead of cutting across or through it;
//! - a change of its offset plays as a move in the parent's frame, in the
//!   child's motion style.
//!
//! No glyph rotates: renderers still draw a glyph at a position, so text
//! terminals are unaffected.

use bevy::prelude::*;

use crate::model::*;

use super::{Easing, MotionState, MotionStyle, ObjectMotion};

/// Longest `BoundTo` chain followed when ordering parents before children.
const MAX_DEPTH: u8 = 8;

/// Where `rotate(offset, facing)` puts an offset, for any heading angle.
///
/// The simulation's `rotate` maps `v` to `R(θ)·(v.x, -v.y)` for a facing
/// `(cos θ, sin θ)` (map y grows downward), so sweeping θ moves continuously
/// between the integer results it gives for the four facings.
pub fn frame(angle: f32, offset: Vec2) -> Vec2 {
    Vec2::from_angle(angle).rotate(Vec2::new(offset.x, -offset.y))
}

/// Exact integer-aligned version of [`frame`] for a facing at rest.
fn framed_by(facing: IVec2, offset: Vec2) -> Vec2 {
    if facing == IVec2::ZERO {
        return offset;
    }
    let dir = facing.as_vec2();
    Vec2::new(
        offset.x * dir.x + offset.y * dir.y,
        -offset.y * dir.x + offset.x * dir.y,
    )
}

fn angle_of(facing: IVec2) -> f32 {
    (facing.y as f32).atan2(facing.x as f32)
}

/// Animation of one child relative to its parent.
#[derive(Component, Clone, Copy, Debug)]
pub struct ChildAnimation {
    /// The child's offset in the parent's frame, animated like any move.
    pub local: MotionState,
    /// The parent facing the heading is heading for (zero: none yet).
    facing: IVec2,
    angle: f32,
    swing_from: f32,
    swing_to: f32,
    elapsed: f32,
    duration: f32,
}

impl ChildAnimation {
    pub fn new(offset: Vec2, facing: IVec2) -> Self {
        let angle = angle_of(facing);
        Self {
            local: MotionState::at(offset),
            facing,
            angle,
            swing_from: angle,
            swing_to: angle,
            elapsed: 0.0,
            duration: 0.0,
        }
    }

    /// Turns the heading toward `facing` over `duration` seconds, along the
    /// shorter arc. A half turn goes counterclockwise on screen.
    pub fn face(&mut self, facing: IVec2, duration: f32) {
        if facing == self.facing {
            return;
        }
        self.facing = facing;
        if facing == IVec2::ZERO {
            self.elapsed = self.duration;
            return;
        }
        let target = angle_of(facing);
        let mut delta = (target - self.angle).rem_euclid(std::f32::consts::TAU);
        if delta > std::f32::consts::PI + 1e-4 {
            delta -= std::f32::consts::TAU;
        }
        self.swing_from = self.angle;
        self.swing_to = self.angle + delta;
        self.elapsed = 0.0;
        self.duration = duration;
    }

    fn swinging(&self) -> bool {
        self.elapsed < self.duration
    }

    pub fn advance(&mut self, dt: f32) {
        self.local.advance(dt);
        if self.swinging() {
            self.elapsed += dt;
            let s = Easing::EaseInOut.apply(self.elapsed / self.duration);
            self.angle = self.swing_from + (self.swing_to - self.swing_from) * s;
        }
        if !self.swinging() {
            self.angle = angle_of(self.facing);
        }
    }

    /// Current offset from the parent's shown position.
    pub fn offset(&self) -> Vec2 {
        if self.swinging() {
            frame(self.angle, self.local.position)
        } else {
            framed_by(self.facing, self.local.position)
        }
    }
}

/// Children get their state when they are bound, so steady frames never
/// queue commands.
pub(super) fn start_child_animation(
    add: On<Add, BoundTo>,
    bound: Query<&BoundTo>,
    facings: Query<&Facing>,
    mut commands: Commands,
) {
    let Ok(bound) = bound.get(add.entity) else {
        return;
    };
    let facing = facings.get(bound.parent).map_or(IVec2::ZERO, |f| f.0);
    commands
        .entity(add.entity)
        .insert(ChildAnimation::new(bound.offset.as_vec2(), facing));
}

/// Places every child at its parent's shown position plus its animated
/// offset. Parents are placed before their children.
#[allow(clippy::type_complexity)]
pub fn animate_children(
    time: Res<Time>,
    style: Res<MotionStyle>,
    mut order: Local<Vec<(u8, Entity)>>,
    chain: Query<&BoundTo>,
    facings: Query<&Facing>,
    mut children: Query<(Entity, &BoundTo, Option<&ObjectMotion>, &mut ChildAnimation)>,
    mut shown: Query<&mut AnimatedPos>,
) {
    let dt = time.delta_secs();
    order.clear();
    for (entity, bound, ..) in &children {
        let mut depth = 0;
        let mut parent = bound.parent;
        while depth < MAX_DEPTH {
            let Ok(next) = chain.get(parent) else { break };
            parent = next.parent;
            depth += 1;
        }
        order.push((depth, entity));
    }
    order.sort_unstable();

    for &(_, entity) in order.iter() {
        let Ok((_, bound, own_style, mut animation)) = children.get_mut(entity) else {
            continue;
        };
        let Ok(parent) = shown.get(bound.parent).copied() else {
            continue;
        };
        let motion = own_style.map_or(*style, |own| own.0);
        let offset = bound.offset.as_vec2();
        if animation.local.target() != offset {
            animation.local.move_to(offset, motion);
        }
        let facing = facings.get(bound.parent).map_or(IVec2::ZERO, |f| f.0);
        animation.face(facing, motion.timing().0);
        animation.advance(dt);

        let next = AnimatedPos {
            position: parent.position + animation.offset(),
            lift: parent.lift + animation.local.lift,
        };
        if let Ok(mut shown) = shown.get_mut(entity) {
            if *shown != next {
                *shown = next;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / 60.0;
    const RIGHT: IVec2 = IVec2::X;
    const DOWN: IVec2 = IVec2::Y;
    const LEFT: IVec2 = IVec2::NEG_X;

    /// Offsets shown over a swing from `from` to `to` facing, for a child
    /// bound one cell ahead.
    fn swing(from: IVec2, to: IVec2) -> Vec<Vec2> {
        let mut child = ChildAnimation::new(Vec2::X, from);
        child.face(to, 0.12);
        (0..10)
            .map(|_| {
                child.advance(FRAME);
                child.offset()
            })
            .collect()
    }

    #[test]
    fn frames_match_the_simulation_rotation() {
        for facing in [RIGHT, DOWN, LEFT, IVec2::NEG_Y] {
            for offset in [IVec2::new(1, 0), IVec2::new(2, 1), IVec2::new(-1, 3)] {
                let exact = rotate(offset, facing).as_vec2();
                let swept = frame(angle_of(facing), offset.as_vec2());
                assert!((swept - exact).length() < 1e-5, "{facing} {offset}");
                assert_eq!(framed_by(facing, offset.as_vec2()), exact);
            }
        }
    }

    #[test]
    fn a_quarter_turn_sweeps_around_the_parent() {
        let path = swing(RIGHT, DOWN);
        for offset in &path {
            assert!(
                (offset.length() - 1.0).abs() < 1e-4,
                "left the circle: {offset}"
            );
        }
        // Shorter arc: through the diagonal between the two, not around.
        assert!(path.iter().any(|o| o.x > 0.5 && o.y > 0.5));
        assert_eq!(*path.last().unwrap(), rotate(IVec2::X, DOWN).as_vec2());
    }

    #[test]
    fn a_half_turn_goes_around_not_through_the_parent() {
        let path = swing(RIGHT, LEFT);
        for offset in &path {
            assert!(
                offset.length() > 0.99,
                "passed through the parent: {offset}"
            );
        }
        assert_eq!(*path.last().unwrap(), Vec2::NEG_X);
    }

    #[test]
    fn offset_changes_play_in_the_parent_frame() {
        let mut child = ChildAnimation::new(Vec2::X, RIGHT);
        let linear = MotionStyle::from_name("linear").unwrap();
        child.local.move_to(Vec2::new(2.0, 0.0), linear);
        child.advance(0.05);
        assert!((child.offset() - Vec2::new(1.5, 0.0)).length() < 1e-5);
        child.advance(0.05);
        assert_eq!(child.offset(), Vec2::new(2.0, 0.0));
    }

    #[test]
    fn no_facing_keeps_the_offset_as_is() {
        let mut child = ChildAnimation::new(Vec2::new(1.0, 1.0), IVec2::ZERO);
        child.advance(FRAME);
        assert_eq!(child.offset(), Vec2::new(1.0, 1.0));
    }
}
