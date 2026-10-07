//! Bevy `Text2d` renderer for the renderer-neutral composed cell buffer.

use std::fmt::Write;

use bevy::image::{ImageFilterMode, ImageSampler};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::text::FontAtlasSet;

use crate::lighting::{palette_color, GRAY};
use crate::model::*;
use crate::schedule::{GamePhase, StartupPhase};
use crate::simulation::Rules;

use super::walls_3d::{billboard_pattern, ExtrudedWalls};
use crate::animation::MotionStyle;

/// Square cells: Unscii-8 glyphs are as wide as they are tall.
pub(super) const CELL_SIZE: Vec2 = Vec2::new(16.0, 16.0);
/// Unscii-8 is an 8 px pixel font, drawn at twice its size.
const CELL_FONT_SIZE: f32 = 16.0;

#[derive(Component)]
struct HudText;

/// The text entity drawing one frame slot (see `RenderBuffers`): the map
/// position it shows changes as the frame follows the player.
#[derive(Component, Clone, Copy)]
pub(super) struct MapCell(pub(super) usize);

/// Text entity drawing one object at its animated position.
#[derive(Component, Clone, Copy)]
pub(super) struct ObjectSprite {
    position: Vec2,
    lift: f32,
    depth: u8,
    pub(super) shown: bool,
}

impl ObjectSprite {
    /// Displayed position in grid cells.
    pub(super) fn position(&self) -> Vec2 {
        self.position
    }

    /// Hop height above the ground, in cells.
    pub(super) fn lift(&self) -> f32 {
        self.lift
    }
}

/// The sprite drawing each object instance, by the object's entity and
/// instance (one object may be seen in several places through portals).
#[derive(Resource, Default)]
pub(super) struct ObjectSprites(HashMap<(Entity, u16), Entity>);

#[derive(Resource, Clone)]
pub(super) struct CellFont(Handle<Font>);

/// Glyphs that depend on direction (wall shapes, facing markers), each as
/// drawn in a view turned 0 to 3 quarter turns counter-clockwise, so they
/// keep pointing the same way on the map when the view turns.
#[derive(Resource, Default)]
pub(super) struct TurnedGlyphs(HashMap<char, [char; 4]>);

impl TurnedGlyphs {
    fn from_rules(rules: &Rules) -> Self {
        let mut table = HashMap::default();
        // Blank slots (a rule without that shape) have nothing to turn.
        for symbol in rules.wall.symbols.into_iter().filter(|s| *s != ' ') {
            table.insert(
                symbol,
                [0, 1, 2, 3].map(|q| rules.wall.turned(symbol, q).unwrap()),
            );
        }
        for rule in [&rules.triangle, &rules.v] {
            for symbol in rule.symbols.into_iter().filter(|s| *s != ' ') {
                table.insert(
                    symbol,
                    [0, 1, 2, 3].map(|q| rule.turned(symbol, q).unwrap()),
                );
            }
        }
        Self(table)
    }

    /// `symbol` as drawn in the view; glyphs without a direction stay.
    pub(super) fn get(&self, symbol: char, camera: &ViewCamera) -> char {
        self.0
            .get(&symbol)
            .map_or(symbol, |turns| turns[camera.quarter_turns() as usize])
    }
}

/// Per-frame work performed by the text renderer.
#[derive(Resource, Debug, Default)]
pub struct TextRenderStats {
    pub changed_cells: usize,
}

/// Renders `RenderBuffers` through one Bevy text entity per map cell.
pub struct TextRendererPlugin;

impl Plugin for TextRendererPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TextRenderStats>()
            .init_resource::<ObjectSprites>()
            .add_systems(Startup, setup.in_set(StartupPhase::Renderer))
            .add_systems(
                Update,
                (
                    cycle_motion,
                    place_cells,
                    smooth_turning_glyphs,
                    flush_cells,
                    spawn_object_sprites.run_if(object_sprites_missing),
                    place_object_sprites,
                    update_hud,
                )
                    .chain()
                    .in_set(GamePhase::Output),
            );
    }
}

