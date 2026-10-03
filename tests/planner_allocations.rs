//! A warm `GridPlanner` plan reuses its buffers: a hunter replanning every
//! turn costs no allocations.

use bevy::prelude::*;
use pav_ecs_game_bevy_port::ai::{Cell, GridPlanner, MapSnapshot, Planner};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell as Flag;
use std::sync::atomic::{AtomicUsize, Ordering};

struct CountingAllocator;

thread_local! {
    static TRACKING: Flag<bool> = const { Flag::new(false) };
}

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACKING.with(Flag::get) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if TRACKING.with(Flag::get) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// A walled 40x20 room with a long wall to route around and a door in it.
fn room() -> MapSnapshot {
    MapSnapshot::from_fn(IVec2::new(40, 20), |at| {
        if at.x == 0 || at.y == 0 || at.x == 39 || at.y == 19 {
            Cell::Wall
        } else if at.x == 20 && at.y == 10 {
            Cell::ClosedDoor
        } else if at.x == 20 && at.y > 2 {
            Cell::Wall
        } else {
            Cell::Floor
        }
    })
}

#[test]
fn a_warm_plan_does_not_allocate() {
    let map = room();
    let to = IVec2::new(37, 15);
    // Warm-up sizes the scratch and the path buffer.
    let mut path = Vec::new();
    GridPlanner.plan(&map, IVec2::new(2, 15), to, &mut path);
    assert!(!path.is_empty());

    ALLOCATIONS.store(0, Ordering::Relaxed);
    TRACKING.with(|tracking| tracking.set(true));
    for x in 2..18 {
        path.clear();
        GridPlanner.plan(&map, IVec2::new(x, 15), to, &mut path);
    }
    TRACKING.with(|tracking| tracking.set(false));
    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed), 0);
}
