use std::time::Duration;

use bevy::prelude::*;

use crate::model::*;
use crate::schedule::GamePhase;

use super::{animate_children, MotionState, MotionStyle, MovePath, ObjectMotion, Path};

/// Marks an object whose animation never holds back the next turn, for
/// decorative or ambient motion. Children (`BoundTo`, such as the player's
/// direction marker) never hold turns either: they follow their parent,
/// which already does.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct NonBlockingAnimation;

/// Animation state of one positioned object.
#[derive(Component, Clone, Copy, Debug)]
pub struct ObjectAnimation {
    pub state: MotionState,
}

/// Animates every positioned object (actors, walls, decor) between the
/// cells the simulation moves it through, places children relative to their
/// animated parents, and holds the next turn until those animations have
/// finished.
pub struct ObjectAnimationPlugin;

impl Plugin for ObjectAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MotionStyle>()
            .add_observer(start_animation)
            .add_systems(
                Update,
                (bump_blocked_movers, animate_objects, animate_children)
                    .chain()
                    .in_set(GamePhase::Animation),
            );
    }
}

/// Objects get their animation state when they are first placed, so steady
/// frames never queue commands. A child's state lives in its parent's frame:
/// it animates its offset from the parent, not its cell.
fn start_animation(
    add: On<Add, Pos>,
    objects: Query<(&Pos, Option<&BoundTo>)>,
    facings: Query<&Facing>,
    mut commands: Commands,
) {
    let Ok((pos, bound)) = objects.get(add.entity) else {
        return;
    };
    let start = match bound {
        Some(bound) => rotate(bound.offset, facing_of(bound.parent, &facings)),
        None => pos.0,
    };
    commands.entity(add.entity).insert((
        ObjectAnimation {
            state: MotionState::at(start.as_vec2()),
        },
        AnimatedPos::at(pos.0),
    ));
}

pub(super) fn facing_of(entity: Entity, facings: &Query<&Facing>) -> IVec2 {
    facings.get(entity).map_or(IVec2::ZERO, |facing| facing.0)
}

/// The style an object moves in: its own if it has one, else the global one.
pub(super) fn style_of(own: Option<&ObjectMotion>, global: MotionStyle) -> MotionStyle {
    own.map_or(global, |own| own.0)
}

/// One frame of an object's animation, shared by top-level objects (in map
/// cells) and children (in their parent's frame): a changed target starts a
/// move along `path`, or a jump when `jumps(from, to)`, and the animation
/// advances by `dt`.
pub(super) fn follow(
    state: &mut MotionState,
    target: Vec2,
    path: Option<Path>,
    motion: MotionStyle,
    jumps: impl Fn(Vec2, Vec2) -> bool,
    dt: f32,
) {
    let from = state.target();
    if from != target {
        if jumps(from, target) {
            *state = MotionState::at(target);
        } else {
            state.move_along(target, path, motion);
        }
    }
    state.advance(dt);
}

/// Shortest step from `from` to `to` on the wrapping map.
fn wrapped_delta(from: IVec2, to: IVec2, grid: &MapGrid) -> IVec2 {
    let wrap = |delta: i32, size: i32| {
        if delta.abs() * 2 > size {
            delta - delta.signum() * size
        } else {
            delta
        }
    };
    let delta = to - from;
    IVec2::new(wrap(delta.x, grid.width), wrap(delta.y, grid.height))
}

/// A move the simulation blocked (walking into a wall or another actor) is
/// shown as a bump toward what blocked it: a move out and back to the same
/// cell, lasting one step.
pub fn bump_blocked_movers(
    style: Res<MotionStyle>,
    grid: Res<MapGrid>,
    collisions: Res<CollisionBuffer>,
    positions: Query<&Pos>,
    facings: Query<&Facing>,
    mut objects: Query<(Option<&ObjectMotion>, &mut ObjectAnimation)>,
) {
    for collision in &collisions.0 {
        let (Ok(source), Ok(target)) = (
            positions.get(collision.source),
            positions.get(collision.target),
        ) else {
            continue;
        };
        let Ok((own_style, mut animation)) = objects.get_mut(collision.source) else {
            continue;
        };
        let motion = style_of(own_style, *style);
        let mut toward = wrapped_delta(source.0, target.0, &grid);
        // Blocked at a portal's exit, what blocked it is far away: bump
        // the way it was stepping.
        if toward.length_squared() != 1 {
            toward = facing_of(collision.source, &facings);
        }
        animation.state.bump(toward.as_vec2(), motion);
    }
}

/// Whether moving from `from` to `to` crossed the map edge: the map wraps,
/// so that move is a teleport, not a glide across the whole map.
fn wrapped(from: Vec2, to: Vec2, grid: &MapGrid) -> bool {
    let delta = (to - from).abs();
    delta.x > grid.width as f32 / 2.0 || delta.y > grid.height as f32 / 2.0
}

#[allow(clippy::type_complexity)]
pub fn animate_objects(
    time: Res<Time>,
    style: Res<MotionStyle>,
    grid: Res<MapGrid>,
    crossings: Option<Res<PortalCrossings>>,
    mut pacing: ResMut<TurnPacing>,
    // Children are placed relative to their parent by `animate_children`.
    mut objects: Query<
        (
            Entity,
            &Pos,
            Option<&ObjectMotion>,
            Option<&MovePath>,
            Has<NonBlockingAnimation>,
            &mut ObjectAnimation,
            &mut AnimatedPos,
        ),
        Without<BoundTo>,
    >,
) {
    let dt = time.delta_secs();
    let mut blocking = 0.0_f32;
    for (entity, pos, own_style, path, non_blocking, mut animation, mut shown) in &mut objects {
        // A step through a portal carries on from the exit: the motion so
        // far is moved there first, so the step starts at the exit face.
        if let Some(through) = crossings
            .as_ref()
            .and_then(|crossings| crossings.arrived(entity, pos.0))
        {
            animation.state.carry(&through);
        }
        follow(
            &mut animation.state,
            pos.0.as_vec2(),
            path.map(|path| path.0),
            style_of(own_style, *style),
            |from, to| wrapped(from, to, &grid),
            dt,
        );
        if !non_blocking {
            blocking = blocking.max(animation.state.remaining());
        }
        shown.set_if_neq(AnimatedPos {
            position: animation.state.position,
            lift: animation.state.lift,
        });
    }
    // The next turn waits until every blocking animation has (nearly)
    // finished, not just the player's: an object moved last turn arrives
    // before it can be moved again.
    pacing.report_animation(Duration::from_secs_f32(blocking));
}
