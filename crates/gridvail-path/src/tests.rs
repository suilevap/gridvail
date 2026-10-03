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

fn search<R: Rules>(rows: &Rows, rules: &R) -> Option<(R::Cost, Vec<Cell>)> {
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
    let no_key = StepFn::new(1u32, |_, to| (rows.at(to) != 'D').then_some(1));
    assert_eq!(search(&rows, &no_key), None);
}

#[test]
fn lexicographic_costs_prefer_fewer_doors_then_fewer_steps() {
    let rows = Rows::new(&DOORS);
    let doors_first = StepFn::new(Lex(0u32, 1u32), |_, to| match rows.at(to) {
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
    let doors_hurt = StepFn::new(0, |_, to| Some(if rows.at(to) == 'D' { 10 } else { 0 }));
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

fn shared(route: &[Cell], other: &[Cell]) -> f32 {
    let interior = &route[1..route.len() - 1];
    interior.iter().filter(|cell| other.contains(cell)).count() as f32 / interior.len() as f32
}

fn routes(rows: &Rows, options: RouteOptions) -> Vec<(u32, Vec<Cell>)> {
    let mut stream = RouteSearch::new(options);
    stream.begin(&rows.grid(), rows.find('S'), rows.find('G'));
    let mut path = Vec::new();
    let mut found = Vec::new();
    while let Some(cost) = stream.next(rows, &mut path) {
        found.push((cost, path.clone()));
    }
    found
}

#[test]
fn routes_stream_different_ways_cheapest_first() {
    let rows = Rows::new(&DOORS);
    let found = routes(&rows, RouteOptions::default());
    assert_eq!(
        found.iter().map(|(cost, _)| *cost).collect::<Vec<_>>(),
        [6, 10],
        "only two ways exist"
    );

    let rows = Rows::new(&[
        "S........",
        ".........",
        ".........",
        ".........",
        "........G",
    ]);
    let options = RouteOptions {
        max_routes: 3,
        max_shared: 0.3,
        ..RouteOptions::default()
    };
    let found = routes(&rows, options);
    assert_eq!(found.len(), 3);
    assert_eq!(found[0].0, 12, "the first route is the cheapest");
    for (i, (_, a)) in found.iter().enumerate() {
        for (_, b) in &found[i + 1..] {
            assert!(shared(b, a) <= 0.3);
        }
    }
}

#[test]
fn adjacent_ends_have_one_route() {
    let rows = Rows::new(&["SG", ".."]);
    assert_eq!(routes(&rows, RouteOptions::default()).len(), 1);
}
