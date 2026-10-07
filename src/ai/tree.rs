use bevy::math::IVec2;
use flatbt_bevy::prelude::*;

use super::await_task;
use crate::model::{EnemyAct, EnemyMind, Mood};
use crate::navigation::Path;

/// Turns an idle stroll lasts at most.
const PATROL_TURNS: u8 = 10;

/// How far the player may stray from where a hunt's path leads before the
/// path is planned again. Until then the enemy keeps walking it, so a plan
/// that takes a while never leaves it standing.
const NEAR_GOAL: i32 = 2;

/// Pursue the player once aware of it, else pass the time.
///
/// Evaluated from the root on every turn the enemy has. The pursuit `seq`
/// keeps its place while the guard holds, so the alert beat plays once per
/// pursuit, not on every re-sighting.
pub fn enemy_tree() -> impl BehaviorNode<EnemyMind, EnemyAct> {
    let (weight, pastimes) = per_child!(|_mind: &EnemyMind| {
        3.0 => walk(Mood::Patrol, |mind| mind.stroll, |_, _| true, PATROL_TURNS),
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
                                // A fresh path whenever the player strays from
                                // where the last one leads.
                                repeat_while(
                                    EnemyMind::sees_player,
                                    walk(Mood::Hunt, |mind| mind.player, player_near_end, u8::MAX),
                                ),
                                leaf(|_: &mut EnemyMind| NodeResult::Running(EnemyAct::Hold)),
                            )),
                        ),
                    ),
                    // Walk to the last sighting, then forget it, as also
                    // when there is no way there.
                    seq((
                        force_success(walk(
                            Mood::Search,
                            |mind| mind.last_seen,
                            |_, _| true,
                            u8::MAX,
                        )),
                        leaf(forget_sighting),
                    )),
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

/// Walk to `goal` in `mood`: ask for a path, wait for it, walk it.
///
/// While the path is on its way the enemy thinks (`EnemyAct::Think`), keeping
/// its turn open; once it lands the enemy walks it (`EnemyAct::Move`), a step
/// per turn, for at most `turns` turns and while `keep` holds. A failed step
/// (someone in the way) or no way at all fails the walk, so whoever asked can
/// plan again or give up.
fn walk(
    mood: Mood,
    goal: fn(&EnemyMind) -> Option<IVec2>,
    keep: fn(&EnemyMind, &Path) -> bool,
    turns: u8,
) -> impl BehaviorNode<EnemyMind, EnemyAct> {
    let plan = action(await_task(
        move |mind: &mut EnemyMind| {
            let goal = goal(mind)?;
            Some(mind.paths.as_ref()?.plan(mind.pos, goal, mind.prefs))
        },
        move |_: &EnemyMind| EnemyAct::Think(mood),
    ));
    scope! {
        let path: Path;
        sequence {
            plan.with(out path);
            action(Follow { mood, keep, turns }).with(path);
        }
    }
}

/// Still worth walking toward the player: it stands near the path's end.
fn player_near_end(mind: &EnemyMind, path: &Path) -> bool {
    match (mind.player, path.end()) {
        (Some(player), Some(end)) => (player - end).abs().element_sum() <= NEAR_GOAL,
        _ => false,
    }
}

/// Enter the tree only on a turn the enemy can act in.
///
/// Out of turn it is skipped, so its standing act is kept rather than
/// re-decided on frames that cannot spend it. A thinking enemy still has its
/// turn, so it is entered again each frame until its path lands.
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

/// Forget the last sighting; fails, so the pursuit ends.
fn forget_sighting(mind: &mut EnemyMind) -> NodeResult<EnemyAct> {
    mind.last_seen = None;
    NodeResult::Failure
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
        self.act.clone()
    }
}

/// Walk a path a step per turn.
struct Follow {
    mood: Mood,
    keep: fn(&EnemyMind, &Path) -> bool,
    turns: u8,
}

/// Where the last step was ordered from, and turns left.
struct Following {
    from: Option<IVec2>,
    left: u8,
}

impl<'a> BtAction<EnemyMind, EnemyAct, &'a Path> for Follow {
    type State = Following;

    fn start(&self, mind: &mut EnemyMind, path: &'a Path) -> Option<Following> {
        path.contains(mind.pos).then_some(Following {
            from: None,
            left: self.turns,
        })
    }

    fn is_in_progress(&self, following: &Following, mind: &EnemyMind, path: &'a Path) -> bool {
        following.from != Some(mind.pos)
            && following.left > 0
            && path.next_after(mind.pos).is_some()
            && (self.keep)(mind, path)
    }

    fn tick(&self, following: &mut Following, mind: &mut EnemyMind, path: &'a Path) -> EnemyAct {
        following.from = Some(mind.pos);
        following.left -= 1;
        EnemyAct::Move(self.mood, path.clone())
    }

    /// Done well unless the last step failed or left the path.
    fn complete(&self, following: &mut Following, mind: &mut EnemyMind, path: &'a Path) -> bool {
        following.from != Some(mind.pos) && path.contains(mind.pos)
    }
}
