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

use bevy::platform::collections::HashSet;

use crate::animation::{Flinging, ObjectAnimation, SWING_CONTACT, SWING_WOUND};
use crate::model::*;
use crate::schedule::{ActionStage, GamePhase};
use crate::simulation::destroy_entities;

/// How long an attacker's swing takes, in seconds, from the wind-up to
/// settling back...
pub const SWING_SECS: f32 = 0.42;
/// ...and so when its blow lands.
pub const CONTACT: f32 = SWING_SECS * SWING_CONTACT;
/// How long a knocked body's slide takes, a cell: flung fast, slowing to
/// a stop.
const FLING_SECS: f32 = 0.22;
/// How long taking a blow shakes, in seconds, and how far, in cells.
const SHAKE_SECS: f32 = 0.3;
const SHAKE_REACH: f32 = 0.25;
/// How long after the contact an attacker's own move (a lunge's step
/// after its target, a swap) starts.
const FOLLOW_GAP: f32 = 0.06;
/// How long an aimed weapon's wielder takes to wind up (it then holds until
/// it strikes); the strike plays the rest of the swing, and its blow lands
/// sooner than a whole swing's.
const AIM_SECS: f32 = 0.5;
const RELEASE_SECS: f32 = SWING_SECS * (1.0 - SWING_WOUND);
pub const RELEASE_CONTACT: f32 = SWING_SECS * (SWING_CONTACT - SWING_WOUND);
/// How long the player takes to ready a swing at what they picked, and how
/// far (a thrust's reach: it draws back a share of that).
const PICK_SECS: f32 = 0.2;
const PICK_REACH: f32 = 0.4;
/// How long dropping a wind-up takes to ease back.
const UNWIND_SECS: f32 = 0.2;

pub struct ChoreographyPlugin;

impl Plugin for ChoreographyPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Gesture>()
            .add_message::<HoldMoves>()
            .add_message::<Fling>()
            .add_message::<WindUp>()
            .add_message::<Release>()
            .add_systems(
                Update,
                (wind_up, swing_attackers, react_to_blows)
                    .chain()
                    .in_set(GamePhase::Visibility),
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

/// Anticipation lasts as long as what it anticipates: a weapon's wielder
/// winds up its swing when it aims and holds it until the strike; the
/// player readies a thrust at what they picked while it stays picked.
/// Either eases back if dropped (the strike and the action play their own
/// swing, from there).
#[allow(clippy::type_complexity)]
pub fn wind_up(
    mut wind_ups: MessageWriter<WindUp>,
    mut releases: MessageWriter<Release>,
    aimed: Query<(Entity, &Weapon, &Telegraph), Changed<Telegraph>>,
    picks: Query<(Entity, &Selected), Changed<Selected>>,
    mut dropped_aims: RemovedComponents<Telegraph>,
    mut dropped_picks: RemovedComponents<Selected>,
    mut strikes: MessageReader<Strike>,
) {
    for (wielder, weapon, telegraph) in &aimed {
        wind_ups.write(WindUp {
            entity: wielder,
            shape: swing_gesture(weapon.swing, telegraph.dir, telegraph.dir.perp()),
            secs: AIM_SECS,
        });
    }
    for (picker, selected) in &picks {
        wind_ups.write(WindUp {
            entity: picker,
            shape: GestureShape::Thrust {
                toward: selected.dir.as_vec2() * PICK_REACH,
            },
            secs: PICK_SECS,
        });
    }
    let struck: HashSet<Entity> = strikes.read().map(|strike| strike.wielder).collect();
    for entity in dropped_aims
        .read()
        .filter(|entity| !struck.contains(entity))
        .chain(dropped_picks.read())
    {
        releases.write(Release {
            entity,
            secs: UNWIND_SECS,
            strike: false,
        });
    }
}

/// An actor acting on what it picked swings at it as the move says (a
/// sideways move sweeps toward its side), its own moves held until just
/// after the blow lands; a weapon's wielder swings the way it strikes,
/// releasing the swing it wound up when it aimed.
pub fn swing_attackers(
    mut actions: MessageReader<ContextAction>,
    mut strikes: MessageReader<Strike>,
    mut gestures: MessageWriter<Gesture>,
    mut holds: MessageWriter<HoldMoves>,
    mut releases: MessageWriter<Release>,
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
        let Ok(weapon) = weapons.get(strike.wielder) else {
            continue;
        };
        if weapon.windup > 0 {
            releases.write(Release {
                entity: strike.wielder,
                secs: RELEASE_SECS,
                strike: true,
            });
            continue;
        }
        let style = weapon.swing;
        gestures.write(swing(
            strike.wielder,
            swing_gesture(style, strike.dir, strike.dir.perp()),
        ));
    }
}

/// What a blow lands on moves only once it lands: a knocked target is
/// flung from the contact, one that stays shakes away from the blow. What a
/// knocked body runs into shakes at the contact too.
pub fn react_to_blows(
    mut hits: MessageReader<Hit>,
    mut collisions: MessageReader<Collision>,
    (mut gestures, mut holds, mut flings): (
        MessageWriter<Gesture>,
        MessageWriter<HoldMoves>,
        MessageWriter<Fling>,
    ),
    mut strikes: MessageReader<Strike>,
    positions: Query<&Pos>,
    weapons: Query<&Weapon>,
) {
    // A wound-up swing, released, lands sooner than a whole one.
    let released: HashSet<Entity> = strikes
        .read()
        .map(|strike| strike.wielder)
        .filter(|wielder| weapons.get(*wielder).is_ok_and(|weapon| weapon.windup > 0))
        .collect();
    let contact = |source: Entity| {
        if released.contains(&source) {
            RELEASE_CONTACT
        } else {
            CONTACT
        }
    };
    let shake = |entity: Entity, from: IVec2, at: IVec2, delay: f32| Gesture {
        entity,
        shape: GestureShape::Shake {
            toward: (at - from).clamp(IVec2::NEG_ONE, IVec2::ONE).as_vec2() * SHAKE_REACH,
        },
        secs: SHAKE_SECS,
        delay,
    };
    for hit in hits.read() {
        let delay = contact(hit.source);
        holds.write(HoldMoves {
            entity: hit.target,
            secs: delay,
        });
        flings.write(Fling {
            entity: hit.target,
            secs: FLING_SECS,
        });
        if let (Ok(from), Ok(at)) = (positions.get(hit.source), positions.get(hit.target)) {
            gestures.write(shake(hit.target, from.0, at.0, delay));
        }
    }
    for collision in collisions.read() {
        let (Ok(from), Ok(at)) = (
            positions.get(collision.mover),
            positions.get(collision.obstacle),
        ) else {
            continue;
        };
        gestures.write(shake(collision.obstacle, from.0, at.0, CONTACT));
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
            Flinging::new(FLING_SECS),
        ));
    }
}

/// What is shown of something that died, until it has sunk.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Remains;
