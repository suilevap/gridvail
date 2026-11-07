//! Choreography: one exchange of blows played in order, at the right
//! moments, on top of a simulation that settles it all in one frame.
//!
//! The simulation stays instant and only says what happened
//! (`ContextAction`, `Strike`, `Hit`, `Collision`, `Died`). These systems
//! turn that into a timeline for the animation step and effects, measured
//! from the frame it happened in:
//!
//! ```text
//! before     an aimed weapon winds up and holds; a picker draws back
//! 0          the attacker swings as its look says (its own moves held),
//!            leaving its streak
//! contact    the blow lands (`BlowLands`): the target's held moves go (a
//!            knock is flung), it shakes and bleeds; what a knocked body
//!            ran into shakes, and the body bleeds
//! after      the dead, left behind as remains, finish their slide and sink
//! ```
//!
//! Each system does one of those things. The timings are data
//! (`assets/rules/choreography.ron`). Nothing here changes the simulation;
//! without an animation step it does nothing.

use bevy::platform::collections::HashSet;
use bevy::prelude::*;

use crate::animation::{Flinging, ObjectAnimation, SWING_CONTACT, SWING_WOUND};
use crate::content::choreography::ChoreographySpec;
use crate::content::data;
use crate::model::*;
use crate::schedule::{ActionStage, GamePhase};

const TIMING_TEXT: &str = include_str!("../../assets/rules/choreography.ron");

pub struct ChoreographyPlugin;

impl Plugin for ChoreographyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Timing>()
            .add_message::<Gesture>()
            .add_message::<HoldMoves>()
            .add_message::<Fling>()
            .add_message::<WindUp>()
            .add_message::<Release>()
            .add_message::<PlayEffect>()
            .add_message::<BlowLands>()
            .add_systems(
                Update,
                (
                    (
                        wind_up_aims,
                        wind_up_picks,
                        unwind_dropped_aims,
                        unwind_dropped_picks,
                    ),
                    (
                        swing_maneuvers,
                        hold_attackers,
                        streak_maneuvers,
                        swing_strikes,
                        streak_strikes,
                    ),
                    time_blows,
                    (
                        hold_struck,
                        shake_struck,
                        bleed_struck,
                        shake_obstacles,
                        bleed_impacts,
                    ),
                )
                    .chain()
                    .in_set(GamePhase::Visibility),
            )
            .add_systems(
                Update,
                leave_remains
                    .in_set(ActionStage::Aftermath)
                    .in_set(GamePhase::Simulation),
            );
    }
}

/// How fights are timed on screen: the bundled timings by default; change
/// it (or insert another) to retime them.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Timing(pub ChoreographySpec);

impl Default for Timing {
    fn default() -> Self {
        data::read("choreography.ron", TIMING_TEXT)
            .map(Self)
            .unwrap_or_else(|error| panic!("{error}"))
    }
}

impl Timing {
    /// When a whole swing's blow lands.
    pub fn contact(&self) -> f32 {
        self.0.swing_secs * SWING_CONTACT
    }

    /// How long the rest of a wound-up swing takes once released, and when
    /// its blow lands.
    pub fn release_secs(&self) -> f32 {
        self.0.swing_secs * (1.0 - SWING_WOUND)
    }

    pub fn release_contact(&self) -> f32 {
        self.0.swing_secs * (SWING_CONTACT - SWING_WOUND)
    }

    /// When a whole swing's strike starts, and how long its streak takes.
    pub fn strike_start(&self) -> f32 {
        self.0.swing_secs * SWING_WOUND
    }

    pub fn streak_secs(&self) -> f32 {
        self.release_contact() * self.0.streak_stretch
    }
}

/// A blow lands, `contact` seconds from now, on what shows `target` (its
/// remains, when it `killed`), driven `toward`, with the hit's `damage`.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub struct BlowLands {
    pub target: Entity,
    pub toward: Vec2,
    pub contact: f32,
    pub damage: i32,
    pub killed: bool,
}

/// What is shown of something that died (the body it was), until it has
/// sunk.
#[derive(Component, Clone, Copy, Debug)]
pub struct Remains(pub Entity);

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

