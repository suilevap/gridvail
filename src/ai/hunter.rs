//! The hunter: an enemy that follows its `Order`, opening doors on the way.
//!
//! Built on flatbt's goal stack. The root goal is always `FollowOrder`; a
//! goal asks for what it needs first with `need`, which pushes a subgoal:
//!
//! ```text
//! FollowOrder ─ the route runs into a door ─> OpenDoor(d)
//! OpenDoor(d) ─ no key ─> GetKey ─ the nearest key ─> Reach(key)
//!             ─ not next to it ─> Approach(d)
//! ```
//!
//! Moving is a request to the navigation service (`EnemyAct::GoTo`), so the
//! tree never sees the map: it learns that a door is in the way from the
//! service's answer on the next turn.

use bevy::math::IVec2;
use flatbt::goal_match;
use flatbt_bevy::prelude::*;

use crate::model::{Dest, EnemyAct, EnemyMind, RouteStatus};

/// What a hunter is working toward.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal {
    /// Go where the order says. The root; never done.
    FollowOrder,
    /// Stand on a cell.
    Reach(IVec2),
    /// Stand next to a cell.
    Approach(IVec2),
    /// Open the closed door at a cell.
    OpenDoor(IVec2),
    /// Carry a key.
    GetKey,
}

impl Goal {
    /// Where a travelling goal leads.
    fn dest(self) -> Dest {
        match self {
            Goal::Reach(cell) | Goal::Approach(cell) | Goal::OpenDoor(cell) => Dest::Cell(cell),
            Goal::FollowOrder | Goal::GetKey => Dest::Order,
        }
    }
}

/// Attack a player in reach; otherwise work through the goal stack; hold
/// when there is no order to follow.
pub fn hunter_tree() -> impl BehaviorNode<EnemyMind, EnemyAct> {
    select((
        guard(
            EnemyMind::next_to_player,
            leaf(|mind: &mut EnemyMind| match mind.player {
                Some(player) => NodeResult::Running(EnemyAct::Attack(player - mind.pos)),
                None => NodeResult::Failure,
            }),
        ),
        goals::<6, _, _>(
            |_: &EnemyMind| Goal::FollowOrder,
            goal_match!(|goal: &Goal| {
                // Head for the destination. When the last route answer says a
                // closed door is in the way, open it first; whether or not
                // that worked, keep going (a door that stays shut leaves the
                // hunter waiting at it).
                Goal::FollowOrder | Goal::Reach(_) | Goal::Approach(_) => select((
                    force_failure(need(|mind: &EnemyMind, goal: &Goal| {
                        let door = mind.door_toward(goal.dest())?;
                        // Approaching a door is not blocked by that door.
                        (*goal != Goal::Approach(door)).then_some(Goal::OpenDoor(door))
                    })),
                    with_goal(leaf_with(go_to)),
                )),
                Goal::OpenDoor(_) => seq((
                    need(|mind: &EnemyMind, _: &Goal| (!mind.has_key).then_some(Goal::GetKey)),
                    need(|mind: &EnemyMind, goal: &Goal| {
                        let Goal::OpenDoor(door) = *goal else { return None };
                        (!mind.adjacent(door)).then_some(Goal::Approach(door))
                    }),
                    with_goal(leaf_with(|mind: &mut EnemyMind, goal: &Goal| {
                        let Goal::OpenDoor(door) = *goal else {
                            return NodeResult::Failure;
                        };
                        NodeResult::Running(EnemyAct::Open(door - mind.pos))
                    })),
                )),
                Goal::GetKey => seq((
                    need(|mind: &EnemyMind, _: &Goal| mind.nearest_key.map(Goal::Reach)),
                    leaf(|mind: &mut EnemyMind| {
                        if mind.has_key {
                            NodeResult::Success
                        } else {
                            NodeResult::Failure
                        }
                    }),
                )),
            }),
        )
        .done(goal_done),
        leaf(|_: &mut EnemyMind| NodeResult::Running(EnemyAct::Hold)),
    ))
}

/// Ask the navigation service to lead toward the goal; fail once it says
/// there is no way.
fn go_to(mind: &mut EnemyMind, goal: &Goal) -> NodeResult<EnemyAct> {
    let dest = goal.dest();
    if mind.resolve(dest).is_none() || mind.route_to(dest) == Some(RouteStatus::Unreachable) {
        NodeResult::Failure
    } else {
        NodeResult::Running(EnemyAct::GoTo(dest))
    }
}

fn goal_done(mind: &EnemyMind, goal: &Goal) -> bool {
    match *goal {
        Goal::FollowOrder => false,
        Goal::Reach(cell) => mind.pos == cell,
        Goal::Approach(cell) => mind.adjacent(cell),
        Goal::OpenDoor(door) => mind.adjacent(door) && !mind.closed_door_at(door),
        Goal::GetKey => mind.has_key,
    }
}
