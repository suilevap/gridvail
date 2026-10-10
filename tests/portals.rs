//! Walking through a portal on the bundled `portals.txt` map, with animation.

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pav_ecs_game_bevy_port::animation::ObjectAnimationPlugin;
use pav_ecs_game_bevy_port::app::{GamePlugin, MapText};
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
