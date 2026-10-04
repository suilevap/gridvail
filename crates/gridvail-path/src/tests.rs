use crate::grid::{Cell, Grid};
use crate::*;

/// A map from rows of text: `#` wall, `D` closed door, `k` key, `.` floor.
struct Rows(Vec<Vec<char>>);

impl Rows {
    fn new(rows: &[&str]) -> Self {
        Self(rows.iter().map(|row| row.chars().collect()).collect())
    }

    fn grid(&self) -> Grid {
        Grid::new(self.0[0].len() as u32, self.0.len() as u32)
    }

    fn at(&self, cell: Cell) -> char {
        self.0[cell.y as usize][cell.x as usize]
    }

    fn find(&self, ch: char) -> Cell {
        for (y, row) in self.0.iter().enumerate() {
            if let Some(x) = row.iter().position(|c| *c == ch) {
                return Cell::new(x as i32, y as i32);
            }
        }
        panic!("no {ch}");
    }

    fn count(&self, path: &[Cell], ch: char) -> usize {
        path.iter().filter(|cell| self.at(**cell) == ch).count()
    }
}

/// Walls block; everything else costs 1.
impl Rules for Rows {
    type Node = Cell;
    type Cost = u32;
    type State = ();

    fn state_count(&self) -> usize {
        1
    }

    fn state_index(&self, _state: &()) -> usize {
        0
    }

    fn start_state(&self, _start: Cell) {}

    fn step(&self, _from: Cell, to: Cell, _state: &()) -> Option<(u32, ())> {
        (self.at(to) != '#').then_some((1, ()))
    }

    fn min_step_cost(&self) -> u32 {
        1
    }
}

/// Passes at most `max` closed doors.
struct DoorLimit<'a> {
    rows: &'a Rows,
    max: u8,
}

impl Rules for DoorLimit<'_> {
    type Node = Cell;
    type Cost = u32;
    type State = u8;

    fn state_count(&self) -> usize {
        self.max as usize + 1
    }

    fn state_index(&self, doors: &u8) -> usize {
        *doors as usize
    }

    fn start_state(&self, _start: Cell) -> u8 {
        0
    }

    fn step(&self, _from: Cell, to: Cell, doors: &u8) -> Option<(u32, u8)> {
        let doors = doors + u8::from(self.rows.at(to) == 'D');
        (doors <= self.max).then_some((0, doors))
    }
}

/// Doors open only after stepping on the key.
struct NeedsKey<'a>(&'a Rows);

impl Rules for NeedsKey<'_> {
    type Node = Cell;
    type Cost = u32;
    type State = bool;

    fn state_count(&self) -> usize {
        2
    }

    fn state_index(&self, has_key: &bool) -> usize {
        usize::from(*has_key)
    }

    fn start_state(&self, _start: Cell) -> bool {
        false
    }

    fn step(&self, _from: Cell, to: Cell, has_key: &bool) -> Option<(u32, bool)> {
        match self.0.at(to) {
            'D' if !has_key => None,
            'k' => Some((0, true)),
            _ => Some((0, *has_key)),
        }
    }
}

// Two ways from S to G: through two doors (6 steps) or around (10).
const DOORS: [&str; 3] = ["S.D.D.G", ".#####.", "......."];

fn search<TestRules: Rules<Node = Cell>>(
    rows: &Rows,
    rules: &TestRules,
) -> Option<(TestRules::Cost, Vec<Cell>)> {
    let mut path = Vec::new();
    PathSearch::new()
        .find(
            &rows.grid(),
            rules,
            rows.find('S'),
            rows.find('G'),
            &mut path,
        )
        .map(|cost| (cost, path))
}

#[test]
fn finds_the_cheapest_path_around_walls() {
    let rows = Rows::new(&["S.#..", "..#..", "....G"]);
    let (cost, path) = search(&rows, &rows).unwrap();
    assert_eq!(cost, 6);
    assert_eq!(path.len(), 7);
    assert_eq!(path.first(), Some(&rows.find('S')));
    assert_eq!(path.last(), Some(&rows.find('G')));
    assert!(path.iter().all(|cell| rows.at(*cell) != '#'));
    assert!(path
        .windows(2)
        .all(|step| rows.grid().distance(step[0], step[1]) == 1));
}

