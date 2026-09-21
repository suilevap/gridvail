use bevy::prelude::*;
use rand::RngExt;

use crate::model::*;

/// Convert one keyboard edge into a stable player command component.
pub fn player_input(
    keys: Res<ButtonInput<KeyCode>>,
    turn: Res<TurnState>,
    mut players: Query<&mut MoveCommand, (With<Player>, With<Active>, Without<DestroyRequested>)>,
) {
    if turn.simulation {
        return;
    }
    let dir = if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
        Some(IVec2::NEG_Y)
    } else if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
        Some(IVec2::Y)
    } else if keys.any_just_pressed([KeyCode::ArrowLeft, KeyCode::KeyA]) {
        Some(IVec2::NEG_X)
    } else if keys.any_just_pressed([KeyCode::ArrowRight, KeyCode::KeyD]) {
        Some(IVec2::X)
    } else {
        None
    };
    if let Some(target) = dir {
        for mut command in players.iter_mut() {
            command.target = target;
            command.relative = true;
            command.active = true;
        }
    }
}

pub fn enemy_ai(
    turn: Res<TurnState>,
    mut rng: ResMut<SharedRng>,
    mut enemies: Query<&mut MoveCommand, (With<Enemy>, With<Active>, Without<DestroyRequested>)>,
) {
    if turn.simulation {
        return;
    }
    const MOVES: [IVec2; 5] = [IVec2::ZERO, IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y];
    for mut command in enemies.iter_mut() {
        command.target = MOVES[rng.0.random_range(0..MOVES.len())];
        command.relative = true;
        command.active = true;
    }
}

pub fn move_commands(
    mut movers: Query<(&mut MoveCommand, &mut Speed, &mut Tokens), Without<DestroyRequested>>,
) {
    for (mut command, mut speed, mut tokens) in movers.iter_mut() {
        if !command.active || tokens.count <= 0 {
            continue;
        }
        if command.relative {
            speed.0 = command.target;
        }
        command.active = false;
        tokens.count -= 1;
    }
}
