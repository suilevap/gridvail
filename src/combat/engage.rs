use bevy::prelude::*;

use crate::model::*;

/// What a maneuver would do, worked out before anything moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManeuverOutcome {
    pub maneuver: Maneuver,
    /// Where the target goes; `None` when it stays (blocked or immovable).
    pub target_to: Option<IVec2>,
    /// Where the engager goes; `None` when it stays.
    pub engager_to: Option<IVec2>,
    /// Damage to the target.
    pub damage: i32,
    /// What blocked the target's way, if anything did: it takes the slam's
    /// damage too (if it has health). A wall or the map's edge is `None`.
    pub slammed: Option<Option<Entity>>,
}

/// Works out the maneuver `second` makes of a target picked in `picked`:
/// the target moves one cell the way of `second`, unless something other
/// than an engager that follows it in stands there, in which case it stays,
/// is slammed into it, and the engager stays too.
#[allow(clippy::too_many_arguments)]
pub fn plan_maneuver(
    grid: &MapGrid,
    rules: &CombatRules,
    maneuvers: &Maneuvers,
    engager: Entity,
    engager_pos: IVec2,
    picked: IVec2,
    second: IVec2,
    movable: bool,
    exposed: bool,
) -> ManeuverOutcome {
    let maneuver = maneuvers.get(Relation::of(picked, second));
    let target_pos = engager_pos + picked;
    let base = if exposed {
        rules.critical_damage
    } else {
        maneuver.damage
    };
    if !movable {
        return ManeuverOutcome {
            maneuver,
            target_to: None,
            engager_to: None,
            damage: base,
            slammed: None,
        };
    }
    let to = grid.safe_pos(target_pos + second);
    // The engager's own cell is free only if it moves out of it.
    let blocker = grid
        .get(to)
        .filter(|&other| other != engager || !maneuver.follow);
    match blocker {
        None => ManeuverOutcome {
            maneuver,
            target_to: Some(to),
            engager_to: maneuver.follow.then_some(grid.safe_pos(target_pos)),
            damage: base,
            slammed: None,
        },
        Some(blocker) => ManeuverOutcome {
            maneuver,
            target_to: None,
            engager_to: None,
            damage: base + rules.slam_damage,
            slammed: Some(Some(blocker)),
        },
    }
}

/// Picks targets and carries out maneuvers, between the player's input
/// and the moves.
///
/// A move into something `Targetable` picks it (a `Selection`) instead of
/// bumping it, and costs nothing: the command is dropped. The next fresh
/// move (not the input that picked it, held on: see `MoveCommand::repeat`)
/// is the maneuver: it spends
/// the turn (as a wait) and moves the target and the engager directly, a
/// swap included, which ordinary moves cannot do. A selection stays while
/// its target stands where it was picked, so it carries over turns; a
/// cancel drops it.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn engage(
    mut commands: Commands,
    rules: Res<CombatRules>,
    turn: Res<TurnState>,
    mut grid: ResMut<MapGrid>,
    mut cancels: MessageReader<CancelIntent>,
    mut hurt: MessageWriter<Hurt>,
    mut engagers: Query<
        (
            Entity,
            &mut Pos,
            &mut PrevPos,
            &mut MoveCommand,
            &Tokens,
            Option<&Selection>,
        ),
        (With<Engager>, With<Active>, Without<DestroyRequested>),
    >,
    mut targets: Query<
        (
            &mut Pos,
            Option<&mut PrevPos>,
            Option<&Maneuvers>,
            Has<Movable>,
            Option<&Exposed>,
            Has<Speed>,
        ),
        (
            With<Targetable>,
            Without<Engager>,
            Without<DestroyRequested>,
        ),
    >,
    health: Query<(), With<Health>>,
) {
    let cancelled = cancels.read().count() > 0;
    for (engager, mut pos, mut previous, mut command, tokens, selection) in &mut engagers {
        let selection = selection.filter(|selection| {
            let still_there = targets
                .get(selection.target)
                .is_ok_and(|target| target.0 .0 == grid.safe_pos(pos.0 + selection.dir));
            let keep = still_there && !cancelled;
            if !keep {
                commands.entity(engager).remove::<Selection>();
            }
            keep
        });
        if !command.active || !command.relative || turn.simulation || tokens.count <= 0 {
            continue;
        }
        let dir = command.target;
        match selection {
            // Still the input that picked it, held on.
            Some(selection) if dir == selection.dir && command.repeat => {
                command.active = false;
            }
            Some(selection) => {
                let target = selection.target;
                let Ok((_, _, own, movable, exposed, _)) = targets.get(target) else {
                    continue;
                };
                let outcome = plan_maneuver(
                    &grid,
                    &rules,
                    own.unwrap_or(&rules.maneuvers),
                    engager,
                    pos.0,
                    selection.dir,
                    dir,
                    movable,
                    exposed.is_some_and(|exposed| exposed.0),
                );
                let Ok((mut target_pos, target_previous, _, _, _, target_walks)) =
                    targets.get_mut(target)
                else {
                    continue;
                };
                let (from, engager_from) = (target_pos.0, pos.0);
                // Both leave before either arrives, so they can swap.
                if outcome.target_to.is_some() && grid.get(from) == Some(target) {
                    grid.clear(from);
                }
                if outcome.engager_to.is_some() && grid.get(engager_from) == Some(engager) {
                    grid.clear(engager_from);
                }
                if let Some(to) = outcome.target_to {
                    if let Some(mut target_previous) = target_previous {
                        target_previous.0 = from;
                    }
                    target_pos.0 = to;
                    grid.set_with_blocking(to, target, !target_walks);
                }
                if let Some(to) = outcome.engager_to {
                    previous.0 = engager_from;
                    pos.0 = to;
                    grid.set_with_blocking(to, engager, false);
                }
                hurt.write(Hurt {
                    target,
                    amount: outcome.damage,
                    source: Some(engager),
                });
                if let Some(Some(blocker)) = outcome.slammed {
                    if health.contains(blocker) {
                        hurt.write(Hurt {
                            target: blocker,
                            amount: rules.slam_damage,
                            source: Some(engager),
                        });
                    }
                }
                commands.entity(engager).remove::<Selection>();
                // The maneuver is the turn's action: spent as a wait.
                command.target = IVec2::ZERO;
            }
            None => {
                let ahead = grid.safe_pos(pos.0 + dir);
                if let Some(target) = grid.get(ahead).filter(|&other| targets.contains(other)) {
                    commands.entity(engager).insert(Selection { target, dir });
                    command.active = false;
                }
            }
        }
    }
}

/// Backspace or `C` cancels what the player is composing (Esc is the
/// settings menu).
pub fn keyboard_cancel(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut cancel: MessageWriter<CancelIntent>,
) {
    if keys.is_some_and(|keys| keys.any_just_pressed([KeyCode::Backspace, KeyCode::KeyC])) {
        cancel.write(CancelIntent);
    }
}
