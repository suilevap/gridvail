use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;

use super::*;
use crate::animation::MotionStyle;
use crate::model::*;
use crate::simulation::test_app;

const FRAME: Duration = Duration::from_millis(50);

fn app_with_player(cell: IVec2) -> (App, Entity) {
    let mut app = test_app::headless();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(FRAME))
        .add_plugins(CameraPlugin);
    let player = app.world_mut().spawn((Player(0), Pos(cell))).id();
    // The first frame has no elapsed time.
    app.update();
    (app, player)
}

fn camera(app: &App) -> ViewCamera {
    *app.world().resource::<ViewCamera>()
}

fn operator(app: &mut App) -> Mut<'_, CameraOperator> {
    app.world_mut().resource_mut::<CameraOperator>()
}

#[test]
fn view_maps_between_map_and_screen() {
    let camera = ViewCamera {
        position: Vec2::new(4.0, 3.0),
        rotation: FRAC_PI_2,
        zoom: 2.0,
        offset: Vec2::new(0.0, 1.0),
    };
    assert_eq!(camera.to_view(camera.position), Vec2::new(0.0, 2.0));
    // Turned a quarter counter-clockwise, east shows up the screen.
    let east = camera.to_view(Vec2::new(5.0, 3.0));
    assert!(east.distance(Vec2::new(0.0, 0.0)) < 1e-5, "{east}");
    let point = Vec2::new(1.5, -2.25);
    assert!(camera.to_map(camera.to_view(point)).distance(point) < 1e-5);

    assert_eq!(camera.quarter_turns(), 1);
    assert!(camera.quarter_remainder().abs() < 1e-6);
    let turning = ViewCamera {
        rotation: 0.6 * FRAC_PI_2,
        ..default()
    };
    assert_eq!(turning.quarter_turns(), 1);
    assert!((turning.quarter_remainder() + 0.4 * FRAC_PI_2).abs() < 1e-6);
    assert_eq!(camera.map_direction(IVec2::NEG_Y), IVec2::X);
    assert_eq!(camera.map_direction(IVec2::X), IVec2::Y);
    let upright = ViewCamera::default();
    assert_eq!(upright.map_direction(IVec2::NEG_Y), IVec2::NEG_Y);
}

#[test]
fn locked_camera_sits_on_the_shown_position() {
    let (mut app, player) = app_with_player(IVec2::new(3, 2));
    assert_eq!(camera(&app).position, Vec2::new(3.0, 2.0));

    let shown = AnimatedPos {
        position: Vec2::new(3.4, 2.0),
        lift: 0.2,
    };
    app.world_mut().entity_mut(player).insert(shown);
    app.update();
    assert_eq!(camera(&app).position, shown.position);
}

#[test]
fn turns_and_zooms_ease_instead_of_jumping() {
    let (mut app, _) = app_with_player(IVec2::new(3, 2));
    {
        let mut operator = operator(&mut app);
        operator.transition = 0.2;
        operator.turn_by_quarters(1);
        operator.zoom_to(1.5);
    }
    app.update();
    let midway = camera(&app);
    assert!(midway.rotation > 0.0 && midway.rotation < FRAC_PI_2);
    assert!(midway.zoom > 1.0 && midway.zoom < 1.5);

    // Turning again mid-turn adds to where it is going.
    operator(&mut app).turn_by_quarters(1);
    for _ in 0..10 {
        app.update();
    }
    let settled = camera(&app);
    assert_eq!(settled.rotation, 2.0 * FRAC_PI_2);
    assert_eq!(settled.zoom, 1.5);
    assert_eq!(
        settled.position,
        Vec2::new(3.0, 2.0),
        "turning keeps the target centred"
    );
}

#[test]
fn zoom_stays_in_range() {
    let (mut app, _) = app_with_player(IVec2::ZERO);
    operator(&mut app).zoom_to(100.0);
    assert_eq!(operator(&mut app).target_zoom(), ZOOM_RANGE.1);
}

#[test]
fn a_new_target_is_reached_by_gliding() {
    let (mut app, _) = app_with_player(IVec2::new(1, 1));
    let view = camera(&app);
    operator(&mut app).look_at(CameraTarget::Point(Vec2::new(5.0, 1.0)), &view);
    app.update();
    let gliding = camera(&app).position.x;
    assert!(gliding > 1.0 && gliding < 5.0, "{gliding}");
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(camera(&app).position, Vec2::new(5.0, 1.0));
}

