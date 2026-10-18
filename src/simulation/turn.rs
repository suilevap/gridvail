use bevy::prelude::*;

use crate::model::*;

pub fn turn_tick(mut turn: ResMut<TurnState>, mut actions: Query<&mut ActionTaken>) {
    for mut action in &mut actions {
        action.active = false;
    }
    if !turn.simulation {
        turn.tick += 1;
    }
}

pub fn recharge_tokens(
    time: Res<Time>,
    mode: Res<TurnMode>,
    mut timer: ResMut<TokenTimer>,
    mut pacing: ResMut<TurnPacing>,
    mut holders: Query<(&mut Tokens, Has<Active>), Without<DestroyRequested>>,
) {
    if *mode == TurnMode::Independent {
        timer.0.tick(time.delta());
    }
    let any_left = holders
        .iter()
        .any(|(tokens, active)| tokens.count > 0 && (*mode == TurnMode::Independent || active));
    // A new turn starts on the timeout, or once every token is spent and the
    // player's action has (almost) finished animating, if anything animates.
    if (*mode == TurnMode::Independent && timer.0.is_finished())
        || (!any_left && pacing.can_advance())
    {
        for (mut tokens, active) in holders.iter_mut() {
            if *mode == TurnMode::Independent || active {
                tokens.count = tokens.recharge;
            }
        }
        timer.0.reset();
        pacing.started();
    }
}

/// Accept manual input before the deadline, or issue a neutral action when
/// it expires. This is the existing turn budget, not a second combat clock.
pub fn accept_turn(
    time: Res<Time>,
    mode: Res<TurnMode>,
    deadline: Res<TurnDeadline>,
    menu: Option<Res<SettingsMenu>>,
    mut turn: ResMut<TurnState>,
    mut timer: ResMut<TokenTimer>,
    mut players: Query<
        (&mut MoveCommand, &Tokens, Option<&mut ForcedWait>),
        (With<TurnLeader>, With<Active>),
    >,
) {
    turn.committed = false;
    turn.timed_out = false;
    if *mode != TurnMode::LeaderDriven || turn.simulation {
        return;
    }
    let Ok((mut command, tokens, mut forced)) = players.single_mut() else {
        return;
    };
    if tokens.count <= 0 {
        return;
    }
    if let Some(forced) = forced.as_deref_mut() {
        forced.0 = false;
    }
    if !command.active {
        if !deadline.enabled || SettingsMenu::is_open(menu.as_deref()) {
            return;
        }
        timer.0.tick(time.delta());
        if !timer.0.is_finished() {
            return;
        }
        *command = MoveCommand {
            target: IVec2::ZERO,
            relative: true,
            active: true,
        };
        turn.timed_out = true;
        if let Some(mut forced) = forced {
            forced.0 = true;
        }
    }
    turn.committed = true;
    timer.0.reset();
}

/// Actors without a move still spend their synchronized turn. AI and
/// locomotion have already had the opportunity to produce ordinary commands.
pub fn synchronize_idle_actors(
    mode: Res<TurnMode>,
    turn: Res<TurnState>,
    mut actors: Query<(&mut MoveCommand, &Tokens), (With<TurnFollower>, With<Active>)>,
) {
    if *mode != TurnMode::LeaderDriven || !turn.committed {
        return;
    }
    for (mut command, tokens) in &mut actors {
        if tokens.count > 0 && !command.active {
            *command = MoveCommand {
                target: IVec2::ZERO,
                relative: true,
                active: true,
            };
        }
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
