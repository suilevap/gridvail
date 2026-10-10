use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;

use super::*;

fn touch_app() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::input::InputPlugin))
        .init_resource::<TurnState>()
        .init_resource::<Swipe>()
        .init_resource::<ControlScheme>()
        .add_systems(PreUpdate, read_swipes.after(bevy::input::InputSystems))
        .add_systems(Update, swipe_commands);
    let player = app
        .world_mut()
        .spawn((
            Player(0),
            Active,
            MoveCommand::default(),
            Tokens {
                count: 0,
                recharge: 1,
            },
        ))
        .id();
    (app, player)
}

fn touch(app: &mut App, phase: TouchPhase, x: f32, y: f32) {
    let window = app.world_mut().spawn_empty().id();
    app.world_mut().write_message(TouchInput {
        phase,
        position: Vec2::new(x, y),
        window,
        force: None,
        id: 7,
    });
    app.update();
}

/// Gives the player a token, runs a frame, and returns the walk it issued.
fn step_taken(app: &mut App, player: Entity) -> Option<IVec2> {
    app.world_mut().get_mut::<Tokens>(player).unwrap().count = 1;
    app.world_mut()
        .get_mut::<MoveCommand>(player)
        .unwrap()
        .active = false;
    app.update();
    // Spend the token, so only this frame can have walked.
    app.world_mut().get_mut::<Tokens>(player).unwrap().count = 0;
    let command = app.world().get::<MoveCommand>(player).unwrap();
    command.active.then_some(command.target)
}

#[test]
fn a_swipe_released_before_a_token_still_steps_once() {
    let (mut app, player) = touch_app();
    touch(&mut app, TouchPhase::Started, 100.0, 100.0);
    touch(&mut app, TouchPhase::Moved, 100.0, 40.0);
    touch(&mut app, TouchPhase::Ended, 100.0, 40.0);
    // Window coordinates grow downward: an upward swipe walks north.
    assert_eq!(step_taken(&mut app, player), Some(IVec2::NEG_Y));
    assert_eq!(step_taken(&mut app, player), None);
}

#[test]
fn a_held_swipe_keeps_walking_and_turns_with_the_finger() {
    let (mut app, player) = touch_app();
    touch(&mut app, TouchPhase::Started, 100.0, 100.0);
    touch(&mut app, TouchPhase::Moved, 110.0, 105.0);
    assert_eq!(step_taken(&mut app, player), None, "below the threshold");
    touch(&mut app, TouchPhase::Moved, 160.0, 110.0);
    assert_eq!(step_taken(&mut app, player), Some(IVec2::X));
    assert_eq!(step_taken(&mut app, player), None, "a swipe is one step");
    std::thread::sleep(std::time::Duration::from_millis(300));
    touch(&mut app, TouchPhase::Moved, 161.0, 110.0);
    assert_eq!(step_taken(&mut app, player), Some(IVec2::X), "held");
    assert_eq!(step_taken(&mut app, player), Some(IVec2::X), "held");
    touch(&mut app, TouchPhase::Moved, 70.0, 100.0);
    assert_eq!(step_taken(&mut app, player), Some(IVec2::NEG_X));
    assert_eq!(step_taken(&mut app, player), None, "turning is a new swipe");
}

#[test]
fn a_touch_switches_the_control_scheme_to_touch() {
    let (mut app, _) = touch_app();
    assert_eq!(
        *app.world().resource::<ControlScheme>(),
        ControlScheme::Keyboard
    );
    touch(&mut app, TouchPhase::Started, 100.0, 100.0);
    assert_eq!(
        *app.world().resource::<ControlScheme>(),
        ControlScheme::Touch
    );
}
