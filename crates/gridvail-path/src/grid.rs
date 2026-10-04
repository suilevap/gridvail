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
