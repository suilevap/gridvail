//! Enemy decision-making with FlatBT behavior trees.
//!
//! Three steps inside `SimulationStep::Decide`, following `flatbt-bevy`'s
//! contract:
//!
//! 1. `perceive` fills each enemy's `EnemyMind` blackboard from the world;
//! 2. the tree ticks and writes what the enemy is doing into `EnemyAct`;
//! 3. `carry_out` turns that act into the ordinary `MoveCommand`.
//!
//! The tree never touches the world, so enemies stay inside the same token,
//! movement, and collision pipeline as the player.

#![allow(clippy::type_complexity)]

mod perception;
mod plugin;
mod tree;

pub use perception::*;
pub use plugin::*;
pub use tree::*;

#[cfg(test)]
mod tests;
