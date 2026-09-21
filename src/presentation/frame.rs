use bevy::prelude::*;

use super::DynamicLight;
use crate::lighting::{light_to_palette, DARK_RED};
use crate::model::*;

pub fn compose_frame(
    dynamic: Res<DynamicLight>,
    visibility: Query<&VisibilityMap, With<Player>>,
    glyphs: Query<(&Pos, &Glyph, Option<&Speed>)>,
    mut buffers: ResMut<RenderBuffers>,
) {
    let Ok(visibility) = visibility.single() else {
        return;
    };
    buffers.swap();
    buffers.current.fill(RenderCell::default());

    for (pos, glyph, speed) in glyphs.iter() {
        let Some(index) = buffers.idx(pos.0) else {
            continue;
        };
        let visible = visibility.data.get(index).copied().unwrap_or_default();
        if visible.contains(Vis::VISIBLE) || (speed.is_none() && visible.contains(Vis::KNOWN)) {
            let cell = &mut buffers.current[index];
            if cell.depth <= glyph.depth {
                *cell = RenderCell {
                    ch: glyph.ch,
                    color: glyph.color,
                    depth: glyph.depth,
                };
            }
        }
    }

    for y in 0..buffers.height {
        for x in 0..buffers.width {
            let index = (y * buffers.width + x) as usize;
            let visible = visibility.data.get(index).copied().unwrap_or_default();
            if visible.contains(Vis::KNOWN) {
                let light = dynamic.data.get(index).copied().unwrap_or_default();
                let cell = &mut buffers.current[index];
                if cell.ch == ' ' || cell.ch == '\0' {
                    if visible.contains(Vis::VISIBLE) || is_hex_pos(IVec2::new(x, y)) {
                        cell.ch = '.';
                        cell.color = light_to_palette(&light);
                    }
                } else {
                    cell.color = light_to_palette(&light);
                }
            } else if borders_known_background(&buffers, visibility, IVec2::new(x, y)) {
                buffers.current[index] = RenderCell {
                    ch: '?',
                    color: DARK_RED,
                    depth: 0,
                };
            }
        }
    }
}

fn borders_known_background(
    buffers: &RenderBuffers,
    visibility: &VisibilityMap,
    at: IVec2,
) -> bool {
    [IVec2::NEG_Y, IVec2::NEG_X, IVec2::X, IVec2::Y]
        .into_iter()
        .any(|delta| {
            let Some(index) = buffers.idx(at + delta) else {
                return false;
            };
            visibility
                .data
                .get(index)
                .copied()
                .unwrap_or_default()
                .contains(Vis::KNOWN)
                && buffers.current[index].depth == 0
        })
}
