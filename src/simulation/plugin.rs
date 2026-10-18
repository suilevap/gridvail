use bevy::prelude::*;
use rand::SeedableRng;

use crate::model::{
    CollisionBuffer, ControlScheme, MapGrid, PortalCrossings, SharedRng, Tile, TokenTimer,
    TurnDeadline, TurnMode, TurnPacing, TurnState,
};
use crate::schedule::{GamePhase, StartupPhase, TurnPhase};

use super::*;

/// Turn simulation, movement, collision resolution, doors and keys, and tile
/// derivation.
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
            .init_resource::<TurnMode>()
            .init_resource::<TurnDeadline>()
            .init_resource::<ControlScheme>()
            .init_resource::<TokenTimer>()
            .init_resource::<TurnPacing>()
            .init_resource::<CollisionBuffer>()
            .init_resource::<PortalCrossings>()
            .init_resource::<CommitBuffer>()
            .init_resource::<super::room_plans::RoomPlanners>()
            .insert_resource(SharedRng(rand::rngs::StdRng::seed_from_u64(self.rng_seed)))
            .add_systems(
                Startup,
                (size_commit_buffer, tile_system)
                    .chain()
                    .in_set(StartupPhase::Derive),
            )
            .configure_sets(
                Update,
                (
                    TurnPhase::Input,
                    TurnPhase::Start,
                    TurnPhase::Decide,
                    TurnPhase::Follow,
                    TurnPhase::Attacks,
                    TurnPhase::Spend,
                    TurnPhase::Move,
                    TurnPhase::Damage,
                    TurnPhase::Cleanup,
                )
                    .chain()
                    .in_set(GamePhase::Simulation),
            )
            .add_systems(
                Update,
                (sync_portals, turn_tick, recharge_tokens)
                    .chain()
                    .before(TurnPhase::Input)
                    .in_set(GamePhase::Simulation),
            )
            .add_systems(
                Update,
                (player_input, hold_to_wait)
                    .chain()
                    .in_set(TurnPhase::Input),
            )
            .add_systems(
                Update,
                (accept_turn, cancel_expired_wait)
                    .chain()
                    .in_set(TurnPhase::Start),
            )
            .add_systems(
                Update,
                (stop_dead_actions, synchronize_idle_actors, move_commands)
                    .chain()
                    .in_set(TurnPhase::Spend),
            )
            .add_systems(
                Update,
                (
                    update_direction,
                    movement,
                    friction,
                    resolve_collect,
                    resolve_unmap,
                    resolve_commit,
                    turn_portal_crossers,
                    relative_position,
                )
                    .chain()
                    .in_set(TurnPhase::Move),
            )
            .add_systems(
                Update,
                (
                    deactivate_dead,
                    verify_map,
                    open_doors,
                    pick_up_items,
                    (
                        press_plates,
                        transform_rooms,
                        tile_system.run_if(any_with_component::<Tile>),
                    )
                        .chain(),
                    drop_items.run_if(has_destroy_requests),
                    destroy_entities.run_if(has_destroy_requests),
                    direction_tiles,
                )
                    .chain()
                    .in_set(TurnPhase::Cleanup),
            )
            .add_systems(
                PreUpdate,
                track_keyboard_scheme
                    .after(bevy::input::InputSystems)
                    .run_if(resource_exists::<ButtonInput<KeyCode>>),
            )
            .add_systems(PreUpdate, request_restart.after(bevy::input::InputSystems))
            .add_systems(Update, turn_update.in_set(GamePhase::Finalize));
    }
}

fn size_commit_buffer(mut commands: Commands, grid: Res<MapGrid>) {
    commands.insert_resource(CommitBuffer::sized(grid.width, grid.height));
}
