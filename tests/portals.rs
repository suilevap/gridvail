//! Walking through a portal on the bundled `portals.txt` map, with animation.

use std::f32::consts::{FRAC_PI_2, PI};
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pav_ecs_game_bevy_port::animation::ObjectAnimationPlugin;
use pav_ecs_game_bevy_port::app::{GamePlugin, MapText};
use pav_ecs_game_bevy_port::foundation::portal::CellTransform;
use pav_ecs_game_bevy_port::model::*;

/// The player starts at (19, 7) in room A. The portal in A's east wall at
/// (26, 7) opens west and leads out of B's west wall at (38, 16), which
/// opens east: cells beyond it are 13 right and 9 down.
const START: IVec2 = IVec2::new(19, 7);
const THROUGH: IVec2 = IVec2::new(13, 9);
const ARRIVAL: IVec2 = IVec2::new(39, 16);

fn boot() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            16,
        )))
        .insert_resource(MapText(include_str!("../assets/maps/portals.txt")))
        .add_plugins((GamePlugin, ObjectAnimationPlugin));
    for _ in 0..30 {
        app.update();
    }
    app
}

fn player(app: &mut App) -> (IVec2, Vec2) {
    let world = app.world_mut();
    let (pos, shown) = world
        .query_filtered::<(&Pos, &AnimatedPos), With<Player>>()
        .single(world)
        .unwrap();
    (pos.0, shown.position)
}

#[test]
fn walking_east_through_the_portal_comes_out_in_the_other_room() {
    let mut app = boot();
    assert_eq!(player(&mut app).0, START);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowRight);

    let mut previous = player(&mut app);
    let mut saw_through = false;
    let mut crossed = false;
    for _ in 0..400 {
        app.update();
        let (pos, shown) = player(&mut app);
        if !crossed && pos == ARRIVAL {
            crossed = true;
            // The step starts at the exit face, carried on from where the
            // player was shown in front of the portal.
            let carried = previous.1 + THROUGH.as_vec2();
            assert!(
                shown.distance(carried) < 0.3,
                "shown {shown} should continue from {carried}"
            );
            assert!(shown.x >= 38.0 && shown.x <= 39.0 && shown.y == 16.0);
        } else {
            // Otherwise the player glides: never a jump across the map.
            assert!(
                shown.distance(previous.1) < 0.3,
                "jumped from {} to {shown}",
                previous.1
            );
        }
        if pos == IVec2::new(25, 7) {
            // In front of the portal, the view runs on into room B.
            let world = app.world_mut();
            let view = world
                .query_filtered::<&PlayerView, With<Player>>()
                .single(world)
                .unwrap();
            if view.pos == pos {
                let beyond = view
                    .samples
                    .iter()
                    .find(|sample| sample.delta == IVec2::new(2, 0))
                    .expect("a sample two cells ahead");
                assert_eq!(beyond.world, IVec2::new(27, 7) + THROUGH);
                saw_through = true;
            }
        }
        previous = (pos, shown);
        if crossed && pos.x >= ARRIVAL.x + 3 {
            break;
        }
    }
    assert!(saw_through, "the player never stood in front of the portal");
    assert!(crossed, "the player never came out of the exit");
    // Holding right keeps walking east inside room B.
    assert!(player(&mut app).0.x >= ARRIVAL.x + 3);
    assert_eq!(player(&mut app).0.y, ARRIVAL.y);
}

