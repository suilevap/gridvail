use bevy::math::IVec2;
use flatbt_bevy::prelude::*;

use crate::model::{EnemyAct, EnemyMind};

/// Hunt the player in sight, else search where it was last seen, else wander.
///
/// Evaluated from the root on every turn the enemy has, so a higher branch
/// preempts a lower one as soon as its guard holds again.
pub fn enemy_tree() -> impl BehaviorNode<EnemyMind, EnemyAct> {
    select((
        guard(
            |mind: &EnemyMind| mind.player.is_some(),
            leaf(|mind: &mut EnemyMind| approach(mind, mind.player, EnemyAct::Hunt)),
        ),
        guard(
            |mind: &EnemyMind| mind.last_seen.is_some(),
            leaf(|mind: &mut EnemyMind| approach(mind, mind.last_seen, EnemyAct::Search)),
        ),
        leaf(|mind: &mut EnemyMind| NodeResult::Running(EnemyAct::Wander(mind.roll))),
    ))
}

/// Enter the tree only on a turn the enemy can act in.
///
/// Out of turn it is skipped, so its standing act is kept rather than
/// re-decided on frames that cannot spend it.
pub fn enemy_tick(mind: &EnemyMind, _: TickAt) -> Tick {
    if mind.has_turn {
        Tick::Evaluate
    } else {
        Tick::Skip
    }
}

/// Step toward `target`, failing when there is none or the way is walled off.
fn approach(
    mind: &EnemyMind,
    target: Option<IVec2>,
    act: fn(IVec2) -> EnemyAct,
) -> NodeResult<EnemyAct> {
    match target.and_then(|target| mind.step_toward(target)) {
        Some(step) => NodeResult::Running(act(step)),
        None => NodeResult::Failure,
    }
}
