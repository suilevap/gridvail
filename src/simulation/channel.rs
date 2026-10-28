use bevy::prelude::*;

use crate::model::*;

/// Gives each channeling actor's turn to its action: spends its token
/// before anyone decides anything, counts the turn off, and says
/// `ChannelDone` when the last one is spent. An actor whose token is gone
/// decides nothing this turn, so deciders need not know it is busy.
pub fn channel(
    mut commands: Commands,
    turn: Res<TurnState>,
    mut pacing: ResMut<TurnPacing>,
    mut done: MessageWriter<ChannelDone>,
    mut actors: Query<(Entity, &mut Tokens, &mut Channeling), Without<DestroyRequested>>,
) {
    if turn.simulation {
        return;
    }
    for (actor, mut tokens, mut channeling) in &mut actors {
        if tokens.count <= 0 {
            continue;
        }
        tokens.count -= 1;
        pacing.action_committed();
        channeling.turns = channeling.turns.saturating_sub(1);
        if channeling.turns == 0 {
            commands.entity(actor).remove::<Channeling>();
            done.write(ChannelDone(actor));
        }
    }
}
