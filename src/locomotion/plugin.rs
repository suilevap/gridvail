use bevy::prelude::*;

use super::{collect_plans, follow_paths, plans_leave_the_frame, share_nav, SharedNav};
use crate::schedule::GamePhase;
use crate::service::ServiceMode;
use crate::simulation::move_commands;

/// Walks actors with a [`Destination`](crate::model::Destination) and a
/// [`PathFollow`](crate::model::PathFollow) there, reporting the outcome.
/// With [`ServiceMode`] other than `Inline`, actors that also have a
/// [`Planning`](super::Planning) plan their paths as service jobs.
pub struct LocomotionPlugin;

impl Plugin for LocomotionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ServiceMode>()
            .init_resource::<SharedNav>()
            .add_systems(
                Update,
                (
                    share_nav.run_if(plans_leave_the_frame),
                    collect_plans,
                    follow_paths,
                )
                    .chain()
                    .in_set(GamePhase::Simulation)
                    .before(move_commands),
            );
    }
}
