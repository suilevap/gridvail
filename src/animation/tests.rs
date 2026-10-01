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

fn player_cell(app: &mut App) -> IVec2 {
    let world = app.world_mut();
    world
        .query_filtered::<&Pos, With<Player>>()
        .single(world)
        .unwrap()
        .0
}

/// Frames between the player's steps while movement keys are held
/// (alternating left and right so walls never block the walk). No renderer
/// is installed: pacing depends only on the animation step.
fn held_step_gaps(motion: Option<MotionStyle>) -> Vec<u32> {
    let mut app = boot(motion);
    let mut gaps = Vec::new();
    let mut last = player_cell(&mut app);
    let mut frames = 0;
    let mut key = KeyCode::ArrowRight;
    while gaps.len() < 4 && frames < 600 {
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release_all();
            keys.press(key);
        }
        app.update();
        frames += 1;
        let now = player_cell(&mut app);
        if now != last {
            gaps.push(frames);
            frames = 0;
            last = now;
            key = if key == KeyCode::ArrowRight {
                KeyCode::ArrowLeft
            } else {
                KeyCode::ArrowRight
            };
        }
    }
    gaps.remove(0);
    gaps
}

fn locomotion_with_step(seconds: f32) -> MotionStyle {
    let MotionStyle::Locomotion {
        start,
        ramp,
        turn_keep,
        stop_frequency,
        stop_damping,
        bob,
        ..
    } = MotionStyle::default()
    else {
        unreachable!("the default style is locomotion");
    };
    MotionStyle::Locomotion {
        step: seconds,
        start,
        ramp,
        turn_keep,
        stop_frequency,
        stop_damping,
        bob,
    }
}

fn tween(seconds: f32) -> MotionStyle {
    MotionStyle::Tween {
        duration: seconds,
        easing: Easing::EaseOut,
    }
}

#[test]
fn held_movement_waits_for_the_step_animation() {
    // 16 ms frames. A 0.12 s step reaches the 20 ms lead after 7 frames and
    // the next move happens one frame later.
    assert_eq!(
        held_step_gaps(Some(locomotion_with_step(0.12))),
        vec![7, 7, 7]
    );
    // Slower steps slow the turns down to match.
    assert_eq!(
        held_step_gaps(Some(locomotion_with_step(0.3))),
        vec![18, 18, 18]
    );
}

#[test]
fn turns_do_not_wait_without_animation() {
    // No animation step at all, or a style that does not animate: a held key
    // moves every frame.
    assert_eq!(held_step_gaps(None), vec![1, 1, 1]);
    assert_eq!(held_step_gaps(Some(MotionStyle::Snap)), vec![1, 1, 1]);
}

#[test]
fn movers_get_animated_positions() {
    let mut app = boot(Some(MotionStyle::default()));
    let world = app.world_mut();
    let unanimated = world
        .query_filtered::<(), (With<PrevPos>, Without<AnimatedPos>)>()
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

/// Holds a movement key until the player steps, pushes `block` two cells as
/// a consequence, then keeps walking. Returns the frames until the player's
/// next step and how far the block was from its cell when that turn began.
fn push_then_step(non_blocking: bool) -> (u32, f32) {
    let mut app = boot(Some(tween(0.1)));
    let mut block = app.world_mut().spawn((
        Pos(IVec2::new(2, 2)),
        Glyph::new('■', 1, 7),
        // Two cells at 0.1 s per cell: twice as long as the player's step.
        ObjectMotion(MotionStyle::from_name("linear").unwrap()),
    ));
    if non_blocking {
        block.insert(NonBlockingAnimation);
    }
    let block = block.id();
    let mut key = KeyCode::ArrowRight;
    let mut last = player_cell(&mut app);
    let mut pushed = false;
    let mut frames = 0;
    let mut gap_to_cell = f32::NAN;
    for _ in 0..600 {
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release_all();
            keys.press(key);
        }
        let shown = app.world().get::<AnimatedPos>(block).unwrap().position;
        app.update();
        frames += 1;
        let now = player_cell(&mut app);
        if now == last {
            continue;
        }
        if pushed {
            // This turn began this frame; the block was last shown `shown`.
            gap_to_cell = shown.distance(Vec2::new(4.0, 2.0));
            break;
        }
        app.world_mut().get_mut::<Pos>(block).unwrap().0 = IVec2::new(4, 2);
        pushed = true;
        frames = 0;
        last = now;
        key = KeyCode::ArrowLeft;
    }
    (frames, gap_to_cell)
}

#[test]
fn the_next_turn_waits_for_every_unfinished_animation() {
    // Without the block the player steps every 7 frames (see above). The
    // block's 0.2 s push holds the next turn until it has nearly arrived.
    let (frames, gap) = push_then_step(false);
    assert!(frames >= 12, "turn started after {frames} frames");
    assert!(gap < 0.25, "block still {gap} cells from its cell");

    // Ambient motion does not hold turns: back to the player's own 0.1 s
    // step (20 ms lead, 16 ms frames).
    let (frames, _) = push_then_step(true);
    assert_eq!(frames, 5);
}

#[test]
fn a_blocked_move_bumps_toward_the_blocker() {
    let mut app = boot(Some(tween(0.1)));
    let start = player_cell(&mut app);
    // A crate right of the player, registered in the map like any collider.
    let blocker = app
        .world_mut()
        .spawn((Pos(start + IVec2::X), Collider, Glyph::new('■', 1, 7)))
        .id();
    app.world_mut()
        .resource_mut::<MapGrid>()
        .set(start + IVec2::X, blocker);

    let mut shown = Vec::new();
    for frame in 0..20 {
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release_all();
            if frame == 0 {
                keys.press(KeyCode::ArrowRight);
            }
        }
        app.update();
        let world = app.world_mut();
        let (pos, animated) = world
            .query_filtered::<(&Pos, &AnimatedPos), With<Player>>()
            .single(world)
            .unwrap();
        assert_eq!(pos.0, start, "the move was blocked");
        shown.push(animated.position.x - start.x as f32);
    }
    let peak = shown.iter().copied().fold(0.0_f32, f32::max);
    assert!(peak > 0.15 && peak <= 0.2, "bump peak {peak}");
    assert_eq!(*shown.last().unwrap(), 0.0, "bump returned");
}

#[test]
fn a_path_override_shapes_cell_moves() {
    let mut app = boot(Some(MotionStyle::default()));
    let linear = MotionStyle::from_name("linear").unwrap();
    let block = app
        .world_mut()
        .spawn((
            Pos(IVec2::new(2, 2)),
            Glyph::new('■', 1, 7),
            ObjectMotion(linear),
            MovePath(Path::Arc { bulge: 0.5 }),
        ))
        .id();
    app.update();
    // Two cells at 0.1 s per cell: about halfway after 6 frames of 16 ms.
    let path = move_and_watch(&mut app, block, IVec2::new(4, 2), 20);
    let off_line = path.iter().map(|p| (p.y - 2.0).abs()).fold(0.0, f32::max);
    assert!(off_line > 0.4, "arc did not curve: {off_line}");
    assert_eq!(*path.last().unwrap(), Vec2::new(4.0, 2.0));
}