#[test]
fn unreachable_or_off_grid_ends_have_no_path() {
    let rows = Rows::new(&["S#G"]);
    assert_eq!(search(&rows, &rows), None);
    let mut path = vec![Cell::new(9, 9)];
    let found = PathSearch::new().find(
        &rows.grid(),
        &rows,
        Cell::new(0, 0),
        Cell::new(5, 0),
        &mut path,
    );
    assert_eq!(found, None);
    assert!(path.is_empty());
}

#[test]
fn start_on_goal_is_a_path_of_one_cell() {
    let rows = Rows::new(&["S.."]);
    let mut path = Vec::new();
    let start = rows.find('S');
    assert_eq!(
        PathSearch::new().find(&rows.grid(), &rows, start, start, &mut path),
        Some(0)
    );
    assert_eq!(path, [start]);
}

#[test]
fn state_layers_limit_closed_doors() {
    let rows = Rows::new(&DOORS);
    let (cost, path) = search(&rows, &rows).unwrap();
    assert_eq!((cost, rows.count(&path, 'D')), (6, 2));
    for max in [0, 1] {
        let (cost, path) = search(&rows, &(&rows, DoorLimit { rows: &rows, max })).unwrap();
        assert_eq!((cost, rows.count(&path, 'D')), (10, 0), "max {max}");
    }
    let (cost, path) = search(
        &rows,
        &(
            &rows,
            DoorLimit {
                rows: &rows,
                max: 2,
            },
        ),
    )
    .unwrap();
    assert_eq!((cost, rows.count(&path, 'D')), (6, 2));
}

#[test]
fn a_cell_is_searched_again_with_a_different_state() {
    // Fetch the key behind the start, then walk back over the same cells.
    let rows = Rows::new(&["k.S.D.G"]);
    let (cost, path) = search(&rows, &(&rows, NeedsKey(&rows))).unwrap();
    assert_eq!(cost, 8);
    assert_eq!(
        path.iter().map(|cell| cell.x).collect::<Vec<_>>(),
        [2, 1, 0, 1, 2, 3, 4, 5, 6]
    );
    // Without the key layer it is impossible.
    let no_key = StepFn::new(1u32, |_, to: Cell| (rows.at(to) != 'D').then_some(1));
    assert_eq!(search(&rows, &no_key), None);
}

#[test]
fn lexicographic_costs_prefer_fewer_doors_then_fewer_steps() {
    let rows = Rows::new(&DOORS);
    let doors_first = StepFn::new(Lex(0u32, 1u32), |_, to: Cell| match rows.at(to) {
        '#' => None,
        'D' => Some(Lex(1, 1)),
        _ => Some(Lex(0, 1)),
    });
    let (cost, path) = search(&rows, &doors_first).unwrap();
    assert_eq!(cost, Lex(0, 10));
    assert_eq!(rows.count(&path, 'D'), 0);
}

#[test]
fn closure_costs_steer_the_path() {
    let rows = Rows::new(&DOORS);
    let doors_hurt = StepFn::new(0u32, |_, to: Cell| {
        Some(if rows.at(to) == 'D' { 10 } else { 0 })
    });
    assert_eq!(search(&rows, &(&rows, doors_hurt)).unwrap().0, 10);
}

#[test]
fn one_search_serves_many_queries() {
    let rows = Rows::new(&["S.#..", "..#..", "....G"]);
    let grid = rows.grid();
    let mut search = PathSearch::new();
    let mut path = Vec::new();
    for _ in 0..3 {
        assert_eq!(
            search.find(&grid, &rows, rows.find('S'), rows.find('G'), &mut path),
            Some(6)
        );
        assert_eq!(
            search.find(&grid, &rows, rows.find('G'), rows.find('S'), &mut path),
            Some(6)
        );
        assert_eq!(
            search.find(&grid, &rows, Cell::new(2, 0), rows.find('S'), &mut path),
            Some(2)
        );
    }
}

/// Every path [`PathSearch::next`] streams: cost, final state, cells.
fn stream<TestRules: Rules<Node = Cell>>(
    rows: &Rows,
    rules: &TestRules,
) -> Vec<(TestRules::Cost, TestRules::State, Vec<Cell>)> {
    let mut search = PathSearch::new();
    let mut path = Vec::new();
    let mut found = Vec::new();
    let grid = rows.grid();
    assert!(search.begin(&grid, rules, rows.find('S'), rows.find('G')));
    while let Some((cost, state)) = search.next(&grid, rules, &mut path) {
        found.push((cost, state, path.clone()));
    }
    assert!(
        search.next(&grid, rules, &mut path).is_none(),
        "an ended stream stays ended"
    );
    found
}

