//! Project attack state into generic glyphs/overlays. This never changes
//! health, targeting, timing, movement or damage.
use crate::lighting::{CYAN, DARK_RED, RED, YELLOW};
use crate::model::*;
use bevy::prelude::*;

pub fn present_attacks(
    mut actors: Query<(
        &AttackCycle,
        &AttackPresentation,
        Option<&Health>,
        Option<&Destination>,
        &mut Glyph,
        &mut CellOverlay,
    )>,
) {
    for (cycle, display, health, destination, mut glyph, mut overlay) in &mut actors {
        if health.is_some_and(|health| health.current <= 0) {
            *overlay = CellOverlay::default();
            continue;
        }
        glyph.ch = if matches!(cycle.phase, AttackPhase::Recovering(_)) {
            display.glyph.to_ascii_lowercase()
        } else {
            display.glyph
        };
        glyph.color = match cycle.phase {
            AttackPhase::Ready => RED,
            AttackPhase::Windup(_) => YELLOW,
            AttackPhase::Recovering(_) => CYAN,
            AttackPhase::Idle => {
                if destination.is_none_or(|destination| destination.goal().is_none()) {
                    DARK_RED
                } else {
                    YELLOW
                }
            }
        };
        let (area, symbol, color, priority) = match cycle.phase {
            AttackPhase::Ready => (cycle.area, '!', RED, 3),
            AttackPhase::Windup(_) => (cycle.area, '?', YELLOW, 2),
            AttackPhase::Recovering(_) => (cycle.spent, '+', CYAN, 1),
            AttackPhase::Idle => (AttackArea::default(), ' ', DARK_RED, 0),
        };
        *overlay = CellOverlay {
            cells: area.cells,
            cell: RenderCell {
                ch: symbol,
                color,
                depth: 0,
            },
            priority,
        };
    }
}

pub(super) fn present_deaths(mut actors: Query<(&Health, Option<&DeathGlyph>, &mut Glyph)>) {
    for (health, death, mut glyph) in &mut actors {
        if health.current <= 0 {
            glyph.color = DARK_RED;
            if let Some(death) = death {
                glyph.ch = death.0;
            }
        }
    }
}

pub(super) fn present_guards(
    guards: Query<(
        &Facing,
        &DirectionalGuard,
        Option<&AttackCycle>,
        Option<&Health>,
    )>,
    mut markers: Query<(&BoundTo, &mut Glyph), With<GuardMarker>>,
) {
    for (bound, mut glyph) in &mut markers {
        if let Ok((facing, guard, cycle, health)) = guards.get(bound.parent) {
            if facing.0 == IVec2::ZERO
                || !guard.window.includes(cycle)
                || health.is_some_and(|health| health.current <= 0)
            {
                glyph.ch = ' ';
            }
        }
    }
}
