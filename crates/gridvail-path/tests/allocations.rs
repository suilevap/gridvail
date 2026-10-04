//! Repeated searches and path streams allocate nothing once warmed up.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell as StdCell;
use std::sync::atomic::{AtomicUsize, Ordering};

use gridvail_path::*;

struct Counting;

thread_local! {
    static TRACKING: StdCell<bool> = const { StdCell::new(false) };
}
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACKING.with(StdCell::get) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if TRACKING.with(StdCell::get) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        System.realloc(ptr, layout, size)
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn allocations(run: impl FnOnce()) -> usize {
    ALLOCATIONS.store(0, Ordering::Relaxed);
    TRACKING.with(|tracking| tracking.set(true));
    run();
    TRACKING.with(|tracking| tracking.set(false));
    ALLOCATIONS.load(Ordering::Relaxed)
}

const W: u32 = 64;
const H: u32 = 32;

/// A maze-like grid: vertical walls with alternating gaps; every 7th column
/// costs 3 to enter, and doors (column 20) count toward a limit of 2.
fn wall(cell: Cell) -> bool {
    cell.x % 8 == 4 && (cell.y + 2) % (H as i32) > 2 && !(cell.x % 16 == 4 && cell.y > H as i32 - 4)
}

struct Doors;

impl Rules for Doors {
    type Cost = u32;
    type State = u8;

    fn state_count(&self) -> usize {
        3
    }

    fn state_index(&self, doors: &u8) -> usize {
        *doors as usize
    }

    fn start_state(&self, _start: Cell) -> u8 {
        0
    }

    fn step(&self, _from: Cell, to: Cell, doors: &u8) -> Option<(u32, u8)> {
        let doors = doors + u8::from(to.x == 20);
        (doors <= 2).then_some((0, doors))
    }

    fn dominates(&self, a: usize, b: usize) -> bool {
        a < b
    }
}

/// A teleport from the top-left area to the bottom-right one.
struct Pads;

impl Rules for Pads {
    type Cost = u32;
    type State = ();

    fn state_count(&self) -> usize {
        1
    }

    fn state_index(&self, _state: &()) -> usize {
        0
    }

    fn start_state(&self, _start: Cell) {}

    fn step(&self, from: Cell, to: Cell, _state: &()) -> Option<(u32, ())> {
        Some((
            if from.x.abs_diff(to.x) + from.y.abs_diff(to.y) > 1 {
                6
            } else {
                0
            },
            (),
        ))
    }

    fn for_each_jump(&self, from: Cell, _state: &(), visit: &mut dyn FnMut(Cell)) {
        if from == Cell::new(2, 2) {
            visit(Cell::new(W as i32 - 3, H as i32 - 3));
        }
    }

    fn has_jumps(&self) -> bool {
        true
    }
}

#[test]
fn warmed_up_searches_and_path_streams_allocate_nothing() {
    let grid = Grid::new(W, H);
    let terrain = StepFn::new(1u32, |_, to| {
        (!wall(to)).then_some(if to.x % 7 == 0 { 3 } else { 1 })
    });
    let rules = (&terrain, Doors);
    let jumping = (&rules, Pads);
    let ends = [
        (Cell::new(0, 0), Cell::new(W as i32 - 1, H as i32 - 1)),
        (Cell::new(W as i32 - 1, 0), Cell::new(0, H as i32 - 1)),
        (Cell::new(10, 10), Cell::new(50, 3)),
        (Cell::new(1, 1), Cell::new(4, 0)), // a wall cell: unreachable
    ];
    let mut search = PathSearch::new();
    let mut stream = PathSearch::new();
    let mut jump_search = PathSearch::new();
    let mut path = Vec::new();

    let run = |search: &mut PathSearch<u32, ((), u8)>,
               stream: &mut PathSearch<u32, ((), u8)>,
               path: &mut Vec<Cell>| {
        let mut found = 0;
        for &(start, goal) in &ends {
            found += usize::from(search.find(&grid, &rules, start, goal, path).is_some());
            stream.begin(&grid, &rules, start, goal);
            while stream.next(&rules, path).is_some() {
                found += 1;
            }
        }
        found
    };
    let run_jumps = |search: &mut PathSearch<u32, (((), u8), ())>, path: &mut Vec<Cell>| {
        ends.iter()
            .filter(|(start, goal)| search.find(&grid, &jumping, *start, *goal, path).is_some())
            .count()
    };

    let warm = run(&mut search, &mut stream, &mut path) + run_jumps(&mut jump_search, &mut path);
    assert!(warm >= 6, "the grid has paths: {warm}");
    let mut again = 0;
    let count = allocations(|| {
        for _ in 0..50 {
            again =
                run(&mut search, &mut stream, &mut path) + run_jumps(&mut jump_search, &mut path);
        }
    });
    assert_eq!(again, warm, "same results every time");
    assert_eq!(count, 0, "allocations after warm-up");
}
