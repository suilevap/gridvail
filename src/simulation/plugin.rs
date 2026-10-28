use bevy::prelude::*;
use rand::SeedableRng;

use crate::model::{
    ChannelDone, CollisionBuffer, ControlScheme, Displace, Impact, MapGrid, PortalCrossings,
    SharedRng, Tile, TokenTimer, TurnPacing, TurnState,
};
use crate::schedule::{ActionStage, GamePhase, PlayerCommands, StartupPhase};

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
            .init_resource::<ControlScheme>()
            .init_resource::<TokenTimer>()
            .init_resource::<TurnPacing>()
            .init_resource::<CollisionBuffer>()
            .init_resource::<PortalCrossings>()
            .init_resource::<CommitBuffer>()
            .init_resource::<super::room_plans::RoomPlanners>()
            .insert_resource(SharedRng(rand::rngs::StdRng::seed_from_u64(self.rng_seed)))
            .add_message::<ChannelDone>()
            .add_message::<Displace>()
            .add_message::<Impact>()
            // A turn's actions, in order: begun ones go on, input is read,
            // actors decide, and the effects play out before the moves.
            .configure_sets(
                Update,
                (
                    ActionStage::Ongoing,
                    PlayerCommands,
                    ActionStage::Decide,
                    ActionStage::Effects,
                    ActionStage::Movement,
                    ActionStage::Hits,
                    ActionStage::Damage,
                )
                    .chain()
                    .after(recharge_tokens)
                    .before(move_commands)
                    .in_set(GamePhase::Simulation),
            )
            .add_systems(
                Update,
                (
                    channel.in_set(ActionStage::Ongoing),
                    displace.in_set(ActionStage::Movement),
                ),
            )
            .add_systems(
                Startup,
                (size_commit_buffer, tile_system)
                    .chain()
                    .in_set(StartupPhase::Derive),
            )
            .add_systems(
                Update,
                (
                    sync_portals,
                    turn_tick,
                    recharge_tokens,
                    player_input.in_set(PlayerCommands),
                    move_commands,
                    update_direction,
                    movement,
                    friction,
                    resolve_collect,
                    resolve_unmap,
                    resolve_commit,
                    turn_portal_crossers,
                    relative_position,
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
                    .in_set(GamePhase::Simulation),
            )
            .add_systems(
                PreUpdate,
                track_keyboard_scheme
                    .after(bevy::input::InputSystems)
                    .run_if(resource_exists::<ButtonInput<KeyCode>>),
            )
            .add_systems(Update, turn_update.in_set(GamePhase::Finalize));
    }
}

fn size_commit_buffer(mut commands: Commands, grid: Res<MapGrid>) {
    commands.insert_resource(CommitBuffer::sized(grid.width, grid.height));
}