/// How an actor's move on a picked target looks, and its gesture: a
/// sideways move sweeps toward its side.
pub fn maneuver_swing(looks: &ManeuverLooks, action: &ContextAction) -> (SwingLook, GestureShape) {
    let look = looks.get(Relation::of(action.picked, action.dir)).clone();
    let across = if action.dir.perp_dot(action.picked) == 0 {
        action.picked.perp()
    } else {
        action.dir
    };
    let shape = swing_gesture(look.swing, action.picked, across);
    (look, shape)
}

/// A weapon's gesture as it strikes toward `dir`.
pub fn weapon_swing(look: &WeaponLook, dir: IVec2) -> GestureShape {
    swing_gesture(look.0.swing, dir, dir.perp())
}

/// The streak `look` leaves along `shape`, from `delay`, if it names one.
fn streak(
    look: &SwingLook,
    entity: Entity,
    dir: IVec2,
    shape: GestureShape,
    delay: f32,
    timing: &Timing,
) -> Option<PlayEffect> {
    look.streak.as_ref().map(|effect| PlayEffect {
        effect: effect.clone(),
        entity,
        toward: dir.as_vec2(),
        swing: Some(shape),
        secs: timing.streak_secs(),
        strength: 0,
        delay,
    })
}

/// A weapon's wielder winds up its swing when it aims, and holds it until
/// the strike: its anticipation is its telegraph.
pub fn wind_up_aims(
    timing: Res<Timing>,
    mut wind_ups: MessageWriter<WindUp>,
    aimed: Query<(Entity, &WeaponLook, &Telegraph), Changed<Telegraph>>,
) {
    for (wielder, look, telegraph) in &aimed {
        wind_ups.write(WindUp {
            entity: wielder,
            shape: weapon_swing(look, telegraph.dir),
            secs: timing.0.aim_secs,
        });
    }
}

/// A picker draws back at what it picked, and holds it while it stays
/// picked.
pub fn wind_up_picks(
    timing: Res<Timing>,
    mut wind_ups: MessageWriter<WindUp>,
    picks: Query<(Entity, &Selected), Changed<Selected>>,
) {
    for (picker, selected) in &picks {
        wind_ups.write(WindUp {
            entity: picker,
            shape: GestureShape::Thrust {
                toward: selected.dir.as_vec2() * timing.0.pick_reach,
            },
            secs: timing.0.pick_secs,
        });
    }
}

/// An aim dropped without striking eases back.
pub fn unwind_dropped_aims(
    timing: Res<Timing>,
    mut releases: MessageWriter<Release>,
    mut dropped: RemovedComponents<Telegraph>,
    mut strikes: MessageReader<Strike>,
) {
    let struck: HashSet<Entity> = strikes.read().map(|strike| strike.wielder).collect();
    for entity in dropped.read().filter(|entity| !struck.contains(entity)) {
        releases.write(Release {
            entity,
            secs: timing.0.unwind_secs,
            strike: false,
        });
    }
}

/// A dropped pick eases back (acting on it swings from there instead).
pub fn unwind_dropped_picks(
    timing: Res<Timing>,
    mut releases: MessageWriter<Release>,
    mut dropped: RemovedComponents<Selected>,
) {
    for entity in dropped.read() {
        releases.write(Release {
            entity,
            secs: timing.0.unwind_secs,
            strike: false,
        });
    }
}

/// An actor acting on what it picked swings at it as its move looks.
pub fn swing_maneuvers(
    timing: Res<Timing>,
    mut actions: MessageReader<ContextAction>,
    mut gestures: MessageWriter<Gesture>,
    looks: Query<&ManeuverLooks>,
) {
    for action in actions.read() {
        let Ok(looks) = looks.get(action.actor) else {
            continue;
        };
        let (_, shape) = maneuver_swing(looks, action);
        gestures.write(Gesture {
            entity: action.actor,
            shape,
            secs: timing.0.swing_secs,
            delay: 0.0,
        });
    }
}

/// An actor acting on what it picked keeps its own moves (a lunge's step, a
/// swap) until just after its blow lands.
pub fn hold_attackers(
    timing: Res<Timing>,
    mut actions: MessageReader<ContextAction>,
    mut holds: MessageWriter<HoldMoves>,
) {
    for action in actions.read() {
        holds.write(HoldMoves {
            entity: action.actor,
            secs: timing.contact() + timing.0.follow_gap,
        });
    }
}

