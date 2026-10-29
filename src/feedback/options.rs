use bevy::prelude::*;

use crate::combat::{preview, Preview, TargetState};
use crate::lighting::{CYAN, GREEN, RED};
use crate::model::*;

use super::mark;

/// While a target is picked, shows what each second direction would do, on
/// the cell that direction points at from the target (where the move sends
/// it; for a swap, the selector's own cell): a dot on free floor, else
/// whatever is there tinted, coloured by the outcome. Green: the target
/// would go down. Red: the selector would stand where a wound-up strike
/// lands. Cyan: neither.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn mark_options(
    grid: Res<MapGrid>,
    rules: (Res<CriticalHit>, Res<ImpactDamage>),
    marks: Option<ResMut<CellMarks>>,
    selectors: Query<(&Pos, &Selected, &Maneuvers)>,
    targets: Query<(&Pos, Has<Movable>, Option<&Exposed>, Option<&Health>)>,
    wielders: Query<(Entity, &Pos, &Weapon, &Telegraph), Without<DestroyRequested>>,
) {
    let Some(mut marks) = marks else {
        return;
    };
    for (actor, selected, maneuvers) in &selectors {
        let Ok((target_pos, movable, exposed, health)) = targets.get(selected.target) else {
            continue;
        };
        let target = TargetState {
            pos: target_pos.0,
            movable,
            exposed: exposed.is_some_and(|exposed| exposed.0),
        };
        let picked = selected.dir;
        let side = IVec2::new(-picked.y, picked.x);
        for dir in [picked, side, -side, -picked] {
            let maneuver = maneuvers.get(Relation::of(picked, dir));
            let outcome = preview(
                &grid,
                maneuver,
                actor.0,
                target,
                picked,
                dir,
                (&rules.0, &rules.1),
            );
            let kills = health.is_some_and(|health| outcome.damage >= health.hp);
            let struck = struck_after(&outcome, selected.target, kills, &wielders);
            let tint = if struck {
                RED
            } else if kills {
                GREEN
            } else {
                CYAN
            };
            marks.0.push((outcome.cell, mark('•', tint)));
        }
    }
}

/// Whether the selector would stand where a wound-up strike lands once the
/// move is made: the target's own strike counted from where it would be
/// (none if it would go down), every other one from where it is.
fn struck_after(
    outcome: &Preview,
    target: Entity,
    kills: bool,
    wielders: &Query<(Entity, &Pos, &Weapon, &Telegraph), Without<DestroyRequested>>,
) -> bool {
    wielders.iter().any(|(wielder, pos, weapon, telegraph)| {
        let from = if wielder == target {
            if kills {
                return false;
            }
            outcome.target_to
        } else {
            pos.0
        };
        weapon
            .area
            .cells(from, telegraph.dir)
            .any(|cell| cell == outcome.actor_to)
    })
}
