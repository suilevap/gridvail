//! Choreography: one exchange of blows played in order, at the right
//! moments, on top of a simulation that settles it all in one frame.
//!
//! The simulation stays instant and only says what happened
//! (`Attack`, `Strike`, `Hit`, `Collision`, `Died`). These systems
//! turn that into a timeline for the animation step and effects, measured
//! from the frame it happened in:
//!
//! ```text
//! before     an aimed weapon winds up and holds
//! 0          the attacker swings as its look says (`SwingStarts`; its own
//!            moves held), leaving its streak
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

use crate::animation::{ObjectAnimation, SWING_CONTACT, SWING_WOUND};
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
            .add_message::<SwingStarts>()
            .add_message::<BlowLands>()
            .add_systems(
                Update,
                (
                    (wind_up_aims, unwind_dropped_aims),
                    (start_maneuver_swings, start_weapon_swings, hold_attackers),
                    (play_swings, streak_swings, time_blows),
                    (
                        hold_struck,
                        fling_struck,
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

    /// When a swing's blow lands, from when it starts.
    pub fn contact_of(&self, released: bool) -> f32 {
        if released {
            self.release_contact()
        } else {
            self.contact()
        }
    }
}

/// How far, in cells, a target must be shown from its cell to count as
/// knocked: still sliding there.
const KNOCKED: f32 = 0.5;

/// `entity` swings as `look` says, along `shape`, toward `dir`: whole, or
/// `released` from the wind-up it holds (sooner, from the wound-up pose).
#[derive(Message, Clone, Debug, PartialEq)]
pub struct SwingStarts {
    pub entity: Entity,
    pub dir: IVec2,
    pub look: SwingLook,
    pub shape: GestureShape,
    pub released: bool,
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

/// The gesture of a swing as it `looks`, through `area` from facing
/// `facing`: a sweep goes the way the area's cells go round. None when it
/// makes no gesture.
pub fn area_swing(look: &SwingLook, area: &Area, facing: IVec2) -> Option<GestureShape> {
    let across = match area.across(facing) {
        IVec2::ZERO => facing.perp(),
        across => across,
    };
    look.swing.map(|swing| swing_gesture(swing, facing, across))
}

/// A weapon's wielder winds up its swing when it aims, and holds it until
/// the strike: its anticipation is its telegraph.
pub fn wind_up_aims(
    timing: Res<Timing>,
    mut wind_ups: MessageWriter<WindUp>,
    aimed: Query<(Entity, &Weapon, &WeaponLook, &Telegraph), Changed<Telegraph>>,
) {
    for (wielder, weapon, look, telegraph) in &aimed {
        let Some(shape) = area_swing(&look.0, &weapon.area, telegraph.dir) else {
            continue;
        };
        wind_ups.write(WindUp {
            entity: wielder,
            shape,
            secs: timing.0.aim_secs,
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

/// An attacker swings, whole, as its maneuver looks, through the
/// maneuver's cells.
pub fn start_maneuver_swings(
    mut attacks: MessageReader<Attack>,
    mut swings: MessageWriter<SwingStarts>,
    actors: Query<(&Maneuvers, &ManeuverLooks)>,
) {
    for attack in attacks.read() {
        let Ok((maneuvers, looks)) = actors.get(attack.actor) else {
            continue;
        };
        let look = looks.get(attack.relation);
        let area = &maneuvers.get(attack.relation).area;
        let Some(shape) = area_swing(look, area, attack.facing) else {
            continue;
        };
        swings.write(SwingStarts {
            entity: attack.actor,
            dir: attack.facing,
            look: look.clone(),
            shape,
            released: false,
        });
    }
}

/// A weapon swings the way it strikes, as it looks: released from its
/// wind-up when it was wound up, else whole.
pub fn start_weapon_swings(
    mut strikes: MessageReader<Strike>,
    mut swings: MessageWriter<SwingStarts>,
    weapons: Query<(&Weapon, &WeaponLook)>,
) {
    for strike in strikes.read() {
        let Ok((weapon, look)) = weapons.get(strike.wielder) else {
            continue;
        };
        let Some(shape) = area_swing(&look.0, &weapon.area, strike.dir) else {
            continue;
        };
        swings.write(SwingStarts {
            entity: strike.wielder,
            dir: strike.dir,
            look: look.0.clone(),
            shape,
            released: weapon.windup > 0,
        });
    }
}

/// An attacker keeps its own moves (a lunge's step, a step back) until
/// just after its blow lands.
pub fn hold_attackers(
    timing: Res<Timing>,
    mut attacks: MessageReader<Attack>,
    mut holds: MessageWriter<HoldMoves>,
) {
    for attack in attacks.read() {
        holds.write(HoldMoves {
            entity: attack.actor,
            secs: timing.contact() + timing.0.follow_gap,
        });
    }
}

/// Each swing plays: the rest of its wind-up, or the whole gesture.
pub fn play_swings(
    timing: Res<Timing>,
    mut swings: MessageReader<SwingStarts>,
    mut releases: MessageWriter<Release>,
    mut gestures: MessageWriter<Gesture>,
) {
    for swing in swings.read() {
        if swing.released {
            releases.write(Release {
                entity: swing.entity,
                secs: timing.release_secs(),
                strike: true,
            });
        } else {
            gestures.write(Gesture {
                entity: swing.entity,
                shape: swing.shape,
                secs: timing.0.swing_secs,
                delay: 0.0,
            });
        }
    }
}

/// Each swing leaves the streak its look names while it strikes.
pub fn streak_swings(
    timing: Res<Timing>,
    mut swings: MessageReader<SwingStarts>,
    mut effects: MessageWriter<PlayEffect>,
) {
    for swing in swings.read() {
        let Some(effect) = &swing.look.streak else {
            continue;
        };
        effects.write(PlayEffect {
            effect: effect.clone(),
            entity: swing.entity,
            toward: swing.dir.as_vec2(),
            swing: Some(swing.shape),
            secs: timing.streak_secs(),
            strength: 0,
            delay: if swing.released {
                0.0
            } else {
                timing.strike_start()
            },
        });
    }
}

/// When and on what each hit lands: at the contact of its swing, on the
/// target or, if it was killed, its remains, driven the way the target is
/// knocked (it is still shown where it was) or else away from the attacker.
pub fn time_blows(
    timing: Res<Timing>,
    grid: Res<MapGrid>,
    mut hits: MessageReader<Hit>,
    mut swings: MessageReader<SwingStarts>,
    mut blows: MessageWriter<BlowLands>,
    remains: Query<(Entity, &Remains)>,
    (positions, shown): (Query<&Pos>, Query<&AnimatedPos>),
) {
    let released: HashSet<Entity> = swings
        .read()
        .filter(|swing| swing.released)
        .map(|swing| swing.entity)
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
        let knocked = shown.get(target).map_or(IVec2::ZERO, |shown| {
            grid.delta(shown.position.round().as_ivec2(), at.0)
        });
        let toward = if knocked.as_vec2().length() > KNOCKED {
            knocked
        } else {
            grid.delta(from.0, at.0)
        };
        blows.write(BlowLands {
            target,
            toward: toward.as_vec2(),
            contact: timing.contact_of(released.contains(&hit.source)),
            damage: hit.damage,
            killed,
        });
    }
}

/// What a blow lands on moves only once it lands.
pub fn hold_struck(mut blows: MessageReader<BlowLands>, mut holds: MessageWriter<HoldMoves>) {
    for blow in blows.read() {
        holds.write(HoldMoves {
            entity: blow.target,
            secs: blow.contact,
        });
    }
}

/// What a blow lands on is flung: its next move fast, slowing to a stop.
pub fn fling_struck(
    timing: Res<Timing>,
    mut blows: MessageReader<BlowLands>,
    mut flings: MessageWriter<Fling>,
) {
    for blow in blows.read() {
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
    grid: Res<MapGrid>,
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
        let toward = grid.delta(from.0, at.0).as_vec2();
        gestures.write(shake(collision.obstacle, toward, timing.contact(), &timing));
    }
}

/// A knocked body bleeds what it `Bleeds` where it runs into something (its
/// remains, if that killed it).
pub fn bleed_impacts(
    timing: Res<Timing>,
    grid: Res<MapGrid>,
    mut collisions: MessageReader<Collision>,
    mut effects: MessageWriter<PlayEffect>,
    remains: Query<(Entity, &Remains)>,
    (positions, bleeds): (Query<&Pos>, Query<&Bleeds>),
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
            toward: grid.delta(from.0, at.0).as_vec2(),
            swing: None,
            secs: 0.0,
            strength: timing.0.impact_strength,
            delay: timing.contact(),
        });
    }
}

/// What dies (and is removed) leaves its remains to be shown: its glyph,
/// carrying on the motion it had (and what it bleeds), held until the blow
/// lands and flung, then sinking into the floor. Only shown objects (with
/// an animation) leave remains.
#[allow(clippy::type_complexity)]
pub fn leave_remains(
    timing: Res<Timing>,
    mut commands: Commands,
    mut died: MessageReader<Died>,
    (mut holds, mut flings): (MessageWriter<HoldMoves>, MessageWriter<Fling>),
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
        ));
        if let Some(bleeds) = bleeds {
            remains.insert(bleeds.clone());
        }
        let remains = remains.id();
        holds.write(HoldMoves {
            entity: remains,
            secs: timing.contact(),
        });
        flings.write(Fling {
            entity: remains,
            secs: timing.0.fling_secs,
        });
    }
}

#[cfg(test)]
mod tests;