#[test]
fn trailing_camera_makes_its_own_move_after_the_target() {
    let (mut app, player) = app_with_player(IVec2::new(1, 1));
    operator(&mut app).follow = CameraFollow::Trailing(MotionStyle::default());
    app.update();
    assert_eq!(camera(&app).position, Vec2::new(1.0, 1.0));

    app.world_mut().get_mut::<Pos>(player).unwrap().0 = IVec2::new(2, 1);
    app.update();
    let trailing = camera(&app).position.x;
    assert!(trailing > 1.0 && trailing < 2.0, "{trailing}");
    for _ in 0..20 {
        app.update();
    }
    assert_eq!(camera(&app).position, Vec2::new(2.0, 1.0));
}

#[test]
fn movement_keys_follow_the_turned_view() {
    use bevy::ecs::system::RunSystemOnce;

    let (mut app, player) = app_with_player(IVec2::new(3, 3));
    app.world_mut().entity_mut(player).insert((
        Active,
        MoveCommand::default(),
        Tokens {
            count: 1,
            recharge: 1,
        },
    ));
    app.world_mut().insert_resource(ViewCamera {
        rotation: FRAC_PI_2,
        ..default()
    });
    let mut keys = ButtonInput::<KeyCode>::default();
    keys.press(KeyCode::ArrowUp);
    app.world_mut().insert_resource(keys);
    app.world_mut()
        .run_system_once(crate::simulation::player_input)
        .unwrap();
    // With east at the top of the screen, "up" walks east.
    assert_eq!(
        app.world().get::<MoveCommand>(player).unwrap().target,
        IVec2::X
    );
}

#[test]
fn a_turning_portal_carries_a_turn_under_way() {
    use crate::foundation::portal::CellTransform;
    let (mut app, player) = app_with_player(IVec2::new(2, 2));
    operator(&mut app).turn_by_quarters(1);
    app.update();
    let before = camera(&app).rotation;
    assert!(before > 0.0 && before < FRAC_PI_2, "mid-turn: {before}");

    // The player steps through a portal that turns a quarter turn
    // counter-clockwise: the view turns back by as much, so the picture
    // stays, and the turn under way carries on from there.
    let through = CellTransform {
        quarters: 1,
        offset: IVec2::new(1, 6),
    };
    let arrival = through.apply(IVec2::new(2, 2));
    app.world_mut().get_mut::<Pos>(player).unwrap().0 = arrival;
    operator(&mut app).carry(&through);
    assert!(operator(&mut app).target_rotation().abs() < 1e-6);
    app.update();
    let after = camera(&app).rotation + FRAC_PI_2;
    assert!(
        after > before && after <= FRAC_PI_2 + 1e-6,
        "{after} should go on from {before}"
    );
    assert_eq!(camera(&app).position, arrival.as_vec2());
}

#[test]
fn keeping_north_turns_back_after_a_turning_portal() {
    use crate::foundation::portal::CellTransform;
    let (mut app, player) = app_with_player(IVec2::new(2, 2));
    operator(&mut app).portal_turn = PortalTurn::KeepNorth;
    let through = CellTransform {
        quarters: 1,
        offset: IVec2::new(1, 6),
    };
    app.world_mut().get_mut::<Pos>(player).unwrap().0 = through.apply(IVec2::new(2, 2));
    operator(&mut app).carry(&through);
    // It heads back to where it was, from the turn that keeps the picture.
    assert!(operator(&mut app).target_rotation().abs() < 1e-6);
    app.update();
    let first = camera(&app).rotation;
    assert!(first > -FRAC_PI_2 - 1e-6 && first < 0.0, "{first}");
    for _ in 0..20 {
        app.update();
    }
    assert!(camera(&app).rotation.abs() < 1e-6);

    // A portal that does not turn leaves the view alone.
    operator(&mut app).carry(&CellTransform::translation(IVec2::new(3, 0)));
    assert!(operator(&mut app).target_rotation().abs() < 1e-6);
}

#[test]
fn n_switches_what_the_view_does_through_turning_portals() {
    let (mut app, _) = app_with_player(IVec2::new(2, 2));
    assert_eq!(operator(&mut app).portal_turn, PortalTurn::WithTarget);
    let mut keys = ButtonInput::<KeyCode>::default();
    keys.press(KeyCode::KeyN);
    app.insert_resource(keys);
    app.update();
    assert_eq!(operator(&mut app).portal_turn, PortalTurn::KeepNorth);
    assert_eq!(PortalTurn::from_name("north"), Some(PortalTurn::KeepNorth));
    assert_eq!(PortalTurn::from_name("turn"), Some(PortalTurn::WithTarget));
}
