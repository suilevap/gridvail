use bevy::prelude::*;

use crate::model::*;

/// Keep producing player commands while a movement key is held.
pub fn player_input(
    keys: Res<ButtonInput<KeyCode>>,
    turn: Res<TurnState>,
    mut players: Query<
        (&mut MoveCommand, &Tokens),
        (With<Player>, With<Active>, Without<DestroyRequested>),
    >,
) {
    if turn.simulation {
        return;
    }
    let dir = if keys.any_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
        Some(IVec2::NEG_Y)
    } else if keys.any_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
        Some(IVec2::Y)
    } else if keys.any_pressed([KeyCode::ArrowLeft, KeyCode::KeyA]) {
        Some(IVec2::NEG_X)
    } else if keys.any_pressed([KeyCode::ArrowRight, KeyCode::KeyD]) {
        Some(IVec2::X)
    } else {
        None
    };
    if let Some(target) = dir {
        for (mut command, tokens) in players.iter_mut() {
            if tokens.count <= 0 {
                continue;
            }
            command.target = target;
            command.relative = true;
            command.active = true;
        }
    }
}

pub fn move_commands(
    mut pacing: ResMut<TurnPacing>,
    mut movers: Query<(&mut MoveCommand, &mut Speed, &mut Tokens), Without<DestroyRequested>>,
) {
    let mut action_committed = false;
    for (mut command, mut speed, mut tokens) in movers.iter_mut() {
        if !command.active || tokens.count <= 0 {
            continue;
        }
        if command.relative {
            speed.0 = command.target;
        }
        command.active = false;
        tokens.count -= 1;
        action_committed = true;
    }
    if action_committed {
        pacing.action_committed();
    }
}
