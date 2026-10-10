use bevy::math::IVec2;
use flatbt_bevy::prelude::*;

use super::{await_future, AwaitFuture};
use crate::model::{EnemyAct, EnemyMind, Mood};
use crate::navigation::Path;
use crate::service::Promise;

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
        3.0 => patrol(),
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
                                repeat_while(EnemyMind::sees_player, hunt()),
                                leaf(|_: &mut EnemyMind| NodeResult::Running(EnemyAct::Hold)),
                            )),
                        ),
                    ),
                    // Walk to the last sighting, then forget it, as also
                    // when there is no way there.
                    seq((force_success(search()), leaf(forget_sighting))),
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

// A walk is three steps over two scope locals: pick a target, ask for a
// path to it (thinking, `EnemyAct::Think`, until it lands), walk the path
// (`EnemyAct::Move`, a step per turn). The target is a local, so anything can
// pick it: what the enemy perceives here, a goal or another service later.

/// Walk at the visible player, until it strays from where the path leads.
fn hunt() -> impl BehaviorNode<EnemyMind, EnemyAct> {
    scope! {
        let target: IVec2;
        let path: Path;
        sequence {
            perceived(|mind| mind.player).with(out target);
            plan_path(Mood::Hunt).with(target, out path);
            follow(Mood::Hunt, player_near_end, u8::MAX).with(path);
        }
    }
}

/// Walk to where the player was last seen.
fn search() -> impl BehaviorNode<EnemyMind, EnemyAct> {
    scope! {
        let target: IVec2;
        let path: Path;
        sequence {
            perceived(|mind| mind.last_seen).with(out target);
            plan_path(Mood::Search).with(target, out path);
            follow(Mood::Search, |_, _| true, u8::MAX).with(path);
        }
    }
}

/// Stroll to a nearby cell, for a few turns at most.
fn patrol() -> impl BehaviorNode<EnemyMind, EnemyAct> {
    scope! {
        let target: IVec2;
        let path: Path;
        sequence {
            perceived(|mind| mind.stroll).with(out target);
            plan_path(Mood::Patrol).with(target, out path);
            follow(Mood::Patrol, |_, _| true, PATROL_TURNS).with(path);
        }
    }
}

/// Pick a target from what the enemy perceives; fails when there is none.
pub(super) fn perceived(
    pick: fn(&EnemyMind) -> Option<IVec2>,
) -> LeafWith<impl Fn(&mut EnemyMind, &mut Option<IVec2>) -> NodeResult<EnemyAct>> {
    leaf_with(move |mind: &mut EnemyMind, target: &mut Option<IVec2>| {
        *target = pick(mind);
        if target.is_some() {
            NodeResult::Success
        } else {
            NodeResult::Failure
        }
    })
}

/// Ask the path service for a path to the target, thinking in `mood` until
/// it lands; fails when there is no way there.
pub(super) fn plan_path(
    mood: Mood,
) -> ActionNode<
    AwaitFuture<
        impl Fn(&mut EnemyMind, &IVec2) -> Option<Promise<Option<Path>>>,
        impl Fn(&EnemyMind) -> EnemyAct,
    >,
> {
    action(await_future(
        |mind: &mut EnemyMind, target: &IVec2| {
            let services = mind.services.as_ref()?;
            Some(services.paths().plan(mind.pos, *target, mind.prefs))
        },
        move |_: &EnemyMind| EnemyAct::Think(mood),
    ))
}

/// Walk the path a step per turn, for at most `turns` turns and while `keep`
/// holds; a failed step (someone in the way) fails it.
pub(super) fn follow(
    mood: Mood,
    keep: fn(&EnemyMind, &Path) -> bool,
    turns: u8,
) -> ActionNode<Follow> {
    action(Follow { mood, keep, turns })
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
pub(super) struct Follow {
    mood: Mood,
    keep: fn(&EnemyMind, &Path) -> bool,
    turns: u8,
}

/// Where the last step was ordered from, and turns left.
pub(super) struct Following {
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
