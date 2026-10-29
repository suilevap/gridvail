//! Feedback: what the player sees of what happens in a fight, apart from
//! the fight itself. Gameplay only exposes state and messages (`Selected`,
//! `Telegraph`, `ContextAction`, `Strike`, `Hit`, `Damage`); these systems
//! turn them into gestures (`Gesture`, played by the animation step) and
//! cell marks (`CellMarks`, drawn by the frame composer), so how a fight
//! looks can change without touching how it works.
//!
//! - picking: the selector leans toward what it picks, which pulses;
//! - options: while picked, the cell each second direction would send the
//!   target to is marked, coloured by the outcome;
//! - attacks: an attacker swings at its target, a weapon's wielder the way
//!   it strikes, and an aimed weapon's cells are marked;
//! - hits: the target recoils and flashes red and white, whatever took
//!   damage shows its hearts above it for a moment, and a death leaves a
//!   burst.

mod attacks;
mod hits;
mod options;
mod selection;

pub use attacks::*;
pub use hits::*;
pub use options::*;
pub use selection::*;

use bevy::prelude::*;

use crate::model::*;
use crate::schedule::{ActionStage, GamePhase, MarkCells};
use crate::simulation::move_commands;

pub struct FeedbackPlugin;

impl Plugin for FeedbackPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Gesture>()
            .add_systems(
                Update,
                (
                    lean_on_pick,
                    swing_on_actions,
                    swing_on_strikes,
                    recoil_on_hits,
                    show_health_on_damage,
                    wear_off,
                )
                    .before(MarkCells)
                    .in_set(GamePhase::Visibility),
            )
            // While the dead still have a place: they are removed after the
            // turn's moves.
            .add_systems(
                Update,
                burst_on_death
                    .after(ActionStage::Damage)
                    .before(move_commands)
                    .in_set(GamePhase::Simulation),
            )
            .add_systems(
                Update,
                // Later marks of a cell win: options over strikes, then
                // hearts, and a flash over everything.
                (
                    mark_strikes,
                    mark_options,
                    mark_picks,
                    mark_health,
                    mark_hits,
                )
                    .chain()
                    .in_set(MarkCells)
                    .in_set(GamePhase::Visibility),
            );
    }
}

/// A mark that draws `ch` on free floor and tints whatever stands there,
/// both in `color`.
pub const fn mark(ch: char, color: Rgb) -> CellMark {
    CellMark {
        ground: RenderCell {
            ch,
            color,
            depth: 0,
        },
        tint: color,
    }
}

/// `a` mixed toward `b` by `t` (0 to 1).
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let channel = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0)) as u8;
    Rgb::new(channel(a.r, b.r), channel(a.g, b.g), channel(a.b, b.b))
}
