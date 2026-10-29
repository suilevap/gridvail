use bevy::prelude::*;

use crate::model::*;
use crate::schedule::ActionStage;

/// Order within `ActionStage::Due`: debts are paid, then what waited on
/// them is released.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeSet {
    Pay,
    Release,
}

/// Lets messages of type `M` be `Scheduled`: held on an entity until its
/// debt is paid.
pub trait ScheduleMessages {
    fn add_scheduled_message<M: Message + Clone>(&mut self) -> &mut Self;
}

impl ScheduleMessages for App {
    fn add_scheduled_message<M: Message + Clone>(&mut self) -> &mut Self {
        self.add_message::<M>()
            .add_systems(Update, release_scheduled::<M>.in_set(TimeSet::Release))
    }
}

pub(super) fn configure(app: &mut App) {
    app.add_message::<Delay>()
        .add_message::<DebtPaid>()
        .configure_sets(
            Update,
            (TimeSet::Pay, TimeSet::Release)
                .chain()
                .in_set(ActionStage::Due),
        )
        .add_systems(
            Update,
            (
                pay_debts.in_set(TimeSet::Pay),
                owe.in_set(ActionStage::Costs),
            ),
        );
}

/// Each indebted actor's turn pays a turn off: its token is spent before
/// anyone decides anything, and `DebtPaid` says when nothing is owed.
pub fn pay_debts(
    mut commands: Commands,
    turn: Res<TurnState>,
    mut pacing: ResMut<TurnPacing>,
    mut paid: MessageWriter<DebtPaid>,
    mut actors: Query<(Entity, &mut Tokens, &mut Debt), Without<DestroyRequested>>,
) {
    if turn.simulation {
        return;
    }
    for (actor, mut tokens, mut debt) in &mut actors {
        if tokens.count <= 0 {
            continue;
        }
        tokens.count -= 1;
        pacing.action_committed();
        debt.turns = debt.turns.saturating_sub(1);
        if debt.turns == 0 {
            commands.entity(actor).remove::<Debt>();
            paid.write(DebtPaid(actor));
        }
    }
}

/// Adds each `Delay` to what its entity owes.
pub fn owe(mut commands: Commands, mut delays: MessageReader<Delay>) {
    for delay in delays.read().filter(|delay| delay.turns > 0) {
        let turns = delay.turns;
        let Ok(mut entity) = commands.get_entity(delay.entity) else {
            continue;
        };
        entity
            .entry::<Debt>()
            .and_modify(move |mut debt| debt.turns = debt.turns.saturating_add(turns))
            .or_insert(Debt { turns });
    }
}

/// Sends the `M` an entity held once its debt is paid.
pub fn release_scheduled<M: Message + Clone>(
    mut commands: Commands,
    mut paid: MessageReader<DebtPaid>,
    mut out: MessageWriter<M>,
    held: Query<&Scheduled<M>>,
) {
    for &DebtPaid(entity) in paid.read() {
        if let Ok(scheduled) = held.get(entity) {
            out.write(scheduled.0.clone());
            commands.entity(entity).remove::<Scheduled<M>>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::GamePhase;

    #[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
    struct Ping;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<TurnState>()
            .init_resource::<TurnPacing>()
            .configure_sets(
                Update,
                (ActionStage::Due, ActionStage::Costs)
                    .chain()
                    .in_set(GamePhase::Simulation),
            );
        configure(&mut app);
        app.add_scheduled_message::<Ping>();
        app
    }

    fn actor(app: &mut App) -> Entity {
        app.world_mut().spawn(Tokens::new(1)).id()
    }

    /// A new turn for `actor`: its token back, one frame.
    fn turn(app: &mut App, actor: Entity) {
        app.world_mut().get_mut::<Tokens>(actor).unwrap().count = 1;
        app.update();
    }

    fn pings(app: &App) -> usize {
        let messages = app.world().resource::<Messages<Ping>>();
        messages.iter_current_update_messages().count()
    }

    #[test]
    fn delays_add_up_and_each_turn_pays_one_off() {
        let mut app = app();
        let actor = actor(&mut app);
        for turns in [1, 2] {
            app.world_mut().write_message(Delay {
                entity: actor,
                turns,
            });
        }
        app.update();
        assert_eq!(app.world().get::<Debt>(actor), Some(&Debt { turns: 3 }));
        turn(&mut app, actor);
        assert_eq!(app.world().get::<Tokens>(actor).unwrap().count, 0, "played");
        assert_eq!(app.world().get::<Debt>(actor), Some(&Debt { turns: 2 }));
    }

    #[test]
    fn a_scheduled_message_is_sent_when_the_debt_is_paid() {
        let mut app = app();
        let actor = actor(&mut app);
        app.world_mut().entity_mut(actor).insert(Scheduled(Ping));
        app.world_mut().write_message(Delay {
            entity: actor,
            turns: 2,
        });
        app.update();
        turn(&mut app, actor);
        assert_eq!(pings(&app), 0, "sent before the debt was paid");
        turn(&mut app, actor);
        assert_eq!(pings(&app), 1);
        assert!(app.world().get::<Scheduled<Ping>>(actor).is_none());
    }
}
