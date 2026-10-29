//! Figures standing in the text renderer (`WallVolume`): objects drawn as
//! stacks of glyph parts (`Figure`), such as people and lamps.
//!
//! Each part is a glyph lying flat, its ink standing from one height to
//! another, like a slice: legs, then a torso, hands, a head. Seen at the
//! wall shader's slant, the top of each slice shows, and its side shows
//! below it toward the viewer, so a figure reads as a little body standing
//! in its cell. The whole figure turns to the way the object faces (eased),
//! so which way it looks shows from any side, and the parts are in map
//! space, so they turn with the view like walls do.
//!
//! Each figure is one quad over where it can be on screen, drawn by
//! `figure.wgsl`: for each pixel it walks down from the figure's top
//! toward the floor, as the wall shader does, and at each height looks for
//! a part that stands there with ink under the pixel's ray. It writes the
//! depth sprites and walls are sorted by, so walls in front hide it.

use bevy::{
    asset::embedded_asset,
    platform::collections::HashMap,
    prelude::*,
    reflect::TypePath,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    sprite_render::{AlphaMode2d, Material2d, Material2dPlugin},
};

use crate::model::{
    Facing, Figure, Figures, LightEffects, PartGlyph, RenderBuffers, ViewCamera, WallVolume,
};
use crate::schedule::GamePhase;

use super::light::LightTextures;
use super::text::{TurnedGlyphs, CELL_SIZE};
use super::volume::{lean, GlyphAtlas, GLYPH_PX};

/// The most parts a figure draws; any more are left out.
const MAX_PARTS: usize = 12;
/// How far a figure turns toward where it faces each second, as a share of
/// the way left (eased).
const TURN_RATE: f32 = 18.0;
const SHADER: &str = "embedded://pav_ecs_game_bevy_port/rendering/figure.wgsl";

/// Draws figures standing when `WallVolume` is not flat. Added by
/// `WallVolumePlugin`, whose glyph atlas it shares.
pub(super) struct FigurePlugin;

impl Plugin for FigurePlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "figure.wgsl");
        app.add_plugins(Material2dPlugin::<FigureMaterial>::default())
            .init_resource::<Figures>()
            .init_resource::<FigureQuads>()
            .add_systems(
                Update,
                draw_figures
                    .in_set(GamePhase::Output)
                    .after(super::text::place_object_sprites)
                    .after(super::light::update_light_textures),
            );
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct FigureMaterial {
    /// xy: the map point at the screen centre, z: rotation, w: zoom.
    #[uniform(0)]
    camera: Vec4,
    /// xy: the view's offset in cells, zw: the lean of height on screen.
    #[uniform(1)]
    offset_lean: Vec4,
    /// xy: the frame's origin, zw: its size, in cells.
    #[uniform(2)]
    frame: Vec4,
    /// x: cell px, y: atlas texels per glyph, z: the figure's top in cells,
    /// w: its draw order (`depth_order`).
    #[uniform(3)]
    params: Vec4,
    /// xy: where the figure is, in map cells, z: how many parts it draws.
    #[uniform(4)]
    figure: Vec4,
    /// rgb: the light it is in, w: 1 to take the light from the light maps
    /// instead, blended between cells.
    #[uniform(5)]
    color: Vec4,
    /// Three for each part: (offset from the figure in map cells, bottom,
    /// top), (cos and sin of its turn, its glyph's atlas corner) and (tint,
    /// scale).
    #[uniform(6)]
    parts: [Vec4; 3 * MAX_PARTS],
    #[texture(7)]
    atlas: Handle<Image>,
    #[texture(8)]
    light_previous: Handle<Image>,
    #[texture(9)]
    #[sampler(10)]
    light_current: Handle<Image>,
    /// y: how far the light has faded from the previous map to the current.
    #[uniform(11)]
    lighting: Vec4,
    /// xy: the previous light map's frame origin, zw: the current one's.
    #[uniform(12)]
    light_origins: Vec4,
}

impl Default for FigureMaterial {
    fn default() -> Self {
        Self {
            camera: Vec4::ZERO,
            offset_lean: Vec4::ZERO,
            frame: Vec4::ZERO,
            params: Vec4::ZERO,
            figure: Vec4::ZERO,
            color: Vec4::ZERO,
            parts: [Vec4::ZERO; 3 * MAX_PARTS],
            atlas: Handle::default(),
            light_previous: Handle::default(),
            light_current: Handle::default(),
            lighting: Vec4::ZERO,
            light_origins: Vec4::ZERO,
        }
    }
}

impl Material2d for FigureMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }

    // Masked materials write depth, which the text sprites test against.
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Mask(0.5)
    }
}

