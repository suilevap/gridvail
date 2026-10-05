//! Decisions: where actors want to go and what things are worth to them.
//!
//! The AI writes intentions (a [`MoveCommand`] for a single random step, or a
//! [`Destination`] to walk to and what a door is worth on the way); the
//! `locomotion` module turns destinations into steps.

#![allow(clippy::type_complexity)]

mod plugin;

pub use plugin::*;

use bevy::prelude::*;
use rand::RngExt;

use crate::model::*;
use crate::navigation::{NavCell, NavMap};

/// Tries at picking a random floor cell for a wanderer per action.
const WANDER_TRIES: usize = 16;

/// What spending a key on a closed door is worth to an actor, in steps of
/// detour it would rather walk.
///
/// The last key is worth the most: spending one of ten keys is cheap, the
/// only one is not. The price is `last_key_cost` divided by the keys held,
/// rounded up, so with the default 20: one key 20 steps, two 10, four 5,
/// ten 2. Insert a different value before adding `AiPlugin` to tune it.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoorPolicy {
    pub last_key_cost: u32,
}

impl Default for DoorPolicy {
    fn default() -> Self {
        Self { last_key_cost: 20 }
    }
}

impl DoorPolicy {
    /// Extra cost of a door for an actor holding `keys` keys; `None`
    /// (avoid doors) without any.
    pub fn door_cost(&self, keys: usize) -> Option<u32> {
        let keys = u32::try_from(keys).ok().filter(|keys| *keys > 0)?;
        Some(self.last_key_cost.div_ceil(keys))
    }
}

/// Enemies without a destination take a random step, or stay, each action
/// (the original game's behaviour).
pub fn random_walk(
    turn: Res<TurnState>,
    mut rng: ResMut<SharedRng>,
    mut enemies: Query<
        &mut MoveCommand,
        (
            With<Enemy>,
            With<Active>,
            Without<DestroyRequested>,
            Without<Destination>,
        ),
    >,
) {
    if turn.simulation {
        return;
    }
    const MOVES: [IVec2; 5] = [IVec2::ZERO, IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y];
    for mut command in enemies.iter_mut() {
        command.target = MOVES[rng.0.random_range(0..MOVES.len())];
        command.relative = true;
        command.active = true;
    }
}

/// Gives every idle wanderer a random floor cell to walk to.
pub fn wander_goals(
    turn: Res<TurnState>,
    nav: Res<NavMap>,
    mut rng: ResMut<SharedRng>,
    mut wanderers: Query<&mut Destination, (With<Wander>, With<Active>, Without<DestroyRequested>)>,
) {
    if turn.simulation || nav.grid().is_empty() {
        return;
    }
    let (width, height) = (nav.grid().width() as i32, nav.grid().height() as i32);
    for mut destination in &mut wanderers {
        if destination.goal.is_some() {
            continue;
        }
        for _ in 0..WANDER_TRIES {
            let goal = IVec2::new(rng.0.random_range(0..width), rng.0.random_range(0..height));
            if nav.at(goal) == NavCell::Floor {
                destination.go_to(goal);
                break;
            }
        }
    }
}

/// Keeps each destination's door cost in line with the keys its actor holds.
pub fn price_doors(
    policy: Res<DoorPolicy>,
    keys: Query<(), With<Key>>,
    mut actors: Query<(&mut Destination, Option<&Inventory>), Without<DestroyRequested>>,
) {
    for (mut destination, inventory) in &mut actors {
        let held = inventory.map_or(0, |inventory| {
            inventory
                .0
                .iter()
                .filter(|&&item| keys.contains(item))
                .count()
        });
        let door_cost = policy.door_cost(held);
        if destination.door_cost != door_cost {
            destination.door_cost = door_cost;
        }
    }
}

#[cfg(test)]
mod tests;
