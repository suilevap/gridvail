use crate::model::*;
use bevy::prelude::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct Hit {
    target: Entity,
    from: IVec2,
    direction: IVec2,
    damage: i32,
}
#[derive(Resource, Default)]
pub(super) struct DamageQueue(Vec<Hit>);

pub(super) fn reserve_hits(grid: Res<MapGrid>, mut hits: ResMut<DamageQueue>) {
    let needed = (grid.width * grid.height) as usize * MAX_ATTACK_CELLS;
    if hits.0.capacity() < needed {
        let additional = needed - hits.0.len();
        hits.0.reserve(additional);
    }
}

/// Geometry producers know nothing about health/defences. This is the common
/// bridge from occupied attack cells to damage requests (including friendly fire).
pub(super) fn collect_hits(
    grid: Res<MapGrid>,
    mut hits: ResMut<DamageQueue>,
    mut attacks: Query<(Entity, Option<&Team>, &mut AttackPoints)>,
    targets: Query<Option<&Team>, With<Health>>,
) {
    hits.0.clear();
    for (source, team, mut points) in &mut attacks {
        if !points.active {
            continue;
        }
        points.active = false;
        for cell in points.area.cells.into_iter().flatten() {
            let Some(target) = grid.get(cell) else {
                continue;
            };
            if source == target {
                continue;
            }
            let Ok(other_team) = targets.get(target) else {
                continue;
            };
            if team.zip(other_team).is_some_and(|(a, b)| a == b) {
                continue;
            }
            hits.0.push(Hit {
                target,
                from: points.area.origin,
                direction: points.area.direction,
                damage: points.area.damage,
            });
        }
    }
}

pub(super) fn exploit_openings(
    mut hits: ResMut<DamageQueue>,
    targets: Query<(&Vulnerability, Option<&AttackCycle>)>,
) {
    for hit in &mut hits.0 {
        if let Ok((weakness, cycle)) = targets.get(hit.target) {
            if weakness.window.includes(cycle) {
                hit.damage += weakness.bonus;
            }
        }
    }
}
pub(super) fn block_guarded_hits(
    mut hits: ResMut<DamageQueue>,
    targets: Query<(&Pos, &Facing, &DirectionalGuard, Option<&AttackCycle>)>,
) {
    for hit in &mut hits.0 {
        if let Ok((pos, facing, guard, cycle)) = targets.get(hit.target) {
            // A lane/bump carries its travel direction, including a wrapped
            // step. Radial attacks derive it from their locked centre.
            let incoming = if hit.direction == IVec2::ZERO {
                (hit.from - pos.0).signum()
            } else {
                -hit.direction.signum()
            };
            if guard.window.includes(cycle)
                && incoming == facing.0.signum()
                && facing.0 != IVec2::ZERO
            {
                hit.damage = 0;
            }
        }
    }
}
pub(super) fn reduce_armoured_hits(
    mut hits: ResMut<DamageQueue>,
    targets: Query<(&Armour, Option<&AttackCycle>)>,
) {
    for hit in &mut hits.0 {
        if let Ok((armour, cycle)) = targets.get(hit.target) {
            if armour.window.includes(cycle) {
                hit.damage = (hit.damage - armour.reduction).max(0);
            }
        }
    }
}
pub(super) fn absorb_braces(mut hits: ResMut<DamageQueue>, mut targets: Query<&mut Brace>) {
    for hit in &mut hits.0 {
        if hit.damage <= 0 {
            continue;
        }
        if let Ok(mut brace) = targets.get_mut(hit.target) {
            if brace.charges > 0 {
                hit.damage = (hit.damage - brace.reduction).max(0);
                brace.charges -= 1;
            }
        }
    }
}
pub(super) fn apply_damage(hits: Res<DamageQueue>, mut targets: Query<&mut Health>) {
    for hit in &hits.0 {
        if let Ok(mut health) = targets.get_mut(hit.target) {
            health.current = (health.current - hit.damage).max(0);
        }
    }
}
pub(super) fn interrupt_attacks(
    hits: Res<DamageQueue>,
    mut targets: Query<(
        &InterruptOnHit,
        Option<&InterruptResistance>,
        &ActionTaken,
        &mut AttackCycle,
    )>,
) {
    for hit in &hits.0 {
        if hit.damage <= 0 {
            continue;
        }
        if let Ok((interruption, resistance, action, mut cycle)) = targets.get_mut(hit.target) {
            if matches!(cycle.phase, AttackPhase::Recovering(_))
                || resistance.is_some_and(|resistance| resistance.window.includes(Some(&cycle)))
            {
                continue;
            }
            cycle.phase = AttackPhase::Recovering(interruption.recovery);
            cycle.spent = AttackArea::default();
            // Bump damage happens before this token is consumed.
            cycle.advanced_at = action.serial + u64::from(!action.active);
        }
    }
}

/// Both attack producers use the same ordered defence/damage pipeline.
/// Register a new modifier once here rather than once per attack source.
pub(super) fn pipeline() -> bevy::ecs::schedule::ScheduleConfigs<bevy::ecs::system::ScheduleSystem>
{
    (
        collect_hits,
        exploit_openings,
        block_guarded_hits,
        reduce_armoured_hits,
        absorb_braces,
        apply_damage,
        interrupt_attacks,
    )
        .chain()
}
