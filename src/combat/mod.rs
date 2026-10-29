//! Combat: what attacks do, composed from small systems that each answer
//! one message. See `COMBAT.md` for the design.
//!
//! - `perform_maneuvers`: a `ContextAction` (from `selection`) by an actor
//!   with `Maneuvers` becomes a `Hit`, and a `Knock` (with a `Follow`) or a
//!   `Swap`;
//! - `ready_weapons` and `strike`: a `UseWeapon` (from the AI) aims a
//!   `Weapon` (`Telegraph`) and owes its wind-up (`Delay`), with the
//!   `Strike` `Scheduled` for when it is paid; the strike hits each cell of
//!   the weapon's `Area`;
//! - `hits_deal_damage`, `critical_hits` (on `Exposed` targets) and
//!   `collisions_deal_damage` (a knocked body's `Collision`) turn what
//!   landed into `Damage`, which `damage` takes off `Health`.
//!
//! Time (debts, scheduled messages) and physics (knocks, collisions) are
//! general mechanisms in `simulation`; combat is one of their users.
//!
//! Each reads and writes messages and components only, so a new move, a
//! new weapon or a new reaction to hits is data or one more system.

mod hits;
mod maneuvers;
mod weapons;

pub use hits::*;
pub use maneuvers::*;
pub use weapons::*;

use bevy::prelude::*;

use crate::model::*;
use crate::schedule::{ActionStage, GamePhase, MarkCells};
use crate::simulation::ScheduleMessages;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CriticalHit>()
            .init_resource::<ImpactDamage>()
            .add_message::<Hit>()
            .add_message::<UseWeapon>()
            .add_scheduled_message::<Strike>()
            .add_systems(
                Update,
                (
                    strike.in_set(ActionStage::Ongoing),
                    (perform_maneuvers, ready_weapons).in_set(ActionStage::Effects),
                    (hits_deal_damage, critical_hits, collisions_deal_damage)
                        .in_set(ActionStage::Hits),
                )
                    .in_set(GamePhase::Simulation),
            )
            .add_systems(
                Update,
                mark_strikes.in_set(MarkCells).in_set(GamePhase::Visibility),
            );
    }
}

#[cfg(test)]
mod tests;
