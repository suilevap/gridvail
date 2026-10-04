/// The graph a search runs on: nodes with dense indices and their
/// neighbours. Rules then price or forbid each move between neighbours.
///
/// The `grid` feature provides [`crate::grid::Grid`], a 4-connected grid of
/// [`crate::grid::Cell`]s; any other graph (a room graph, a hex map, a
/// navmesh's polygons) works by implementing this trait.
pub trait Space {
    type Node: Copy + Eq;

    /// Number of nodes; indices run from 0 to this.
    fn node_count(&self) -> usize;

    /// Index of `node`, below [`Space::node_count`]; `None` if it is not in
    /// the space.
    fn index(&self, node: Self::Node) -> Option<usize>;

    /// The node at `index`, below [`Space::node_count`].
    fn node(&self, index: usize) -> Self::Node;

    /// Calls `visit` with every neighbour of `node`.
    fn for_each_neighbor(&self, node: Self::Node, visit: &mut dyn FnMut(Self::Node));

    /// A lower bound on the number of moves from `from` to `to`, ignoring
    /// rules; it feeds [`crate::Rules::heuristic`]. Zero, the default, is
    /// always correct.
    fn min_moves(&self, _from: Self::Node, _to: Self::Node) -> u32 {
        0
    }
}

#[cfg(test)]
mod tests {
    use crate::{PathSearch, Rules, Space};

    /// Rooms joined by corridors of given lengths; no grid involved.
    struct Rooms {
        corridors: &'static [(usize, usize, u32)],
        count: usize,
    }

    impl Space for Rooms {
        type Node = usize;

        fn node_count(&self) -> usize {
            self.count
        }

        fn index(&self, room: usize) -> Option<usize> {
            (room < self.count).then_some(room)
        }

        fn node(&self, index: usize) -> usize {
            index
        }

        fn for_each_neighbor(&self, room: usize, visit: &mut dyn FnMut(usize)) {
            for &(from, to, _) in self.corridors {
                if from == room {
                    visit(to);
                } else if to == room {
                    visit(from);
                }
            }
        }
    }

    impl Rules for Rooms {
        type Node = usize;
        type Cost = u32;
        type State = ();

        fn state_count(&self) -> usize {
            1
        }

        fn state_index(&self, _state: &()) -> usize {
            0
        }

        fn start_state(&self, _start: usize) {}

        fn step(&self, from: usize, to: usize, _state: &()) -> Option<(u32, ())> {
            self.corridors
                .iter()
                .find(|&&(one, other, _)| (one, other) == (from, to) || (other, one) == (from, to))
                .map(|&(_, _, length)| (length, ()))
        }
    }

    #[test]
    fn any_graph_can_be_searched() {
        // 0 -5- 1 -5- 3, and a longer way 0 -2- 2 -9- 3.
        let rooms = Rooms {
            corridors: &[(0, 1, 5), (1, 3, 5), (0, 2, 2), (2, 3, 9)],
            count: 4,
        };
        let mut path = Vec::new();
        let cost = PathSearch::new().find(&rooms, &rooms, 0, 3, &mut path);
        assert_eq!(cost, Some(10));
        assert_eq!(path, [0, 1, 3]);
        assert_eq!(
            PathSearch::new().find(&rooms, &rooms, 0, 9, &mut path),
            None
        );
    }
}
