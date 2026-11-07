use bevy::prelude::*;

/// A set of cells around an origin, in the frame of a facing: `x` counts
/// cells ahead, `y` cells to the side (a quarter turn from ahead). Placed at
/// an origin facing a direction, it gives the cells it covers: what a
/// weapon strikes today; a blast, a noise or an aimed throw later. The
/// cells are in the order a swing goes through them, so the area also says
/// which way the swing travels at each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Area(pub Vec<IVec2>);

impl Area {
    /// The one cell ahead.
    pub fn ahead() -> Self {
        Self(vec![IVec2::X])
    }

    /// The cells covered from `origin` facing `dir`.
    pub fn cells(&self, origin: IVec2, dir: IVec2) -> impl Iterator<Item = IVec2> + '_ {
        let side = IVec2::new(-dir.y, dir.x);
        self.0
            .iter()
            .map(move |offset| origin + dir * offset.x + side * offset.y)
    }

    /// The cells covered from `origin` facing `dir`, in order, each with the
    /// way the swing goes there (a unit step): on to the next cell, on from
    /// the one before for the last, and out from the origin for a single
    /// cell.
    pub fn sweep(&self, origin: IVec2, dir: IVec2) -> impl Iterator<Item = (IVec2, IVec2)> + '_ {
        let side = IVec2::new(-dir.y, dir.x);
        let turn = move |offset: IVec2| dir * offset.x + side * offset.y;
        let cells = &self.0;
        cells.iter().enumerate().map(move |(i, &offset)| {
            let travel = match (i.checked_sub(1), cells.get(i + 1)) {
                (_, Some(&next)) => next - offset,
                (Some(before), None) => offset - cells[before],
                (None, None) => offset,
            };
            (
                origin + turn(offset),
                turn(travel).clamp(IVec2::NEG_ONE, IVec2::ONE),
            )
        })
    }

    /// Which side of `dir` the swing ends on, from where it starts: the
    /// unit side step it goes toward, or none for a swing straight ahead.
    pub fn across(&self, dir: IVec2) -> IVec2 {
        let side = IVec2::new(-dir.y, dir.x);
        match (self.0.first(), self.0.last()) {
            (Some(first), Some(last)) => side * (last.y - first.y).signum(),
            _ => IVec2::ZERO,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_area_turns_with_its_facing() {
        let area = Area(vec![IVec2::X, IVec2::new(1, 1)]);
        let cells = |dir| area.cells(IVec2::new(5, 5), dir).collect::<Vec<_>>();
        assert_eq!(cells(IVec2::X)[0], IVec2::new(6, 5));
        assert_eq!(cells(IVec2::NEG_X)[0], IVec2::new(4, 5));
        assert_eq!(cells(IVec2::Y)[0], IVec2::new(5, 6));
        assert_eq!(cells(IVec2::NEG_Y)[0], IVec2::new(5, 4));
        // The second cell turns with the first: diagonally ahead.
        let ahead_side = cells(IVec2::Y)[1] - IVec2::new(5, 5);
        assert_eq!(ahead_side.y, 1);
        assert_eq!(ahead_side.x.abs(), 1);
    }

    #[test]
    fn a_sweep_travels_from_cell_to_cell_and_ends_on_its_side() {
        // Ahead, ahead-left, left: a sweep to the left, facing north.
        let area = Area(vec![IVec2::X, IVec2::new(1, -1), IVec2::new(0, -1)]);
        let swept: Vec<_> = area.sweep(IVec2::new(5, 5), IVec2::NEG_Y).collect();
        assert_eq!(swept[0], (IVec2::new(5, 4), IVec2::NEG_X), "pushed left");
        assert_eq!(swept[1], (IVec2::new(4, 4), IVec2::Y), "pushed back");
        assert_eq!(swept[2], (IVec2::new(4, 5), IVec2::Y), "pushed back");
        assert_eq!(area.across(IVec2::NEG_Y), IVec2::NEG_X);
        let thrust: Vec<_> = Area::ahead().sweep(IVec2::new(5, 5), IVec2::X).collect();
        assert_eq!(thrust, vec![(IVec2::new(6, 5), IVec2::X)]);
    }
}
