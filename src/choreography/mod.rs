//! Choreography: one exchange of blows played in order, at the right
//! moments, on top of a simulation that settles it all in one frame.
//!
//! The simulation stays instant and only says what happened
//! (`ContextAction`, `Strike`, `Hit`, `Collision`, `Died`). These systems
//! turn that into a timeline for the animation step, measured from the
//! frame it happened in:
//!
//! ```text
//! 0          attacker: swings as its weapon or move says (its own moves held)
//! CONTACT    the blow lands: the target's held moves go (a knock slides),
//!            or it shakes; what a knocked body ran into shakes
//! after      the dead, left behind as remains, finish their slide and sink
//! ```
//!
//! Nothing here changes the simulation; without an animation step it does
//! nothing.

use bevy::prelude::*;

use crate::animation::ObjectAnimation;
use crate::model::*;
use crate::schedule::{ActionStage, GamePhase};
use crate::simulation::destroy_entities;

/// How long an attacker's swing takes, in seconds...
pub const SWING_SECS: f32 = 0.3;
/// ...and so when its blow lands: at the middle of the swing.
pub const CONTACT: f32 = SWING_SECS / 2.0;
/// How long taking a blow shakes, in seconds, and how far, in cells.
const SHAKE_SECS: f32 = 0.3;
const SHAKE_REACH: f32 = 0.25;
/// How long after the contact an attacker's own move (a lunge's step
/// after its target, a swap) starts.
const FOLLOW_GAP: f32 = 0.06;

pub struct ChoreographyPlugin;

impl Plugin for ChoreographyPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Gesture>()
            .add_message::<HoldMoves>()
            .add_systems(
                Update,
                (swing_attackers, react_to_blows).in_set(GamePhase::Visibility),
            )
            // While the dead still have their place and motion.
            .add_systems(
                Update,
                leave_remains
                    .after(ActionStage::Damage)
                    .before(destroy_entities)
                    .in_set(GamePhase::Simulation),
            );
    }
}

/// The gesture of `swing` at a target in grid direction `at`; a sweep
/// swings toward `across`.
pub fn swing_gesture(swing: Swing, at: IVec2, across: IVec2) -> GestureShape {
    let at = at.as_vec2();
    match swing {
        Swing::Thrust { reach } => GestureShape::Thrust { toward: at * reach },
        Swing::Sweep { reach } => GestureShape::Sweep {
            toward: at * reach,
            across: across.as_vec2(),
        },
        Swing::Hook { reach } => GestureShape::Hook { toward: at * reach },
    }
}

/// An actor acting on what it picked swings at it as the move says (a
/// sideways move sweeps toward its side), its own moves held until just
/// after the blow lands; a weapon's wielder swings the way it strikes.
pub fn swing_attackers(
    mut actions: MessageReader<ContextAction>,
    mut strikes: MessageReader<Strike>,
    mut gestures: MessageWriter<Gesture>,
    mut holds: MessageWriter<HoldMoves>,
    maneuvers: Query<&Maneuvers>,
    weapons: Query<&Weapon>,
) {
    let swing = |entity: Entity, shape: GestureShape| Gesture {
        entity,
        shape,
        secs: SWING_SECS,
        delay: 0.0,
    };
    for action in actions.read() {
        let style = maneuvers.get(action.actor).map_or_else(
            |_| Swing::default(),
            |moves| moves.get(Relation::of(action.picked, action.dir)).swing,
        );
        let across = if action.dir.perp_dot(action.picked) == 0 {
            action.picked.perp()
        } else {
            action.dir
        };
        gestures.write(swing(
            action.actor,
            swing_gesture(style, action.picked, across),
        ));
        holds.write(HoldMoves {
            entity: action.actor,
            secs: CONTACT + FOLLOW_GAP,
        });
    }
    for strike in strikes.read() {
        let style = weapons
            .get(strike.wielder)
            .map_or_else(|_| Swing::default(), |weapon| weapon.swing);
        gestures.write(swing(
            strike.wielder,
            swing_gesture(style, strike.dir, strike.dir.perp()),
        ));
    }
}

/// What a blow lands on moves only once it lands: a knocked target slides
/// from the contact, one that stays shakes away from the blow. What a
/// knocked body runs into shakes at the contact too.
pub fn react_to_blows(
    mut hits: MessageReader<Hit>,
    mut collisions: MessageReader<Collision>,
    mut gestures: MessageWriter<Gesture>,
    mut holds: MessageWriter<HoldMoves>,
    positions: Query<&Pos>,
) {
    let shake = |entity: Entity, from: IVec2, at: IVec2| Gesture {
        entity,
        shape: GestureShape::Shake {
            toward: (at - from).clamp(IVec2::NEG_ONE, IVec2::ONE).as_vec2() * SHAKE_REACH,
        },
        secs: SHAKE_SECS,
        delay: CONTACT,
    };
    for hit in hits.read() {
        holds.write(HoldMoves {
            entity: hit.target,
            secs: CONTACT,
        });
        if let (Ok(from), Ok(at)) = (positions.get(hit.source), positions.get(hit.target)) {
            gestures.write(shake(hit.target, from.0, at.0));
        }
    }
    for collision in collisions.read() {
        let (Ok(from), Ok(at)) = (
            positions.get(collision.mover),
            positions.get(collision.obstacle),
        ) else {
            continue;
        };
        gestures.write(shake(collision.obstacle, from.0, at.0));
    }
}

/// What dies (and is removed) leaves its remains to be shown: its glyph,
/// carrying on the motion it had, held until the blow lands, then sinking
/// into the floor. Only shown objects (with an animation) leave remains.
#[allow(clippy::type_complexity)]
pub fn leave_remains(
    mut commands: Commands,
    mut died: MessageReader<Died>,
    bodies: Query<(&Pos, &Glyph, &ObjectAnimation, &AnimatedPos), Without<KeepOnDeath>>,
) {
    for &Died(body) in died.read() {
        let Ok((pos, glyph, animation, shown)) = bodies.get(body) else {
            continue;
        };
        commands.spawn((
            Remains,
            Sinking,
            *pos,
            *glyph,
            animation.clone(),
            *shown,
            crate::animation::HeldMoves(CONTACT),
        ));
    }
}

/// What is shown of something that died, until it has sunk.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Remains;
