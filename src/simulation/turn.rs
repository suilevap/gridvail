use bevy::prelude::*;

use crate::model::*;

pub fn turn_tick(mut turn: ResMut<TurnState>) {
    if !turn.simulation {
        turn.tick += 1;
    }
}

pub fn recharge_tokens(
    time: Res<Time>,
    mut timer: ResMut<TokenTimer>,
    mut pacing: ResMut<TurnPacing>,
    mut holders: Query<&mut Tokens, Or<(With<Player>, With<Enemy>)>>,
) {
    timer.0.tick(time.delta());
    pacing.tick(time.delta());
    let any_left = holders.iter().any(|tokens| tokens.count > 0);
    if pacing.can_advance() && (timer.0.is_finished() || !any_left) {
        for mut tokens in holders.iter_mut() {
            tokens.count = tokens.recharge;
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
