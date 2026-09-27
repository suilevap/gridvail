use bevy::prelude::*;

use super::DynamicLight;
use crate::lighting::{light_to_palette, DARK_RED};
use crate::model::*;

pub fn compose_frame(
    dynamic: Res<DynamicLight>,
    visibility: Query<&VisibilityMap, With<Player>>,
    glyphs: Query<(Entity, &Pos, &Glyph, Option<&Speed>)>,
    mut buffers: ResMut<RenderBuffers>,
) {
    let Ok(visibility) = visibility.single() else {
        return;
    };
    buffers.swap();
    buffers.current.fill(RenderCell::default());
    buffers.objects.clear();

    for (entity, pos, glyph, speed) in glyphs.iter() {
        let Some(index) = buffers.idx(pos.0) else {
            continue;
        };
        let visible = visibility.data.get(index).copied().unwrap_or_default();
        if visible.contains(Vis::VISIBLE) || (speed.is_none() && visible.contains(Vis::KNOWN)) {
            let drawn = RenderCell {
                ch: glyph.ch,
                color: glyph.color,
                depth: glyph.depth,
            };
            let RenderBuffers {
                current, objects, ..
            } = &mut *buffers;
            if current[index].depth <= glyph.depth {
                current[index] = drawn;
            }
            objects.push(ObjectCell {
                entity,
                pos: pos.0,
                cell: drawn,
            });
        }
    }

    for y in 0..buffers.height {
        for x in 0..buffers.width {
            let position = IVec2::new(x, y);
            let index = (y * buffers.width + x) as usize;
            let visible = visibility.data.get(index).copied().unwrap_or_default();
            let occupied = !matches!(buffers.current[index].ch, ' ' | '\0');
            if visible.contains(Vis::KNOWN) {
                let light = dynamic.data.get(index).copied().unwrap_or_default();
                let floor = visible.contains(Vis::VISIBLE) || is_hex_pos(position);
                shade(&mut buffers.current[index], floor, light_to_palette(&light));
            } else if borders_known_background(&buffers, visibility, position) {
                buffers.current[index] = RenderCell {
                    ch: '?',
                    color: DARK_RED,
                    depth: 0,
                };
            }
            // Objects draw themselves; the ground under them stays blank.
            buffers.ground[index] = if occupied {
                RenderCell::default()
            } else {
                buffers.current[index]
            };
        }
    }

    // Keep only objects that won their cell, and give them its lit color.
    let RenderBuffers {
        width,
        current,
        objects,
        ..
    } = &mut *buffers;
    objects.retain_mut(|object| {
        let cell = current[(object.pos.y * *width + object.pos.x) as usize];
        let shown = cell.ch == object.cell.ch && cell.depth == object.cell.depth;
        object.cell.color = cell.color;
        shown
    });
}

fn shade(cell: &mut RenderCell, floor: bool, color: u8) {
    if cell.ch == ' ' || cell.ch == '\0' {
        if floor {
            cell.ch = '.';
            cell.color = color;
        }
    } else {
        cell.color = color;
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
