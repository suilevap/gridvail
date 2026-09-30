use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;

use super::*;
use crate::app::GamePlugin;
use crate::model::*;

fn boot(motion: Option<MotionStyle>) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            16,
        )))
        .add_plugins(GamePlugin);
    if let Some(motion) = motion {
        app.insert_resource(motion)
            .add_plugins(ObjectAnimationPlugin);
    }
    for _ in 0..64 {
        app.update();
    }
    app
}

#[test]
fn every_object_gets_an_animated_position() {
    let mut app = boot(Some(MotionStyle::default()));
    let world = app.world_mut();
    let unanimated = world
        .query_filtered::<(), (With<Pos>, Without<AnimatedPos>)>()
        .iter(world)
        .count();
    assert_eq!(unanimated, 0);
    let (pos, shown) = world
        .query_filtered::<(&Pos, &AnimatedPos), With<Player>>()
        .single(world)
        .unwrap();
    assert_eq!(shown.position, pos.0.as_vec2());
}

/// Moves `entity` to `to` (outside any turn, the way a level script would)
/// and returns its shown position over the next `frames` frames.
fn move_and_watch(app: &mut App, entity: Entity, to: IVec2, frames: usize) -> Vec<Vec2> {
    app.world_mut().get_mut::<Pos>(entity).unwrap().0 = to;
    (0..frames)
        .map(|_| {
            app.update();
            app.world().get::<AnimatedPos>(entity).unwrap().position
        })
        .collect()
}

#[test]
fn any_object_glides_several_cells_at_once() {
    let linear = MotionStyle::from_name("linear").unwrap();
    let mut app = boot(Some(MotionStyle::default()));
    // A wall-like block that is not an actor, with its own style.
    let block = app
        .world_mut()
        .spawn((
            Pos(IVec2::new(2, 2)),
            Glyph::new('#', 1, 7),
            ObjectMotion(linear),
        ))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<AnimatedPos>(block).unwrap().position,
        Vec2::new(2.0, 2.0)
    );

    // Four cells at 0.1 s per cell, in 16 ms frames.
    let path = move_and_watch(&mut app, block, IVec2::new(6, 2), 30);
    let xs: Vec<f32> = path.iter().map(|p| p.x).collect();
    assert!(xs.windows(2).all(|w| w[1] >= w[0]), "glide went backwards");
    assert!(
        (xs[11] - 4.0).abs() < 0.2,
        "halfway after ~0.2 s: {}",
        xs[11]
    );
    assert!(xs[22] < 6.0, "arrived too early: {}", xs[22]);
    assert_eq!(*path.last().unwrap(), Vec2::new(6.0, 2.0));
}

#[test]
fn moves_across_the_map_edge_teleport() {
    let mut app = boot(Some(MotionStyle::default()));
    let block = app
        .world_mut()
        .spawn((Pos(IVec2::new(0, 2)), Glyph::new('#', 1, 7)))
        .id();
    app.update();
    let width = app.world().resource::<MapGrid>().width;
    let path = move_and_watch(&mut app, block, IVec2::new(width - 1, 2), 1);
    assert_eq!(path[0], Vec2::new((width - 1) as f32, 2.0));
}
