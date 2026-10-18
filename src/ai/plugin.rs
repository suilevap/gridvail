use bevy::prelude::*;
use flatbt_bevy::prelude::*;

use super::*;
use crate::locomotion::follow_paths;
use crate::navigation::share_paths;
use crate::schedule::{GamePhase, TurnPhase};
use crate::simulation::direction_tiles;

/// Enemy decisions: behavior trees, wandering goals, random steps, and what
/// doors are worth.
///
/// Every turn, between the player's input and the moves: door prices and
/// perception, the trees' tick, then `carry_out` orders each act's step, and
/// `wait_if_idle` spends the token of an enemy that took none (a thinking
/// one after a few frames).
///
/// An enemy runs a tree with `EnemyMind` and a `Behavior` for one of them:
/// `enemy_tree` (watch, hunt, wander) or `hunter_tree` (follow the `Order`,
/// fetching keys; needs `Order` too).
pub struct AiPlugin;

impl Plugin for AiPlugin {
    fn build(&self, app: &mut App) {
        super::attacks::install(app);
        app.init_resource::<DoorPolicy>()
            .init_resource::<crate::model::TurnMode>()
            .init_resource::<PortalPolicy>()
            .add_plugins((
                BehaviorPlugin::for_tree(enemy_tree).tick_mode(enemy_tick),
                BehaviorPlugin::for_tree(hunter_tree).tick_mode(enemy_tick),
            ))
            .configure_sets(
                Update,
                BehaviorSystems
                    .in_set(TurnPhase::Decide)
                    .in_set(GamePhase::Simulation),
            )
            .add_systems(
                Update,
                (
                    (
                        random_walk,
                        wander_goals,
                        price_doors,
                        apply_portal_policy,
                        order_hunters,
                        perceive,
                        perceive_objectives,
                    )
                        .chain()
                        .after(share_paths)
                        .before(BehaviorSystems)
                        .in_set(TurnPhase::Decide),
                    carry_out.after(BehaviorSystems).in_set(TurnPhase::Decide),
                    wait_if_idle
                        .after(follow_paths)
                        .after(carry_out)
                        .in_set(TurnPhase::Follow),
                )
                    .in_set(GamePhase::Simulation),
            )
            .add_systems(
                Update,
                show_mood
                    .after(direction_tiles)
                    .in_set(GamePhase::Simulation),
            );
    }
}
