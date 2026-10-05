//! Walking actors to their [`Destination`] along planned paths, one step
//! per action, through their ordinary [`MoveCommand`]s.

#![allow(clippy::type_complexity)]

mod plugin;

pub use plugin::*;

use bevy::prelude::*;

use crate::model::*;
use crate::navigation::{NavMap, PathPlanner};

/// Steps an actor may fail in a row (another actor in the way) before it
/// gives up on its destination.
pub const MAX_BLOCKED_STEPS: u8 = 3;

/// Orders the next step toward each actor's destination, planning the path
/// first when the destination (or what a door is worth) changed or the
/// actor left its path. Clears the destination on arrival, when it cannot
/// be reached, or after [`MAX_BLOCKED_STEPS`] failed steps.
pub fn follow_paths(
    turn: Res<TurnState>,
    nav: Res<NavMap>,
    mut planner: ResMut<PathPlanner>,
    mut walkers: Query<
        (&Pos, &mut Destination, &mut PathFollow, &mut MoveCommand),
        (With<Active>, Without<DestroyRequested>),
    >,
) {
    if turn.simulation || nav.grid().is_empty() {
        return;
    }
    for (pos, mut destination, mut follow, mut command) in &mut walkers {
        let Some(goal) = destination.goal else {
            if follow.planned_for.is_some() {
                follow.clear();
            }
            continue;
        };
        if command.active {
            continue;
        }
        let pos = pos.0;
        if pos == goal {
            destination.goal = None;
            follow.clear();
            continue;
        }
        // Skip cells already reached (the start, or a step just taken).
        while follow.steps.get(follow.next) == Some(&pos) {
            follow.next += 1;
        }
        let wanted = Some((goal, destination.door_cost));
        let off_path = follow
            .steps
            .get(follow.next)
            .is_none_or(|next| (*next - pos).abs().element_sum() != 1);
        if follow.planned_for != wanted || off_path {
            let PathFollow { steps, .. } = &mut *follow;
            if !planner.plan(&nav, pos, goal, destination.door_cost, steps) {
                destination.goal = None;
                follow.clear();
                continue;
            }
            follow.next = 1;
            follow.planned_for = wanted;
            follow.ordered_from = None;
            follow.blocked = 0;
        }
        // An order from the same cell as the last one means that step failed.
        if follow.ordered_from == Some(pos) {
            follow.blocked += 1;
            if follow.blocked >= MAX_BLOCKED_STEPS {
                destination.goal = None;
                follow.clear();
                continue;
            }
        } else {
            follow.blocked = 0;
        }
        follow.ordered_from = Some(pos);
        command.target = follow.steps[follow.next] - pos;
        command.relative = true;
        command.active = true;
    }
}

#[cfg(test)]
pub(crate) mod tests;
