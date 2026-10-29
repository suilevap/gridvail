//! Figures in the text renderer: objects with a `Figure` drawn, while
//! `WallVolume` is not flat, as a few small upright glyphs placed together
//! so they read as a shape (`@` over `/|\` over `/\`), in place of their
//! one glyph.
//!
//! Each part is a text sprite like any object's, in the same font and the
//! light its object is in, at its own offset, size and turn from the
//! object's shown position. Which parts are drawn follows the way the
//! object faces as seen on screen (`FigureView`): toward the viewer, away,
//! or to either side, so it stays right as the view turns. Parts share
//! their object's draw order (`depth_order`), back to front in the order
//! listed, so walls in front hide a figure and walls behind do not.

use bevy::{platform::collections::HashMap, prelude::*};

use crate::model::{
    Facing, Figure, FigureView, Figures, PartGlyph, RenderBuffers, ViewCamera, WallVolume,
};
use crate::schedule::GamePhase;

use super::text::{view_translation, CellFont, CELL_FONT_SIZE, CELL_SIZE};
use super::volume::lean;

/// Draws figures when `WallVolume` is not flat.
pub(super) struct FigurePlugin;

impl Plugin for FigurePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Figures>()
            .init_resource::<FigureSprites>()
            .add_systems(
                Update,
                draw_figures
                    .in_set(GamePhase::Output)
                    .after(super::text::place_object_sprites),
            );
    }
}

/// The text sprite drawing each part, by object, instance (one object may
/// be seen in several places through portals) and part, kept while drawn.
#[derive(Resource, Default)]
struct FigureSprites {
    sprites: HashMap<(Entity, u16, u8), Entity>,
    /// Sprites not drawing a part this frame, kept for the next ones.
    spare: Vec<Entity>,
}

/// Marks a text sprite drawing a figure part.
#[derive(Component)]
struct FigurePartSprite;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn draw_figures(
    mut commands: Commands,
    volume: Res<WallVolume>,
    camera: Res<ViewCamera>,
    buffers: Res<RenderBuffers>,
    figures: Res<Figures>,
    font: Option<Res<CellFont>>,
    mut state: ResMut<FigureSprites>,
    mut used: Local<HashMap<(Entity, u16, u8), Entity>>,
    objects: Query<(&Figure, Option<&Facing>)>,
    mut sprites: Query<
        (&mut Text2d, &mut TextColor, &mut Transform, &mut Visibility),
        With<FigurePartSprite>,
    >,
) {
    let Some(font) = font else {
        return;
    };
    let state = &mut *state;
    used.clear();
    if *volume != WallVolume::Flat {
        let lean = lean(*volume);
        let cell = CELL_SIZE.y * camera.zoom;
        for object in &buffers.objects {
            let Ok((figure, facing)) = objects.get(object.entity) else {
                continue;
            };
            let Some(shape) = figures.get(figure) else {
                continue;
            };
            // The way it faces on screen (y down): turned by a portal it is
            // seen through (counter-clockwise quarters), then by the view.
            let mut toward = facing.map_or(IVec2::ZERO, |facing| facing.0);
            for _ in 0..object.quarters {
                toward = IVec2::new(toward.y, -toward.x);
            }
            let view = FigureView::of(camera.turn() * toward.as_vec2());
            let (parts, mirrored) = shape.parts(view);
            let flip = if mirrored { -1.0 } else { 1.0 };

            let base = view_translation(&camera, object.position).truncate()
                + Vec2::Y * object.lift * cell;
            let depth = 1.5 - base.dot(lean) / 16384.0 + object.cell.depth as f32 * 1e-5;
            let light = Color::from(object.cell.color).to_linear();
            for (index, part) in parts.iter().enumerate().take(u8::MAX as usize) {
                if !part.shown_for(object.cell.ch) {
                    continue;
                }
                let ch = match part.glyph {
                    PartGlyph::Char(ch) => ch,
                    PartGlyph::Own => object.cell.ch,
                };
                let key = (object.entity, object.instance, index as u8);
                let sprite = match state.sprites.remove(&key).or_else(|| state.spare.pop()) {
                    Some(sprite) => sprite,
                    None => commands
                        .spawn((
                            FigurePartSprite,
                            Text2d::new(String::with_capacity(4)),
                            TextLayout::justify(Justify::Center),
                            TextFont {
                                font: font.0.clone().into(),
                                font_size: FontSize::Px(CELL_FONT_SIZE),
                                ..default()
                            },
                            TextColor(Color::NONE),
                            Transform::default(),
                            Visibility::Hidden,
                        ))
                        .id(),
                };
                used.insert(key, sprite);
                let Ok((mut text, mut color, mut transform, mut visibility)) =
                    sprites.get_mut(sprite)
                else {
                    // Spawned this frame: placed from the next.
                    continue;
                };
                let mut buffer = [0; 4];
                let glyph = ch.encode_utf8(&mut buffer);
                if text.0 != *glyph {
                    text.0.clear();
                    text.0.push_str(glyph);
                }
                let tint = Color::from(part.tint).to_linear();
                let desired = Color::LinearRgba(LinearRgba::new(
                    light.red * tint.red,
                    light.green * tint.green,
                    light.blue * tint.blue,
                    object.height.clamp(0.0, 1.0),
                ));
                if color.0 != desired {
                    color.0 = desired;
                }
                let offset = Vec2::new(part.offset.x * flip, part.offset.y) * cell;
                let translation = (base + offset).extend(depth + index as f32 * 1e-6);
                let glyph_flip = if part.no_flip { 1.0 } else { flip };
                let scale = Vec3::new(glyph_flip * part.scale, part.scale, 1.0) * camera.zoom;
                let rotation = Quat::from_rotation_z(part.turn * glyph_flip);
                if transform.translation != translation
                    || transform.scale != scale
                    || transform.rotation != rotation
                {
                    *transform = Transform {
                        translation,
                        rotation,
                        scale,
                    };
                }
                visibility.set_if_neq(Visibility::Visible);
            }
        }
    }
    // Parts no longer drawn give their sprites back.
    for (_, sprite) in state.sprites.drain() {
        if let Ok((.., mut visibility)) = sprites.get_mut(sprite) {
            visibility.set_if_neq(Visibility::Hidden);
        }
        state.spare.push(sprite);
    }
    std::mem::swap(&mut state.sprites, &mut *used);
}