/// Walks from the start along row 7 to the column of the portal wall at
/// `entry`, then holds `forward` into it and on for three cells beyond its
/// exit, checking every frame that the picture never jumps: the player
/// stays in the centre of the screen, `mark` (a point beyond the exit)
/// glides across it, and the direction marker stays in front of the player
/// through the crossing. Returns the app and the portal's transform.
fn walk_through_turning_portal(
    entry: IVec2,
    approach: KeyCode,
    forward: KeyCode,
    mark: Vec2,
) -> (App, CellTransform) {
    let mut app = boot();
    let through = app
        .world()
        .resource::<MapGrid>()
        .portal_at(entry)
        .expect("a portal face")
        .through;
    let exit_floor = through.apply(entry);

    let press = |app: &mut App, key: KeyCode| {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release_all();
        keys.press(key);
    };
    press(&mut app, approach);
    for _ in 0..200 {
        app.update();
        if player(&mut app).0.x == entry.x {
            break;
        }
    }
    assert_eq!(player(&mut app).0.x, entry.x, "never reached the portal");
    press(&mut app, forward);

    // Where things are on screen: the player, the mark and the player's
    // direction marker. Before the crossing the mark is seen through the
    // portal, at the inverse of its own position.
    let on_screen = |app: &mut App, crossed: bool| {
        let camera = *app.world().resource::<ViewCamera>();
        let (_, shown) = player(app);
        let world = app.world_mut();
        let marker = world
            .query_filtered::<&AnimatedPos, With<BoundTo>>()
            .single(world)
            .unwrap()
            .position;
        let mark = if crossed {
            mark
        } else {
            through.inverse().apply_point(mark)
        };
        (
            camera.to_view(shown),
            camera.to_view(mark),
            camera.to_view(marker),
        )
    };

    let mut crossed = false;
    let mut previous = on_screen(&mut app, crossed);
    for _ in 0..400 {
        app.update();
        let (pos, _) = player(&mut app);
        let crossing = !crossed && pos == exit_floor;
        crossed |= crossing;
        let now = on_screen(&mut app, crossed);
        assert!(
            now.0.length() < 1e-3,
            "the player left the centre: {}",
            now.0
        );
        assert!(
            now.1.distance(previous.1) < 0.3,
            "the picture jumped from {} to {}",
            previous.1,
            now.1
        );
        // The marker swings round the player as they turn; through the
        // portal it stays in front of them on screen.
        if crossing {
            assert!(
                now.2.distance(previous.2) < 0.3,
                "the marker jumped from {} to {}",
                previous.2,
                now.2
            );
        }
        previous = now;
        if crossed && (pos - exit_floor).abs().max_element() >= 3 {
            break;
        }
    }
    assert!(crossed, "the player never came out of the exit");
    (app, through)
}

fn facing(app: &mut App) -> IVec2 {
    let world = app.world_mut();
    world
        .query_filtered::<&Facing, With<Player>>()
        .single(world)
        .unwrap()
        .0
}

/// Portal 4 leads from room A's north wall at (17, 3), open to the south,
/// out of room B's east wall at (54, 14), open to the west: walking north
/// into it comes out walking west, a quarter turn counter-clockwise.
#[test]
fn walking_through_a_quarter_turning_portal_turns_the_view_without_a_jump() {
    let (mut app, through) = walk_through_turning_portal(
        IVec2::new(17, 3),
        KeyCode::ArrowLeft,
        KeyCode::ArrowUp,
        Vec2::new(50.0, 14.0),
    );
    assert_eq!(through.quarters, 1);
    // Holding up keeps walking up the screen: west in room B.
    let (pos, _) = player(&mut app);
    assert_eq!(pos, IVec2::new(50, 14));
    assert_eq!(facing(&mut app), IVec2::NEG_X);
    let camera = *app.world().resource::<ViewCamera>();
    assert_eq!(camera.map_direction(IVec2::NEG_Y), IVec2::NEG_X);
    assert!((camera.rotation + FRAC_PI_2).abs() < 1e-5);
}

/// Portal 5 leads from room A's south wall at (15, 11) out of room B's
/// south wall at (44, 21), both open to the north: walking south into it
/// comes out walking north, a half turn.
#[test]
fn walking_through_a_half_turning_portal_turns_the_view_without_a_jump() {
    let (mut app, through) = walk_through_turning_portal(
        IVec2::new(15, 11),
        KeyCode::ArrowLeft,
        KeyCode::ArrowDown,
        Vec2::new(44.0, 15.0),
    );
    assert_eq!(through.quarters, 2);
    // Holding down keeps walking down the screen: north in room B.
    let (pos, _) = player(&mut app);
    assert_eq!(pos, IVec2::new(44, 17));
    assert_eq!(facing(&mut app), IVec2::NEG_Y);
    let camera = *app.world().resource::<ViewCamera>();
    assert_eq!(camera.map_direction(IVec2::Y), IVec2::NEG_Y);
    assert!((camera.rotation + PI).abs() < 1e-5);
}

