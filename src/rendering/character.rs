//! Four-direction pixel actors over the composed frame. Animation reads the
//! existing movement and gesture cues; it never writes simulation state.

use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    platform::collections::HashMap,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    sprite::Anchor,
};

use super::text::{ObjectSprite, CELL_SIZE};
use crate::content::character::{CharacterSheet, FRAMES_PER_DIRECTION};
use crate::model::*;
use crate::schedule::{GamePhase, StartupPhase};

pub struct CharacterSpritePlugin;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct CharacterOutput;

impl Plugin for CharacterSpritePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PixelInstances>()
            .add_message::<Gesture>()
            .add_message::<WindUp>()
            .add_message::<Release>()
            .add_observer(attach_animation)
            .add_systems(Startup, setup.in_set(StartupPhase::Renderer))
            .add_systems(
                Update,
                (take_cues, advance_animation)
                    .chain()
                    .after(GamePhase::Animation)
                    .before(GamePhase::Presentation)
                    .run_if(resource_exists::<PixelAtlas>),
            )
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
    idle_seconds: f32,
    walk_frames_per_cell: f32,
}

#[derive(Resource, Default)]
struct PixelInstances(HashMap<(Entity, u16), Entity>);

#[derive(Component)]
struct PixelInstance {
    source: Entity,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Clip {
    #[default]
    Rest,
    Windup,
    Strike,
    Recover,
    Hurt,
}

#[derive(Component, Default)]
struct CharacterAnimation {
    last_position: Option<Vec2>,
    distance: f32,
    age: f32,
    clip: Clip,
    remaining: f32,
    duration: f32,
    frame: usize,
}

impl CharacterAnimation {
    fn play(&mut self, clip: Clip, seconds: f32) {
        self.clip = clip;
        self.duration = seconds.max(0.0);
        self.remaining = self.duration;
    }

