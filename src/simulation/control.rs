use bevy::prelude::*;

use crate::model::*;

/// Marks the keyboard as the control scheme when any key is pressed.
pub fn track_keyboard_scheme(keys: Res<ButtonInput<KeyCode>>, mut scheme: ResMut<ControlScheme>) {
    if keys.get_just_pressed().next().is_some() {
        scheme.set_if_neq(ControlScheme::Keyboard);
    }
}

/// Space or Backspace cancels the player's readied move (`Cancel`).
pub fn cancel_keys(
    keys: Res<ButtonInput<KeyCode>>,
    menu: Option<Res<SettingsMenu>>,
    cutscene: Option<Res<Cutscene>>,
    mut cancels: MessageWriter<Cancel>,
    players: Query<Entity, (With<Player>, With<Active>, Without<DestroyRequested>)>,
) {
    if SettingsMenu::is_open(menu.as_deref())
        || cutscene.is_some()
        || !keys.any_just_pressed([KeyCode::Space, KeyCode::Backspace])
    {
        return;
    }
    for player in &players {
        cancels.write(Cancel(player));
    }
}

/// The four directions on screen, in the order held keys are read.
const SCREEN_DIRECTIONS: [IVec2; 4] = [IVec2::NEG_Y, IVec2::Y, IVec2::NEG_X, IVec2::X];

/// The keys for a direction on screen.
fn direction_keys(screen: IVec2) -> [KeyCode; 2] {
    match (screen.x, screen.y) {
        (0, -1) => [KeyCode::ArrowUp, KeyCode::KeyW],
        (0, 1) => [KeyCode::ArrowDown, KeyCode::KeyS],
        (-1, 0) => [KeyCode::ArrowLeft, KeyCode::KeyA],
        _ => [KeyCode::ArrowRight, KeyCode::KeyD],
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
    cutscene: Option<Res<Cutscene>>,
    mut players: Query<
        (&mut MoveCommand, &Tokens, Option<&mut MoveIntent>),
        (With<Player>, With<Active>, Without<DestroyRequested>),
    >,
) {
    // The settings menu takes the arrow keys while it is open, and a
    // cutscene ignores them.
    let blocked = SettingsMenu::is_open(menu.as_deref()) || cutscene.is_some();
    let held = |screen: IVec2| !blocked && keys.any_pressed(direction_keys(screen));
    // A key pressed now wins over one held from before, so a second key
    // pressed while the first is still down turns to it.
    let pressed = SCREEN_DIRECTIONS
        .into_iter()
        .find(|&screen| held(screen) && keys.any_just_pressed(direction_keys(screen)));
    let first_held = SCREEN_DIRECTIONS.into_iter().find(|&screen| held(screen));
    let map_direction = |screen: IVec2| {
        camera
            .as_ref()
            .map_or(screen, |camera| camera.map_direction(screen))
    };
    for (mut command, tokens, intent) in players.iter_mut() {
        // The intent follows the keys every frame, between turns too, so a
        // key held while the view turns keeps what it meant when pressed.
        let before = intent
            .as_ref()
            .and_then(|intent| intent.held)
            .map(|held| held.screen);
        let screen = pressed
            .or(before.filter(|&screen| held(screen)))
            .or(first_held);
        let (target, repeat) = match intent {
            Some(mut intent) => (
                intent.hold(screen, map_direction),
                before.is_some() && before == screen,
            ),
            None => (screen.map(map_direction), false),
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
        command.repeat = repeat;
        command.at_once = false;
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
