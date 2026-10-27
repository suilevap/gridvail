//! Cells scattered at random, the same for the same seed.

use bevy::prelude::*;
use rand::{RngExt, SeedableRng};

use super::{Layout, LayoutRequest, LayoutSource, Oval};

/// Cells within the oval at `centre` with `radii`, inside a `bounds`-sized
/// grid, each picked with chance `density`; the same for the same `seed`.
pub fn scatter_in_oval(
    centre: Vec2,
    radii: Vec2,
    bounds: IVec2,
    density: f64,
    seed: u64,
) -> Vec<IVec2> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let inside = |p: IVec2| {
        radii.min_element() > 0.0 && ((p.as_vec2() - centre) / radii).length_squared() <= 1.0
    };
    (0..bounds.y)
        .flat_map(|y| (0..bounds.x).map(move |x| IVec2::new(x, y)))
        .filter(|&p| inside(p))
        .filter(|_| rng.random_bool(density.clamp(0.0, 1.0)))
        .collect()
}

/// Walls scattered within `radius` cells of the request's centre, each cell
/// a wall with chance `density`; the same for the same `seed`.
#[derive(Clone, Copy, Debug)]
pub struct ScatterLayout {
    pub radius: f32,
    pub density: f64,
    pub seed: u64,
}

impl LayoutSource for ScatterLayout {
    fn layout(&self, request: &LayoutRequest) -> Option<Layout> {
        let frame = Oval {
            centre: request.centre.as_vec2(),
            radii: Vec2::splat(self.radius),
        };
        let walls = scatter_in_oval(
            frame.centre,
            frame.radii,
            request.bounds,
            self.density,
            self.seed,
        );
        Some(Layout { walls, frame })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_fall_inside_the_oval_and_the_grid_and_repeat_by_seed() {
        let scatter = |seed| {
            scatter_in_oval(
                Vec2::new(1.0, 1.0),
                Vec2::splat(4.0),
                IVec2::new(6, 6),
                0.5,
                seed,
            )
        };
        let cells = scatter(7);
        assert!(!cells.is_empty());
        assert!(cells
            .iter()
            .all(|p| p.cmpge(IVec2::ZERO).all() && p.cmplt(IVec2::splat(6)).all()));
        assert!(cells
            .iter()
            .all(|p| (p.as_vec2() - Vec2::ONE).length() <= 4.0));
        assert_eq!(cells, scatter(7));
        assert_ne!(cells, scatter(8));
    }

    #[test]
    fn its_layout_is_around_the_centre() {
        let source = ScatterLayout {
            radius: 3.0,
            density: 0.4,
            seed: 2,
        };
        let request = LayoutRequest {
            bounds: IVec2::new(8, 8),
            centre: IVec2::new(1, 6),
        };
        let layout = source.layout(&request).unwrap();
        assert!(!layout.walls.is_empty());
        super::super::tests::check(&layout, &request);
    }
}
