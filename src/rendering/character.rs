//! Static four-direction sprites extracted from the approved generated roster.
//! Only facing chooses the image; movement and combat keep their existing
//! presentation without adding character animation state or cue consumers.

use bevy::{
    asset::RenderAssetUsages,
    image::{CompressedImageFormats, ImageSampler, ImageType},
    platform::collections::HashMap,
    prelude::*,
    sprite::Anchor,
};
use serde::Deserialize;

use super::text::{ObjectSprite, CELL_SIZE};
use crate::model::*;
use crate::schedule::{GamePhase, StartupPhase};

pub struct CharacterSpritePlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct CharacterOutput;

impl Plugin for CharacterSpritePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PixelInstances>()
            .add_systems(Startup, setup.in_set(StartupPhase::Renderer))
            .add_systems(
                Update,
                (
                    drop_gone,
                    spawn_instances.run_if(instances_missing),
                    place_instances,
                )
                    .chain()
                    .in_set(GamePhase::Output)
                    .in_set(CharacterOutput)
                    .after(super::text::place_object_sprites)
                    .run_if(resource_exists::<PixelAtlas>),
            );
    }
}

/// Presence also enables replacement of the fallback glyph in other backends.
#[derive(Resource)]
pub(super) struct PixelAtlas {
    image: Handle<Image>,
    size: Vec2,
    world_height: f32,
}

#[derive(Resource, Default)]
struct PixelInstances(HashMap<(Entity, u16), Entity>);

#[derive(Component)]
struct PixelInstance {
    source: Entity,
}

#[derive(Deserialize)]
struct CharacterLook {
    world_height: f32,
}

fn decode_png(bytes: &[u8]) -> Image {
    Image::from_buffer(
        bytes,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::nearest(),
        RenderAssetUsages::default(),
    )
    .expect("valid character PNG")
}

fn setup(mut commands: Commands, images: Option<ResMut<Assets<Image>>>) {
    let Some(mut images) = images else { return };
    let look: CharacterLook = crate::content::data::read(
        "characters/trespasser.ron",
        include_str!("../../assets/characters/trespasser.ron"),
    )
    .expect("valid character look");
    assert!(look.world_height.is_finite() && look.world_height > 0.0);
    let image = decode_png(include_bytes!("../../assets/characters/trespasser.png"));
    assert_eq!(
        image.width(),
        image.height() * 4,
        "four equal square directional cells"
    );
    let size = Vec2::splat(image.height() as f32);
    commands.insert_resource(PixelAtlas {
        image: images.add(image),
        size,
        world_height: look.world_height,
    });
}

fn drop_gone(
    mut commands: Commands,
    actors: Query<(), (With<PixelCharacter>, With<Glyph>)>,
    sprites: Query<(Entity, &PixelInstance)>,
    mut index: ResMut<PixelInstances>,
) {
    for (entity, instance) in &sprites {
        if !actors.contains(instance.source) {
            commands.entity(entity).despawn();
        }
    }
    index.0.retain(|&(source, _), _| actors.contains(source));
}

fn instances_missing(
    buffers: Res<RenderBuffers>,
    index: Res<PixelInstances>,
    actors: Query<(), With<PixelCharacter>>,
) -> bool {
    buffers
        .objects
        .iter()
        .any(|o| actors.contains(o.entity) && !index.0.contains_key(&(o.entity, o.instance)))
}

fn spawn_instances(world: &mut World) {
    let missing: Vec<ObjectCell> = {
        let index = world.resource::<PixelInstances>();
        world
            .resource::<RenderBuffers>()
            .objects
            .iter()
            .filter(|o| {
                world.get::<PixelCharacter>(o.entity).is_some()
                    && !index.0.contains_key(&(o.entity, o.instance))
            })
            .copied()
            .collect()
    };
    let image = world.resource::<PixelAtlas>().image.clone();
    for object in missing {
        let entity = world
            .spawn((
                PixelInstance {
                    source: object.entity,
                },
                ObjectSprite::from_object(&object),
                Sprite::from_image(image.clone()),
                Anchor::BOTTOM_CENTER,
                Transform::default(),
                Visibility::Hidden,
            ))
            .id();
        world
            .resource_mut::<PixelInstances>()
            .0
            .insert((object.entity, object.instance), entity);
    }
}

/// Row order in the sheet: left, right, away, toward the camera. Map Y is
/// down; portal and camera turns are both counter-clockwise on screen.
fn direction_row(facing: IVec2, quarters: i32) -> usize {
    let turned = (0..quarters.rem_euclid(4)).fold(facing, |v, _| IVec2::new(v.y, -v.x));
    if turned.x < 0 {
        0
    } else if turned.x > 0 {
        1
    } else if turned.y < 0 {
        2
    } else {
        3
    }
}

