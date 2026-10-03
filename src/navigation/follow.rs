//! Actors walking along planned paths.

use bevy::prelude::*;
use rand::RngExt;

use super::path::{Cell, PathSearch};
use super::{cell_of, pos_of, NavCell, NavMap, Terrain};
use crate::model::*;

/// Steps an actor may fail in a row (another actor in the way) before it
/// gives up on its goal.
pub const MAX_BLOCKED_STEPS: u8 = 3;

/// Extra cost, in steps, of planning through a closed door with a key: a
/// key is spent on it, so detours of up to this length are preferred.
pub const DOOR_COST: u32 = 10;

/// Tries at picking a random floor cell for a wanderer per action.
const WANDER_TRIES: usize = 16;

/// Working memory for planning, sized to the map so planning never
/// allocates.
#[derive(Resource, Default)]
pub struct PathPlanner {
    search: PathSearch<u32, ()>,
    cells: Vec<Cell>,
}

impl PathPlanner {
    fn fit(&mut self, nav: &NavMap) {
        if !self.search.fits(nav.grid(), 1) {
            self.search = PathSearch::with_capacity(nav.grid(), 1);
        }
        if self.cells.capacity() < nav.grid().len() {
            self.cells = Vec::with_capacity(nav.grid().len());
        }
    }

    /// Plans from `from` to `goal` into `follow.steps`; false if there is
    /// no path.
    fn plan(
        &mut self,
        nav: &NavMap,
        from: IVec2,
        goal: IVec2,
        keys: usize,
        follow: &mut PathFollow,
    ) -> bool {
        // With a key in hand, a closed door is a way through at a price.
        let terrain = if keys > 0 {
            Terrain::through_doors(nav, DOOR_COST)
        } else {
            Terrain::walls_and_doors(nav)
        };
        let found = self
            .search
            .find(
                nav.grid(),
                &terrain,
                cell_of(from),
                cell_of(goal),
                &mut self.cells,
            )
            .is_some();
        follow.steps.clear();
        follow.steps.extend(self.cells.iter().copied().map(pos_of));
        follow.next = 1;
        follow.ordered_from = None;
        follow.blocked = 0;
        found
    }
}

/// Gives every idle wanderer a random floor cell to walk to.
pub fn wander_goals(
    turn: Res<TurnState>,
    nav: Res<NavMap>,
    mut rng: ResMut<SharedRng>,
    mut wanderers: Query<&mut PathFollow, (With<Wander>, With<Active>, Without<DestroyRequested>)>,
) {
    if turn.simulation || nav.grid().is_empty() {
        return;
    }
    let (width, height) = (nav.grid().width() as i32, nav.grid().height() as i32);
    for mut follow in &mut wanderers {
        if follow.goal.is_some() {
            continue;
        }
        for _ in 0..WANDER_TRIES {
            let goal = IVec2::new(rng.0.random_range(0..width), rng.0.random_range(0..height));
            if nav.at(goal) == NavCell::Floor {
                follow.go_to(goal);
                break;
            }
        }
    }
}

/// Orders the next step along each actor's path, planning it first when
/// needed. Unreachable goals and actors blocked too often stop.
pub fn follow_paths(
    turn: Res<TurnState>,
    nav: Res<NavMap>,
    mut planner: ResMut<PathPlanner>,
    keys: Query<(), With<Key>>,
    mut walkers: Query<
        (&Pos, &mut PathFollow, &mut MoveCommand, Option<&Inventory>),
        (With<Active>, Without<DestroyRequested>),
    >,
) {
    if turn.simulation || nav.grid().is_empty() {
        return;
    }
    planner.fit(&nav);
    for (pos, mut follow, mut command, inventory) in &mut walkers {
        let Some(goal) = follow.goal else {
            continue;
        };
        if command.active {
            continue;
        }
        let pos = pos.0;
        if pos == goal {
            follow.stop();
            continue;
        }
        // Skip cells already reached (the start, or a step just taken).
        while follow.steps.get(follow.next) == Some(&pos) {
            follow.next += 1;
        }
        let off_path = follow
            .steps
            .get(follow.next)
            .is_none_or(|next| (*next - pos).abs().element_sum() != 1);
        if off_path {
            let held_keys = inventory.map_or(0, |inventory| {
                inventory
                    .0
                    .iter()
                    .filter(|&&item| keys.contains(item))
                    .count()
            });
            if !planner.plan(&nav, pos, goal, held_keys, &mut follow) {
                follow.stop();
                continue;
            }
        }
        // An order from the same cell as the last one means that step failed.
        if follow.ordered_from == Some(pos) {
            follow.blocked += 1;
            if follow.blocked >= MAX_BLOCKED_STEPS {
                follow.stop();
                continue;
            }
        } else {
            follow.blocked = 0;
        }
        follow.ordered_from = Some(pos);
        command.target = follow.steps[follow.next] - pos;
        command.relative = true;
        command.active = true;
    }
}