/// A move leaves the streak its look names while it strikes.
pub fn streak_maneuvers(
    timing: Res<Timing>,
    mut actions: MessageReader<ContextAction>,
    mut effects: MessageWriter<PlayEffect>,
    looks: Query<&ManeuverLooks>,
) {
    for action in actions.read() {
        let Ok(looks) = looks.get(action.actor) else {
            continue;
        };
        let (look, shape) = maneuver_swing(looks, action);
        let start = timing.strike_start();
        effects.write_batch(streak(
            &look,
            action.actor,
            action.picked,
            shape,
            start,
            &timing,
        ));
    }
}

/// A weapon's wielder releases the swing it wound up when it aimed, or, not
/// wound up, swings whole.
pub fn swing_strikes(
    timing: Res<Timing>,
    mut strikes: MessageReader<Strike>,
    mut releases: MessageWriter<Release>,
    mut gestures: MessageWriter<Gesture>,
    weapons: Query<(&Weapon, &WeaponLook)>,
) {
    for strike in strikes.read() {
        let Ok((weapon, look)) = weapons.get(strike.wielder) else {
            continue;
        };
        if weapon.windup > 0 {
            releases.write(Release {
                entity: strike.wielder,
                secs: timing.release_secs(),
                strike: true,
            });
        } else {
            gestures.write(Gesture {
                entity: strike.wielder,
                shape: weapon_swing(look, strike.dir),
                secs: timing.0.swing_secs,
                delay: 0.0,
            });
        }
    }
}

/// A weapon leaves the streak its look names while it strikes.
pub fn streak_strikes(
    timing: Res<Timing>,
    mut strikes: MessageReader<Strike>,
    mut effects: MessageWriter<PlayEffect>,
    weapons: Query<(&Weapon, &WeaponLook)>,
) {
    for strike in strikes.read() {
        let Ok((weapon, look)) = weapons.get(strike.wielder) else {
            continue;
        };
        let start = if weapon.windup > 0 {
            0.0
        } else {
            timing.strike_start()
        };
        let shape = weapon_swing(look, strike.dir);
        effects.write_batch(streak(
            &look.0,
            strike.wielder,
            strike.dir,
            shape,
            start,
            &timing,
        ));
    }
}

/// When and on what each hit lands: at the contact of its swing (sooner
/// for a released wind-up), on the target or, if it was killed, its
/// remains, driven the way the target is knocked (it is still shown where
/// it was) or else away from the attacker.
pub fn time_blows(
    timing: Res<Timing>,
    mut hits: MessageReader<Hit>,
    mut strikes: MessageReader<Strike>,
    mut blows: MessageWriter<BlowLands>,
    weapons: Query<&Weapon>,
    remains: Query<(Entity, &Remains)>,
    (positions, shown): (Query<&Pos>, Query<&AnimatedPos>),
) {
    let released: HashSet<Entity> = strikes
        .read()
        .map(|strike| strike.wielder)
        .filter(|wielder| weapons.get(*wielder).is_ok_and(|weapon| weapon.windup > 0))
        .collect();
    let shown_by = |entity: Entity| {
        remains
            .iter()
            .find(|(_, of)| of.0 == entity)
            .map_or((entity, false), |(remains, _)| (remains, true))
    };
    for hit in hits.read() {
        let (target, killed) = shown_by(hit.target);
        let (Ok(from), Ok(at)) = (positions.get(shown_by(hit.source).0), positions.get(target))
        else {
            continue;
        };
        let knocked = shown
            .get(target)
            .map_or(Vec2::ZERO, |shown| at.0.as_vec2() - shown.position);
        let toward = if knocked.length() > 0.5 {
            knocked
        } else {
            (at.0 - from.0).as_vec2()
        };
        let contact = if released.contains(&hit.source) {
            timing.release_contact()
        } else {
            timing.contact()
        };
        blows.write(BlowLands {
            target,
            toward,
            contact,
            damage: hit.damage,
            killed,
        });
    }
}

/// What a blow lands on moves only once it lands, flung.
pub fn hold_struck(
    timing: Res<Timing>,
    mut blows: MessageReader<BlowLands>,
    mut holds: MessageWriter<HoldMoves>,
    mut flings: MessageWriter<Fling>,
) {
    for blow in blows.read() {
        holds.write(HoldMoves {
            entity: blow.target,
            secs: blow.contact,
        });
        flings.write(Fling {
            entity: blow.target,
            secs: timing.0.fling_secs,
        });
    }
}

