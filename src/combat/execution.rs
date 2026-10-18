use crate::model::*;
use bevy::prelude::*;

pub(super) fn clear_execution(mut attacks: Query<(&mut AttackPoints, &mut AttackExecuted)>) {
    for (mut points, mut executed) in &mut attacks {
        points.active = false;
        executed.fired = false;
    }
}

pub(super) fn brace_actions(
    mut actors: Query<(&MoveCommand, &Tokens, Option<&ForcedWait>, &mut Brace), With<Active>>,
) {
    for (command, tokens, forced, mut brace) in &mut actors {
        if command.active && tokens.count > 0 {
            brace.charges =
                u8::from(command.target == IVec2::ZERO && !forced.is_some_and(|forced| forced.0));
        }
    }
}

/// Bumps are an attack producer, not a special player damage implementation.
pub(super) fn emit_bump_points(
    grid: Res<MapGrid>,
    mut actors: Query<(&Pos, &MoveCommand, &Tokens, &BumpAttack, &mut AttackPoints), With<Active>>,
) {
    for (pos, command, tokens, bump, mut points) in &mut actors {
        if command.active && tokens.count > 0 && command.relative && command.target != IVec2::ZERO {
            let mut cells = [None; MAX_ATTACK_CELLS];
            cells[0] = Some(grid.safe_pos(pos.0 + command.target));
            *points = AttackPoints {
                active: true,
                area: AttackArea {
                    origin: pos.0,
                    direction: command.target,
                    cells,
                    damage: bump.damage,
                },
            };
        }
    }
}

/// Attacks use the same movement/token pipeline as every other action.
/// A lunge is optional; any attack shape can be composed with it.
pub(super) fn order_attack_moves(
    turn: Res<TurnState>,
    mode: Res<TurnMode>,
    mut actors: Query<(&AttackCycle, Option<&Lunge>, &Tokens, &mut MoveCommand), With<Active>>,
) {
    if !turn.actors_may_act(*mode) {
        return;
    }
    for (cycle, lunge, tokens, mut command) in &mut actors {
        if tokens.count <= 0 || cycle.phase == AttackPhase::Idle {
            continue;
        }
        *command = MoveCommand {
            target: if cycle.phase == AttackPhase::Ready {
                lunge.map_or(IVec2::ZERO, |lunge| cycle.area.direction * lunge.cells)
            } else {
                IVec2::ZERO
            },
            relative: true,
            active: true,
        };
    }
}

/// Advance only actors whose ordinary token was consumed. Footprints remain
/// locked throughout wind-up; collecting hits happens separately after moves.
pub(super) fn advance_attacks(
    mut actors: Query<
        (
            &ActionTaken,
            &AttackTiming,
            Option<&Health>,
            &mut AttackCycle,
            &mut AttackPoints,
            &mut AttackExecuted,
        ),
        With<Active>,
    >,
) {
    for (action, timing, health, mut cycle, mut points, mut executed) in &mut actors {
        if !action.active
            || health.is_some_and(|health| health.current <= 0)
            || cycle.advanced_at == action.serial
        {
            continue;
        }
        cycle.advanced_at = action.serial;
        match cycle.phase {
            AttackPhase::Windup(left) => {
                cycle.phase = if left > 1 {
                    AttackPhase::Windup(left - 1)
                } else {
                    AttackPhase::Ready
                }
            }
            AttackPhase::Ready => {
                *points = AttackPoints {
                    area: cycle.area,
                    active: true,
                };
                *executed = AttackExecuted {
                    fired: true,
                    direction: cycle.area.direction,
                };
                cycle.spent = cycle.area;
                cycle.phase = if timing.recovery > 0 {
                    AttackPhase::Recovering(timing.recovery)
                } else {
                    AttackPhase::Idle
                };
            }
            AttackPhase::Recovering(left) => {
                cycle.phase = if left > 1 {
                    AttackPhase::Recovering(left - 1)
                } else {
                    AttackPhase::Idle
                }
            }
            AttackPhase::Idle => {}
        }
    }
}
