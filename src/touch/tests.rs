use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;

use super::*;

fn touch_app() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, bevy::input::InputPlugin))
        .init_resource::<TurnState>()
        .init_resource::<Swipe>()
        .init_resource::<Pinch>()
        .init_resource::<ControlScheme>()
        .init_resource::<crate::camera::CameraOperator>()
        .add_systems(
            PreUpdate,
            (read_swipes, read_pinch).after(bevy::input::InputSystems),
        )
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
    finger(app, 7, phase, x, y);
    app.update();
}

/// Moves one finger without running a frame, so several can move at once.
fn finger(app: &mut App, id: u64, phase: TouchPhase, x: f32, y: f32) {
    let window = app.world_mut().spawn_empty().id();
    app.world_mut().write_message(TouchInput {
        phase,
        position: Vec2::new(x, y),
        window,
        force: None,
        id,
    });
}

/// Puts two fingers down at `a` and `b`, then moves them to `to_a` and
/// `to_b`.
fn two_fingers(app: &mut App, a: Vec2, b: Vec2, to_a: Vec2, to_b: Vec2) {
    finger(app, 1, TouchPhase::Started, a.x, a.y);
    finger(app, 2, TouchPhase::Started, b.x, b.y);
    app.update();
    finger(app, 1, TouchPhase::Moved, to_a.x, to_a.y);
    finger(app, 2, TouchPhase::Moved, to_b.x, to_b.y);
    app.update();
}

fn operator(app: &App) -> &crate::camera::CameraOperator {
    app.world().resource::<crate::camera::CameraOperator>()
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

#[test]
fn spreading_two_fingers_zooms_in_and_pinching_zooms_out() {
    let (mut app, _) = touch_app();
    two_fingers(
        &mut app,
        Vec2::new(100.0, 100.0),
        Vec2::new(200.0, 100.0),
        Vec2::new(75.0, 100.0),
        Vec2::new(225.0, 100.0),
    );
    assert!((operator(&app).target_zoom() - 1.5).abs() < 1e-4);
    finger(&mut app, 1, TouchPhase::Moved, 125.0, 100.0);
    finger(&mut app, 2, TouchPhase::Moved, 175.0, 100.0);
    app.update();
    // Half the starting span, clamped to the operator's range.
    assert!((operator(&app).target_zoom() - 0.5).abs() < 1e-4);
}

#[test]
fn twisting_two_fingers_turns_the_view_a_quarter_at_a_time() {
    use std::f32::consts::FRAC_PI_2;
    let (mut app, _) = touch_app();
    // A clockwise twist past an eighth of a turn turns the picture
    // clockwise: the view's angle, counter-clockwise, goes down a quarter.
    two_fingers(
        &mut app,
        Vec2::new(100.0, 100.0),
        Vec2::new(200.0, 100.0),
        Vec2::new(125.0, 57.0),
        Vec2::new(175.0, 143.0),
    );
    assert!((operator(&app).target_rotation() + FRAC_PI_2).abs() < 1e-4);
    // Less than another eighth past the new quarter changes nothing.
    finger(&mut app, 1, TouchPhase::Moved, 150.0, 50.0);
    finger(&mut app, 2, TouchPhase::Moved, 150.0, 150.0);
    app.update();
    assert!((operator(&app).target_rotation() + FRAC_PI_2).abs() < 1e-4);
}

#[test]
fn a_second_finger_cancels_the_swipe_until_all_fingers_lift() {
    let (mut app, player) = touch_app();
    two_fingers(
        &mut app,
        Vec2::new(100.0, 100.0),
        Vec2::new(200.0, 100.0),
        Vec2::new(20.0, 100.0),
        Vec2::new(200.0, 100.0),
    );
    assert_eq!(step_taken(&mut app, player), None);
    finger(&mut app, 2, TouchPhase::Ended, 200.0, 100.0);
    app.update();
    finger(&mut app, 1, TouchPhase::Moved, 0.0, 100.0);
    app.update();
    assert_eq!(step_taken(&mut app, player), None, "the leftover finger");
    finger(&mut app, 1, TouchPhase::Ended, 0.0, 100.0);
    app.update();
    touch(&mut app, TouchPhase::Started, 100.0, 100.0);
    touch(&mut app, TouchPhase::Moved, 160.0, 100.0);
    assert_eq!(step_taken(&mut app, player), Some(IVec2::X));
}
