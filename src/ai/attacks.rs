//! Attack decisions publish footprints; pursuit only writes destinations.
//! Geometry and capabilities compose independently of map archetypes.
use super::perception::target_visible;
use crate::model::*;
use crate::schedule::GamePhase;
use bevy::prelude::*;

pub(super) fn install(app: &mut App) {
    app.add_systems(
        Update,
        (
            perceive_attack_targets,
            plan_neighbour_attacks,
            plan_line_attacks,
            aim_attacks,
            chase_targets,
        )
            .chain()
            .in_set(GamePhase::Planning),
    );
}

fn lock(cycle: &mut AttackCycle, timing: &AttackTiming, area: AttackArea) {
    cycle.area = area;
    cycle.phase = if timing.windup == 0 {
        AttackPhase::Ready
    } else {
        AttackPhase::Windup(timing.windup)
    };
}

fn plan_line_attacks(
    grid: Res<MapGrid>,
    mut actors: Query<
        (
            &Pos,
            &LineAttack,
            &AttackPower,
            &AttackTiming,
            &AttackTarget,
            &mut AttackCycle,
        ),
        With<Active>,
    >,
) {
    for (pos, line, power, timing, target, mut cycle) in &mut actors {
        let Some(target) = target.0 else {
            continue;
        };
        if cycle.phase != AttackPhase::Idle {
            continue;
        }
        let delta = target - pos.0;
        let distance = delta.abs().element_sum();
        let reach = i32::from(line.reach).min(MAX_ATTACK_CELLS as i32);
        if (delta.x != 0 && delta.y != 0) || distance == 0 || distance > reach {
            continue;
        }
        let direction = delta.signum();
        let mut area = AttackArea {
            origin: pos.0,
            direction,
            damage: power.0,
            ..default()
        };
        for (cell, distance) in area.cells.iter_mut().zip(1..=reach) {
            let point = pos.0 + direction * distance;
            if !grid.is_valid(point) || grid.blocks_vision(point) {
                break;
            }
            *cell = Some(point);
        }
        lock(&mut cycle, timing, area);
    }
}

fn plan_neighbour_attacks(
    mut actors: Query<
        (
            &Pos,
            &AttackPower,
            &AttackTiming,
            &AttackTarget,
            &mut AttackCycle,
        ),
        (With<NeighbourAttack>, With<Active>),
    >,
) {
    for (pos, power, timing, target, mut cycle) in &mut actors {
        let Some(target) = target.0 else {
            continue;
        };
        if cycle.phase != AttackPhase::Idle || (target - pos.0).abs().max_element() != 1 {
            continue;
        }
        let mut area = AttackArea {
            origin: pos.0,
            damage: power.0,
            ..default()
        };
        let mut index = 0;
        for y in -1..=1 {
            for x in -1..=1 {
                if x != 0 || y != 0 {
                    area.cells[index] = Some(pos.0 + IVec2::new(x, y));
                    index += 1;
                }
            }
        }
        lock(&mut cycle, timing, area);
    }
}

fn aim_attacks(mut actors: Query<(&AttackCycle, Has<AimAtAttack>, &mut Facing)>) {
    for (cycle, aim, mut facing) in &mut actors {
        if cycle.phase != AttackPhase::Idle
            && cycle.area.direction != IVec2::ZERO
            && (aim || facing.0 == IVec2::ZERO)
        {
            facing.0 = cycle.area.direction;
        }
    }
}

fn chase_targets(
    mut actors: Query<(&AttackCycle, &AttackTarget, &PathFollow, &mut Destination), With<Active>>,
) {
    for (cycle, target, follow, mut destination) in &mut actors {
        let target = target.0.filter(|_| cycle.phase == AttackPhase::Idle);
        match target {
            Some(target) if destination.goal() != Some(target) || follow.status.is_done() => {
                destination.go_to(target)
            }
            None if destination.goal().is_some() => destination.stop(),
            _ => {}
        }
    }
}

/// Target selection is independent of attack shape and movement. Other
/// controllers can write AttackTarget without installing TrackPlayer.
fn perceive_attack_targets(
    grid: Res<MapGrid>,
    players: Query<&Pos, (With<Player>, With<Active>)>,
    mut actors: Query<
        (&Pos, Option<&FovResult>, &mut AttackTarget),
        (With<TrackPlayer>, With<Active>),
    >,
) {
    let player = players.single().ok().map(|player| player.0);
    for (pos, sight, mut target) in &mut actors {
        target.0 = player.filter(|&player| target_visible(&grid, pos.0, sight, player));
    }
}
