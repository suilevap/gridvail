//! The hunter: an enemy that follows its `Order`, fetching a key when a
//! locked door is the only way.
//!
//! Built on flatbt's goal stack. The root goal is always `FollowOrder`; a
//! goal asks for what it needs first with `need`, which pushes a subgoal:
//!
//! ```text
//! FollowOrder ─ no way there, no key ─> Unlock(here) ─> GetKey ─┬─> FetchKey(key 1)
//!                                                               ├─> FetchKey(key 2)
//!                                                               └─> …
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
//! "No way there" is the answer of the path task itself: a walk only fails
//! when a plan lands with no path (see `tree::pursue`). Doors need no goal of
//! their own: once the hunter holds a key, `price_doors` makes doors
//! passable for it, the next plan goes through the door, and walking into it
//! opens it.

use bevy::prelude::{Entity, IVec2};
use flatbt::goal_match;
use flatbt_bevy::prelude::*;

use super::tree::{attack, hold, pursue};
use crate::model::{EnemyAct, EnemyMind, Mood};
use crate::navigation::Path;

/// How far the order may move from where a walk to it leads before the walk
/// is planned again.
const NEAR_ORDER: i32 = 2;

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

/// Attack a player in reach; otherwise work through the goal stack; hold
/// when there is nothing to do.
pub fn hunter_tree() -> impl BehaviorNode<EnemyMind, EnemyAct> {
    select((
        guard(EnemyMind::next_to_player, leaf(attack)),
        goals::<12, _, _>(
            |_: &EnemyMind| Goal::FollowOrder,
            goal_match!(|goal: &Goal| {
                // Walk to the order. With no way there, get a key first, if
                // there is one; either way, wait for a way to open up. "No
                // way" means something only once the map is known.
                Goal::FollowOrder => select((
                    no_params(pursue(Mood::Order, |mind| mind.order, order_near_end)),
                    force_failure(need(|mind: &EnemyMind, _: &Goal| {
                        let knows_map = mind
                            .services
                            .as_ref()
                            .is_some_and(|services| services.paths().knows_map());
                        (knows_map && !mind.has_key && mind.keys[0].is_some())
                            .then_some(Goal::Unlock(mind.pos))
                    })),
                    leaf(hold),
                )),
                Goal::Unlock(_) => need(|_: &EnemyMind, _: &Goal| Some(Goal::GetKey)),
                // Each key in turn, nearest first, until one is carried;
                // fails only when every key it knows of has failed.
                Goal::GetKey => select((
                    seq((need(fetch_key_in::<0>), leaf(carrying_key))),
                    seq((need(fetch_key_in::<1>), leaf(carrying_key))),
                    seq((need(fetch_key_in::<2>), leaf(carrying_key))),
                    seq((need(fetch_key_in::<3>), leaf(carrying_key))),
                )),
                // Aim at the key while it lies on the map, and walk there.
                Goal::FetchKey(_) => seq((
                    with_goal(leaf_with(aim_at_key)),
                    no_params(pursue(Mood::Fetch, |mind| mind.target, |_, _| true)),
                )),
            }),
        )
        .done(goal_done),
        leaf(hold),
    ))
}

/// Still worth walking: the order stands near where the path leads.
fn order_near_end(mind: &EnemyMind, path: &Path) -> bool {
    match (mind.order, path.end()) {
        (Some(order), Some(end)) => (order - end).abs().element_sum() <= NEAR_ORDER,
        _ => false,
    }
}

/// The `SLOT`th nearest key to fetch, if one is known there.
fn fetch_key_in<const SLOT: usize>(mind: &EnemyMind, _: &Goal) -> Option<Goal> {
    mind.keys[SLOT].map(|(key, _)| Goal::FetchKey(key))
}

fn carrying_key(mind: &mut EnemyMind) -> NodeResult<EnemyAct> {
    if mind.has_key {
        NodeResult::Success
    } else {
        NodeResult::Failure
    }
}

/// Make the key the walk's target; fail once it is gone from the map.
fn aim_at_key(mind: &mut EnemyMind, goal: &Goal) -> NodeResult<EnemyAct> {
    let Goal::FetchKey(key) = *goal else {
        return NodeResult::Failure;
    };
    mind.target = mind.key_at(key);
    if mind.target.is_some() {
        NodeResult::Success
    } else {
        NodeResult::Failure
    }
}

fn goal_done(mind: &EnemyMind, goal: &Goal) -> bool {
    match *goal {
        Goal::FollowOrder => false,
        Goal::Unlock(_) | Goal::GetKey | Goal::FetchKey(_) => mind.has_key,
    }
}
