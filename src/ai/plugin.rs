use bevy::prelude::*;
use flatbt_bevy::prelude::*;

use crate::schedule::{GamePhase, SimulationStep};

use super::*;

/// Enemy behavior trees: orders and perception, the tick, then services and
/// carrying out, all in `Decide`.
///
/// An enemy opts in with `EnemyMind` and a `Behavior` for one of the trees:
/// `enemy_tree` (watch, hunt, wander) or `hunter_tree` (follow the `Order`,
/// opening doors; needs `Order` and `Route` too).
pub struct AiPlugin;

impl Plugin for AiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            BehaviorPlugin::for_tree(enemy_tree).tick_mode(enemy_tick),
            BehaviorPlugin::for_tree(hunter_tree).tick_mode(enemy_tick),
            NavigationPlugin::<GridRouter>::default(),
        ))
        .configure_sets(Update, BehaviorSystems.in_set(SimulationStep::Decide))
        .add_systems(
            Update,
            (
                (order_hunters, perceive, perceive_objectives)
                    .chain()
                    .before(BehaviorSystems),
                carry_out.after(BehaviorSystems),
            )
                .in_set(SimulationStep::Decide),
        )
        .add_systems(
            Update,
            show_mood
                .after(SimulationStep::Resolve)
                .in_set(GamePhase::Simulation),
        );
    }
}
