//! The hunter: an enemy that follows its `Order`, fetching a key when a
//! locked door is the only way.
//!
//! Built on flatbt's goal stack. The root goal is always `FollowOrder`; a
//! goal asks for what it needs first with `need`, which pushes a subgoal:
//!
//! ```text
//! FollowOrder ─ unreachable, no key ─> Unlock(here) ─> GetKey ─┬─> FetchKey(key 1)
//!                                                              ├─> FetchKey(key 2)
//!                                                              └─> …
//! ```
//!
//! `GetKey` is any key: it asks for the keys it knows of one by one, nearest
//! first, and fails only once every one has. `FetchKey` is one key: it walks
//! to that key and fails if there is no way to it or it is gone. flatbt
//! remembers how each subgoal ended while its asker is on the stack, so a key
//! that failed is not asked for again, and the next one is.
//!
//! `Unlock` carries where the hunter got stuck, so getting stuck again further
//! on (the next door: each door spends a key) is a new goal with a fresh
//! `GetKey` under it, while getting stuck again in the same place, key and
//! all, is not retried.
//!
//! Walking is locomotion's: the tree asks for a walk (`EnemyAct::GoTo`) and
//! reads how it goes from the blackboard. Doors need no goal of their own:
//! once the hunter holds a key, `price_doors` makes doors passable for it,
//! locomotion replans through the door, and walking into it opens it.

use bevy::prelude::{Entity, IVec2};
use flatbt::goal_match;
use flatbt_bevy::prelude::*;

use crate::model::{Dest, EnemyAct, EnemyMind, WalkStatus};

/// What a hunter is working toward.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal {
    /// Go where the order says. The root; never done.
    FollowOrder,
    /// Get past where the hunter got stuck: a key opens the door.
    Unlock(IVec2),
    /// Carry a key, any key.
    GetKey,
    /// Pick up this key.
    FetchKey(Entity),
}

/// Ask for the `slot`th nearest key; succeed once carrying a key. One that
/// fails, the `select` around these moves on to the next.
macro_rules! try_key {
    ($slot:literal) => {
        seq((
            need(|mind: &EnemyMind, _: &Goal| mind.keys[$slot].map(|(key, _)| Goal::FetchKey(key))),
            leaf(|mind: &mut EnemyMind| {
                if mind.has_key {
                    NodeResult::Success
                } else {
                    NodeResult::Failure
                }
            }),
        ))
    };
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
        goals::<12, _, _>(
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
                        (stuck && !mind.has_key && mind.keys[0].is_some())
                            .then_some(Goal::Unlock(mind.pos))
                    })),
                    leaf(follow_order),
                )),
                Goal::Unlock(_) => need(|_: &EnemyMind, _: &Goal| Some(Goal::GetKey)),
                // Fails only when every key it knows of has failed.
                Goal::GetKey => select((try_key!(0), try_key!(1), try_key!(2), try_key!(3))),
                Goal::FetchKey(_) => with_goal(leaf_with(fetch_key)),
            }),
        )
        .done(goal_done),
        leaf(|_: &mut EnemyMind| NodeResult::Running(EnemyAct::Hold)),
    ))
}

/// Walk to the order; locomotion replans as it moves or doors get cheaper.
fn follow_order(mind: &mut EnemyMind) -> NodeResult<EnemyAct> {
    match mind.order {
        Some(_) => NodeResult::Running(EnemyAct::GoTo(Dest::Order)),
        None => NodeResult::Failure,
    }
}

/// Walk to the key; fail once it is gone or there is no way to it.
fn fetch_key(mind: &mut EnemyMind, goal: &Goal) -> NodeResult<EnemyAct> {
    let Goal::FetchKey(key) = *goal else {
        return NodeResult::Failure;
    };
    match mind.key_at(key) {
        Some(cell) if !mind.failed_to_reach(cell) => {
            NodeResult::Running(EnemyAct::GoTo(Dest::Cell(cell)))
        }
        _ => NodeResult::Failure,
    }
}

fn goal_done(mind: &EnemyMind, goal: &Goal) -> bool {
    match *goal {
        Goal::FollowOrder => false,
        Goal::Unlock(_) | Goal::GetKey | Goal::FetchKey(_) => mind.has_key,
    }
}
