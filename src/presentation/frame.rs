use bevy::prelude::*;

use super::DynamicLight;
use crate::foundation::portal::CellTransform;
use crate::lighting::{dimmed, light_to_palette, DARK_RED};
use crate::model::*;

/// Composes the frame the player sees.
///
/// The frame is a window of its own size onto the map (`RenderBuffers`),
/// recentred on the player when it follows them. Frame cells the player
/// sees (`PlayerView`) show the map cell they look onto, which behind a
/// portal is somewhere else on the map, even past its edge; objects there
/// are drawn once per frame cell showing their cell. Frame cells not seen
/// now show the map as remembered at their own position, in dimmer colours.
///
/// Two indexes are kept apart: map data (visibility, light) is indexed by
/// `MapGrid::idx`, frame data by `RenderBuffers::idx`.
#[allow(clippy::type_complexity)]
pub fn compose_frame(
    grid: Res<MapGrid>,
    dynamic: Res<DynamicLight>,
    reveal: Option<Res<RevealAll>>,
    viewers: Query<(&Pos, &VisibilityMap, Option<&PlayerView>), With<Player>>,
    glyphs: Query<(
        Entity,
        &Pos,
        &Glyph,
        Option<&Speed>,
        Option<&AnimatedPos>,
        Has<BoundTo>,
    )>,
    children: Query<(Entity, &Pos, &Glyph, Option<&AnimatedPos>, &BoundTo)>,
    mut buffers: ResMut<RenderBuffers>,
) {
    let Ok((player, visibility, view)) = viewers.single() else {
        return;
    };
    let known = |p: IVec2| {
        grid.idx(p)
            .and_then(|i| visibility.data.get(i))
            .is_some_and(|v| v.contains(Vis::KNOWN))
    };
    if buffers.follows_player {
        buffers.centre_on(player.0);
    }
    buffers.swap();
    buffers.current.fill(RenderCell::default());
    buffers.objects.clear();
    mark_seen(&mut buffers, &grid, visibility, view, reveal.is_some());

    for (entity, pos, glyph, speed, shown, bound) in glyphs.iter() {
        let Some(world) = grid.idx(pos.0) else {
            continue;
        };
        if bound {
            continue;
        }
        let drawn = RenderCell {
            ch: glyph.ch,
            color: glyph.color,
            depth: glyph.depth,
        };
        let shown = shown.copied().unwrap_or(AnimatedPos::at(pos.0));
        let mut instance = 0;
        let mut frame = buffers.seen_first[world];
        while frame != RenderBuffers::NO_CELL {
            let index = frame as usize;
            let seen = buffers.seen[index].expect("listed frame cells are seen");
            // Frame cells map to the map cells they show, so the object is
            // shown at the inverse of its own position.
            let inverse = seen.transform.inverse();
            draw(
                &mut buffers,
                index,
                entity,
                instance,
                inverse.apply_point(shown.position),
                shown.lift,
                inverse.quarters,
                drawn,
            );
            instance += 1;
            frame = buffers.seen_next[index];
        }
        // Static objects stay where they were last seen, where the frame
        // shows their own cell as remembered.
        if instance == 0 && speed.is_none() && known(pos.0) {
            if let Some(index) = buffers.idx(pos.0) {
                if buffers.seen[index].is_none() {
                    draw(
                        &mut buffers,
                        index,
                        entity,
                        0,
                        shown.position,
                        shown.lift,
                        0,
                        drawn,
                    );
                }
            }
        }
    }

    // Children (such as the player's direction marker) go with each place
    // their parent is drawn, through the same transform: a marker in front
    // of the player stays there even when that is a portal's wall.
    for (entity, pos, glyph, shown, bound) in children.iter() {
        let drawn = RenderCell {
            ch: glyph.ch,
            color: glyph.color,
            depth: glyph.depth,
        };
        let shown = shown.copied().unwrap_or(AnimatedPos::at(pos.0));
        for i in 0..buffers.objects.len() {
            let parent = buffers.objects[i];
            if parent.entity != bound.parent {
                continue;
            }
            let inverse = buffers
                .idx(parent.pos)
                .and_then(|index| buffers.seen[index])
                .map_or(CellTransform::IDENTITY, |seen| seen.transform)
                .inverse();
            let Some(index) = buffers.idx(inverse.apply(pos.0)) else {
                continue;
            };
            let position = inverse.apply_point(shown.position);
            draw(
                &mut buffers,
                index,
                entity,
                parent.instance,
                position,
                shown.lift,
                inverse.quarters,
                drawn,
            );
        }
    }

    for index in 0..buffers.current.len() {
        let position = buffers.pos_of(index);
        let occupied = !matches!(buffers.current[index].ch, ' ' | '\0');
        let light_at = |p: IVec2| {
            grid.idx(p)
                .and_then(|i| dynamic.data.get(i))
                .copied()
                .unwrap_or_default()
        };
        if let Some(seen) = buffers.seen[index] {
            let light = light_at(seen.world);
            shade(&mut buffers.current[index], true, light_to_palette(&light));
        } else if known(position) {
            let light = light_at(position);
            shade(
                &mut buffers.current[index],
                is_hex_pos(position),
                dimmed(light_to_palette(&light)),
            );
        } else if borders_known_background(&buffers, &known, position) {
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

    // Keep only objects that won their cell, and give them its lit color.
    let buffers = &mut *buffers;
    let mut objects = std::mem::take(&mut buffers.objects);
    objects.retain_mut(|object| {
        let Some(index) = buffers.idx(object.pos) else {
            return false;
        };
        let cell = buffers.current[index];
        let shown = cell.ch == object.cell.ch && cell.depth == object.cell.depth;
        object.cell.color = cell.color;
        shown
    });
    buffers.objects = objects;
}

/// Fills `seen` and the lists of frame cells showing each map cell, from
/// the player's view through portals. Without one (tests that build only a
/// visibility map), visible cells are seen directly. With `reveal`, every
/// other cell is seen directly too.
fn mark_seen(
    buffers: &mut RenderBuffers,
    grid: &MapGrid,
    visibility: &VisibilityMap,
    view: Option<&PlayerView>,
    reveal: bool,
) {
    let map_cells = (grid.width * grid.height) as usize;
    if buffers.seen_first.len() != map_cells {
        buffers.seen_first.resize(map_cells, RenderBuffers::NO_CELL);
    }
    buffers.seen.fill(None);
    buffers.seen_first.fill(RenderBuffers::NO_CELL);
    let mark = |buffers: &mut RenderBuffers, frame: IVec2, seen: SeenCell| {
        let (Some(index), Some(world)) = (buffers.idx(frame), grid.idx(seen.world)) else {
            return;
        };
        buffers.seen[index] = Some(seen);
        buffers.seen_next[index] = buffers.seen_first[world];
        buffers.seen_first[world] = index as u32;
    };
    let directly = |p: IVec2| SeenCell {
        world: p,
        transform: CellTransform::IDENTITY,
    };
    match view {
        Some(view) => {
            for sample in &view.samples {
                if sample.value > VISIBILITY_THRESHOLD {
                    let seen = SeenCell {
                        world: sample.world,
                        transform: sample.transform,
                    };
                    mark(buffers, view.pos + sample.delta, seen);
                }
            }
        }
        None => {
            for index in 0..buffers.seen.len() {
                let p = buffers.pos_of(index);
                let visible = grid
                    .idx(p)
                    .and_then(|i| visibility.data.get(i))
                    .is_some_and(|v| v.contains(Vis::VISIBLE));
                if visible {
                    mark(buffers, p, directly(p));
                }
            }
        }
    }
    if reveal {
        for index in 0..buffers.seen.len() {
            if buffers.seen[index].is_none() {
                let p = buffers.pos_of(index);
                mark(buffers, p, directly(p));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    buffers: &mut RenderBuffers,
    index: usize,
    entity: Entity,
    instance: u16,
    position: Vec2,
    lift: f32,
    quarters: u8,
    drawn: RenderCell,
) {
    if buffers.current[index].depth <= drawn.depth {
        buffers.current[index] = drawn;
    }
    let pos = buffers.pos_of(index);
    buffers.objects.push(ObjectCell {
        entity,
        instance,
        pos,
        position,
        lift,
        quarters,
        cell: drawn,
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

/// Whether an unknown cell borders a known, empty one (drawn as `?`). A
/// frame cell seen now counts as known.
fn borders_known_background(
    buffers: &RenderBuffers,
    known: &impl Fn(IVec2) -> bool,
    at: IVec2,
) -> bool {
    [IVec2::NEG_Y, IVec2::NEG_X, IVec2::X, IVec2::Y]
        .into_iter()
        .any(|delta| {
            let neighbour = at + delta;
            let Some(index) = buffers.idx(neighbour) else {
                return false;
            };
            (buffers.seen[index].is_some() || known(neighbour)) && buffers.current[index].depth == 0
        })
}
