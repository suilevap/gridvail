//! Combat: what attacks do, composed from small systems that each answer
//! one message. See `COMBAT.md` for the design.
//!
//! - `perform_maneuvers`: a `ContextAction` (from `selection`) by an actor
//!   with `Maneuvers` becomes a `Hit` and `Displace`ments;
//! - `ready_weapons` and `strike`: a `UseWeapon` (from the AI) aims a
//!   `Weapon` (`Telegraph`) and winds it up (`Channeling`); when that is
//!   done its cells are struck: a `Hit` on each;
//! - `hits_deal_damage`, `critical_hits` (on `Exposed` targets) and
//!   `impacts_deal_damage` (an `Impact` from a blocked displacement) turn
//!   what landed into `Damage`, which `damage` takes off `Health`.
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
use crate::simulation::channel;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CriticalHit>()
            .init_resource::<ImpactDamage>()
            .add_message::<Hit>()
            .add_message::<UseWeapon>()
            .add_systems(
                Update,
                (
                    strike.after(channel).in_set(ActionStage::Ongoing),
                    (perform_maneuvers, ready_weapons).in_set(ActionStage::Effects),
                    (hits_deal_damage, critical_hits, impacts_deal_damage)
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
