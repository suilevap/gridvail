//! Touch controls: walking by swiping, with no on-screen buttons. Independent
//! of keyboard input; both write the player's `MoveCommand`.

#![allow(clippy::type_complexity)]

use bevy::prelude::*;

use crate::model::*;
use crate::schedule::GamePhase;
use crate::simulation::{move_commands, player_input};

#[cfg(test)]
mod tests;

/// Logical pixels a finger travels before a touch counts as a swipe.
const SWIPE_THRESHOLD: f32 = 24.0;
/// Seconds a finger stays down after a swipe before it keeps walking, so a
/// quick swipe is one step.
const SWIPE_HOLD_SECS: f32 = 0.25;

pub struct TouchControlPlugin;

impl Plugin for TouchControlPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Swipe>()
            .init_resource::<ControlScheme>()
            .add_systems(
                PreUpdate,
                read_swipes
                    .after(bevy::input::InputSystems)
                    .run_if(resource_exists::<Touches>),
            )
            .add_systems(
                Update,
                swipe_commands
                    .after(player_input)
                    .before(move_commands)
                    .in_set(GamePhase::Simulation),
            );
    }
}

/// A swipe steps once toward its dominant axis, and keeping the finger down
/// a moment longer keeps walking, like a held key. The finger works as a
/// floating stick around where it landed, so sliding it to the other side
/// turns.
#[derive(Resource, Default, Debug)]
pub struct Swipe {
    /// The tracked finger and where it landed.
    origin: Option<(u64, Vec2)>,
    /// The direction the finger points while it stays down, and seconds
    /// since it started pointing there.
    held: Option<(IVec2, f32)>,
    /// A direction swiped but not walked yet, so a swipe released before the
    /// player has a token still steps once.
    pending: Option<IVec2>,
}

impl Swipe {
    fn direction(&self) -> Option<IVec2> {
        self.pending.or(self
            .held
            .and_then(|(dir, secs)| (secs >= SWIPE_HOLD_SECS).then_some(dir)))
    }
}

pub fn read_swipes(
    time: Res<Time>,
    touches: Res<Touches>,
    mut swipe: ResMut<Swipe>,
    mut scheme: ResMut<ControlScheme>,
) {
    if touches.any_just_pressed() {
        scheme.set_if_neq(ControlScheme::Touch);
    }
    if swipe.origin.is_none() {
        swipe.origin = touches
            .iter_just_pressed()
            .next()
            .map(|touch| (touch.id(), touch.position()));
    }
    let Some((id, origin)) = swipe.origin else {
        return;
    };
    let Some(touch) = touches.get_pressed(id) else {
        swipe.origin = None;
        swipe.held = None;
        return;
    };
    let delta = touch.position() - origin;
    if delta.length() < SWIPE_THRESHOLD {
        return;
    }
    // Window coordinates grow downward, like grid rows.
    let dir = if delta.x.abs() > delta.y.abs() {
        IVec2::new(delta.x.signum() as i32, 0)
    } else {
        IVec2::new(0, delta.y.signum() as i32)
    };
    match &mut swipe.held {
        Some((held, secs)) if *held == dir => *secs += time.delta_secs(),
        _ => {
            swipe.held = Some((dir, 0.0));
            swipe.pending = Some(dir);
        }
    }
}

/// Walks the player the way the finger swiped. Swipes are directions on
/// screen, like keys: when the view is turned, "up" walks toward the top of
/// the screen.
pub fn swipe_commands(
    mut swipe: ResMut<Swipe>,
    turn: Res<TurnState>,
    camera: Option<Res<ViewCamera>>,
    mut players: Query<
        (&mut MoveCommand, &Tokens),
        (With<Player>, With<Active>, Without<DestroyRequested>),
    >,
) {
    if turn.simulation {
        return;
    }
    let Some(screen) = swipe.direction() else {
        return;
    };
    let target = camera.map_or(screen, |camera| camera.map_direction(screen));
    for (mut command, tokens) in players.iter_mut() {
        // A key pressed this frame wins.
        if tokens.count <= 0 || command.active {
            continue;
        }
        command.target = target;
        command.relative = true;
        command.active = true;
        swipe.pending = None;
    }
}
