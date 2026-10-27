//! Cells scattered at random, the same for the same seed.

use bevy::prelude::*;
use rand::{RngExt, SeedableRng};

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
}