/// The quads drawing figures, reused frame to frame, and the way each
/// figure instance is shown facing, by object and instance.
#[derive(Resource, Default)]
struct FigureQuads {
    quads: Vec<(Entity, Handle<FigureMaterial>)>,
    turns: HashMap<(Entity, u16), Turn>,
}

/// The way a figure is shown facing, in radians clockwise from north, as
/// it eases toward the way it faces.
#[derive(Clone, Copy)]
struct Turn {
    angle: f32,
    /// Whether it was drawn this frame; those not drawn are forgotten.
    seen: bool,
}

/// Radians clockwise on screen from north (up) to `facing`, a map
/// direction (y down).
fn facing_angle(facing: IVec2) -> f32 {
    (facing.x as f32).atan2(-facing.y as f32)
}

/// `angle` moved `share` of the way to `target`, the shorter way round.
fn ease_angle(angle: f32, target: f32, share: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut delta = (target - angle).rem_euclid(tau);
    if delta > tau / 2.0 {
        delta -= tau;
    }
    if delta.abs() < 1e-3 {
        return target;
    }
    angle + delta * share
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw_figures(
    mut commands: Commands,
    time: Res<Time>,
    volume: Res<WallVolume>,
    camera: Res<ViewCamera>,
    buffers: Res<RenderBuffers>,
    turned: Res<TurnedGlyphs>,
    figures: Res<Figures>,
    mut state: ResMut<FigureQuads>,
    mut atlas: ResMut<GlyphAtlas>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FigureMaterial>>,
    objects: Query<(&Figure, Option<&Facing>)>,
    mut quads: Query<(&mut Transform, &mut Visibility), With<MeshMaterial2d<FigureMaterial>>>,
    (light, effects): (Option<Res<LightTextures>>, Option<Res<LightEffects>>),
) {
    let state = &mut *state;
    for turn in state.turns.values_mut() {
        turn.seen = false;
    }
    let mut used = 0;
    if *volume != WallVolume::Flat {
        let light = light.filter(|_| effects.is_some_and(|effects| *effects != LightEffects::Flat));
        let lean = lean(*volume);
        let share = 1.0 - (-TURN_RATE * time.delta_secs()).exp();
        let shared = FigureMaterial {
            camera: camera.position.extend(camera.rotation).extend(camera.zoom),
            offset_lean: Vec4::new(camera.offset.x, camera.offset.y, lean.x, lean.y),
            frame: Vec4::new(
                buffers.origin.x as f32,
                buffers.origin.y as f32,
                buffers.width as f32,
                buffers.height as f32,
            ),
            atlas: atlas.handle.clone(),
            lighting: light
                .as_ref()
                .map_or(Vec4::ZERO, |light| Vec4::new(1.0, light.blend, 0.0, 0.0)),
            light_origins: light.as_ref().map_or(Vec4::ZERO, |light| light.origins()),
            light_previous: light
                .as_ref()
                .map_or_else(Handle::default, |light| light.previous.light.clone()),
            light_current: light
                .as_ref()
                .map_or_else(Handle::default, |light| light.current.light.clone()),
            ..default()
        };
        for object in &buffers.objects {
            let Ok((figure, facing)) = objects.get(object.entity) else {
                continue;
            };
            let Some(shape) = figures.get(figure) else {
                continue;
            };
            // The way it faces as seen here: through a portal that turns
            // the view, turned with it (`quarters` are counter-clockwise).
            let target = facing
                .filter(|facing| facing.0 != IVec2::ZERO)
                .map(|facing| facing_angle(facing.0));
            let quarters = object.quarters as f32 * std::f32::consts::FRAC_PI_2;
            let turn = state
                .turns
                .entry((object.entity, object.instance))
                .or_insert(Turn {
                    angle: target.unwrap_or(std::f32::consts::PI) - quarters,
                    seen: true,
                });
            if let Some(target) = target {
                turn.angle = ease_angle(turn.angle, target - quarters, share);
            }
            turn.seen = true;
            let angle = turn.angle;
            let (sin, cos) = angle.sin_cos();
            let rotate = |v: Vec2| Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos);

            // A hop lifts it; rising or sinking, it stands partly in the
            // floor.
            let top = shape.top();
            let base = object.lift - (1.0 - object.height.clamp(0.0, 1.0)) * top;
            let mut parts = [Vec4::ZERO; 3 * MAX_PARTS];
            let mut count = 0;
            for part in shape.parts.iter().filter(|part| part.shown_for(object.cell.ch)) {
                if count == MAX_PARTS {
                    break;
                }
                let ch = match part.glyph {
                    PartGlyph::Char(ch) => ch,
                    // In map space: only a portal's turn, not the view's.
                    PartGlyph::Own => turned.turned(object.cell.ch, object.quarters as i32),
                };
                let Some(corner) = atlas.corner(ch, &mut images) else {
                    continue;
                };
                let (part_sin, part_cos) = if part.upright {
                    (0.0, 1.0)
                } else {
                    (sin, cos)
                };
                let offset = rotate(part.offset);
                let tint = Color::from(part.tint).to_linear();
                parts[count * 3] = Vec4::new(
                    offset.x,
                    offset.y,
                    part.bottom + base,
                    part.top + base,
                );
                parts[count * 3 + 1] = Vec4::new(part_cos, part_sin, corner.x, corner.y);
                parts[count * 3 + 2] = Vec4::new(tint.red, tint.green, tint.blue, part.scale);
                count += 1;
            }
            let top = top + base.max(0.0);
            if count == 0 || top <= 0.0 {
                continue;
            }

            // With smooth light, a figure in its cell's own light takes it
            // from the light maps, blended between cells as walls do; any
            // other colour (a portal's) is kept.
            let in_light = buffers
                .idx(object.pos)
                .is_some_and(|index| buffers.light[index] == object.cell.color);
            let lit = Color::from(object.cell.color).to_linear();
            let smooth = light.is_some() && in_light;
            let screen = view_point(&camera, object.position);
            let depth = 1.5 - screen.dot(lean) / 16384.0 + object.cell.depth as f32 * 1e-5;
            let material = FigureMaterial {
                params: Vec4::new(CELL_SIZE.y, GLYPH_PX as f32, top, depth),
                figure: Vec4::new(object.position.x, object.position.y, count as f32, 0.0),
                color: Vec4::new(lit.red, lit.green, lit.blue, if smooth { 1.0 } else { 0.0 }),
                parts,
                ..shared.clone()
            };

            // The quad covers the figure's cell and a little round it (for
            // parts held out), and its height along the lean.
            let cell = CELL_SIZE.y * camera.zoom;
            let reach = 2.0 * cell;
            let rise = lean * top * cell;
            let size = Vec2::new(reach + rise.x.abs(), reach + rise.y.abs());
            let centre = screen + rise / 2.0;
            let translation = Vec3::new(centre.x, centre.y, depth);
            let scale = size.extend(1.0);
            match state.quads.get(used) {
                Some((entity, handle)) => {
                    if let Ok((mut transform, mut visibility)) = quads.get_mut(*entity) {
                        if transform.translation != translation || transform.scale != scale {
                            transform.translation = translation;
                            transform.scale = scale;
                        }
                        visibility.set_if_neq(Visibility::Visible);
                    }
                    if let Some(mut drawn) = materials.get_mut(handle) {
                        *drawn = material;
                    }
                }
                None => {
                    let handle = materials.add(material);
                    let entity = commands
                        .spawn((
                            Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))),
                            MeshMaterial2d(handle.clone()),
                            Transform::from_translation(translation).with_scale(scale),
                        ))
                        .id();
                    state.quads.push((entity, handle));
                }
            }
            used += 1;
        }
    }
    for (entity, _) in &state.quads[used.min(state.quads.len())..] {
        if let Ok((_, mut visibility)) = quads.get_mut(*entity) {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
    state.turns.retain(|_, turn| turn.seen);
}

/// Where the view shows map point `map`, in 2D world units around the
/// screen centre (`view.wgsl`'s `to_screen`).
fn view_point(camera: &ViewCamera, map: Vec2) -> Vec2 {
    let d = map - camera.position;
    let (s, c) = camera.rotation.sin_cos();
    let turned = Vec2::new(c * d.x + s * d.y, -s * d.x + c * d.y);
    let v = (turned + camera.offset) * camera.zoom * CELL_SIZE.y;
    Vec2::new(v.x, -v.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facing_angles_turn_clockwise_from_north() {
        use std::f32::consts::{FRAC_PI_2, PI};
        assert!((facing_angle(IVec2::NEG_Y) - 0.0).abs() < 1e-6);
        assert!((facing_angle(IVec2::X) - FRAC_PI_2).abs() < 1e-6);
        assert!((facing_angle(IVec2::Y) - PI).abs() < 1e-6);
        assert!((facing_angle(IVec2::NEG_X) + FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn turning_takes_the_shorter_way_round() {
        use std::f32::consts::PI;
        // From just west of south to just east of it: through south.
        let eased = ease_angle(PI * 0.9, -PI * 0.9, 0.5);
        assert!((eased - PI).abs() < 1e-4, "{eased}");
        assert_eq!(ease_angle(1.0, 1.0 + 1e-4, 0.5), 1.0 + 1e-4);
    }
}