#[test]
fn one_search_streams_the_cheapest_path_per_final_state() {
    let rows = Rows::new(&DOORS);
    let found = stream(
        &rows,
        &(
            &rows,
            DoorLimit {
                rows: &rows,
                max: 2,
            },
        ),
    );
    // Through both doors, around, and around after stepping into the first
    // door and back: legal for these rules, which only count doors.
    let summary: Vec<_> = found
        .iter()
        .map(|(cost, (_, doors), _)| (*cost, *doors))
        .collect();
    assert_eq!(summary, [(6, 2), (10, 0), (14, 1)]);
    for (_, (_, doors), path) in &found {
        assert_eq!(rows.count(path, 'D'), *doors as usize);
        assert_eq!(path.first(), Some(&rows.find('S')));
        assert_eq!(path.last(), Some(&rows.find('G')));
    }
    // Declaring that fewer doors beat more drops the pointless detour.
    let pruned = (
        &rows,
        FewerDoorsDominate(DoorLimit {
            rows: &rows,
            max: 2,
        }),
    );
    let summary: Vec<_> = stream(&rows, &pruned)
        .iter()
        .map(|(cost, (_, doors), _)| (*cost, *doors))
        .collect();
    assert_eq!(summary, [(6, 2), (10, 0)]);
}

#[test]
fn streamed_paths_come_cheapest_first_and_skip_dominated_ones() {
    let rows = Rows::new(&DOORS);
    // Doors cost 10 more each: going around is cheaper.
    let doors_hurt = StepFn::new(0u32, |_, to: Cell| {
        Some(if rows.at(to) == 'D' { 10 } else { 0 })
    });
    let plain = (
        &doors_hurt,
        (
            &rows,
            DoorLimit {
                rows: &rows,
                max: 2,
            },
        ),
    );
    let costs: Vec<_> = stream(&rows, &plain)
        .iter()
        .map(|(cost, ..)| *cost)
        .collect();
    assert_eq!(costs, [10, 24, 26]);
    // With fewer doors dominating more, every way through doors is
    // pointless after the cheaper one without any.
    let pruned = (
        &doors_hurt,
        (
            &rows,
            FewerDoorsDominate(DoorLimit {
                rows: &rows,
                max: 2,
            }),
        ),
    );
    let costs: Vec<_> = stream(&rows, &pruned)
        .iter()
        .map(|(cost, ..)| *cost)
        .collect();
    assert_eq!(costs, [10]);
}

#[test]
fn stateless_rules_stream_a_single_path() {
    let rows = Rows::new(&["S........", ".........", "........G"]);
    let found = stream(&rows, &rows);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, 10);
}

#[test]
fn begin_rejects_ends_off_the_grid() {
    let rows = Rows::new(&["S.G"]);
    let mut search = PathSearch::new();
    let mut path = vec![Cell::new(1, 0)];
    assert!(!search.begin(&rows.grid(), &rows, Cell::new(0, 0), Cell::new(9, 0)));
    assert_eq!(search.next(&rows.grid(), &rows, &mut path), None);
    assert!(path.is_empty());
}

/// [`DoorLimit`] where fewer doors so far dominates more.
struct FewerDoorsDominate<'a>(DoorLimit<'a>);

impl Rules for FewerDoorsDominate<'_> {
    type Node = Cell;
    type Cost = u32;
    type State = u8;

    fn state_count(&self) -> usize {
        self.0.state_count()
    }

    fn state_index(&self, doors: &u8) -> usize {
        self.0.state_index(doors)
    }

    fn start_state(&self, start: Cell) -> u8 {
        self.0.start_state(start)
    }

    fn step(&self, from: Cell, to: Cell, doors: &u8) -> Option<(u32, u8)> {
        self.0.step(from, to, doors)
    }

    fn dominates(&self, dominant: usize, dominated: usize) -> bool {
        dominant < dominated
    }
}

