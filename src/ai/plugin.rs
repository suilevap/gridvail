use bevy::prelude::*;
use flatbt_bevy::prelude::*;

use crate::schedule::SimulationStep;

use super::*;

/// Enemy behavior trees: perceive, tick, carry out, all in `Decide`.
///
/// Enemies opt in by carrying `EnemyMind` and `Behavior::for_tree(enemy_tree)`.
pub struct AiPlugin;

impl Plugin for AiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(BehaviorPlugin::for_tree(enemy_tree).tick_mode(enemy_tick))
            .configure_sets(Update, BehaviorSystems.in_set(SimulationStep::Decide))
            .add_systems(
                Update,
                (
                    perceive.before(BehaviorSystems),
                    carry_out.after(BehaviorSystems),
                )
                    .in_set(SimulationStep::Decide),
            );
    }
}