pub(super) fn setup(
    mut commands: Commands,
    buffers: Res<RenderBuffers>,
    rules: Option<Res<Rules>>,
    fonts: Option<ResMut<Assets<Font>>>,
) {
    let camera = commands
        .spawn((
            Camera2d,
            // One world unit per screen pixel keeps the pixel font sharp;
            // the view follows the player, so the map need not fit.
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: bevy::camera::ScalingMode::WindowSize,
                ..OrthographicProjection::default_2d()
            }),
        ))
        .id();
    // The default Bevy font omits the box-drawing and marker glyphs. Headless
    // tests do not install font assets, so they use the default handle.
    let font = fonts
        .map(|mut fonts| {
            fonts.add(Font::from_bytes(
                include_bytes!("../../assets/fonts/unscii-8.ttf").to_vec(),
            ))
        })
        .unwrap_or_default();

    // One text entity per frame slot; `place_cells` puts each where the
    // map position it shows is on screen.
    for index in 0..buffers.current.len() {
        // The perspective backend reuses this allocation for small
        // multiline ASCII billboards.
        let mut cell_text = String::with_capacity(32);
        cell_text.push(' ');
        commands.spawn((
            MapCell(index),
            Text2d::new(cell_text),
            TextLayout::justify(Justify::Center),
            TextFont {
                font: font.clone().into(),
                font_size: FontSize::Px(CELL_FONT_SIZE),
                ..default()
            },
            TextColor(palette_color(GRAY)),
            Transform::default(),
        ));
    }

    commands.insert_resource(CellFont(font.clone()));
    commands.insert_resource(rules.map_or_else(TurnedGlyphs::default, |rules| {
        TurnedGlyphs::from_rules(&rules)
    }));
    commands.spawn((
        HudText,
        UiTargetCamera(camera),
        Text::new(String::with_capacity(256)),
        // Plain ASCII: Bevy's default font is narrower than the square
        // cell font and keeps the HUD on two lines.
        TextFont {
            font_size: FontSize::Px(15.0),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(8.0),
            right: Val::Px(8.0),
            ..default()
        },
    ));
}

fn flush_cells(
    buffers: Res<RenderBuffers>,
    extruded_walls: Option<Res<ExtrudedWalls>>,
    mut stats: ResMut<TextRenderStats>,
    mut cells: Query<(&MapCell, &mut Text2d, &mut TextColor)>,
) {
    stats.changed_cells = 0;
    for (cell, mut text, mut color) in cells.iter_mut() {
        let index = cell.0;
        if buffers.ground[index] != buffers.previous_ground[index] {
            stats.changed_cells += 1;
            let cell = buffers.ground[index];
            text.0.clear();
            // The perspective backend draws the floor as a mesh.
            let replaced_by_mesh = extruded_walls.is_some() && cell.ch == '.';
            if cell.ch == '\0' || replaced_by_mesh {
                text.0.push(' ');
            } else {
                text.0.push(cell.ch);
            }
            color.0 = palette_color(cell.color);
        }
    }
}

fn push_glyph(text: &mut String, ch: char, billboards: bool) {
    match billboard_pattern(ch).filter(|_| billboards) {
        Some(pattern) => text.push_str(pattern),
        None => text.push(ch),
    }
}

fn object_sprites_missing(buffers: Res<RenderBuffers>, sprites: Res<ObjectSprites>) -> bool {
    buffers
        .objects
        .iter()
        .any(|object| !sprites.0.contains_key(&(object.entity, object.instance)))
}

/// Spawns sprites for newly seen objects. It is exclusive and runs only when
/// one is missing, so steady frames do not pay for a command sync point.
fn spawn_object_sprites(world: &mut World) {
    let missing: Vec<ObjectCell> = {
        let sprites = world.resource::<ObjectSprites>();
        world
            .resource::<RenderBuffers>()
            .objects
            .iter()
            .filter(|object| !sprites.0.contains_key(&(object.entity, object.instance)))
            .copied()
            .collect()
    };
    let camera = *world.resource::<ViewCamera>();
    let font = world.resource::<CellFont>().0.clone();
    for object in missing {
        let sprite = world
            .spawn((
                // Hidden until the placement pass fills in the glyph this frame.
                ObjectSprite {
                    position: object.position,
                    lift: object.lift,
                    depth: object.cell.depth,
                    shown: false,
                },
                Text2d::new(String::with_capacity(32)),
                TextLayout::justify(Justify::Center),
                TextFont {
                    font: font.clone().into(),
                    font_size: FontSize::Px(CELL_FONT_SIZE),
                    ..default()
                },
                TextColor(palette_color(object.cell.color)),
                Transform::from_translation(view_translation(&camera, object.position) + Vec3::Z)
                    .with_rotation(glyph_rotation(&camera)),
                Visibility::Hidden,
            ))
            .id();
        world
            .resource_mut::<ObjectSprites>()
            .0
            .insert((object.entity, object.instance), sprite);
    }
}

