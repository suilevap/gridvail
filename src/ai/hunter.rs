//! The hunter: an enemy that follows its `Order`, fetching a key when a
//! locked door is the only way.
//!
//! Built on flatbt's goal stack. The root goal is always `FollowOrder`; a
//! goal asks for what it needs first with `need`, which pushes a subgoal:
//!
//! ```text
//! FollowOrder ─ unreachable, no key ─> GetKey ─ the nearest key ─> Reach(key)
//! ```
//!
//! Walking is locomotion's: the tree asks for a walk (`EnemyAct::GoTo`) and
//! reads how it goes from the blackboard. Doors need no goal of their own:
//! once the hunter holds a key, `price_doors` makes doors passable for it,
//! locomotion replans through the door, and walking into it opens it.

use flatbt::goal_match;
use flatbt_bevy::prelude::*;

use crate::model::{Dest, EnemyAct, EnemyMind, WalkStatus};

/// What a hunter is working toward.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal {
    /// Go where the order says. The root; never done.
    FollowOrder,
    /// Stand on a cell.
    Reach(bevy::math::IVec2),
    /// Carry a key.
    GetKey,
}

impl Goal {
    /// Where a walking goal leads.
    fn dest(self) -> Dest {
        match self {
            Goal::Reach(cell) => Dest::Cell(cell),
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
        goals::<4, _, _>(
            |_: &EnemyMind| Goal::FollowOrder,
            goal_match!(|goal: &Goal| {
                // No way to the order without opening a door: get a key
                // first, if there is one. Either way, keep walking: with a
                // key the walk goes through the door, without one the hunter
                // waits for a way to open up.
                Goal::FollowOrder => select((
                    force_failure(need(|mind: &EnemyMind, _: &Goal| {
                        let order = mind.order?;
                        let stuck = mind.walk_to(order) == Some(WalkStatus::Unreachable);
                        (stuck && !mind.has_key && mind.nearest_key.is_some())
                            .then_some(Goal::GetKey)
                    })),
                    with_goal(leaf_with(go_to)),
                )),
                Goal::Reach(_) => with_goal(leaf_with(go_to)),
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

/// Ask locomotion to walk toward the goal. A fixed cell it found no way to
/// fails the goal; a moving order keeps being asked for, since locomotion
/// replans when the order moves or doors get cheaper.
fn go_to(mind: &mut EnemyMind, goal: &Goal) -> NodeResult<EnemyAct> {
    let dest = goal.dest();
    let Some(cell) = mind.resolve(dest) else {
        return NodeResult::Failure;
    };
    if dest != Dest::Order && mind.failed_to_reach(cell) {
        NodeResult::Failure
    } else {
        NodeResult::Running(EnemyAct::GoTo(dest))
    }
}

fn goal_done(mind: &EnemyMind, goal: &Goal) -> bool {
    match *goal {
        Goal::FollowOrder => false,
        Goal::Reach(cell) => mind.pos == cell,
        Goal::GetKey => mind.has_key,
    }
}