#[test]
fn dominated_states_are_skipped_without_changing_results() {
    // Doors everywhere and the goal walled off, so the whole state space is
    // searched: without dominance, every cell at every door count.
    let rows = Rows::new(&[
        "S.D.D.D.D.",
        ".D.D.D.D.D",
        "D.D.D.D.D.",
        ".D.D.D.D.#",
        "D.D.D.D.#G",
    ]);
    let grid = rows.grid();
    let mut path = Vec::new();
    for max in [1u8, 3, 6] {
        let plain = (&rows, DoorLimit { rows: &rows, max });
        let pruned = (&rows, FewerDoorsDominate(DoorLimit { rows: &rows, max }));
        let mut search = PathSearch::new();
        let expected: Option<u32> =
            search.find(&grid, &plain, rows.find('S'), rows.find('G'), &mut path);
        let plain_expanded = search.expanded();
        let found = search.find(&grid, &pruned, rows.find('S'), rows.find('G'), &mut path);
        assert_eq!((found, expected), (None, None), "max {max}");
        assert!(
            search.expanded() < plain_expanded,
            "max {max}: {} < {plain_expanded}",
            search.expanded()
        );
    }
}

#[test]
fn tuple_dominance_needs_every_part() {
    let rows = Rows::new(&DOORS);
    let pair = (
        FewerDoorsDominate(DoorLimit {
            rows: &rows,
            max: 2,
        }),
        NeedsKey(&rows),
    );
    // Layers are doors * 2 + has_key.
    assert!(pair.dominates(0, 2), "fewer doors, same key");
    assert!(
        !pair.dominates(1, 2),
        "fewer doors but NeedsKey never dominates"
    );
    assert!(!pair.dominates(2, 2), "a state does not dominate itself");
    assert!(!pair.dominates(2, 0));
}

/// Teleports from one cell to another for a fixed cost.
struct Teleport {
    from: Cell,
    to: Cell,
    cost: u32,
}

impl Rules for Teleport {
    type Node = Cell;
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
        let jump = (from, to) == (self.from, self.to);
        Some((if jump { self.cost } else { 0 }, ()))
    }

    fn for_each_jump(&self, from: Cell, _state: &(), visit: &mut dyn FnMut(Cell)) {
        if from == self.from {
            visit(self.to);
        }
    }

    fn has_jumps(&self) -> bool {
        true
    }
}

#[test]
fn jumps_cross_walls_and_are_priced_by_every_rule() {
    // The wall column is closed: only the teleport gets across.
    let rows = Rows::new(&["S.#..", "..#.G", "..#.."]);
    assert_eq!(search(&rows, &rows), None);
    let teleport = Teleport {
        from: Cell::new(1, 1),
        to: Cell::new(3, 0),
        cost: 5,
    };
    let (cost, path) = search(&rows, &(&rows, teleport)).unwrap();
    // 2 steps to the pad, the jump (1 from `Rows` + 5), 2 steps to G.
    assert_eq!(cost, 2 + 6 + 2);
    let jump = path
        .windows(2)
        .position(|step| rows.grid().distance(step[0], step[1]) > 1)
        .expect("a jump in the path");
    assert_eq!(
        (path[jump], path[jump + 1]),
        (Cell::new(1, 1), Cell::new(3, 0))
    );

    // A wall at the landing forbids the jump: `Rows` prices it too.
    let blocked = Rows::new(&["S.##.", "..#.G", "..#.."]);
    let teleport = Teleport {
        from: Cell::new(1, 1),
        to: Cell::new(3, 0),
        cost: 5,
    };
    assert_eq!(search(&blocked, &(&blocked, teleport)), None);
}

#[test]
fn jumps_that_beat_walking_are_still_found() {
    // A cheap teleport across an open room: a distance estimate would think
    // the far side is far and could miss it; the search ignores estimates.
    let rows = Rows::new(&["S........G"]);
    let teleport = Teleport {
        from: Cell::new(1, 0),
        to: Cell::new(8, 0),
        cost: 0,
    };
    let (cost, path) = search(&rows, &(&rows, teleport)).unwrap();
    assert_eq!(cost, 3);
    assert_eq!(path.len(), 4);
}

#[test]
fn dominance_keeps_the_cheapest_path_within_limits() {
    let rows = Rows::new(&[
        "S.D.D.D.D.",
        ".D.D.D.D.D",
        "D.D.D.D.D.",
        ".D.D.D.D.D",
        "D.D.D.D.DG",
    ]);
    for max in [0u8, 1, 2, 4] {
        let plain = search(&rows, &(&rows, DoorLimit { rows: &rows, max }));
        let pruned = search(
            &rows,
            &(&rows, FewerDoorsDominate(DoorLimit { rows: &rows, max })),
        );
        assert_eq!(
            pruned.as_ref().map(|(cost, _)| *cost),
            plain.map(|(cost, _)| cost),
            "max {max}"
        );
        if let Some((_, path)) = pruned {
            assert!(rows.count(&path, 'D') <= max as usize);
        }
    }
}
