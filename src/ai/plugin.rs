use bevy::prelude::*;

use super::{price_doors, random_walk, wander_goals, DoorPolicy};
use crate::locomotion::follow_paths;
use crate::schedule::GamePhase;
use crate::simulation::{move_commands, player_input};

/// Enemy decisions: random steps, wandering goals, and what doors are worth.
pub struct AiPlugin;

impl Plugin for AiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DoorPolicy>().add_systems(
            Update,
            (random_walk, wander_goals, price_doors)
                .chain()
                .in_set(GamePhase::Simulation)
                .after(player_input)
                .before(follow_paths)
                .before(move_commands),
        );
    }
}
