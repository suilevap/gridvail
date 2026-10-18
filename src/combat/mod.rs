//! Reusable attack execution and damage. AI supplies footprints, controls
//! supply movement/wait commands, and presentation observes the results.

#![allow(clippy::type_complexity)]

mod damage;
mod execution;

use crate::schedule::{StartupPhase, TurnPhase};
use bevy::prelude::*;
use damage::*;
use execution::*;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DamageQueue>()
            .add_systems(
                Startup,
                reserve_hits
                    .after(StartupPhase::Content)
                    .before(StartupPhase::Derive),
            )
            .add_systems(Update, reserve_hits.before(TurnPhase::Input))
            .add_systems(
                Update,
                (
                    clear_execution,
                    brace_actions,
                    emit_bump_points,
                    damage::pipeline(),
                    order_attack_moves,
                )
                    .chain()
                    .in_set(TurnPhase::Attacks),
            )
            .add_systems(
                Update,
                (advance_attacks, damage::pipeline())
                    .chain()
                    .in_set(TurnPhase::Damage),
            );
    }
}

#[cfg(test)]
mod tests;
