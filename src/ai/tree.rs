use bevy::math::IVec2;
use flatbt_bevy::prelude::*;

use crate::model::{EnemyAct, EnemyMind, STEPS};

/// Pursue the player once aware of it, else pass the time.
///
/// Evaluated from the root on every turn the enemy has. The pursuit `seq`
/// keeps its place while the guard holds, so the alert beat plays once per
/// pursuit, not on every re-sighting.
pub fn enemy_tree() -> impl BehaviorNode<EnemyMind, EnemyAct> {
    let (weight, pastimes) = per_child!(|_mind: &EnemyMind| {
        3.0 => action(Patrol),
        1.0 => action(Pause { turns: 2, act: EnemyAct::Rest }),
    });
    select((
        guard(
            EnemyMind::aware,
            seq((
                action(Pause {
                    turns: 1,
                    act: EnemyAct::Alert,
                }),
                select((
                    guard(
                        EnemyMind::sees_player,
                        if_else(
                            EnemyMind::next_to_player,
                            leaf(|mind: &mut EnemyMind| approach(mind, EnemyAct::Attack)),
                            select((
                                leaf(|mind: &mut EnemyMind| approach(mind, EnemyAct::Hunt)),
                                leaf(|_: &mut EnemyMind| NodeResult::Running(EnemyAct::Hold)),
                            )),
                        ),
                    ),
                    leaf(search),
                )),
            )),
        ),
        // A new pastime as soon as one ends, so an idle enemy is never without
        // an act.
        repeat_while(
            |_: &EnemyMind| true,
            weighted_select(EnemyMind::next_random, weight, pastimes),
        ),
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

/// Step toward the visible player, failing when no step closes in.
fn approach(mind: &mut EnemyMind, act: fn(IVec2) -> EnemyAct) -> NodeResult<EnemyAct> {
    match mind.player.and_then(|player| mind.step_toward(player)) {
        Some(step) => NodeResult::Running(act(step)),
        None => NodeResult::Failure,
    }
}

/// Head for the last sighting; give up, forgetting it, when walled off.
fn search(mind: &mut EnemyMind) -> NodeResult<EnemyAct> {
    match mind.last_seen.and_then(|seen| mind.step_toward(seen)) {
        Some(step) => NodeResult::Running(EnemyAct::Search(step)),
        None => {
            mind.last_seen = None;
            NodeResult::Failure
        }
    }
}

/// Report `act` for `turns` turns, then succeed.
struct Pause {
    turns: u8,
    act: EnemyAct,
}

impl BtAction<EnemyMind, EnemyAct> for Pause {
    type State = u8;

    fn start(&self, _: &mut EnemyMind, _: ()) -> Option<u8> {
        Some(self.turns)
    }

    fn is_in_progress(&self, left: &u8, _: &EnemyMind, _: ()) -> bool {
        *left > 0
    }

    fn tick(&self, left: &mut u8, _: &mut EnemyMind, _: ()) -> EnemyAct {
        *left -= 1;
        self.act
    }
}

/// Walk 2 to 5 cells in one random open direction, stopping early at a wall.
struct Patrol;

impl BtAction<EnemyMind, EnemyAct> for Patrol {
    /// Direction and cells left.
    type State = (IVec2, u8);

    fn start(&self, mind: &mut EnemyMind, _: ()) -> Option<(IVec2, u8)> {
        let open = STEPS.iter().filter(|&&step| mind.is_open(step)).count() as u32;
        if open == 0 {
            return None;
        }
        let pick = mind.next_random() % open;
        let step = *STEPS
            .iter()
            .filter(|&&step| mind.is_open(step))
            .nth(pick as usize)?;
        Some((step, 2 + (mind.next_random() % 4) as u8))
    }

    fn is_in_progress(&self, &(step, left): &(IVec2, u8), mind: &EnemyMind, _: ()) -> bool {
        left > 0 && mind.is_open(step)
    }

    fn tick(&self, (step, left): &mut (IVec2, u8), _: &mut EnemyMind, _: ()) -> EnemyAct {
        *left -= 1;
        EnemyAct::Patrol(*step)
    }
}
