use super::{Layout, LayoutRequest, LayoutSource};

/// Lays out with `first`, or with `then` when `first` cannot.
#[derive(Clone, Copy, Debug)]
pub struct Fallback<A, B> {
    pub first: A,
    pub then: B,
}

impl<A: LayoutSource, B: LayoutSource> LayoutSource for Fallback<A, B> {
    fn layout(&self, request: &LayoutRequest) -> Option<Layout> {
        self.first
            .layout(request)
            .or_else(|| self.then.layout(request))
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::super::{Art, ArtLayout, ScatterLayout};
    use super::*;

    #[test]
    fn the_second_source_lays_out_when_the_first_cannot() {
        let both = Fallback {
            first: ArtLayout::new(Art::parse("XXXXX"), 0.0),
            then: ScatterLayout {
                radius: 2.0,
                density: 1.0,
                seed: 1,
            },
        };
        let wide = LayoutRequest {
            bounds: IVec2::new(9, 3),
            centre: IVec2::new(4, 1),
        };
        assert_eq!(both.layout(&wide).unwrap().walls.len(), 5, "the art");
        let narrow = LayoutRequest {
            bounds: IVec2::new(3, 3),
            centre: IVec2::ONE,
        };
        assert_eq!(both.layout(&narrow).unwrap().walls.len(), 9, "the scatter");
    }
}
