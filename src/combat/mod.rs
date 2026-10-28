//! Combat: picking targets, maneuvers, telegraphed strikes and damage.
//!
//! See `COMBAT.md` for the design. The pieces are kept apart so each can
//! be extended on its own:
//!
//! - what can be picked and pushed is data on the target (`Targetable`,
//!   `Movable`, its own `Maneuvers`), not a kind of entity: an enemy, a
//!   crate or a lamp alike;
//! - what each second direction does is a `Maneuver` (where the engager and
//!   the target end up, the damage), looked up by how the direction relates
//!   to the one the target was picked in;
//! - a strike's cells are a `StrikeShape` in the striker's frame, so any
//!   shape works and moves with the striker;
//! - damage only travels as `Hurt` messages, so anything can hurt anything
//!   with `Health`, and whatever reacts to damage reads them.
//!
//! Who strikes when is the AI's decision: it puts a `Telegraph` on a
//! striker. Who is caught off guard is too: it writes `Exposed`.

#![allow(clippy::type_complexity)]

mod engage;
mod strikes;

pub use engage::*;
pub use strikes::*;

use bevy::prelude::*;

use crate::model::*;
use crate::schedule::{GamePhase, PlayerCommands};
use crate::simulation::{move_commands, recharge_tokens};

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CombatRules>()
            .init_resource::<CellMarks>()
            .add_message::<Hurt>()
            .add_message::<Defeated>()
            .add_message::<CancelIntent>()
            .add_systems(
                Update,
                (
                    // Strikes land at the start of the striker's turn,
                    // before anyone acts in it.
                    (give_player_health, land_strikes)
                        .chain()
                        .after(recharge_tokens)
                        .before(PlayerCommands),
                    (keyboard_cancel, engage)
                        .chain()
                        .after(PlayerCommands)
                        .before(move_commands),
                    take_damage.after(engage).before(move_commands),
                )
                    .in_set(GamePhase::Simulation),
            )
            .add_systems(Update, mark_cells.in_set(GamePhase::Visibility));
    }
}

#[cfg(test)]
mod tests;
