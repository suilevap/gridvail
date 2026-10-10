//! Decisions: where actors want to go and what things are worth to them.
//!
//! The AI writes intentions: a [`MoveCommand`] for a single random step, or
//! a [`Destination`] to walk to plus [`TraversalPrefs`] (what a door is worth
//! to the actor). The `locomotion` module turns destinations into steps and
//! reports back in [`PathFollow::status`]; the AI decides what to do next.
//!
//! Enemies decide with FlatBT behavior trees, in three steps around the tick:
//!
//! 1. `perceive` (and `perceive_objectives` for agents with orders) fills
//!    each enemy's `EnemyMind` blackboard (and hands it the `Services`
//!    handle once);
//! 2. the tree (`enemy_tree`, or `hunter_tree` for hunters) writes what the
//!    enemy is doing into `EnemyAct`: walks are paths it asked a service for
//!    (flatbt's [`await_future`]);
//! 3. `carry_out` turns that act into a single step (a [`MoveCommand`]).
//!
//! The tree never touches the world, so enemies share the player's tokens,
//! movement, and collision rules.

#![allow(clippy::type_complexity)]

mod hunter;
mod perception;
mod plugin;
mod tree;

pub use hunter::*;
pub use perception::*;
pub use plugin::*;
pub use tree::*;

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

/// Gives every wanderer without a goal, or whose walk ended (arrived,
/// unreachable or blocked), a new random floor cell to walk to.
pub fn wander_goals(
    turn: Res<TurnState>,
    nav: Res<NavMap>,
    mut rng: ResMut<SharedRng>,
    mut wanderers: Query<
        (&mut Destination, Option<&PathFollow>),
        (With<Wander>, With<Active>, Without<DestroyRequested>),
    >,
) {
    if turn.simulation || nav.grid().is_empty() {
        return;
    }
    let (width, height) = (nav.grid().width() as i32, nav.grid().height() as i32);
    for (mut destination, follow) in &mut wanderers {
        let walk_ended = follow.is_some_and(|follow| follow.status.is_done());
        if destination.goal().is_some() && !walk_ended {
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

/// Keeps each actor's door cost in line with the keys it holds.
pub fn price_doors(
    policy: Res<DoorPolicy>,
    keys: Query<(), With<Key>>,
    mut actors: Query<(&mut TraversalPrefs, Option<&Inventory>), Without<DestroyRequested>>,
) {
    for (mut prefs, inventory) in &mut actors {
        let held = inventory.map_or(0, |inventory| {
            inventory
                .0
                .iter()
                .filter(|&&item| keys.contains(item))
                .count()
        });
        let door_cost = policy.door_cost(held);
        if prefs.door_cost != door_cost {
            prefs.door_cost = door_cost;
        }
    }
}

#[cfg(test)]
mod tests;