/// The way a body shakes when something drives it `toward`.
fn shake(entity: Entity, toward: Vec2, delay: f32, timing: &Timing) -> Gesture {
    Gesture {
        entity,
        shape: GestureShape::Shake {
            toward: toward.normalize_or_zero() * timing.0.shake_reach,
        },
        secs: timing.0.shake_secs,
        delay,
    }
}

/// What a blow lands on shakes the way it was driven.
pub fn shake_struck(
    timing: Res<Timing>,
    mut blows: MessageReader<BlowLands>,
    mut gestures: MessageWriter<Gesture>,
) {
    for blow in blows.read() {
        gestures.write(shake(blow.target, blow.toward, blow.contact, &timing));
    }
}

/// What a blow lands on bleeds what it `Bleeds`, the way it was driven,
/// more when it was killed.
pub fn bleed_struck(
    timing: Res<Timing>,
    mut blows: MessageReader<BlowLands>,
    mut effects: MessageWriter<PlayEffect>,
    bleeds: Query<&Bleeds>,
) {
    for blow in blows.read() {
        let Ok(bleeds) = bleeds.get(blow.target) else {
            continue;
        };
        let bonus = if blow.killed {
            timing.0.kill_strength
        } else {
            0
        };
        effects.write(PlayEffect {
            effect: bleeds.0.clone(),
            entity: blow.target,
            toward: blow.toward,
            swing: None,
            secs: 0.0,
            strength: blow.damage + bonus,
            delay: blow.contact,
        });
    }
}

/// What a knocked body runs into shakes when the blow lands.
pub fn shake_obstacles(
    timing: Res<Timing>,
    mut collisions: MessageReader<Collision>,
    mut gestures: MessageWriter<Gesture>,
    positions: Query<&Pos>,
) {
    for collision in collisions.read() {
        let (Ok(from), Ok(at)) = (
            positions.get(collision.mover),
            positions.get(collision.obstacle),
        ) else {
            continue;
        };
        let toward = (at.0 - from.0).as_vec2();
        gestures.write(shake(collision.obstacle, toward, timing.contact(), &timing));
    }
}

/// A knocked body bleeds what it `Bleeds` where it runs into something (its
/// remains, if that killed it).
pub fn bleed_impacts(
    timing: Res<Timing>,
    mut collisions: MessageReader<Collision>,
    mut effects: MessageWriter<PlayEffect>,
    remains: Query<(Entity, &Remains)>,
    positions: Query<&Pos>,
    bleeds: Query<&Bleeds>,
) {
    for collision in collisions.read() {
        let mover = remains
            .iter()
            .find(|(_, of)| of.0 == collision.mover)
            .map_or(collision.mover, |(remains, _)| remains);
        let (Ok(from), Ok(at), Ok(bleeds)) = (
            positions.get(mover),
            positions.get(collision.obstacle),
            bleeds.get(mover),
        ) else {
            continue;
        };
        effects.write(PlayEffect {
            effect: bleeds.0.clone(),
            entity: mover,
            toward: (at.0 - from.0).as_vec2(),
            swing: None,
            secs: 0.0,
            strength: 1,
            delay: timing.contact(),
        });
    }
}

/// What dies (and is removed) leaves its remains to be shown: its glyph,
/// carrying on the motion it had (and what it bleeds), held until the blow
/// lands, then sinking into the floor. Only shown objects (with an
/// animation) leave remains.
#[allow(clippy::type_complexity)]
pub fn leave_remains(
    timing: Res<Timing>,
    mut commands: Commands,
    mut died: MessageReader<Died>,
    bodies: Query<
        (
            &Pos,
            &Glyph,
            &ObjectAnimation,
            &AnimatedPos,
            Option<&Bleeds>,
        ),
        Without<KeepOnDeath>,
    >,
) {
    for &Died(body) in died.read() {
        let Ok((pos, glyph, animation, shown, bleeds)) = bodies.get(body) else {
            continue;
        };
        let mut remains = commands.spawn((
            Remains(body),
            Sinking,
            *pos,
            *glyph,
            animation.clone(),
            *shown,
            crate::animation::HeldMoves(timing.contact()),
            Flinging::new(timing.0.fling_secs),
        ));
        if let Some(bleeds) = bleeds {
            remains.insert(bleeds.clone());
        }
    }
}

#[cfg(test)]
mod tests;
