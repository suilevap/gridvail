//! Walking actors to their [`Destination`] along planned paths, one step
//! per action, through their ordinary [`MoveCommand`]s. Paths are planned
//! with each actor's [`TraversalPrefs`]; [`PathFollow::status`] reports the
//! outcome to whoever set the destination.

#![allow(clippy::type_complexity)]

mod planning;
mod plugin;

pub use planning::*;
pub use plugin::*;

use bevy::prelude::*;

use crate::model::*;
use crate::navigation::{cell_of, Crowd, NavMap, PathPlanner, Terrain};
use crate::service::ServiceMode;

/// Steps an actor may fail in a row (another actor in the way) before it
/// reports [`WalkStatus::Blocked`].
pub const MAX_BLOCKED_STEPS: u8 = 3;

/// A landed plan for a goal at most this far from the current one is still
/// worth following while a fresher one is on its way.
const NEAR_GOAL: i32 = 2;

/// Orders the next step toward each actor's [`Destination`], planning the
/// path first for a new request, a changed door cost, or when the actor
/// left its path. Reports how it goes in [`PathFollow::status`] and never
/// changes the destination: whoever set it decides what to do once walking
/// is done.
///
/// Actors with a [`Planning`] component plan as service jobs when
/// [`ServiceMode`] is not `Inline`: see the `planning` module.
pub fn follow_paths(
    turn: Res<TurnState>,
    mode: Res<ServiceMode>,
    nav: Res<NavMap>,
    shared: Res<SharedNav>,
    grid: Res<MapGrid>,
    mut planner: ResMut<PathPlanner>,
    mut walkers: Query<
        (
            &Pos,
            &Destination,
            Option<&TraversalPrefs>,
            &mut PathFollow,
            &mut MoveCommand,
            Option<&mut Planning>,
        ),
        (With<Active>, Without<DestroyRequested>),
    >,
) {
    if turn.simulation || nav.grid().is_empty() {
        return;
    }
    for (pos, destination, prefs, mut follow, mut command, planning) in &mut walkers {
        let pos = pos.0;
        let door_cost = prefs.and_then(|prefs| prefs.door_cost);
        let crowd_cost = prefs.and_then(|prefs| prefs.crowd_cost);
        let mut planning = planning.filter(|_| *mode != ServiceMode::Inline);
        let Some(goal) = destination.goal() else {
            if follow.status != WalkStatus::Idle {
                follow.finish(WalkStatus::Idle);
                follow.planned_for = None;
            }
            if let Some(planning) = planning.as_deref_mut() {
                planning.cancel();
            }
            continue;
        };
        if command.active {
            continue;
        }
        let wanted = Some((destination.request(), door_cost));
        if let Some(planning) = planning.as_deref_mut() {
            take_landed_plan(planning, &mut follow, pos, goal, wanted);
        }
        let replan = follow.planned_for != wanted;
        if !replan && follow.status.is_done() {
            continue;
        }
        if pos == goal {
            follow.finish(WalkStatus::Arrived);
            follow.planned_for = wanted;
            if let Some(planning) = planning.as_deref_mut() {
                planning.cancel();
            }
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
        if let (true, Some(planning)) = (replan || off_path, planning.as_deref_mut()) {
            if !planning.is_planning() {
                let request = PlanRequest {
                    from: pos,
                    goal,
                    wanted,
                    door_cost,
                    crowd_cost,
                };
                planning.start(request, &shared, *mode);
            }
            follow.status = WalkStatus::Planning;
            // Keep to the old path while it still leads near the goal and on
            // from here; otherwise wait for the plan.
            let leads_near = follow
                .steps
                .last()
                .is_some_and(|end| (*end - goal).abs().element_sum() <= NEAR_GOAL);
            if off_path || !leads_near {
                follow.ordered_from = None;
                continue;
            }
        } else if replan || off_path {
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

/// Takes a plan that landed if it still serves: planned for the current
/// door cost and a goal near the current one, and passing where the actor
/// now stands (it may have walked on along its old path meanwhile). An
/// older plan is followed until the fresh one lands; a plan that found no
/// way ends the walk only if it was for the current request.
fn take_landed_plan(
    planning: &mut Planning,
    follow: &mut PathFollow,
    pos: IVec2,
    goal: IVec2,
    wanted: Option<(u32, Option<u32>)>,
) {
    let Some(result) = planning.take_landed() else {
        return;
    };
    let request = result.request;
    let current = request.wanted == wanted;
    let serves = request.door_cost == wanted.and_then(|(_, door_cost)| door_cost)
        && (request.goal - goal).abs().element_sum() <= NEAR_GOAL;
    if result.path.is_empty() {
        if current {
            follow.finish(WalkStatus::Unreachable);
            follow.planned_for = wanted;
        }
        planning.recycle(result.path);
        return;
    }
    let here = result.path.iter().position(|&cell| cell == pos);
    let (true, Some(here)) = (current || serves, here) else {
        planning.recycle(result.path);
        return;
    };
    let old = std::mem::replace(&mut follow.steps, result.path);
    planning.recycle(old);
    follow.next = here + 1;
    follow.status = WalkStatus::Walking;
    follow.ordered_from = None;
    follow.blocked = 0;
    follow.planned_for = request.wanted;
}

#[cfg(test)]
pub(crate) mod tests;