/// Holds `key` until the player reaches `cell`, for at most 400 frames.
fn hold_until(app: &mut App, key: KeyCode, cell: IVec2) {
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keys.release_all();
    keys.press(key);
    for _ in 0..400 {
        app.update();
        if player(app).0 == cell {
            return;
        }
    }
    panic!("never reached {cell}, stopped at {}", player(app).0);
}

#[test]
fn going_back_through_a_turning_portal_turns_the_view_back() {
    let mut app = boot();
    hold_until(&mut app, KeyCode::ArrowLeft, IVec2::new(17, 7));
    // Up through portal 4 and one step on, west in room B...
    hold_until(&mut app, KeyCode::ArrowUp, IVec2::new(52, 14));
    let camera = *app.world().resource::<ViewCamera>();
    assert!((camera.rotation + FRAC_PI_2).abs() < 1e-5);
    // ...then down the screen, east in room B, back through it.
    hold_until(&mut app, KeyCode::ArrowDown, IVec2::new(17, 6));
    let camera = *app.world().resource::<ViewCamera>();
    assert!(camera.rotation.abs() < 1e-5, "{}", camera.rotation);
}

/// With `PortalTurn::KeepNorth`, the view still turns with the player on
/// the frame they come out of portal 4, so nothing jumps, then eases back
/// to north up; holding up then walks north in room B.
#[test]
fn keeping_north_eases_the_view_back_after_a_turning_portal() {
    use pav_ecs_game_bevy_port::camera::{CameraOperator, PortalTurn};
    const ENTRY: IVec2 = IVec2::new(17, 3);
    const EXIT_FLOOR: IVec2 = IVec2::new(53, 14);
    // A point in room B, beyond the exit.
    const MARK: Vec2 = Vec2::new(50.0, 14.0);
    let mut app = boot();
    app.world_mut().resource_mut::<CameraOperator>().portal_turn = PortalTurn::KeepNorth;
    let through = app
        .world()
        .resource::<MapGrid>()
        .portal_at(ENTRY)
        .unwrap()
        .through;
    hold_until(&mut app, KeyCode::ArrowLeft, IVec2::new(17, 7));
    hold_until(&mut app, KeyCode::ArrowUp, IVec2::new(17, 4));

    let mark_on_screen = |app: &mut App, crossed: bool| {
        let camera = *app.world().resource::<ViewCamera>();
        let mark = if crossed {
            MARK
        } else {
            through.inverse().apply_point(MARK)
        };
        camera.to_view(mark)
    };
    let mut previous = (
        mark_on_screen(&mut app, false),
        app.world().resource::<ViewCamera>().rotation,
    );
    let mut crossed = false;
    let mut turned = false;
    for _ in 0..400 {
        app.update();
        let (pos, shown) = player(&mut app);
        let crossing = !crossed && pos == EXIT_FLOOR;
        crossed |= crossing;
        let camera = *app.world().resource::<ViewCamera>();
        let now = (mark_on_screen(&mut app, crossed), camera.rotation);
        assert!(camera.to_view(shown).length() < 1e-3);
        if crossing {
            // The view turns with the player: the picture stays.
            assert!(
                now.0.distance(previous.0) < 0.3,
                "the picture jumped from {} to {}",
                previous.0,
                now.0
            );
            assert!((now.1 + FRAC_PI_2).abs() < 0.2, "turned with the player");
        } else {
            // Then it eases back, a little each frame.
            assert!(
                (now.1 - previous.1).abs() < 0.2,
                "the view jumped from {} to {}",
                previous.1,
                now.1
            );
        }
        turned |= crossed && now.1 < -0.5;
        previous = now;
        // North of the exit, the wall of room B at row 11 stops the player.
        if crossed && pos.y == 12 {
            break;
        }
    }
    assert!(crossed, "the player never came out of the exit");
    assert!(turned, "the view first turned with the player");
    let (pos, _) = player(&mut app);
    assert_eq!(pos.y, 12, "holding up walks north in room B again");
    let camera = *app.world().resource::<ViewCamera>();
    assert!(
        camera.rotation.abs() < 1e-5,
        "north is up: {}",
        camera.rotation
    );
}
