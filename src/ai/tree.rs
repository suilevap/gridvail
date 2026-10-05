use bevy::math::IVec2;
use flatbt_bevy::prelude::*;

use crate::model::{EnemyAct, EnemyMind, WalkStatus};

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
                            leaf(attack),
                            select((
                                leaf(hunt),
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

/// Bump into the adjacent player.
fn attack(mind: &mut EnemyMind) -> NodeResult<EnemyAct> {
    match mind.player {
        Some(player) => NodeResult::Running(EnemyAct::Attack(player - mind.pos)),
        None => NodeResult::Failure,
    }
}

/// Walk to the visible player, until locomotion finds no way there.
fn hunt(mind: &mut EnemyMind) -> NodeResult<EnemyAct> {
    match mind.player {
        Some(player) if mind.walk_to(player) != Some(WalkStatus::Unreachable) => {
            NodeResult::Running(EnemyAct::Hunt(player))
        }
        _ => NodeResult::Failure,
    }
}

/// Walk to the last sighting; give up, forgetting it, once locomotion finds
/// no way there or keeps getting blocked.
fn search(mind: &mut EnemyMind) -> NodeResult<EnemyAct> {
    match mind.last_seen {
        Some(seen) if !mind.failed_to_reach(seen) => NodeResult::Running(EnemyAct::Search(seen)),
        _ => {
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

/// Stroll to a nearby floor cell, for at most `PATROL_TURNS` turns.
struct Patrol;

const PATROL_TURNS: u8 = 10;

impl BtAction<EnemyMind, EnemyAct> for Patrol {
    /// The cell, and turns left.
    type State = (IVec2, u8);

    fn start(&self, mind: &mut EnemyMind, _: ()) -> Option<(IVec2, u8)> {
        mind.stroll.map(|cell| (cell, PATROL_TURNS))
    }

    fn is_in_progress(&self, &(cell, left): &(IVec2, u8), mind: &EnemyMind, _: ()) -> bool {
        left > 0 && mind.walk_to(cell).is_none_or(|status| !status.is_done())
    }

    fn tick(&self, (cell, left): &mut (IVec2, u8), _: &mut EnemyMind, _: ()) -> EnemyAct {
        *left -= 1;
        EnemyAct::Patrol(*cell)
    }
}
