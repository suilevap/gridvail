use bevy::prelude::*;

use crate::model::*;

/// Keep producing player commands while a movement key is held. Keys are
/// directions on screen: when the view is turned, "up" walks toward the top
/// of the screen. The map direction is the one the key meant when it was
/// pressed (`MoveIntent`): holding it keeps walking the same way on the map
/// while the view turns.
pub fn player_input(
    keys: Res<ButtonInput<KeyCode>>,
    turn: Res<TurnState>,
    camera: Option<Res<ViewCamera>>,
    mut players: Query<
        (&mut MoveCommand, &Tokens, Option<&mut MoveIntent>),
        (With<Player>, With<Active>, Without<DestroyRequested>),
    >,
) {
    let screen = if keys.any_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
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
    let map_direction = |screen: IVec2| {
        camera
            .as_ref()
            .map_or(screen, |camera| camera.map_direction(screen))
    };
    for (mut command, tokens, intent) in players.iter_mut() {
        // The intent follows the keys every frame, between turns too, so a
        // key held while the view turns keeps what it meant when pressed.
        let target = match intent {
            Some(mut intent) => intent.hold(screen, map_direction),
            None => screen.map(map_direction),
        };
        let Some(target) = target else {
            continue;
        };
        if turn.simulation || tokens.count <= 0 {
            continue;
        }
        command.target = target;
        command.relative = true;
        command.active = true;
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
