use bevy::prelude::*;

use crate::model::*;

/// Marks the keyboard as the control scheme when any key is pressed.
pub fn track_keyboard_scheme(keys: Res<ButtonInput<KeyCode>>, mut scheme: ResMut<ControlScheme>) {
    if keys.get_just_pressed().next().is_some() {
        scheme.set_if_neq(ControlScheme::Keyboard);
    }
}

/// Keep producing player commands while a movement key is held. Keys are
/// directions on screen: when the view is turned, "up" walks toward the top
/// of the screen. The map direction is the one the key meant when it was
/// pressed (`MoveIntent`): holding it keeps walking the same way on the map
/// while the view turns.
pub fn player_input(
    keys: Res<ButtonInput<KeyCode>>,
    turn: Res<TurnState>,
    camera: Option<Res<ViewCamera>>,
    menu: Option<Res<SettingsMenu>>,
    mut players: Query<
        (&mut MoveCommand, &Tokens, Option<&mut MoveIntent>),
        (With<Player>, With<Active>, Without<DestroyRequested>),
    >,
) {
    let pressed = |codes| keys.any_pressed(codes);
    // The settings menu takes the arrow keys while it is open.
    let screen = if SettingsMenu::is_open(menu.as_deref()) {
        None
    } else if pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
        Some(IVec2::NEG_Y)
    } else if pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
        Some(IVec2::Y)
    } else if pressed([KeyCode::ArrowLeft, KeyCode::KeyA]) {
        Some(IVec2::NEG_X)
    } else if pressed([KeyCode::ArrowRight, KeyCode::KeyD]) {
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
    mut movers: Query<
        (
            &mut MoveCommand,
            &mut Speed,
            &mut Tokens,
            Option<&mut ActionTaken>,
        ),
        Without<DestroyRequested>,
    >,
) {
    let mut action_committed = false;
    for (mut command, mut speed, mut tokens, mut action) in movers.iter_mut() {
        if let Some(action) = action.as_deref_mut() {
            action.active = false;
        }
        if !command.active || tokens.count <= 0 {
            continue;
        }
        if command.relative {
            speed.0 = command.target;
        }
        command.active = false;
        tokens.count -= 1;
        if let Some(mut action) = action {
            action.active = true;
            action.serial += 1;
        }
        action_committed = true;
    }
    if action_committed {
        pacing.action_committed();
    }
}

/// Keyboard/touch hold recognition produces the same neutral command as any
/// other deliberate wait. Combat decides what that action does.
pub fn hold_to_wait(
    time: Res<Time>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    menu: Option<Res<SettingsMenu>>,
    turn: Res<TurnState>,
    wait: Option<ResMut<WaitHold>>,
    mut players: Query<(&mut MoveCommand, &Tokens), (With<Player>, With<Active>)>,
) {
    let Some(mut wait) = wait else {
        return;
    };
    let Ok((mut command, tokens)) = players.single_mut() else {
        return;
    };
    let held = !SettingsMenu::is_open(menu.as_deref())
        && (keys.is_some_and(|keys| keys.pressed(KeyCode::Space)) || wait.touch_held);
    if held && command.active && command.target != IVec2::ZERO {
        wait.elapsed = 0.0;
        wait.latched = true;
        return;
    }
    if wait.update(
        held,
        !turn.simulation && tokens.count > 0,
        time.delta_secs(),
    ) {
        *command = MoveCommand {
            target: IVec2::ZERO,
            relative: true,
            active: true,
        };
    }
}

pub fn cancel_expired_wait(turn: Res<TurnState>, wait: Option<ResMut<WaitHold>>) {
    if turn.timed_out {
        if let Some(mut wait) = wait {
            wait.latched |= wait.elapsed > 0.0;
            wait.elapsed = 0.0;
        }
    }
}

pub fn request_restart(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    menu: Option<Res<SettingsMenu>>,
    request: Option<ResMut<RestartRequest>>,
) {
    if !SettingsMenu::is_open(menu.as_deref())
        && keys.is_some_and(|keys| keys.just_pressed(KeyCode::KeyR))
    {
        if let Some(mut request) = request {
            request.0 = true;
        }
    }
}
