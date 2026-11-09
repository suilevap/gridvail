//! One-finger swipes walk the player; a tap cancels a readied move.

use bevy::prelude::*;

use crate::model::*;

/// Logical pixels a finger travels before a touch counts as a swipe.
const SWIPE_THRESHOLD: f32 = 24.0;
/// Seconds a finger stays down after a swipe before it keeps walking, so a
/// quick swipe is one step.
const SWIPE_HOLD_SECS: f32 = 0.25;

/// A swipe steps once toward its dominant axis, and keeping the finger down
/// a moment longer keeps walking, like a held key. Where the finger points
/// is the way it last moved (from where it last turned or went on), not
/// where it is from where it landed: sliding it sideways turns it sideways
/// from wherever it is, without coming back first (an attack aimed ahead
/// turns to one aside).
#[derive(Resource, Default, Debug)]
pub struct Swipe {
    /// The tracked finger, and where its way is measured from: where it
    /// landed, then where it last moved far enough to point.
    origin: Option<(u64, Vec2)>,
    /// The direction the finger points while it stays down, and seconds
    /// since it started pointing there.
    held: Option<(IVec2, f32)>,
    /// A direction swiped but not walked yet, so a swipe released before the
    /// player has a token still steps once.
    pending: Option<IVec2>,
    /// A second finger landed: two-finger gestures are not swipes, so
    /// nothing walks until every finger lifts.
    cancelled: bool,
    /// The tracked finger travelled far enough to swipe: lifting it is not
    /// a tap.
    swiped: bool,
    /// A finger lifted without swiping, not yet answered.
    tapped: bool,
}

impl Swipe {
    fn direction(&self) -> Option<IVec2> {
        self.pending.or(self
            .held
            .and_then(|(dir, secs)| (secs >= SWIPE_HOLD_SECS).then_some(dir)))
    }
}

/// A finger lifted without swiping is a tap (`tap_cancels`).
pub fn read_swipes(
    time: Res<Time>,
    touches: Res<Touches>,
    mut swipe: ResMut<Swipe>,
    mut scheme: ResMut<ControlScheme>,
) {
    if touches.any_just_pressed() {
        scheme.set_if_neq(ControlScheme::Touch);
    }
    if touches.iter().nth(1).is_some() {
        *swipe = Swipe {
            cancelled: true,
            ..default()
        };
        return;
    }
    if swipe.cancelled {
        swipe.cancelled = touches.iter().next().is_some();
        return;
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
        swipe.tapped |= !swipe.swiped;
        swipe.origin = None;
        swipe.held = None;
        swipe.swiped = false;
        return;
    };
    // Kept down, it keeps pointing where it did, held a little longer.
    if let Some((_, secs)) = &mut swipe.held {
        *secs += time.delta_secs();
    }
    let delta = touch.position() - origin;
    if delta.length() < SWIPE_THRESHOLD {
        return;
    }
    swipe.swiped = true;
    swipe.origin = Some((id, touch.position()));
    // Window coordinates grow downward, like grid rows.
    let dir = if delta.x.abs() > delta.y.abs() {
        IVec2::new(delta.x.signum() as i32, 0)
    } else {
        IVec2::new(0, delta.y.signum() as i32)
    };
    if swipe.held.is_none_or(|(held, _)| held != dir) {
        swipe.held = Some((dir, 0.0));
        swipe.pending = Some(dir);
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
        // A swipe is fresh; a finger kept down walking on is not.
        command.repeat = swipe.pending.is_none();
        command.at_once = false;
        swipe.pending = None;
    }
}

/// A tap cancels the player's readied move (`Cancel`).
pub fn tap_cancels(
    mut swipe: ResMut<Swipe>,
    mut cancels: MessageWriter<Cancel>,
    players: Query<Entity, (With<Player>, With<Active>, Without<DestroyRequested>)>,
) {
    if !std::mem::take(&mut swipe.tapped) {
        return;
    }
    for player in &players {
        cancels.write(Cancel(player));
    }
}

/// Where a finger kept down points is what the player holds
/// (`MoveIntent`), as a held key is: an attack aimed by a swipe is made
/// when the finger lifts, the way it then points.
pub fn swipe_intent(
    swipe: Res<Swipe>,
    camera: Option<Res<ViewCamera>>,
    mut players: Query<&mut MoveIntent, (With<Player>, With<Active>, Without<DestroyRequested>)>,
) {
    if swipe.origin.is_none() {
        return;
    }
    let screen = swipe.held.map(|(dir, _)| dir);
    for mut intent in &mut players {
        intent.hold(screen, |screen| {
            camera
                .as_ref()
                .map_or(screen, |camera| camera.map_direction(screen))
        });
    }
}
