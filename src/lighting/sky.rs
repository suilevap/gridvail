//! Daylight on a grid with roofs, as seen from above.
//!
//! Open cells under the sky get the sun, or the dimmer light of the sky
//! where a wall or a roof stands between them and the sun (within the
//! shadow's reach, which grows as the sun gets lower). The slanting sun
//! also shines in through openings onto the floor under a roof, a patch as
//! long as a shadow. Sunlight is warm and skylight in shadow cool. Under the roofs, daylight spreads in from the open
//! cells and the patches, fading with each cell, as in Minecraft's sky
//! light: rooms with an opening to the sky get some, and the further in
//! the darker it is.

use bevy::prelude::IVec2;

use crate::model::{LightCell, LightKind, SkyLight};

/// The share of the sun a patch of sunlight under a roof gets.
const PATCH: f32 = 0.85;

/// Scratch storage for [`Daylight::apply`], reused between calls.
#[derive(Debug, Default)]
pub struct Daylight {
    sunlit: Vec<bool>,
    /// Per cell, how bright daylight is and whether it is the sun's or
    /// the sky's.
    light: Vec<(f32, LightKind)>,
    next: Vec<(f32, LightKind)>,
}

impl Daylight {
    /// Lights `layer` (`width` cells a row) with daylight where it is
    /// brighter than what is there. `blocks` and `roofed` say, per cell,
    /// whether it blocks light and whether it is under a roof.
    pub fn apply(
        &mut self,
        layer: &mut [LightCell],
        width: i32,
        blocks: &[bool],
        roofed: &[bool],
        sky: &SkyLight,
    ) {
        let Some((travel, reach, sun)) = sky.time.sun() else {
            return;
        };
        let cells = layer.len();
        let height = cells as i32 / width.max(1);
        let index = |p: IVec2| {
            (p.x >= 0 && p.y >= 0 && p.x < width && p.y < height)
                .then(|| (p.y * width + p.x) as usize)
        };
        let at = |i: usize| IVec2::new(i as i32 % width, i as i32 / width);
        let toward_sun = -travel;
        let sun = sun as f32;
        let open_sky = |i: usize| !blocks[i] && !roofed[i];

        // Open cells in the sun: nothing within the shadow's reach towards
        // the sun stands up or covers them.
        self.sunlit.clear();
        self.sunlit.extend((0..cells).map(|i| {
            open_sky(i)
                && (1..=reach)
                    .all(|k| index(at(i) + toward_sun * k).is_none_or(|p| !blocks[p] && !roofed[p]))
        }));

        self.light.clear();
        self.light.extend((0..cells).map(|i| {
            if open_sky(i) {
                if self.sunlit[i] {
                    (sun, LightKind::Sun)
                } else {
                    (sun * sky.shade, LightKind::Sky)
                }
            } else if !roofed[i] {
                // The tops of walls under the sky.
                (sun, LightKind::Sun)
            } else if !blocks[i]
                && patch(
                    &self.sunlit,
                    at(i),
                    toward_sun,
                    reach,
                    &index,
                    blocks,
                    roofed,
                )
            {
                (sun * PATCH, LightKind::Sun)
            } else {
                (0.0, LightKind::Sky)
            }
        }));

        // Under the roofs, daylight spreads in from open cells, each cell
        // passing on the brightest it gets less a share.
        self.next.clone_from(&self.light);
        for _ in 0..sky.steps {
            for (i, _) in roofed.iter().enumerate().filter(|(_, &roofed)| roofed) {
                let mut brightest = self.light[i];
                for step in [IVec2::NEG_Y, IVec2::X, IVec2::Y, IVec2::NEG_X] {
                    if let Some(n) = index(at(i) + step).filter(|&n| !blocks[n]) {
                        let (light, kind) = self.light[n];
                        if light * sky.falloff > brightest.0 {
                            brightest = (light * sky.falloff, kind);
                        }
                    }
                }
                self.next[i] = brightest;
            }
            std::mem::swap(&mut self.light, &mut self.next);
        }

        for (cell, &(light, kind)) in layer.iter_mut().zip(&self.light) {
            if light >= 2.0 && light > cell.value as f32 {
                *cell = LightCell {
                    value: light.round().min(255.0) as u8,
                    kind,
                };
            }
        }
    }
}

/// Whether the sun shines on the roofed floor cell `from` through an
/// opening: going towards the sun, past open cells only, it reaches a
/// sunlit cell under the sky within `reach` steps. A diagonal step
/// needs one of the two cells beside it open, so the sun does not slip
/// between walls that meet at a corner.
fn patch(
    sunlit: &[bool],
    mut from: IVec2,
    toward_sun: IVec2,
    reach: i32,
    index: &impl Fn(IVec2) -> Option<usize>,
    blocks: &[bool],
    roofed: &[bool],
) -> bool {
    let open = |p: IVec2| index(p).is_some_and(|p| !blocks[p]);
    for _ in 0..reach {
        let to = from + toward_sun;
        let Some(next) = index(to).filter(|&p| !blocks[p]) else {
            return false;
        };
        if toward_sun.x != 0
            && toward_sun.y != 0
            && !open(from + IVec2::new(toward_sun.x, 0))
            && !open(from + IVec2::new(0, toward_sun.y))
        {
            return false;
        }
        if !roofed[next] {
            return sunlit[next];
        }
        from = to;
    }
    false
}
