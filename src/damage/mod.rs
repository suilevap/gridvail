//! Health and death, whatever deals the damage: combat, hazards, falls.
//!
//! Anything sends `Damage`; `take_damage` takes it off `Health` and says
//! `Died` when it runs out; `destroy_dead` removes the dead, except what
//! keeps on death (the player), which is left to whatever reads `Died`.

use bevy::prelude::*;

use crate::model::*;
use crate::schedule::{ActionStage, GamePhase};

pub struct DamagePlugin;

impl Plugin for DamagePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Damage>()
            .add_message::<Died>()
            .add_systems(
                Update,
                (take_damage, destroy_dead)
                    .chain()
                    .in_set(ActionStage::Damage)
                    .in_set(GamePhase::Simulation),
            );
    }
}

/// Takes each `Damage` off its target's health; one that runs out has
/// `Died`.
pub fn take_damage(
    mut damage: MessageReader<Damage>,
    mut died: MessageWriter<Died>,
    mut bodies: Query<&mut Health, Without<DestroyRequested>>,
) {
    for hit in damage.read() {
        let Ok(mut health) = bodies.get_mut(hit.target) else {
            continue;
        };
        if health.is_dead() {
            continue;
        }
        health.hp -= hit.amount;
        if health.is_dead() {
            died.write(Died(hit.target));
        }
    }
}

/// Destroys what died, unless it keeps on death.
pub fn destroy_dead(
    mut commands: Commands,
    mut died: MessageReader<Died>,
    kept: Query<(), With<KeepOnDeath>>,
) {
    for &Died(entity) in died.read() {
        if !kept.contains(entity) {
            commands.entity(entity).insert(DestroyRequested);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(DamagePlugin);
        app
    }

    #[test]
    fn damage_comes_off_health_and_the_dead_are_destroyed() {
        let mut app = app();
        let target = app.world_mut().spawn(Health::full(2)).id();
        app.world_mut().write_message(Damage { target, amount: 1 });
        app.update();
        assert_eq!(app.world().get::<Health>(target).unwrap().hp, 1);
        app.world_mut().write_message(Damage { target, amount: 1 });
        app.update();
        assert!(app.world().get::<DestroyRequested>(target).is_some());
    }

    #[test]
    fn what_keeps_on_death_stays_and_is_reported() {
        let mut app = app();
        let target = app.world_mut().spawn((Health::full(1), KeepOnDeath)).id();
        app.world_mut().write_message(Damage { target, amount: 3 });
        app.update();
        assert!(app.world().get::<DestroyRequested>(target).is_none());
        let died = app.world().resource::<Messages<Died>>();
        assert_eq!(
            died.iter_current_update_messages().next(),
            Some(&Died(target))
        );
    }
}
