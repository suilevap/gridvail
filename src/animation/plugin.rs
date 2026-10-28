use std::time::Duration;

use bevy::prelude::*;

use crate::model::*;
use crate::schedule::GamePhase;

use super::{animate_children, MotionState, MotionStyle, MovePath, ObjectMotion, Path, Tween};

/// How long an object takes to rise out of the floor or sink into it.
const RISE_SECONDS: f32 = 0.4;
/// How long a [`Sliding`] object takes per cell...
const SLIDE_SECONDS: f32 = 0.2;
/// ...and at most for a whole move, so long slides go faster.
const MAX_SLIDE_SECONDS: f32 = 0.8;

/// Marks an object whose animation never holds back the next turn, for
/// decorative or ambient motion. Children (`BoundTo`, such as the player's
/// direction marker) never hold turns either: they follow their parent,
/// which already does.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct NonBlockingAnimation;

/// Animation state of one positioned object.
#[derive(Component, Clone, Debug)]
pub struct ObjectAnimation {
    pub state: MotionState,
    /// How far it stands out of the floor (`AnimatedPos::height`).
    pub height: Tween<f32>,
}

impl ObjectAnimation {
    fn remaining(&self) -> f32 {
        self.state.remaining().max(self.height.remaining())
    }
}

/// Animates every positioned object (actors, walls, decor) between the
/// cells the simulation moves it through, places children relative to their
/// animated parents, raises [`Rising`] objects out of the floor and sinks
/// [`Sinking`] ones into it (despawning them once sunk), and holds the next
/// turn until those animations have finished.
pub struct ObjectAnimationPlugin;

impl Plugin for ObjectAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MotionStyle>()
            .add_observer(start_animation)
            .add_systems(
                Update,
                (
                    bump_blocked_movers,
                    animate_objects,
                    animate_children,
                    remove_sunk,
                )
                    .chain()
                    .in_set(GamePhase::Animation),
            );
    }
}

/// Objects get their animation state when they are first placed, so steady
/// frames never queue commands. A child's state lives in its parent's frame:
/// it animates its offset from the parent, not its cell. A [`Rising`]
/// object starts sunk into the floor at its `from` and rises there; it
/// moves to its cell once it stands.
#[allow(clippy::type_complexity)]
fn start_animation(
    add: On<Add, Pos>,
    style: Res<MotionStyle>,
    objects: Query<(
        &Pos,
        Option<&BoundTo>,
        Option<&ObjectMotion>,
        Option<&Rising>,
    )>,
    facings: Query<&Facing>,
    mut commands: Commands,
) {
    let Ok((pos, bound, own_style, rising)) = objects.get(add.entity) else {
        return;
    };
    let start = match (bound, rising) {
        (Some(bound), _) => rotate(bound.offset, facing_of(bound.parent, &facings)),
        (None, Some(rising)) => rising.from,
        (None, None) => pos.0,
    };
    let mut height = Tween::at(1.0);
    if rising.is_some() {
        height.snap(0.0);
        ease_height(&mut height, 1.0, style_of(own_style, *style));
    }
    let shown = AnimatedPos {
        height: height.value(),
        ..AnimatedPos::at(if bound.is_some() { pos.0 } else { start })
    };
    commands.entity(add.entity).insert((
        ObjectAnimation {
            state: MotionState::at(start.as_vec2()),
            height,
        },
        shown,
    ));
}

/// How long rising or sinking takes: [`RISE_SECONDS`], or no time when
/// objects snap.
fn rise_seconds(style: MotionStyle) -> f32 {
    if style == MotionStyle::Snap {
        0.0
    } else {
        RISE_SECONDS
    }
}

fn ease_height(height: &mut Tween<f32>, to: f32, style: MotionStyle) {
    height.ease_to(to, rise_seconds(style), EaseFunction::SmoothStep);
}

/// Despawns [`Sinking`] objects once they have sunk.
fn remove_sunk(mut commands: Commands, sinking: Query<(Entity, &ObjectAnimation), With<Sinking>>) {
    for (entity, animation) in &sinking {
        if animation.height.target() == 0.0 && animation.height.done() {
            commands.entity(entity).despawn();
        }
    }
}

pub(super) fn facing_of(entity: Entity, facings: &Query<&Facing>) -> IVec2 {
    facings.get(entity).map_or(IVec2::ZERO, |facing| facing.0)
}

/// The style an object moves in: its own if it has one, else the global one.
pub(super) fn style_of(own: Option<&ObjectMotion>, global: MotionStyle) -> MotionStyle {
    own.map_or(global, |own| own.0)
}

/// A [`Sliding`] object without a style of its own glides at a steady pace,
/// whatever the global style (only `snap` still snaps): [`SLIDE_SECONDS`]
/// a cell for a move of `cells`, but no more than [`MAX_SLIDE_SECONDS`] in
/// all.
fn slide_style(own: Option<&ObjectMotion>, global: MotionStyle, cells: f32) -> MotionStyle {
    match (own, global) {
        (Some(own), _) => own.0,
        (None, MotionStyle::Snap) => MotionStyle::Snap,
        (None, _) => MotionStyle::Tween {
            duration: SLIDE_SECONDS.min(MAX_SLIDE_SECONDS / cells.max(1.0)),
            easing: EaseFunction::SmoothStep,
        },
    }
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
            (Has<Sliding>, Has<Rising>, Has<Sinking>),
            &mut ObjectAnimation,
            &mut AnimatedPos,
        ),
        Without<BoundTo>,
    >,
) {
    let dt = time.delta_secs();
    let mut blocking = 0.0_f32;
    for (
        entity,
        pos,
        own_style,
        path,
        non_blocking,
        (sliding, rising, sinking),
        mut animation,
        mut shown,
    ) in &mut objects
    {
        // A step through a portal carries on from the exit: the motion so
        // far is moved there first, so the step starts at the exit face.
        if let Some(through) = crossings
            .as_ref()
            .and_then(|crossings| crossings.arrived(entity, pos.0))
        {
            animation.state.carry(&through);
        }
        let motion = if sliding {
            let cells = (pos.0.as_vec2() - animation.state.target()).length();
            slide_style(own_style, *style, cells)
        } else {
            style_of(own_style, *style)
        };
        // A rising object moves to its cell once it stands; a sinking one
        // sinks once it has got where it goes. Either waits for the other
        // step, which the next turn waits for too.
        let still_rising = rising && !animation.height.done();
        let mut to_come = 0.0;
        if still_rising {
            let cells = (pos.0.as_vec2() - animation.state.target()).length();
            to_come = cells * motion.timing().0;
        } else {
            follow(
                &mut animation.state,
                pos.0.as_vec2(),
                path.map(|path| path.0),
                motion,
                // A sliding wall stays in its room, so however far it goes
                // it never crossed the map's edge.
                |from, to| !sliding && wrapped(from, to, &grid),
                dt,
            );
        }
        if sinking && animation.height.target() == 1.0 {
            if animation.state.at_rest() {
                ease_height(&mut animation.height, 0.0, style_of(own_style, *style));
            } else {
                to_come = rise_seconds(style_of(own_style, *style));
            }
        }
        animation.height.advance(dt);
        if !non_blocking {
            blocking = blocking.max(animation.remaining() + to_come);
        }
        shown.set_if_neq(AnimatedPos {
            position: animation.state.position,
            lift: animation.state.lift,
            height: animation.height.value(),
        });
    }
    // The next turn waits until every blocking animation has (nearly)
    // finished, not just the player's: an object moved last turn arrives
    // before it can be moved again.
    pacing.report_animation(Duration::from_secs_f32(blocking));
}