    fn advance(&mut self, position: Vec2, dt: f32, atlas: &PixelAtlas) {
        self.age += dt;
        let travelled = self
            .last_position
            .map_or(0.0, |last| last.distance(position));
        self.last_position = Some(position);
        // Portal/wrap jumps are not steps across the intervening map.
        let moving = travelled > 0.0001 && travelled < 1.5;
        if moving {
            self.distance += travelled;
        }
        self.remaining = (self.remaining - dt).max(0.0);
        if self.clip != Clip::Windup && self.remaining == 0.0 {
            self.clip = Clip::Rest;
        }
        self.frame = match self.clip {
            Clip::Windup => 6,
            Clip::Strike if self.remaining > self.duration * 0.35 => 7,
            Clip::Strike | Clip::Recover => 8,
            Clip::Hurt => 9,
            Clip::Rest if moving => 2 + (self.distance * atlas.walk_frames_per_cell) as usize % 4,
            Clip::Rest => (self.age / atlas.idle_seconds) as usize % 2,
        };
    }
}

fn attach_animation(add: On<Add, PixelCharacter>, mut commands: Commands) {
    commands
        .entity(add.entity)
        .insert(CharacterAnimation::default());
}

fn setup(mut commands: Commands, images: Option<ResMut<Assets<Image>>>) {
    let Some(mut images) = images else { return };
    let sheet = CharacterSheet::bundled();
    let mut image = Image::new(
        Extent3d {
            width: sheet.width * FRAMES_PER_DIRECTION as u32,
            height: sheet.height * 4,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        sheet.pixels().expect("valid pixel atlas"),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::nearest();
    commands.insert_resource(PixelAtlas {
        image: images.add(image),
        size: Vec2::new(sheet.width as f32, sheet.height as f32),
        world_height: sheet.world_height,
        idle_seconds: sheet.idle_seconds,
        walk_frames_per_cell: sheet.walk_frames_per_cell,
    });
}

/// Cues are already timed to the choreography's windup/contact/recoil.
fn take_cues(
    mut windups: MessageReader<WindUp>,
    mut releases: MessageReader<Release>,
    mut gestures: MessageReader<Gesture>,
    mut actors: Query<&mut CharacterAnimation>,
) {
    for cue in windups.read() {
        if let Ok(mut animation) = actors.get_mut(cue.entity) {
            animation.play(Clip::Windup, cue.secs);
        }
    }
    for cue in releases.read() {
        if let Ok(mut animation) = actors.get_mut(cue.entity) {
            animation.play(
                if cue.strike {
                    Clip::Strike
                } else {
                    Clip::Recover
                },
                cue.secs,
            );
        }
    }
    for cue in gestures.read() {
        if let Ok(mut animation) = actors.get_mut(cue.entity) {
            let clip = match cue.shape {
                GestureShape::Shake { .. } => Clip::Hurt,
                GestureShape::Reach { .. } => Clip::Recover,
                _ => Clip::Strike,
            };
            animation.play(clip, cue.secs);
        }
    }
}

fn advance_animation(
    time: Res<Time>,
    atlas: Res<PixelAtlas>,
    mut actors: Query<(&AnimatedPos, &mut CharacterAnimation)>,
) {
    for (position, mut animation) in &mut actors {
        animation.advance(position.position, time.delta_secs(), &atlas);
    }
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
    volume: Option<Res<WallVolume>>,
    extruded: Option<Res<super::extruded::ExtrudedWalls>>,
    actors: Query<(&Facing, &CharacterAnimation)>,
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
        let Ok((facing, animation)) = actors.get(object.entity) else {
            continue;
        };
        let Ok((mut drawn, mut sprite, mut transform, mut visibility)) = sprites.get_mut(entity)
        else {
            continue;
        };
        *drawn = ObjectSprite::from_object(object);
        drawn.shown = true;
        let row = direction_row(facing.0, camera.quarter_turns() + object.quarters as i32);
        let corner = Vec2::new(animation.frame as f32, row as f32) * atlas.size;
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

    fn atlas() -> PixelAtlas {
        PixelAtlas {
            image: Handle::default(),
            size: Vec2::new(24.0, 32.0),
            world_height: 28.0,
            idle_seconds: 0.65,
            walk_frames_per_cell: 4.0,
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

    #[test]
    fn gait_tracks_distance_stops_at_rest_and_ignores_teleports() {
        let atlas = atlas();
        let mut a = CharacterAnimation::default();
        a.advance(Vec2::ZERO, 0.01, &atlas);
        for step in 1..=4 {
            a.advance(Vec2::X * step as f32 * 0.25, 0.01, &atlas);
            assert!((2..6).contains(&a.frame));
        }
        assert_eq!(a.distance, 1.0);
        a.advance(Vec2::X, 0.01, &atlas);
        assert!(a.frame < 2);
        a.advance(Vec2::splat(30.0), 0.01, &atlas);
        assert_eq!(a.distance, 1.0);
        assert!(a.frame < 2);
    }

    #[test]
    fn held_windup_strike_recovery_and_hit_return_to_idle() {
        let mut a = CharacterAnimation::default();
        a.play(Clip::Windup, 0.2);
        a.advance(Vec2::ZERO, 2.0, &atlas());
        assert_eq!(a.frame, 6, "windup holds until release");
        a.play(Clip::Strike, 0.3);
        a.advance(Vec2::ZERO, 0.05, &atlas());
        assert_eq!(a.frame, 7);
        a.advance(Vec2::ZERO, 0.2, &atlas());
        assert_eq!(a.frame, 8);
        a.advance(Vec2::ZERO, 0.1, &atlas());
        assert!(a.frame < 2);
        a.play(Clip::Hurt, 0.2);
        a.advance(Vec2::ZERO, 0.05, &atlas());
        assert_eq!(a.frame, 9);
        a.advance(Vec2::ZERO, 0.2, &atlas());
        assert!(a.frame < 2);
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
            assert_eq!(sprite.rect.unwrap().height(), 32.0);
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
    fn choreography_messages_choose_attack_and_hurt_frames() {
        let mut app = boot();
        let world = app.world_mut();
        let actor = world
            .query_filtered::<Entity, With<PixelCharacter>>()
            .single(world)
            .unwrap();
        world.write_message(WindUp {
            entity: actor,
            shape: GestureShape::Thrust { toward: Vec2::X },
            secs: 0.2,
        });
        app.update();
        assert_eq!(
            app.world().get::<CharacterAnimation>(actor).unwrap().frame,
            6
        );
        app.world_mut().write_message(Release {
            entity: actor,
            secs: 0.3,
            strike: true,
        });
        app.update();
        assert_eq!(
            app.world().get::<CharacterAnimation>(actor).unwrap().frame,
            7
        );
        app.world_mut().write_message(Gesture {
            entity: actor,
            shape: GestureShape::Shake { toward: Vec2::X },
            secs: 0.3,
        });
        app.update();
        assert_eq!(
            app.world().get::<CharacterAnimation>(actor).unwrap().frame,
            9
        );
    }
}