/// Draws one text sprite per visible object at its animated position.
pub(super) fn place_object_sprites(
    camera: Res<ViewCamera>,
    buffers: Res<RenderBuffers>,
    index: Res<ObjectSprites>,
    turned: Res<TurnedGlyphs>,
    extruded_walls: Option<Res<ExtrudedWalls>>,
    mut drawn: Local<String>,
    mut sprites: Query<(
        &mut ObjectSprite,
        &mut Text2d,
        &mut TextColor,
        &mut Transform,
        &mut Visibility,
    )>,
) {
    let billboards = extruded_walls.is_some();
    for (mut sprite, ..) in &mut sprites {
        sprite.shown = false;
    }

    for object in &buffers.objects {
        // The perspective backend draws walls as meshes instead.
        if extruded_walls
            .as_ref()
            .is_some_and(|walls| walls.symbol_of(object.entity) == Some(object.cell.ch))
        {
            continue;
        }
        let Some(&entity) = index.0.get(&(object.entity, object.instance)) else {
            continue;
        };
        let Ok((mut sprite, mut text, mut color, ..)) = sprites.get_mut(entity) else {
            continue;
        };
        drawn.clear();
        push_glyph(&mut drawn, turned.get(object.cell.ch, &camera), billboards);
        sprite.position = object.position;
        sprite.lift = object.lift;
        sprite.depth = object.cell.depth;
        sprite.shown = true;
        if text.0 != *drawn {
            text.0.clear();
            text.0.push_str(&drawn);
        }
        let desired = palette_color(object.cell.color);
        if color.0 != desired {
            color.0 = desired;
        }
    }

    for (sprite, _, _, mut transform, mut visibility) in &mut sprites {
        let desired = if sprite.shown {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if *visibility != desired {
            *visibility = desired;
        }
        // The perspective backend projects sprites itself.
        if !sprite.shown || billboards {
            continue;
        }
        // Top-down, a hop shows as a small shift up the screen.
        let translation = object_translation(&sprite, &camera)
            + Vec3::Y * sprite.lift * CELL_SIZE.y * camera.zoom;
        if transform.translation != translation {
            transform.translation = translation;
        }
        let scale = Vec3::splat(camera.zoom);
        if transform.scale != scale {
            transform.scale = scale;
        }
        let rotation = glyph_rotation(&camera);
        if transform.rotation != rotation {
            transform.rotation = rotation;
        }
    }
}

/// Places the ground cells where the view camera shows them. The map turns
/// and zooms around the screen centre as one picture, glyphs included; the
/// 2D camera itself never moves. The perspective backend projects cells
/// itself.
fn place_cells(
    camera: Res<ViewCamera>,
    buffers: Res<RenderBuffers>,
    extruded_walls: Option<Res<ExtrudedWalls>>,
    mut placed: Local<Option<(ViewCamera, IVec2)>>,
    mut cells: Query<(&MapCell, &mut Transform)>,
) {
    let placement = (*camera, buffers.origin);
    if extruded_walls.is_some() || *placed == Some(placement) {
        return;
    }
    *placed = Some(placement);
    let scale = Vec3::splat(camera.zoom);
    let rotation = glyph_rotation(&camera);
    for (cell, mut transform) in &mut cells {
        let position = buffers.pos_of(cell.0);
        transform.translation = view_translation(&camera, position.as_vec2());
        transform.rotation = rotation;
        transform.scale = scale;
    }
}

/// M cycles through the motion presets.
fn cycle_motion(keys: Option<Res<ButtonInput<KeyCode>>>, motion: Option<ResMut<MotionStyle>>) {
    if let Some(mut motion) = motion {
        if keys.is_some_and(|keys| keys.just_pressed(KeyCode::KeyM)) {
            *motion = motion.next();
        }
    }
}

/// Objects draw above the ground cells, deeper glyphs on top.
fn object_translation(sprite: &ObjectSprite, camera: &ViewCamera) -> Vec3 {
    view_translation(camera, sprite.position) + Vec3::Z * (1.0 + sprite.depth as f32 * 0.1)
}

/// Glyphs are pixel art sampled nearest (`ImagePlugin::default_nearest`),
/// which keeps them sharp upright but stair-steps their edges when tilted.
/// While the view is between quarter turns the font atlases sample linearly,
/// smoothing tilted edges; at rest they return to the pixel-exact default.
/// The sampler changes only when a turn starts or ends.
fn smooth_turning_glyphs(
    camera: Res<ViewCamera>,
    extruded_walls: Option<Res<ExtrudedWalls>>,
    atlases: Option<Res<FontAtlasSet>>,
    images: Option<ResMut<Assets<Image>>>,
) {
    let (Some(atlases), Some(mut images)) = (atlases, images) else {
        return;
    };
    // The perspective backend keeps its text upright.
    let tilted = extruded_walls.is_none() && camera.quarter_remainder() != 0.0;
    for atlas in atlases.values().flatten() {
        let Some(image) = images.get(&atlas.texture) else {
            continue;
        };
        let linear = matches!(
            &image.sampler,
            ImageSampler::Descriptor(sampler) if sampler.mag_filter == ImageFilterMode::Linear
        );
        if linear == tilted {
            continue;
        }
        if let Some(mut image) = images.get_mut(&atlas.texture) {
            image.sampler = if tilted {
                ImageSampler::linear()
            } else {
                ImageSampler::Default
            };
        }
    }
}

/// Where the view camera shows a map point, in 2D world units around the
/// screen centre.
///
/// The map turns as one rigid picture. With square cells that is all; if
/// the cell font were not square, the spacing along each map axis eases
/// from the cell width to its height as that axis turns from horizontal to
/// vertical on screen, so every quarter turn is the upright grid
/// (`ViewCamera::to_view` in cell units) and half way the cells are square.
fn view_translation(camera: &ViewCamera, map: Vec2) -> Vec3 {
    let turn = camera.turn();
    // 1 while map x runs across the screen, 0 while it runs up and down.
    let across = turn.cos * turn.cos;
    let spacing = Vec2::new(
        CELL_SIZE.y.lerp(CELL_SIZE.x, across),
        CELL_SIZE.x.lerp(CELL_SIZE.y, across),
    );
    let turned = turn * ((map - camera.position) * spacing);
    let view = (turned + camera.offset * CELL_SIZE) * camera.zoom;
    Vec3::new(view.x, -view.y, 0.0)
}

/// Glyphs are drawn for the nearest quarter turn (`TurnedGlyphs`) and tilted
/// by the rest of the turn, so they lie in the turning picture. Half way,
/// a glyph tilted +45° and its quarter-turned glyph tilted -45° are the
/// same shape, so the swap is not seen; at rest glyphs are upright.
fn glyph_rotation(camera: &ViewCamera) -> Quat {
    Quat::from_rotation_z(camera.quarter_remainder())
}

#[allow(clippy::too_many_arguments)]
fn update_hud(
    turn: Res<TurnState>,
    grid: Res<MapGrid>,
    extruded_walls: Option<Res<ExtrudedWalls>>,
    players: Query<(&Pos, &Tokens, Option<&Inventory>), With<Player>>,
    keys: Query<(), With<Key>>,
    enemies: Query<Entity, (With<Enemy>, Without<DestroyRequested>)>,
    collisions: Res<CollisionBuffer>,
    motion: Option<Res<MotionStyle>>,
    mut hud: Query<&mut Text, With<HudText>>,
) {
    let (position, tokens, held_keys) = players
        .iter()
        .next()
        .map(|(pos, tokens, inventory)| {
            let held_keys = inventory.map_or(0, |inventory| {
                inventory
                    .0
                    .iter()
                    .filter(|&&item| keys.contains(item))
                    .count()
            });
            (pos.0, tokens.count, held_keys)
        })
        .unwrap_or((IVec2::ZERO, 0, 0));
    for mut text in hud.iter_mut() {
        text.0.clear();
        write!(
            text.0,
            "PavEcsGame Lite Bevy port | arrows/WASD | Q/E turn | Z/X zoom{} | M motion: {}\nTick {} | {} | map {}x{} | player ({},{}) | tokens {} | keys {} | enemies {} | bumps {}",
            if extruded_walls.is_some() {
                " | wheel zoom"
            } else {
                ""
            },
            motion.as_ref().map_or("off", |motion| motion.name()),
            turn.tick,
            turn.phase_name(),
            grid.width,
            grid.height,
            position.x,
            position.y,
            tokens,
            held_keys,
            enemies.iter().count(),
            collisions.0.len(),
        )
        .expect("writing to String cannot fail");
    }
}

pub(super) fn grid_to_world(position: IVec2, width: i32, height: i32) -> Vec3 {
    grid_to_world_f(position.as_vec2(), width, height)
}

pub(super) fn grid_to_world_f(position: Vec2, width: i32, height: i32) -> Vec3 {
    Vec3::new(
        (position.x - width as f32 / 2.0 + 0.5) * CELL_SIZE.x,
        (height as f32 / 2.0 - position.y - 0.5) * CELL_SIZE.y,
        0.0,
    )
}

#[cfg(test)]
mod tests {
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
    use std::time::Duration;

    use bevy::time::TimeUpdateStrategy;

    use super::*;
    use crate::animation::ObjectAnimationPlugin;
    use crate::app::GamePlugin;
    use crate::camera::CameraOperator;
    use crate::simulation::Rules;

    fn player_sprite(app: &mut App) -> (IVec2, Vec2) {
        let world = app.world_mut();
        let (player, pos) = world
            .query_filtered::<(Entity, &Pos), With<Player>>()
            .single(world)
            .unwrap();
        let pos = pos.0;
        let sprite = world.resource::<ObjectSprites>().0[&(player, 0)];
        let sprite = world.get::<ObjectSprite>(sprite).expect("player sprite");
        (pos, sprite.position())
    }

    fn player_transform(app: &mut App) -> Transform {
        let world = app.world_mut();
        let player = world
            .query_filtered::<Entity, With<Player>>()
            .single(world)
            .unwrap();
        let sprite = world.resource::<ObjectSprites>().0[&(player, 0)];
        *world.get::<Transform>(sprite).expect("player sprite")
    }

    fn player_lift(app: &mut App) -> f32 {
        let world = app.world_mut();
        let player = world
            .query_filtered::<Entity, With<Player>>()
            .single(world)
            .unwrap();
        let sprite = world.resource::<ObjectSprites>().0[&(player, 0)];
        world
            .get::<ObjectSprite>(sprite)
            .expect("player sprite")
            .lift()
    }

    fn boot(animated: bool) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                16,
            )))
            .add_plugins((GamePlugin, TextRendererPlugin));
        if animated {
            app.add_plugins(ObjectAnimationPlugin);
        }
        for _ in 0..64 {
            app.update();
        }
        app
    }

    fn step_right(app: &mut App) {
        let world = app.world_mut();
        world
            .query_filtered::<&mut Speed, With<Player>>()
            .single_mut(world)
            .unwrap()
            .0 = IVec2::X;
        app.update();
    }

    #[test]
    fn player_sprite_follows_the_animation() {
        let mut app = boot(true);
        let (start, at_rest) = player_sprite(&mut app);
        assert_eq!(at_rest, start.as_vec2());

        step_right(&mut app);
        let (moved, gliding) = player_sprite(&mut app);
        assert_eq!(moved, start + IVec2::X);
        assert!(
            gliding.x > start.x as f32 && gliding.x < moved.x as f32,
            "sprite should be between cells, was {gliding}"
        );

        // Enemies keep taking turns, but the player's sprite comes to rest.
        for _ in 0..60 {
            app.update();
        }
        assert_eq!(player_sprite(&mut app).1, moved.as_vec2());
    }

    #[test]
    fn camera_keeps_the_player_centred_while_it_glides() {
        let mut app = boot(true);
        step_right(&mut app);
        let (moved, gliding) = player_sprite(&mut app);
        assert_ne!(gliding, moved.as_vec2(), "player should be mid-move");

        // The world moves past the player, who stays at the screen centre
        // (only a hop lifts them up the screen).
        let lift = player_lift(&mut app);
        assert_eq!(
            player_transform(&mut app).translation.xy(),
            Vec2::new(0.0, lift * CELL_SIZE.y)
        );
    }

    #[test]
    fn turned_view_keeps_glyphs_upright_around_the_player() {
        let mut app = boot(false);
        let (cell, _) = player_sprite(&mut app);
        let mut operator = app.world_mut().resource_mut::<CameraOperator>();
        operator.turn_by_quarters(1);
        operator.snap();
        app.update();

        // A quarter turn counter-clockwise shows the cell east of the
        // player above them, one cell height up the screen.
        let world = app.world_mut();
        let slot = world
            .resource::<RenderBuffers>()
            .idx(cell + IVec2::X)
            .expect("east of the player is in the frame");
        let east = world
            .query::<(&MapCell, &Transform)>()
            .iter(world)
            .find(|(map_cell, _)| map_cell.0 == slot)
            .map(|(_, transform)| *transform)
            .expect("ground cell east of the player");
        assert!(east.translation.xy().distance(Vec2::new(0.0, CELL_SIZE.y)) < 1e-3);
        assert_eq!(east.rotation, Quat::IDENTITY);
        assert_eq!(player_transform(&mut app).translation.xy(), Vec2::ZERO);
    }

    #[test]
    fn turned_view_turns_wall_shapes_and_facing_markers() {
        let mut app = boot(false);
        let mut operator = app.world_mut().resource_mut::<CameraOperator>();
        operator.turn_by_quarters(1);
        operator.snap();
        app.update();

        let world = app.world_mut();
        let drawn: Vec<(char, String)> = {
            let sprites = world.resource::<ObjectSprites>().0.clone();
            let mut glyphs = world.query::<&Glyph>();
            let mut texts = world.query::<(&ObjectSprite, &Text2d)>();
            sprites
                .iter()
                .filter_map(|((object, _), sprite)| {
                    let glyph = glyphs.get(world, *object).ok()?;
                    let (sprite, text) = texts.get(world, *sprite).ok()?;
                    sprite.shown.then(|| (glyph.ch, text.0.clone()))
                })
                .collect()
        };
        let rules = world.resource::<Rules>();
        let mut walls = 0;
        let mut markers = 0;
        for (ch, text) in drawn {
            let expected = if let Some(turned) = rules.wall.turned(ch, 1) {
                walls += 1;
                turned
            } else if let Some(turned) = rules.triangle.turned(ch, 1) {
                markers += 1;
                turned
            } else {
                ch
            };
            assert_eq!(text, expected.to_string(), "glyph {ch}");
        }
        assert!(walls > 0 && markers > 0, "walls {walls}, markers {markers}");
    }

    #[test]
    fn layout_rests_on_the_upright_grid_at_every_quarter_turn() {
        for quarters in -2..=4 {
            let camera = ViewCamera {
                position: Vec2::new(3.0, 2.0),
                rotation: quarters as f32 * FRAC_PI_2,
                zoom: 1.5,
                offset: Vec2::new(0.5, 1.0),
            };
            for map in [
                Vec2::new(4.0, 2.0),
                Vec2::new(3.0, 5.0),
                Vec2::new(-1.5, 0.25),
            ] {
                let view = camera.to_view(map) * CELL_SIZE;
                let expected = Vec2::new(view.x, -view.y);
                let placed = view_translation(&camera, map).xy();
                assert!(
                    placed.distance(expected) < 1e-3,
                    "{quarters}: {placed} vs {expected}"
                );
            }
            assert!(camera.quarter_remainder().abs() < 1e-6);
            assert!(glyph_rotation(&camera).angle_between(Quat::IDENTITY) < 1e-6);
        }
    }

    #[test]
    fn mid_turn_the_picture_turns_rigidly_and_the_glyph_swap_is_seamless() {
        let at = |rotation: f32| ViewCamera {
            rotation,
            ..default()
        };
        // Half way the cells are square: east and south are equally far.
        let half = at(FRAC_PI_4);
        let east = view_translation(&half, Vec2::X).xy();
        let south = view_translation(&half, Vec2::Y).xy();
        assert!((east.length() - south.length()).abs() < 1e-3);
        assert!(east.dot(south).abs() < 1e-3, "axes stay square");

        // Either side of the half-way swap, cells barely move and the glyph
        // tilt flips from +45° (old glyph) to -45° (quarter-turned glyph).
        let (before, after) = (at(FRAC_PI_4 - 1e-4), at(FRAC_PI_4 + 1e-4));
        for map in [Vec2::X, Vec2::new(-3.0, 2.0)] {
            let moved = view_translation(&before, map).distance(view_translation(&after, map));
            assert!(moved < 0.05, "{map}: jumped {moved}");
        }
        assert_eq!(before.quarter_turns() + 1, after.quarter_turns());
        let tilt = |camera: &ViewCamera| glyph_rotation(camera).to_euler(EulerRot::XYZ).2;
        assert!((tilt(&before) - FRAC_PI_4).abs() < 1e-3);
        assert!((tilt(&after) + FRAC_PI_4).abs() < 1e-3);
    }

    #[test]
    fn glyphs_tilt_with_the_picture_while_the_view_turns() {
        let mut app = boot(false);
        app.world_mut()
            .resource_mut::<CameraOperator>()
            .turn_by_quarters(1);
        // Partway into the eased turn.
        app.update();
        let camera = *app.world().resource::<ViewCamera>();
        assert!(camera.rotation > 0.0 && camera.rotation < FRAC_PI_2);
        let expected = Quat::from_rotation_z(camera.quarter_remainder());
        assert!(camera.quarter_remainder() != 0.0);
        let world = app.world_mut();
        for transform in world
            .query_filtered::<&Transform, With<MapCell>>()
            .iter(world)
        {
            assert!(transform.rotation.angle_between(expected) < 1e-5);
        }
        assert!(player_transform(&mut app).rotation.angle_between(expected) < 1e-5);
    }

    #[test]
    fn font_atlases_sample_smoothly_only_while_the_view_turns() {
        use bevy::ecs::system::RunSystemOnce;
        use bevy::text::{FontAtlas, FontAtlasKey, FontHinting, FontSmoothing};

        let mut app = App::new();
        let mut images = Assets::<Image>::default();
        let atlas = FontAtlas::new(&mut images, UVec2::splat(64), FontSmoothing::AntiAliased);
        let texture = atlas.texture.clone();
        let mut atlases = FontAtlasSet::default();
        let key = FontAtlasKey {
            id: 0,
            index: 0,
            font_size_bits: 16.0_f32.to_bits(),
            variations_hash: 0,
            hinting: FontHinting::Disabled,
            font_smoothing: FontSmoothing::AntiAliased,
        };
        atlases.insert(key, vec![atlas]);
        app.insert_resource(images).insert_resource(atlases);
        let linear = |app: &App| {
            let image = app
                .world()
                .resource::<Assets<Image>>()
                .get(&texture)
                .unwrap();
            matches!(
                &image.sampler,
                ImageSampler::Descriptor(sampler) if sampler.mag_filter == ImageFilterMode::Linear
            )
        };

        for (rotation, smooth) in [
            (0.0, false),
            (0.3, true),
            (FRAC_PI_4 + 0.1, true),
            (FRAC_PI_2, false),
            (-FRAC_PI_2, false),
        ] {
            app.insert_resource(ViewCamera {
                rotation,
                ..default()
            });
            app.world_mut()
                .run_system_once(smooth_turning_glyphs)
                .unwrap();
            assert_eq!(linear(&app), smooth, "rotation {rotation}");
        }
    }

    #[test]
    fn sprites_sit_on_their_cells_without_animation() {
        let mut app = boot(false);
        step_right(&mut app);
        let (moved, shown) = player_sprite(&mut app);
        assert_eq!(shown, moved.as_vec2());
    }
}
