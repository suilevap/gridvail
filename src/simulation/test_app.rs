use bevy::prelude::*;
use rand::SeedableRng;

use super::*;
use crate::model::*;
use crate::vision;

pub fn headless() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(MapGrid::new(8, 8))
        .init_resource::<vision::FovShared>()
        .init_resource::<TurnState>()
        .init_resource::<TokenTimer>()
        .init_resource::<TurnPacing>()
        .init_resource::<CollisionBuffer>()
        .init_resource::<CommitBuffer>()
        .insert_resource(SharedRng(rand::rngs::StdRng::seed_from_u64(42)))
        .add_systems(
            Update,
            (
                turn_tick,
                recharge_tokens,
                move_commands,
                update_direction,
                movement,
                resolve_collect,
                resolve_unmap,
                resolve_missing_positions,
                resolve_commit,
                relative_position,
                friction,
                destroy_entities,
                turn_update,
                vision::ensure_fov_storage,
                vision::compute_fov,
                vision::player_visibility,
            )
                .chain(),
        );
    app
}
