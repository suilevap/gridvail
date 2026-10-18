use bevy::prelude::*;

use crate::model::*;

pub fn turn_tick(mut turn: ResMut<TurnState>) {
    if !turn.simulation {
        turn.tick += 1;
    }
}

pub fn recharge_tokens(
    time: Res<Time>,
    battle: Option<Res<BattleState>>,
    mut timer: ResMut<TokenTimer>,
    mut pacing: ResMut<TurnPacing>,
    mut holders: Query<(&mut Tokens, Has<Active>), Or<(With<Player>, With<Enemy>)>>,
) {
    if battle.as_ref().is_some_and(|battle| battle.defeated) {
        return;
    }
    if battle.is_none() {
        timer.0.tick(time.delta());
    }
    // BattlePlugin owns the decision deadline: expiry must commit an enemy
    // turn, rather than silently refilling everyone's unspent tokens.
    let any_left = holders
        .iter()
        .any(|(tokens, active)| tokens.count > 0 && (battle.is_none() || active));
    // A new turn starts on the timeout, or once every token is spent and the
    // player's action has (almost) finished animating, if anything animates.
    if (battle.is_none() && timer.0.is_finished()) || (!any_left && pacing.can_advance()) {
        for (mut tokens, active) in holders.iter_mut() {
            if battle.is_none() || active {
                tokens.count = tokens.recharge;
            }
        }
        timer.0.reset();
        pacing.started();
    }
}

pub fn turn_update(
    mut turn: ResMut<TurnState>,
    commands: Query<(&MoveCommand, &Tokens), (With<Speed>, Without<DestroyRequested>)>,
    speeds: Query<&Speed, (With<Active>, Without<DestroyRequested>)>,
    pendings: Query<&PendingPos>,
    doomed: Query<(), With<DestroyRequested>>,
) {
    turn.simulation = commands
        .iter()
        .any(|(command, tokens)| command.active && tokens.count > 0)
        || speeds.iter().any(|speed| speed.0 != IVec2::ZERO)
        || pendings.iter().any(|pending| *pending != PendingPos::None)
        || !doomed.is_empty();
}
