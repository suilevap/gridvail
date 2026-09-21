use bevy::prelude::*;
use rand::SeedableRng;

use crate::model::{CollisionBuffer, SharedRng, TokenTimer, TurnState};
use crate::schedule::{GamePhase, StartupPhase};

use super::*;

/// Turn simulation, movement, collision resolution, and tile derivation.
pub struct SimulationPlugin {
    rng_seed: u64,
}

impl SimulationPlugin {
    pub const fn new(rng_seed: u64) -> Self {
        Self { rng_seed }
    }
}

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TurnState>()
            .init_resource::<TokenTimer>()
            .init_resource::<CollisionBuffer>()
            .init_resource::<CommitBuffer>()
            .insert_resource(SharedRng(rand::rngs::StdRng::seed_from_u64(self.rng_seed)))
            .add_systems(Startup, tile_system.in_set(StartupPhase::Derive))
            .add_systems(
                Update,
                (
                    turn_tick,
                    recharge_tokens,
                    player_input,
                    enemy_ai,
                    move_commands,
                    update_direction,
                    movement,
                    friction,
                    resolve_collect,
                    resolve_unmap,
                    resolve_commit,
                    relative_position,
                    verify_map,
                    destroy_entities.run_if(has_destroy_requests),
                    direction_tiles,
                )
                    .chain()
                    .in_set(GamePhase::Simulation),
            )
            .add_systems(Update, turn_update.in_set(GamePhase::Finalize));
    }
}
