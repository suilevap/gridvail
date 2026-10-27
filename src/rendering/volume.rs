//! Walls with height in the text renderer (`WallVolume`).
//!
//! The view looks down at a slant from the bottom of the screen, an oblique
//! projection: the floor stays as drawn, and height goes straight up the
//! screen whichever way the map is turned, so a wall's top is its glyph
//! moved up by its height and the wall itself is filled in below it. Nearer
//! rows (lower on screen) are drawn over the rows behind them. `Leaning`
//! leans height to the right as well, a cavalier oblique view.

use bevy::{ecs::entity::EntityHashMap, prelude::*};

use crate::model::{ViewCamera, WallVolume};

use super::text::{glyph_rotation, CellFont, ObjectSprite, CELL_FONT_SIZE, CELL_SIZE};

/// How far a standing wall's top is above the floor, in cells.
const WALL_HEIGHT: f32 = 0.625;
/// One pixel of the 8 px font drawn at 16 px: layers this far apart join
/// into a solid side, even for one-pixel lines.
const FONT_PIXEL: f32 = 2.0;
/// Layers under a wall's top, from the floor up.
const LAYERS: usize = (WALL_HEIGHT * CELL_SIZE.y / FONT_PIXEL) as usize;
/// The full block, for the sides and top of solid walls.
const BLOCK: char = '█';

/// The layers drawn under each wall sprite's top, by sprite.
#[derive(Resource, Default)]
pub(super) struct WallLayers(EntityHashMap<Vec<Entity>>);

#[derive(Component)]
pub(super) struct WallLayer;

/// Which way height goes on screen, per unit of height.
fn lean(volume: WallVolume) -> Vec2 {
    match volume {
        WallVolume::Leaning => Vec2::new(0.5, 1.0),
        _ => Vec2::Y,
    }
}

/// Where on screen a wall's top is drawn from its foot: its height, less
/// while it rises out of the floor or sinks into it.
pub(super) fn rise(sprite: &ObjectSprite, camera: &ViewCamera, volume: WallVolume) -> Vec2 {
    lean(volume) * WALL_HEIGHT * CELL_SIZE.y * sprite.height * camera.zoom
}

/// Draw order for a sprite seen at a slant: further along the lean is
/// further away, nearer is drawn over what is behind it, then deeper glyphs
/// on top within a cell. It stays between the ground (0) and the HUD.
pub(super) fn depth_order(sprite: &ObjectSprite, camera: &ViewCamera, volume: WallVolume) -> f32 {
    let base = super::text::object_translation(sprite, camera);
    1.5 - base.truncate().dot(lean(volume)) / 16384.0 + sprite.depth as f32 * 1e-5
}

/// Fills in each standing wall below its top: copies of its glyph
/// (`Stacked`) or blocks (`Solid`), darker toward the floor.
#[allow(clippy::type_complexity)]
pub(super) fn place_wall_layers(
    mut commands: Commands,
    volume: Res<WallVolume>,
    camera: Res<ViewCamera>,
    font: Res<CellFont>,
    mut index: ResMut<WallLayers>,
    sprites: Query<(Entity, &ObjectSprite, &Text2d, &TextColor, &Transform), Without<WallLayer>>,
    mut layers: Query<
        (&mut Text2d, &mut TextColor, &mut Transform, &mut Visibility),
        With<WallLayer>,
    >,
) {
    let volume = *volume;
    // Walls not seen before get their layers.
    if volume != WallVolume::Flat {
        for (sprite, drawn, ..) in &sprites {
            if drawn.wall && drawn.shown && !index.0.contains_key(&sprite) {
                let spawned = (0..=LAYERS)
                    .map(|_| {
                        commands
                            .spawn((
                                WallLayer,
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
                            .id()
                    })
                    .collect();
                index.0.insert(sprite, spawned);
            }
        }
    }

    index.0.retain(|&sprite, owned| {
        let Ok((_, drawn, text, color, top)) = sprites.get(sprite) else {
            // The sprite is gone with its object.
            for &layer in owned.iter() {
                commands.entity(layer).despawn();
            }
            return false;
        };
        let standing = volume != WallVolume::Flat && drawn.wall && drawn.shown;
        for (i, &layer) in owned.iter().enumerate() {
            let Ok((mut layer_text, mut layer_color, mut transform, mut visibility)) =
                layers.get_mut(layer)
            else {
                continue;
            };
            // Stacked walls use LAYERS copies under the glyph; solid ones
            // add a block under the glyph for the top.
            let used = match volume {
                WallVolume::Solid => i <= LAYERS,
                _ => i < LAYERS,
            };
            let shown = standing && used;
            visibility.set_if_neq(if shown {
                Visibility::Visible
            } else {
                Visibility::Hidden
            });
            if !shown {
                continue;
            }
            let (glyph, shade) = match volume {
                // Brighter toward the top.
                WallVolume::Stacked | WallVolume::Leaning => {
                    (None, 0.3 + 0.4 * i as f32 / LAYERS as f32)
                }
                // A dark front face, and a dimmer top than the glyph on it.
                _ if i == LAYERS => (Some(BLOCK), 0.45),
                _ => (Some(BLOCK), 0.18 + 0.12 * i as f32 / LAYERS as f32),
            };
            let mut block = [0; 4];
            let wanted: &str = match glyph {
                Some(ch) => ch.encode_utf8(&mut block),
                None => text.0.as_str(),
            };
            if layer_text.0 != wanted {
                layer_text.0.clear();
                layer_text.0.push_str(wanted);
            }
            let lit = color.0.to_linear();
            let desired = Color::LinearRgba(LinearRgba {
                red: lit.red * shade,
                green: lit.green * shade,
                blue: lit.blue * shade,
                alpha: lit.alpha,
            });
            if layer_color.0 != desired {
                layer_color.0 = desired;
            }
            // Layer i sits i font pixels above the floor; the top (i ==
            // LAYERS) is where the glyph is.
            let below = rise(drawn, &camera, volume) * (LAYERS - i) as f32 / LAYERS as f32;
            let placed = Transform {
                translation: top.translation - below.extend((LAYERS + 1 - i) as f32 * 1e-6),
                rotation: glyph_rotation(&camera),
                scale: top.scale,
            };
            if *transform != placed {
                *transform = placed;
            }
        }
        true
    });
}
