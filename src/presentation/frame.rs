use bevy::prelude::*;

use super::DynamicLight;
use crate::foundation::portal::CellTransform;
use crate::lighting::{
    dimmed, dimmed_rgb, light_to_palette, light_to_rgb, DARK_MAGENTA, DARK_RED, MAGENTA,
};
use crate::model::*;

/// Colour of a portal's threshold and of portal walls seen from behind or
/// side-on; remembered ones are dimmer.
pub const PORTAL_COLOR: u8 = MAGENTA;
pub const REMEMBERED_PORTAL_COLOR: u8 = DARK_MAGENTA;
/// What an empty threshold shows instead of the floor's dot.
pub const PORTAL_GLYPH: char = '░';

/// Composes the frame the player sees.
///
/// The frame is a window of its own size onto the map (`RenderBuffers`),
/// recentred on the player when it follows them. Frame cells the player
/// sees (`PlayerView`) show the map cell they look onto, which behind a
/// portal is somewhere else on the map, even past its edge; objects there
/// are drawn once per frame cell showing their cell. Frame cells not seen
/// now show the map as remembered at their own position, in dimmer colours.
///
/// Portals show in `PORTAL_COLOR`: the threshold (the floor where the view
/// passes through a portal face, drawn as `PORTAL_GLYPH` when empty) and a
/// portal's wall seen from behind or side-on, so a portal stands out from
/// any side.
///
/// Two indexes are kept apart: map data (visibility, light) is indexed by
/// `MapGrid::idx`, frame data by `RenderBuffers::idx`.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn compose_frame(
    grid: Res<MapGrid>,
    dynamic: Res<DynamicLight>,
    style: Option<Res<LightStyle>>,
    (floor_style, floor_light, lamps, shown_viewer): (
        Option<Res<FloorStyle>>,
        Option<ResMut<FloorLight>>,
        Query<(&Pos, Option<&AnimatedPos>, &LightSource)>,
        Query<&AnimatedPos, With<Player>>,
    ),
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
            rgb: None,
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
                shown.height,
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
                        shown.height,
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
            rgb: None,
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
                shown.height,
                inverse.quarters,
                drawn,
            );
        }
    }

    let smooth = style.is_some_and(|style| *style == LightStyle::Smooth);
    let light_at = |p: IVec2| {
        grid.idx(p)
            .and_then(|i| dynamic.data.get(i))
            .copied()
            .unwrap_or_default()
    };
    for index in 0..buffers.current.len() {
        let position = buffers.pos_of(index);
        let occupied = !matches!(buffers.current[index].ch, ' ' | '\0');
        let seen = buffers.seen[index];
        if seen.is_some() || known(position) {
            let cell = &mut buffers.current[index];
            light_cell(cell, seen, position, &grid, &light_at, smooth);
        } else if borders_known_background(&buffers, &known, position) {
            buffers.current[index] = RenderCell {
                ch: '?',
                color: DARK_RED,
                depth: 0,
                rgb: None,
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
    // One partly sunk into the floor (rising or sinking) does not hide it.
    let buffers = &mut *buffers;
    let mut objects = std::mem::take(&mut buffers.objects);
    objects.retain_mut(|object| {
        let Some(index) = buffers.idx(object.pos) else {
            return false;
        };
        let cell = buffers.current[index];
        let shown = cell.ch == object.cell.ch && cell.depth == object.cell.depth;
        object.cell.color = cell.color;
        object.cell.rgb = cell.rgb;
        if shown && object.height < 1.0 {
            let mut floor = RenderCell::default();
            let position = buffers.pos_of(index);
            light_cell(
                &mut floor,
                buffers.seen[index],
                position,
                &grid,
                &light_at,
                smooth,
            );
            buffers.ground[index] = floor;
        }
        shown
    });
    buffers.objects = objects;

    let shader_floor = floor_style.is_some_and(|style| *style != FloorStyle::Text);
    if let Some(mut floor) = floor_light.filter(|_| shader_floor) {
        let direct_at = |p: IVec2| {
            grid.idx(p)
                .and_then(|i| dynamic.direct.get(i))
                .copied()
                .unwrap_or_default()
        };
        fill_floor(buffers, &grid, &known, &light_at, &direct_at, &mut floor);
        floor.viewer = shown_viewer
            .iter()
            .next()
            .map_or(player.0.as_vec2(), |shown| shown.position);
        floor.lamps.clear();
        let window = Rect::from_corners(
            buffers.origin.as_vec2(),
            (buffers.origin + IVec2::new(buffers.width, buffers.height)).as_vec2(),
        );
        floor.lamps.extend(
            lamps
                .iter()
                .map(|(pos, shown, light)| FloorLamp {
                    position: shown.map_or(pos.0.as_vec2(), |shown| shown.position),
                    radius: light.radius as f32,
                    value: light.value as f32,
                    kind: light.kind,
                })
                .filter(|lamp| {
                    let nearest = lamp.position.clamp(window.min, window.max);
                    nearest.distance(lamp.position) <= lamp.radius + 1.0
                }),
        );
    }
}

/// Fills `floor` for the frame: every cell's light and whether it blocks
/// light, which cells have floor (seen or remembered, under objects too),
/// and which floor the shader lights itself, a pixel at a time from
/// the lamps: floor seen directly. Its own light then only adds what
/// bounced onto it (`light_at` brighter than `direct_at`).
fn fill_floor(
    buffers: &RenderBuffers,
    grid: &MapGrid,
    known: &impl Fn(IVec2) -> bool,
    light_at: &impl Fn(IVec2) -> LightCell,
    direct_at: &impl Fn(IVec2) -> LightCell,
    floor: &mut FloorLight,
) {
    floor.origin = buffers.origin;
    floor.cells = IVec2::new(buffers.width, buffers.height);
    let cells = (buffers.width * buffers.height) as usize;
    floor.light.clear();
    floor.light.resize(cells * 4, 0);
    floor.info.clear();
    floor.info.resize(cells * 4, 0);
    for index in 0..buffers.ground.len() {
        let position = buffers.pos_of(index);
        let local = (position - buffers.origin).as_uvec2();
        let at = (local.y * buffers.width as u32 + local.x) as usize * 4;
        let info = &mut floor.info[at..at + 4];
        info[0] = if grid.blocks_vision(position) { 255 } else { 0 };
        // Floor goes on under whatever stands on it (its ground cell is
        // blank), but not under walls or a portal's threshold.
        let seen = buffers.seen[index];
        let shown = seen.map_or(position, |seen| seen.world);
        let floor_here = match buffers.ground[index].ch {
            '.' => true,
            '\0' | ' ' => (seen.is_some() || known(position)) && !grid.blocks_vision(shown),
            _ => false,
        };
        if !floor_here {
            continue;
        }
        let (light, remembered) = match seen {
            Some(seen) => {
                if seen.transform.is_identity() {
                    let (light, direct) = (light_at(seen.world), direct_at(seen.world));
                    info[1] = 255;
                    info[2] = if light.value > direct.value {
                        light.value
                    } else {
                        0
                    };
                }
                (light_at(seen.world), false)
            }
            None => (light_at(position), true),
        };
        let rgb = light_to_rgb(&light);
        let (rgb, value) = if remembered {
            (dimmed_rgb(rgb), light.value / 2)
        } else {
            (rgb, light.value)
        };
        floor.light[at..at + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], value.max(1)]);
    }
}

