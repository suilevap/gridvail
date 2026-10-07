//! Walking actors to their [`Destination`] along planned paths, one step
//! per action, through their ordinary [`MoveCommand`]s. Paths are planned
//! with each actor's [`TraversalPrefs`]; [`PathFollow::status`] reports the
//! outcome to whoever set the destination.

#![allow(clippy::type_complexity)]

mod plugin;

pub use plugin::*;

use bevy::prelude::*;

use crate::model::*;
use crate::navigation::{cell_of, Crowd, NavMap, PathPlanner, Terrain};

/// Steps an actor may fail in a row (another actor in the way) before it
/// reports [`WalkStatus::Blocked`].
pub const MAX_BLOCKED_STEPS: u8 = 3;

/// Orders the next step toward each actor's [`Destination`], planning the
/// path first for a new request, a changed door cost, or when the actor
/// left its path. Reports how it goes in [`PathFollow::status`] and never
/// changes the destination: whoever set it decides what to do once walking
/// is done.
pub fn follow_paths(
    turn: Res<TurnState>,
    nav: Res<NavMap>,
    grid: Res<MapGrid>,
    mut planner: ResMut<PathPlanner>,
    mut walkers: Query<
        (
            &Pos,
            &Destination,
            Option<&TraversalPrefs>,
            &mut PathFollow,
            &mut MoveCommand,
        ),
        (With<Active>, Without<DestroyRequested>),
    >,
) {
    if turn.simulation || nav.grid().is_empty() {
        return;
    }
    for (pos, destination, prefs, mut follow, mut command) in &mut walkers {
        let pos = pos.0;
        let door_cost = prefs.and_then(|prefs| prefs.door_cost);
        let crowd_cost = prefs.and_then(|prefs| prefs.crowd_cost);
        let Some(goal) = destination.goal() else {
            if follow.status != WalkStatus::Idle {
                follow.finish(WalkStatus::Idle);
                follow.planned_for = None;
            }
            continue;
        };
        if command.active {
            continue;
        }
        let wanted = Some((destination.request(), door_cost));
        let replan = follow.planned_for != wanted;
        if !replan && follow.status.is_done() {
            continue;
        }
        if pos == goal {
            follow.finish(WalkStatus::Arrived);
            follow.planned_for = wanted;
            continue;
        }
        // Skip cells already reached (the start, or a step just taken).
        while follow.steps.get(follow.next) == Some(&pos) {
            follow.next += 1;
        }
        let off_path = follow
            .steps
            .get(follow.next)
            .is_none_or(|next| (*next - pos).abs().element_sum() != 1);
        if replan || off_path {
            follow.planned_for = wanted;
            let PathFollow { steps, .. } = &mut *follow;
            let terrain = Terrain {
                nav: &nav,
                door_cost,
            };
            let found = match crowd_cost {
                None => planner.plan_with(&nav, &terrain, pos, goal, steps),
                Some(cost) => {
                    let crowd = Crowd {
                        terrain,
                        occupied: &*grid,
                        goal: cell_of(goal),
                        cost,
                    };
                    planner.plan_with(&nav, &crowd, pos, goal, steps)
                }
            };
            if !found {
                follow.finish(WalkStatus::Unreachable);
                continue;
            }
            follow.status = WalkStatus::Walking;
            follow.next = 1;
            follow.ordered_from = None;
            follow.blocked = 0;
        }
        // An order from the same cell as the last one means that step failed.
        if follow.ordered_from == Some(pos) {
            follow.blocked += 1;
            if follow.blocked >= MAX_BLOCKED_STEPS {
                follow.finish(WalkStatus::Blocked);
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
