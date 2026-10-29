use bevy::prelude::*;

/// A set of cells around an origin, in the frame of a facing: `x` counts
/// cells ahead, `y` cells to the side (a quarter turn from ahead). Placed at
/// an origin facing a direction, it gives the cells it covers: what a
/// weapon strikes today; a blast, a noise or an aimed throw later.
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
}
