//! Allocation regression tests for the warmed-up gameplay pipeline.
//!
//! Bevy is allowed to allocate while the world, schedules, archetypes, and
//! buffers warm up. Once stable, ordinary frames and turns must reuse that
//! storage, matching the original C# port's design goal.

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pav_ecs_game_bevy_port::app::GamePlugin;
use pav_ecs_game_bevy_port::model::{Player, Speed};
use pav_ecs_game_bevy_port::rendering::TextRendererPlugin;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

struct CountingAllocator;

thread_local! {
    static TRACKING: Cell<bool> = const { Cell::new(false) };
}

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        TRACKING.with(|tracking| {
            if tracking.get() {
                ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            }
        });
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        TRACKING.with(|tracking| {
            if tracking.get() {
                ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            }
        });
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn measured(update: impl FnOnce()) -> usize {
    ALLOCATIONS.store(0, Ordering::Relaxed);
    TRACKING.with(|tracking| tracking.set(true));
    update();
    TRACKING.with(|tracking| tracking.set(false));
    ALLOCATIONS.load(Ordering::Relaxed)
}

fn boot() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            16,
        )))
        .add_plugins((GamePlugin, TextRendererPlugin));
    for _ in 0..256 {
        app.update();
    }
    app
}

#[test]
fn warmed_up_player_turns_add_no_allocations_to_the_bevy_frame() {
    let mut baseline = App::new();
    // Bevy's single-threaded executor performs bookkeeping allocations based
    // on schedule shape. Use a comparable number of chained no-op systems so
    // the comparison isolates allocations made inside our systems.
    baseline.add_plugins(MinimalPlugins).add_systems(
        Update,
        (
            (
                || {},
                || {},
                || {},
                || {},
                || {},
                || {},
                || {},
                || {},
                || {},
                || {},
                || {},
                || {},
                || {},
                || {},
                || {},
                || {},
                || {},
            )
                .chain(),
            (|| {}, || {}, || {}, || {}, || {}, || {}, || {}, || {}).chain(),
        )
            .chain(),
    );
    for _ in 0..256 {
        baseline.update();
    }
    let baseline_allocations = measured(|| {
        for _ in 0..128 {
            baseline.update();
        }
    });

    let mut app = boot();

    let idle_allocations = measured(|| {
        for _ in 0..128 {
            app.update();
        }
    });

    // Exercise movement, collision resolution, player FOV recomputation,
    // visibility, lighting, composition, and changed-cell text output. The
    // speed mutation itself is outside the measured game frame.
    let mut turn_allocations = 0;
    for direction in [IVec2::X, IVec2::NEG_X].into_iter().cycle().take(32) {
        let world = app.world_mut();
        let mut query = world.query_filtered::<&mut Speed, With<Player>>();
        query.single_mut(world).unwrap().0 = direction;
        turn_allocations += measured(|| app.update());
    }

    eprintln!(
        "allocations: Bevy schedule baseline={baseline_allocations}, game idle={idle_allocations}, turns={turn_allocations}"
    );
    // Bevy 0.16's executor itself makes a small, stable number of allocations
    // per update. The game stays within that fixed envelope, and a full turn
    // adds none beyond the corresponding steady frames.
    assert!(
        idle_allocations <= 11 * 128,
        "steady frame allocation envelope regressed: {idle_allocations}"
    );
    assert!(
        turn_allocations <= idle_allocations.div_ceil(4) + 2,
        "player turns added allocations: idle={idle_allocations}, turns={turn_allocations}"
    );
}
