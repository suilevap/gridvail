use std::time::Duration;

use bevy::prelude::*;

use crate::model::*;
use crate::schedule::GamePhase;

use super::{
    animate_children, start_child_animation, MotionState, MotionStyle, MovePath, ObjectMotion,
};

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
            .add_observer(start_child_animation)
            .add_systems(
                Update,
                (bump_blocked_movers, animate_objects, animate_children)
                    .chain()
                    .in_set(GamePhase::Animation),
            );
    }
}

/// Objects get their animation state when they are first placed, so steady
/// frames never queue commands.
fn start_animation(add: On<Add, Pos>, objects: Query<&Pos>, mut commands: Commands) {
    let Ok(pos) = objects.get(add.entity) else {
        return;
    };
    commands.entity(add.entity).insert((
        ObjectAnimation {
            state: MotionState::at(pos.0.as_vec2()),
        },
        AnimatedPos::at(pos.0),
    ));
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
        let motion = own_style.map_or(*style, |own| own.0);
        let toward = wrapped_delta(source.0, target.0, &grid).as_vec2();
        animation.state.bump(toward, motion);
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
    mut pacing: ResMut<TurnPacing>,
    // Children are placed relative to their parent by `animate_children`.
    mut objects: Query<
        (
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
    for (pos, own_style, path, non_blocking, mut animation, mut shown) in &mut objects {
        let motion = own_style.map_or(*style, |own| own.0);
        let target = pos.0.as_vec2();
        let from = animation.state.target();
        if from != target {
            if wrapped(from, target, &grid) {
                animation.state = MotionState::at(target);
            } else {
                animation
                    .state
                    .move_along(target, path.map(|path| path.0), motion);
            }
        }
        animation.state.advance(dt);
        if !non_blocking {
            blocking = blocking.max(animation.state.remaining());
        }

        let next = AnimatedPos {
            position: animation.state.position,
            lift: animation.state.lift,
        };
        if *shown != next {
            *shown = next;
        }
    }
    // The next turn waits until every blocking animation has (nearly)
    // finished, not just the player's: an object moved last turn arrives
    // before it can be moved again.
    pacing.report_animation(Duration::from_secs_f32(blocking));
}
