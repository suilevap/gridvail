//! Text art: a picture drawn in characters, `X`/`x` for walls and anything
//! else for floor, such as a title spelled out in walls.

use std::f32::consts::SQRT_2;

use bevy::prelude::*;

use super::{Layout, LayoutRequest, LayoutSource, Oval};

/// Parsed text art: its size and its wall cells, from its top-left corner.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Art {
    pub size: IVec2,
    pub walls: Vec<IVec2>,
}

impl Art {
    pub fn parse(text: &str) -> Self {
        let rows: Vec<&str> = text
            .lines()
            .map(|row| row.strip_suffix('\r').unwrap_or(row))
            .collect();
        let size = IVec2::new(
            rows.iter()
                .map(|row| row.chars().count())
                .max()
                .unwrap_or(0) as i32,
            rows.len() as i32,
        );
        let walls = rows
            .iter()
            .enumerate()
            .flat_map(|(y, row)| {
                row.chars().enumerate().filter_map(move |(x, c)| {
                    matches!(c, 'X' | 'x').then_some(IVec2::new(x as i32, y as i32))
                })
            })
            .collect();
        Self { size, walls }
    }

    /// Where to put its top-left corner to centre it on `centre` while
    /// keeping it inside a `bounds`-sized map; `None` if it does not fit.
    pub fn corner_near(&self, centre: IVec2, bounds: IVec2) -> Option<IVec2> {
        if self.size.cmpgt(bounds).any() {
            return None;
        }
        Some((centre - self.size / 2).clamp(IVec2::ZERO, bounds - self.size))
    }
}

/// Lays `art` out centred on the request's centre, kept inside the map, in
/// the oval through its corners plus `margin` cells; nothing if it does not
/// fit.
#[derive(Clone, Debug)]
pub struct ArtLayout {
    pub art: Art,
    pub margin: f32,
}

impl ArtLayout {
    pub fn new(art: Art, margin: f32) -> Self {
        Self { art, margin }
    }
}

impl LayoutSource for ArtLayout {
    fn layout(&self, request: &LayoutRequest) -> Option<Layout> {
        let corner = self.art.corner_near(request.centre, request.bounds)?;
        let size = self.art.size;
        // An oval through a rectangle's corners is √2 times its half size.
        let frame = Oval {
            centre: corner.as_vec2() + (size - 1).as_vec2() / 2.0,
            radii: size.as_vec2() / 2.0 * SQRT_2 + self.margin,
        };
        let walls = self.art.walls.iter().map(|&p| corner + p).collect();
        Some(Layout { walls, frame })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walls_are_read_from_the_top_left() {
        let art = Art::parse("X.X\r\n.x\n");
        assert_eq!(art.size, IVec2::new(3, 2));
        assert_eq!(
            art.walls,
            [IVec2::new(0, 0), IVec2::new(2, 0), IVec2::new(1, 1)]
        );
    }

    #[test]
    fn it_is_centred_and_kept_inside_the_map() {
        let art = Art::parse("XXX\nXXX");
        let bounds = IVec2::new(10, 6);
        assert_eq!(
            art.corner_near(IVec2::new(5, 3), bounds),
            Some(IVec2::new(4, 2))
        );
        assert_eq!(art.corner_near(IVec2::ZERO, bounds), Some(IVec2::ZERO));
        assert_eq!(
            art.corner_near(IVec2::new(9, 5), bounds),
            Some(IVec2::new(7, 4))
        );
        assert_eq!(art.corner_near(IVec2::ZERO, IVec2::new(2, 6)), None);
    }

    #[test]
    fn its_layout_holds_every_wall_in_its_frame() {
        let source = ArtLayout::new(Art::parse("X.X\n.X.\nX.X"), 0.0);
        let request = LayoutRequest {
            bounds: IVec2::new(10, 6),
            centre: IVec2::new(9, 0),
        };
        let layout = source.layout(&request).unwrap();
        assert_eq!(layout.walls.len(), 5);
        super::super::tests::check(&layout, &request);
    }
}
