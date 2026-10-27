//! The composite wall volume: standing objects without the volume shader.
//!
//! A standing object's glyph is its top, drawn up the screen by its height
//! (`place_object_sprites`), and a face sprite hangs below it: the front of
//! the object, seen from the bottom of the screen. Faces are drawn only
//! where nothing stands on the next cell down the screen, which would hide
//! them. A face hangs from the glyph's ink down by the object's height, a
//! texel per font pixel: `Extruded` from each inked column's lowest ink, as
//! the glyph extruded looks from the front, and the patterns full width
//! from the glyph's lowest ink.

use bevy::{
    asset::RenderAssetUsages,
    platform::collections::{HashMap, HashSet},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use crate::model::{GlyphStatures, RenderBuffers, Stature, ViewCamera, WallFace, WallVolume};

use super::text::{ObjectSprite, ObjectSprites, TurnedGlyphs, CELL_SIZE};
use super::volume::{glyph_ink, GLYPH_PX};

/// Faces per atlas row, the tallest face in font pixels, and a slot's
/// height: the glyph's cell and the face below it.
const SLOTS: usize = 16;
const FACE_PX: usize = 2 * GLYPH_PX;
const SLOT_PX: usize = GLYPH_PX + FACE_PX;
/// Faces are darker than the top: brighter toward it.
const SHADE_AT_FLOOR: f32 = 0.3;
const SHADE_AT_TOP: f32 = 0.7;

/// The face sprites, reused frame to frame, and their atlas.
#[derive(Resource, Default)]
pub(super) struct Faces {
    sprites: Vec<Entity>,
    atlas: Option<Handle<Image>>,
    /// The atlas slot of each face, by pattern, the glyph it hangs from and
    /// its height in font pixels.
    slots: HashMap<(WallFace, char, usize), usize>,
}

/// Whether the face pattern has a texel at column `x`, `down` font pixels
/// below where the face starts.
fn inked(face: WallFace, x: usize, down: usize) -> bool {
    match face {
        WallFace::Extruded | WallFace::Block => true,
        WallFace::Shade => (x + down).is_multiple_of(2),
        WallFace::Bricks => {
            // Courses of three, offset by half a brick each course.
            let joint = if (down / 4).is_multiple_of(2) {
                0
            } else {
                GLYPH_PX / 2
            };
            down % 4 != 3 && x != joint
        }
    }
}

fn atlas_image() -> Image {
    Image::new_fill(
        Extent3d {
            width: (GLYPH_PX * SLOTS) as u32,
            height: (SLOT_PX * SLOTS) as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    )
}

/// Draws the face of `ch`, `rows` font pixels tall, into atlas slot `slot`:
/// the glyph's cell (left clear: the top's text sprite draws the glyph),
/// then the face below its ink, brighter toward the top.
fn draw_face(face: WallFace, ch: char, rows: usize, slot: usize, atlas: &mut Image) {
    let glyph = glyph_ink(ch);
    let lowest = |x: Option<usize>| {
        glyph
            .iter()
            .rposition(|row| x.map_or(row.iter().any(|&ink| ink), |x| row[x]))
    };
    let glyph_bottom = lowest(None);
    let Some(data) = atlas.data.as_mut() else {
        return;
    };
    let width = GLYPH_PX * SLOTS;
    let (slot_x, slot_y) = (slot % SLOTS * GLYPH_PX, slot / SLOTS * SLOT_PX);
    for x in 0..GLYPH_PX {
        let start = match face {
            WallFace::Extruded => lowest(Some(x)),
            _ => glyph_bottom,
        };
        let Some(start) = start else {
            continue;
        };
        for down in 0..rows {
            let up = (rows - down) as f32 / rows as f32;
            let shade = SHADE_AT_FLOOR + (SHADE_AT_TOP - SHADE_AT_FLOOR) * up;
            let value = (shade.powf(1.0 / 2.2) * 255.0) as u8;
            let alpha = if inked(face, x, down) { 255 } else { 0 };
            let texel = ((slot_y + start + 1 + down) * width + slot_x + x) * 4;
            data[texel..texel + 4].copy_from_slice(&[value, value, value, alpha]);
        }
    }
}

/// Places a face under each standing object in front of nothing standing.
#[allow(clippy::too_many_arguments)]
pub(super) fn place_faces(
    mut commands: Commands,
    volume: Option<Res<WallVolume>>,
    face: Res<WallFace>,
    camera: Res<ViewCamera>,
    buffers: Res<RenderBuffers>,
    index: Res<ObjectSprites>,
    turned: Res<TurnedGlyphs>,
    statures: Option<Res<GlyphStatures>>,
    own_statures: Query<&Stature>,
    images: Option<ResMut<Assets<Image>>>,
    mut faces: ResMut<Faces>,
    tops: Query<(&ObjectSprite, &TextColor, &Transform), Without<Sprite>>,
    mut sprites: Query<(&mut Sprite, &mut Transform, &mut Visibility)>,
) {
    let faces = &mut *faces;
    let mut used = 0;
    let composite = volume.is_some_and(|volume| *volume == WallVolume::Composite);
    if let (true, Some(mut images), Some(statures)) = (composite, images, statures) {
        let atlas = faces
            .atlas
            .get_or_insert_with(|| images.add(atlas_image()))
            .clone();
        // Where things stand, and which way is down the screen.
        let standing: HashSet<IVec2> = buffers
            .objects
            .iter()
            .filter(|object| {
                statures.of(object.cell.ch, own_statures.get(object.entity).ok()) > 0.0
            })
            .map(|object| object.pos)
            .collect();
        let down = camera.map_direction(IVec2::Y);
        for object in &buffers.objects {
            let Some(&top) = index.0.get(&(object.entity, object.instance)) else {
                continue;
            };
            let Ok((sprite, color, transform)) = tops.get(top) else {
                continue;
            };
            if !sprite.shown || sprite.stature <= 0.0 || standing.contains(&(object.pos + down)) {
                continue;
            }
            // The glyph as the top shows it, turned with the view, and as
            // many font pixels of face below it as the object is tall.
            let ch = turned.get(object.cell.ch, object.quarters, &camera);
            let rows = (sprite.stature * GLYPH_PX as f32)
                .round()
                .clamp(1.0, FACE_PX as f32) as usize;
            let key = (*face, ch, rows);
            let next = faces.slots.len();
            let slot = match faces.slots.get(&key) {
                Some(&slot) => slot,
                None if next < SLOTS * SLOTS => {
                    if let Some(mut image) = images.get_mut(&atlas) {
                        draw_face(*face, ch, rows, next, &mut image);
                    }
                    faces.slots.insert(key, next);
                    next
                }
                None => continue,
            };
            let corner = Vec2::new(
                (slot % SLOTS * GLYPH_PX) as f32,
                (slot / SLOTS * SLOT_PX) as f32,
            );
            let texels = Vec2::new(GLYPH_PX as f32, (GLYPH_PX + rows) as f32);
            // A texel per font pixel, at the view's zoom.
            let size = texels * (CELL_SIZE.x / GLYPH_PX as f32) * camera.zoom;
            let drawn = Sprite {
                image: atlas.clone(),
                rect: Some(Rect::from_corners(corner, corner + texels)),
                custom_size: Some(size),
                color: color.0,
                ..default()
            };
            // Its top edge on the top glyph's, just under it.
            let half_cell = CELL_SIZE.y / 2.0 * camera.zoom;
            let placed = Transform {
                translation: transform.translation - Vec3::new(0.0, size.y / 2.0 - half_cell, 1e-6),
                rotation: transform.rotation,
                scale: Vec3::ONE,
            };
            match faces.sprites.get(used) {
                Some(&entity) => {
                    if let Ok((mut sprite, mut transform, mut visibility)) = sprites.get_mut(entity)
                    {
                        if sprite.rect != drawn.rect
                            || sprite.color != drawn.color
                            || sprite.custom_size != drawn.custom_size
                        {
                            *sprite = drawn;
                        }
                        if *transform != placed {
                            *transform = placed;
                        }
                        visibility.set_if_neq(Visibility::Visible);
                    }
                }
                None => faces
                    .sprites
                    .push(commands.spawn((drawn, placed, Visibility::Visible)).id()),
            }
            used += 1;
        }
    }
    for &entity in &faces.sprites[used.min(faces.sprites.len())..] {
        if let Ok((.., mut visibility)) = sprites.get_mut(entity) {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
}
