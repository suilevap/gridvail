//! A 4-connected grid as a [`Space`]: cells move to their up, down, left and
//! right neighbours, and the Manhattan distance bounds the moves needed.

use crate::Space;

/// A grid cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Cell {
    pub x: i32,
    pub y: i32,
}

impl Cell {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Size of a 4-connected grid with cells `(0, 0)` to `(width - 1, height - 1)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    width: i32,
    height: i32,
}

impl Grid {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width: i32::try_from(width).expect("grid width fits i32"),
            height: i32::try_from(height).expect("grid height fits i32"),
        }
    }

    pub fn width(&self) -> u32 {
        self.width as u32
    }

    pub fn height(&self) -> u32 {
        self.height as u32
    }

    /// Number of cells.
    pub fn len(&self) -> usize {
        self.width as usize * self.height as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn contains(&self, cell: Cell) -> bool {
        cell.x >= 0 && cell.y >= 0 && cell.x < self.width && cell.y < self.height
    }

    /// Row-major index of `cell`, if it is on the grid.
    pub fn index(&self, cell: Cell) -> Option<usize> {
        self.contains(cell)
            .then(|| cell.y as usize * self.width as usize + cell.x as usize)
    }

    /// The cell at row-major `index`.
    pub fn cell(&self, index: usize) -> Cell {
        let width = self.width as usize;
        Cell::new((index % width) as i32, (index / width) as i32)
    }

    /// Fewest steps between two cells, ignoring obstacles.
    pub fn distance(&self, from: Cell, to: Cell) -> u32 {
        from.x.abs_diff(to.x) + from.y.abs_diff(to.y)
    }
}

impl Space for Grid {
    type Node = Cell;

    fn node_count(&self) -> usize {
        self.len()
    }

    fn index(&self, cell: Cell) -> Option<usize> {
        Grid::index(self, cell)
    }

    fn node(&self, index: usize) -> Cell {
        self.cell(index)
    }

    fn for_each_neighbor(&self, cell: Cell, visit: &mut dyn FnMut(Cell)) {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let next = Cell::new(cell.x + dx, cell.y + dy);
            if self.contains(next) {
                visit(next);
            }
        }
    }

    fn min_moves(&self, from: Cell, to: Cell) -> u32 {
        self.distance(from, to)
    }
}

/// A one-move link through a portal: a step from `from` toward `toward` (a
/// unit step, such as `(1, 0)`) lands on `to` instead of the next cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PortalLink {
    pub from: Cell,
    pub toward: Cell,
    pub to: Cell,
}

/// The portal links of a grid, kept sorted so a cell's links are found by
/// binary search. Searching through them goes through [`Portals::space`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Portals {
    links: Vec<PortalLink>,
}

impl Portals {
    pub fn new(links: impl IntoIterator<Item = PortalLink>) -> Self {
        let mut portals = Self::default();
        portals.set(links);
        portals
    }

    /// Replaces the links, reusing storage.
    pub fn set(&mut self, links: impl IntoIterator<Item = PortalLink>) {
        self.links.clear();
        self.links.extend(links);
        self.links.sort_unstable();
        self.links.dedup();
    }

    pub fn links(&self) -> &[PortalLink] {
        &self.links
    }

    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }

    /// Fills `bounds` with a lower bound, per link, on the moves from the
    /// link's landing cell to `goal`: walking straight there, or walking to
    /// another link and going through it, any number of times. It is the
    /// distance with every wall removed but portals kept, so it never
    /// overestimates. Costs `O(links²)` per pass; there are few links.
    pub fn bounds_to(&self, grid: &Grid, goal: Cell, bounds: &mut Vec<u32>) {
        bounds.clear();
        bounds.extend(self.links.iter().map(|link| grid.distance(link.to, goal)));
        // Relax until nothing improves: each pass can only lower a bound,
        // and bounds never go below zero.
        loop {
            let mut improved = false;
            for k in 0..self.links.len() {
                for (j, through) in self.links.iter().enumerate() {
                    let via = grid
                        .distance(self.links[k].to, through.from)
                        .saturating_add(1)
                        .saturating_add(bounds[j]);
                    if via < bounds[k] {
                        bounds[k] = via;
                        improved = true;
                    }
                }
            }
            if !improved {
                return;
            }
        }
    }

    /// The grid with these portals, for a search toward `goal` whose
    /// `bounds` came from [`Portals::bounds_to`]. Its neighbours include the
    /// portal links, as single moves, and its move estimate counts them, so
    /// A* keeps its guidance: no need for jumps, which turn the heuristic
    /// off.
    pub fn space<'a>(&'a self, grid: &'a Grid, goal: Cell, bounds: &'a [u32]) -> PortalGrid<'a> {
        debug_assert_eq!(bounds.len(), self.links.len());
        PortalGrid {
            grid,
            links: &self.links,
            goal,
            bounds,
        }
    }
}

/// A [`Grid`] whose portal links are moves too, for searches toward one
/// goal; see [`Portals::space`].
#[derive(Clone, Copy, Debug)]
pub struct PortalGrid<'a> {
    grid: &'a Grid,
    links: &'a [PortalLink],
    goal: Cell,
    bounds: &'a [u32],
}

impl PortalGrid<'_> {
    /// The links leaving `cell`.
    fn links_from(&self, cell: Cell) -> &[PortalLink] {
        let start = self.links.partition_point(|link| link.from < cell);
        let end = start + self.links[start..].partition_point(|link| link.from == cell);
        &self.links[start..end]
    }
}

impl Space for PortalGrid<'_> {
    type Node = Cell;

    fn node_count(&self) -> usize {
        self.grid.len()
    }

    fn index(&self, cell: Cell) -> Option<usize> {
        self.grid.index(cell)
    }

    fn node(&self, index: usize) -> Cell {
        self.grid.cell(index)
    }

    fn for_each_neighbor(&self, cell: Cell, visit: &mut dyn FnMut(Cell)) {
        let links = self.links_from(cell);
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let toward = Cell::new(dx, dy);
            let next = match links.iter().find(|link| link.toward == toward) {
                Some(link) => link.to,
                None => Cell::new(cell.x + dx, cell.y + dy),
            };
            if self.grid.contains(next) {
                visit(next);
            }
        }
    }

    /// The fewest moves to the goal with walls removed but portals kept:
    /// straight there, or to a link's entrance, through it (one move), and
    /// on from its landing. Consistent, since a move changes each straight
    /// distance by at most one and a portal move costs one. Toward any other
    /// cell it is zero, which is always correct.
    fn min_moves(&self, from: Cell, to: Cell) -> u32 {
        if to != self.goal {
            return 0;
        }
        self.links
            .iter()
            .zip(self.bounds)
            .map(|(link, bound)| {
                self.grid
                    .distance(from, link.from)
                    .saturating_add(1)
                    .saturating_add(*bound)
            })
            .fold(self.grid.distance(from, to), u32::min)
    }
}