fn place_instances(
    buffers: Res<RenderBuffers>,
    camera: Res<ViewCamera>,
    atlas: Res<PixelAtlas>,
    index: Res<PixelInstances>,
    (volume, extruded): (
        Option<Res<WallVolume>>,
        Option<Res<super::extruded::ExtrudedWalls>>,
    ),
    actors: Query<&Facing, With<PixelCharacter>>,
    mut sprites: Query<
        (
            &mut ObjectSprite,
            &mut Sprite,
            &mut Transform,
            &mut Visibility,
        ),
        With<PixelInstance>,
    >,
) {
    let volume = volume.map_or(WallVolume::Flat, |v| *v);
    for (mut object, _, _, mut visibility) in &mut sprites {
        object.shown = false;
        visibility.set_if_neq(Visibility::Hidden);
    }
    for object in &buffers.objects {
        let Some(&entity) = index.0.get(&(object.entity, object.instance)) else {
            continue;
        };
        let Ok(facing) = actors.get(object.entity) else {
            continue;
        };
        let Ok((mut drawn, mut sprite, mut transform, mut visibility)) = sprites.get_mut(entity)
        else {
            continue;
        };
        *drawn = ObjectSprite::from_object(object);
        drawn.shown = true;
        let row = direction_row(facing.0, camera.quarter_turns() + object.quarters as i32);
        let corner = Vec2::new(row as f32, 0.0) * atlas.size;
        sprite.rect = Some(Rect::from_corners(corner, corner + atlas.size));
        sprite.custom_size = Some(atlas.size * (atlas.world_height / atlas.size.y));
        // Preserve the palette while responding to scene light and marks.
        sprite.color = Color::from(object.cell.color).with_alpha(object.height);
        transform.translation.z = 1.0 + drawn.depth as f32 * 0.1;
        if extruded.is_none() {
            transform.translation = super::text::object_translation(&drawn, &camera)
                + Vec3::Y * (object.lift * CELL_SIZE.y - CELL_SIZE.y * 0.3) * camera.zoom;
            if volume != WallVolume::Flat {
                transform.translation.z = super::volume::depth_order(&drawn, &camera, volume);
            }
            transform.rotation = Quat::IDENTITY;
            transform.scale = Vec3::splat(camera.zoom);
        }
        visibility.set_if_neq(Visibility::Visible);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_pixels_come_directly_from_the_reference() {
        let image = decode_png(include_bytes!("../../assets/characters/trespasser.png"));
        let source = decode_png(include_bytes!("../../docs/art/gridvail-sprite-roster.png"));
        assert_eq!(image.size(), UVec2::new(768, 192));
        let (pixels, original) = (image.data.unwrap(), source.data.unwrap());
        for (column, pivot) in [162, 425, 695, 960].into_iter().enumerate() {
            let mut opaque = 0;
            for y in 0..192 {
                for x in 0..192 {
                    let offset = (y * 768 + column * 192 + x) * 4;
                    if pixels[offset + 3] == 0 {
                        continue;
                    }
                    opaque += 1;
                    let source_x = pivot + x - 96;
                    let source_y = y + 199 - 175;
                    let source_offset = (source_y * 1122 + source_x) * 4;
                    assert_eq!(
                        &pixels[offset..offset + 3],
                        &original[source_offset..source_offset + 3]
                    );
                }
            }
            assert!(opaque > 4000, "complete character in each direction");
        }
    }

    #[test]
    fn four_facings_follow_camera_and_portal_turns() {
        let ways = [IVec2::NEG_X, IVec2::X, IVec2::NEG_Y, IVec2::Y];
        for (row, way) in ways.into_iter().enumerate() {
            assert_eq!(direction_row(way, 0), row);
        }
        assert_eq!(direction_row(IVec2::X, 1), 2);
        assert_eq!(direction_row(IVec2::X, 2), 0);
        assert_eq!(direction_row(IVec2::X, 3), 3);
        assert_eq!(direction_row(IVec2::X, 4), 1);
    }

    fn boot() -> App {
        use crate::{
            animation::ObjectAnimationPlugin,
            app::{GamePlugin, MapText},
            rendering::TextRendererPlugin,
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Image>>()
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(16),
            ))
            .insert_resource(MapText("XXXXXX\nXp...X\nX....X\nXXXXXX"))
            .add_plugins((
                GamePlugin,
                ObjectAnimationPlugin,
                TextRendererPlugin,
                CharacterSpritePlugin,
            ));
        for _ in 0..4 {
            app.update();
        }
        app
    }

    #[test]
    fn player_draws_as_pixels_in_all_wall_modes_and_cleans_up_on_reload() {
        let mut app = boot();
        for volume in WallVolume::ALL {
            *app.world_mut().resource_mut::<WallVolume>() = volume;
            app.update();
            let world = app.world_mut();
            let mut sprites = world
                .query_filtered::<(&Sprite, &ObjectSprite, &Visibility), With<PixelInstance>>();
            let (sprite, object, visibility) = sprites.single(world).unwrap();
            assert!(object.shown);
            assert_eq!(*visibility, Visibility::Visible);
            assert_eq!(sprite.rect.unwrap().height(), 192.0);
            // No fallback @ or extruded player is drawn on top of the atlas.
            let mut text = world.query::<(&Text2d, &ObjectSprite)>();
            assert!(!text
                .iter(world)
                .any(|(text, s)| s.shown && text.0.contains('@')));
        }
        app.world_mut()
            .write_message(crate::app::LoadMap("XXXXX\nXp..X\nXXXXX"));
        for _ in 0..4 {
            app.update();
        }
        let world = app.world_mut();
        assert_eq!(world.query::<&PixelInstance>().iter(world).count(), 1);
    }

    #[test]
    fn facing_selects_one_static_image_and_time_does_not_cycle_it() {
        let mut app = boot();
        let world = app.world_mut();
        let actor = world
            .query_filtered::<Entity, With<PixelCharacter>>()
            .single(world)
            .unwrap();
        for (column, way) in [IVec2::NEG_X, IVec2::X, IVec2::NEG_Y, IVec2::Y]
            .into_iter()
            .enumerate()
        {
            app.world_mut().get_mut::<Facing>(actor).unwrap().0 = way;
            for _ in 0..8 {
                app.update();
                let world = app.world_mut();
                let sprite = world
                    .query_filtered::<&Sprite, With<PixelInstance>>()
                    .single(world)
                    .unwrap();
                assert_eq!(
                    sprite.rect.unwrap().min,
                    Vec2::new(column as f32 * 192.0, 0.0)
                );
            }
        }
    }
}
