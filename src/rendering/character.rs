//! Static four-direction sprites extracted from the approved generated roster.
//! Only facing chooses the image; movement and combat keep their existing
//! presentation without adding character animation state or cue consumers.

use crate::content::catalog::Catalog;
use bevy::{
    asset::RenderAssetUsages,
    image::{CompressedImageFormats, ImageSampler, ImageType},
    platform::collections::HashMap,
    prelude::*,
    sprite::Anchor,
};

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
                    .run_if(resource_exists::<CharacterImages>),
            );
    }
}

/// Presence also enables replacement of the fallback glyph in other backends.
#[derive(Resource)]
pub(super) struct CharacterImages {
    textures: Vec<CharacterTexture>,
    looks: Catalog<CharacterId>,
}

struct CharacterTexture {
    image: Handle<Image>,
    size: Vec2,
}

include!(concat!(env!("OUT_DIR"), "/character_images.rs"));

#[derive(Resource, Default)]
struct PixelInstances(HashMap<(Entity, u16), Entity>);

#[derive(Component)]
struct PixelInstance {
    source: Entity,
}

impl CharacterImages {
    pub(super) fn height(&self, look: CharacterId) -> f32 {
        self.looks
            .get(look)
            .expect("known character look")
            .world_height
    }
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
    let looks = Catalog::<CharacterId>::bundled().expect("valid character catalog");
    let mut textures = Vec::with_capacity(CharacterId::ALL.len());
    for &(id, bytes) in CHARACTER_IMAGES {
        assert_eq!(id.index(), textures.len(), "images follow catalog order");
        let look = looks.get(id).expect("known character look");
        assert!(look.world_height.is_finite() && look.world_height > 0.0);
        let image = decode_png(bytes);
        assert_eq!(
            image.width(),
            image.height() * 4,
            "four square directional cells"
        );
        let size = Vec2::splat(image.height() as f32);
        textures.push(CharacterTexture {
            image: images.add(image),
            size,
        });
    }
    commands.insert_resource(CharacterImages { textures, looks });
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
    for object in missing {
        let look = world
            .get::<PixelCharacter>(object.entity)
            .expect("sprite look")
            .0;
        let image = world.resource::<CharacterImages>().textures[look.index()]
            .image
            .clone();
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
    images: Res<CharacterImages>,
    index: Res<PixelInstances>,
    (volume, extruded): (
        Option<Res<WallVolume>>,
        Option<Res<super::extruded::ExtrudedWalls>>,
    ),
    actors: Query<(&Facing, &PixelCharacter)>,
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
        let Ok((facing, look)) = actors.get(object.entity) else {
            continue;
        };
        let Ok((mut drawn, mut sprite, mut transform, mut visibility)) = sprites.get_mut(entity)
        else {
            continue;
        };
        *drawn = ObjectSprite::from_object(object);
        drawn.shown = true;
        let row = direction_row(facing.0, camera.quarter_turns() + object.quarters as i32);
        let texture = &images.textures[look.0.index()];
        let corner = Vec2::new(row as f32, 0.0) * texture.size;
        sprite.image = texture.image.clone();
        sprite.rect = Some(Rect::from_corners(corner, corner + texture.size));
        sprite.custom_size = Some(texture.size * (images.height(look.0) / texture.size.y));
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
    use crate::choreography::Remains;

    #[test]
    fn all_sprite_pixels_come_directly_from_the_reference() {
        let source = decode_png(include_bytes!("../../docs/art/gridvail-sprite-roster.png"));
        let original = source.data.unwrap();
        for &(id, bytes) in CHARACTER_IMAGES {
            let feet = match id {
                CharacterId::Trespasser => 199_i32,
                CharacterId::Watchman => 453,
                CharacterId::Scavenger => 664,
                CharacterId::FurnaceGuard => 909,
                CharacterId::Hound => 1090,
                CharacterId::Surveyor => 1356,
            };
            let image = decode_png(bytes);
            assert_eq!(image.size(), UVec2::new(1024, 256));
            let pixels = image.data.unwrap();
            for (column, pivot) in [162_i32, 425, 695, 960].into_iter().enumerate() {
                let mut opaque = 0;
                for y in 0..256 {
                    for x in 0..256 {
                        let offset = (y * 1024 + column * 256 + x) * 4;
                        if pixels[offset + 3] == 0 {
                            continue;
                        }
                        opaque += 1;
                        let source_x = pivot + x as i32 - 128;
                        let source_y = y as i32 + feet - 239;
                        assert!(source_x >= 0 && source_y >= 0);
                        let source_offset = (source_y as usize * 1122 + source_x as usize) * 4;
                        assert_eq!(
                            &pixels[offset..offset + 3],
                            &original[source_offset..source_offset + 3]
                        );
                    }
                }
                assert!(
                    opaque > 500,
                    "complete character {id:?}, direction {column}"
                );
            }
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
        boot_map("XXXXXX\nXp...X\nX....X\nXXXXXX")
    }

    fn boot_map(map: &'static str) -> App {
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
            .insert_resource(MapText(map))
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
            assert_eq!(sprite.rect.unwrap().height(), 256.0);
            // No fallback @ or extruded player is drawn on top of the images.
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
                    Vec2::new(column as f32 * 256.0, 0.0)
                );
            }
        }
    }
    #[test]
    fn every_enemy_type_uses_its_own_configured_image() {
        let mut app = boot_map("XXXXXXXXXXXX\nXpehfrsg...X\nX..........X\nXXXXXXXXXXXX");
        let world = app.world_mut();
        let actors: Vec<_> = world
            .query::<(Entity, &PixelCharacter)>()
            .iter(world)
            .map(|(entity, look)| (entity, look.0))
            .collect();
        assert_eq!(actors.len(), 7, "player and six existing enemy kinds");
        for (entity, look) in actors {
            let index = world.resource::<PixelInstances>();
            let sprite = world.get::<Sprite>(index.0[&(entity, 0)]).unwrap();
            let images = world.resource::<CharacterImages>();
            assert_eq!(sprite.image, images.textures[look.index()].image);
            assert_eq!(sprite.rect.unwrap().min.y, 0.0);
        }
        let kinds = world.resource::<crate::app::ActorKinds>();
        for (glyph, expected) in [
            ('e', CharacterId::Watchman),
            ('g', CharacterId::Watchman),
            ('h', CharacterId::Hound),
            ('f', CharacterId::FurnaceGuard),
            ('r', CharacterId::Scavenger),
            ('s', CharacterId::Surveyor),
        ] {
            assert_eq!(kinds.get(glyph).sprite, Some(expected));
        }
    }

    #[test]
    fn enemy_alert_markers_remain_visible_above_the_sprite() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = boot_map("XXXXXXX\nXp.e..X\nX.....X\nXXXXXXX");
        let world = app.world_mut();
        let enemy = world
            .query_filtered::<Entity, With<Enemy>>()
            .single(world)
            .unwrap();
        for object in &mut world.resource_mut::<RenderBuffers>().objects {
            if object.entity == enemy {
                object.cell.ch = '!';
            }
        }
        world
            .run_system_once(super::super::text::place_object_sprites)
            .unwrap();
        let mut text = world.query::<(&Text2d, &ObjectSprite)>();
        assert!(text
            .iter(world)
            .any(|(text, sprite)| text.0 == "!" && sprite.shown && sprite.lift() > 2.0));
        let index = world.resource::<PixelInstances>();
        assert!(
            world
                .get::<ObjectSprite>(index.0[&(enemy, 0)])
                .unwrap()
                .shown
        );
    }

    #[test]
    fn slain_enemies_keep_their_look_and_facing_while_sinking() {
        use bevy::ecs::system::RunSystemOnce;
        let mut app = boot_map("XXXXXXX\nXp.e..X\nX.....X\nXXXXXXX");
        let world = app.world_mut();
        let enemy = world
            .query_filtered::<Entity, With<Enemy>>()
            .single(world)
            .unwrap();
        let expected = world.get::<PixelCharacter>(enemy).unwrap().0;
        let facing = *world.get::<Facing>(enemy).unwrap();
        world.write_message(Died(enemy));
        world
            .run_system_once(crate::choreography::leave_remains)
            .unwrap();
        let mut remains = world.query::<(&Remains, &PixelCharacter, &Facing)>();
        assert!(remains
            .iter(world)
            .any(|(body, look, way)| body.0 == enemy && look.0 == expected && *way == facing));
    }
}
