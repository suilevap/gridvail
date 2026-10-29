use bevy::prelude::*;
use rand::SeedableRng;

use crate::model::{
    Collision, CollisionBuffer, ControlScheme, Follow, Knock, MapGrid, PortalCrossings, SharedRng,
    Swap, Tile, TokenTimer, TurnPacing, TurnState,
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
        time::configure(app);
        app.init_resource::<TurnState>()
            .init_resource::<ControlScheme>()
            .init_resource::<TokenTimer>()
            .init_resource::<TurnPacing>()
            .init_resource::<CollisionBuffer>()
            .init_resource::<PortalCrossings>()
            .init_resource::<CommitBuffer>()
            .init_resource::<super::room_plans::RoomPlanners>()
            .insert_resource(SharedRng(rand::rngs::StdRng::seed_from_u64(self.rng_seed)))
            .add_message::<Knock>()
            .add_message::<Follow>()
            .add_message::<Swap>()
            .add_message::<Collision>()
            // A turn's actions, in order: begun ones go on, input is read,
            // actors decide, and the effects play out before the moves.
            .configure_sets(
                Update,
                (
                    ActionStage::Due,
                    ActionStage::Ongoing,
                    PlayerCommands,
                    ActionStage::Decide,
                    ActionStage::Effects,
                    ActionStage::Costs,
                    ActionStage::Movement,
                )
                    .chain()
                    .after(recharge_tokens)
                    .before(move_commands)
                    .in_set(GamePhase::Simulation),
            )
            // Damage is taken once the pass has moved everything, knocked
            // bodies included: a lethal blow knocks its target like any
            // other, and it dies where it lands, before the dead are
            // removed.
            .configure_sets(
                Update,
                (ActionStage::Hits, ActionStage::Damage)
                    .chain()
                    .after(resolve_commit)
                    .before(drop_items)
                    .in_set(GamePhase::Simulation),
            )
            // Physics rides the movement passes: knocks start after each
            // mover's own step is chosen, and what moved is reported once
            // the pass is resolved.
            .add_systems(
                Update,
                (
                    swap_places.in_set(ActionStage::Movement),
                    knock.after(update_direction).before(movement),
                    (report_collisions, settle_knocked)
                        .chain()
                        .after(resolve_collect)
                        .before(resolve_unmap),
                    // After friction too, so the step it gives is not
                    // slowed before the next pass takes it.
                    follow
                        .after(resolve_commit)
                        .after(friction)
                        .before(turn_portal_crossers),
                )
                    .in_set(GamePhase::Simulation),
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