/// Lights frame cell `cell` at `position`, seen now through `seen` or else
/// remembered: an empty one becomes floor (or a portal's threshold), and an
/// object's glyph takes the light's colour (or a portal's), `smooth`ly
/// shaded or in palette steps.
fn light_cell(
    cell: &mut RenderCell,
    seen: Option<SeenCell>,
    position: IVec2,
    grid: &MapGrid,
    light_at: &impl Fn(IVec2) -> LightCell,
    smooth: bool,
) {
    let occupied = !matches!(cell.ch, ' ' | '\0');
    if let Some(seen) = seen {
        let light = light_at(seen.world);
        let rgb = smooth.then(|| light_to_rgb(&light));
        shade(cell, true, light_to_palette(&light), rgb);
        // The threshold's floor, or the portal's own wall when the view
        // does not pass through it.
        if seen.portal && !occupied {
            *cell = RenderCell {
                ch: PORTAL_GLYPH,
                color: PORTAL_COLOR,
                depth: 0,
                rgb: None,
            };
        } else if occupied && grid.portal_at(seen.world).is_some() {
            cell.color = PORTAL_COLOR;
            cell.rgb = None;
        }
    } else {
        let light = light_at(position);
        let rgb = smooth.then(|| dimmed_rgb(light_to_rgb(&light)));
        shade(
            cell,
            is_hex_pos(position),
            dimmed(light_to_palette(&light)),
            rgb,
        );
        if occupied && grid.portal_at(position).is_some() {
            cell.color = REMEMBERED_PORTAL_COLOR;
            cell.rgb = None;
        }
    }
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
        portal: false,
    };
    match view {
        Some(view) => {
            for sample in &view.samples {
                if sample.value > VISIBILITY_THRESHOLD {
                    let seen = SeenCell {
                        world: sample.world,
                        transform: sample.transform,
                        portal: sample.portal,
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
    height: f32,
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
        height,
        quarters,
        cell: drawn,
    });
}

fn shade(cell: &mut RenderCell, floor: bool, color: u8, rgb: Option<[u8; 3]>) {
    if cell.ch == ' ' || cell.ch == '\0' {
        if floor {
            cell.ch = '.';
            cell.color = color;
            cell.rgb = rgb;
        }
    } else {
        cell.color = color;
        cell.rgb = rgb;
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
