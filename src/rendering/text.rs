//! Bevy `Text2d` renderer for the renderer-neutral composed cell buffer.

use std::fmt::Write;

use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;

use crate::lighting::{palette_color, GRAY};
use crate::model::*;
use crate::schedule::{GamePhase, StartupPhase};

use super::walls_3d::{billboard_pattern, ExtrudedWalls};
use crate::animation::MotionStyle;

pub(super) const CELL_SIZE: Vec2 = Vec2::new(12.0, 20.0);

#[derive(Component)]
struct HudText;

#[derive(Component, Clone, Copy)]
pub(super) struct MapCell(pub(super) IVec2);

/// Text entity drawing one object at its animated position.
#[derive(Component, Clone, Copy)]
pub(super) struct ObjectSprite {
    position: Vec2,
    depth: u8,
    pub(super) shown: bool,
}

impl ObjectSprite {
    /// Displayed position in grid cells.
    pub(super) fn position(&self) -> Vec2 {
        self.position
    }
}

/// The sprite drawing each object, by the object's entity.
#[derive(Resource, Default)]
pub(super) struct ObjectSprites(EntityHashMap<Entity>);

#[derive(Resource, Clone)]
pub(super) struct CellFont(Handle<Font>);

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
    grid: Res<MapGrid>,
    fonts: Option<ResMut<Assets<Font>>>,
) {
    let (width, height) = (grid.width, grid.height);
    let camera = commands
        .spawn((
            Camera2d,
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: bevy::camera::ScalingMode::AutoMin {
                    min_width: (width + 2) as f32 * CELL_SIZE.x,
                    min_height: (height + 4) as f32 * CELL_SIZE.y,
                },
                ..OrthographicProjection::default_2d()
            }),
        ))
        .id();
    // The default Bevy font omits the box-drawing and marker glyphs. Headless
    // tests do not install font assets, so they use the default handle.
    let font = fonts
        .map(|mut fonts| {
            fonts.add(Font::from_bytes(
                include_bytes!("../../assets/fonts/DejaVuSansMono.ttf").to_vec(),
            ))
        })
        .unwrap_or_default();

    for y in 0..height {
        for x in 0..width {
            let position = IVec2::new(x, y);
            // The perspective backend reuses this allocation for small
            // multiline ASCII billboards.
            let mut cell_text = String::with_capacity(32);
            cell_text.push(' ');
            commands.spawn((
                MapCell(position),
                Text2d::new(cell_text),
                TextLayout::justify(Justify::Center),
                TextFont {
                    font: font.clone().into(),
                    font_size: FontSize::Px(20.0),
                    ..default()
                },
                TextColor(palette_color(GRAY)),
                Transform::from_translation(grid_to_world(position, width, height)),
            ));
        }
    }

    commands.insert_resource(CellFont(font.clone()));
    commands.spawn((
        HudText,
        UiTargetCamera(camera),
        Text::new(String::with_capacity(224)),
        TextFont {
            font: font.into(),
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
        let Some(index) = buffers.idx(cell.0) else {
            continue;
        };
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
        .any(|object| !sprites.0.contains_key(&object.entity))
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
            .filter(|object| !sprites.0.contains_key(&object.entity))
            .copied()
            .collect()
    };
    let grid = world.resource::<MapGrid>();
    let (width, height) = (grid.width, grid.height);
    let font = world.resource::<CellFont>().0.clone();
    for object in missing {
        let sprite = world
            .spawn((
                // Hidden until the placement pass fills in the glyph this frame.
                ObjectSprite {
                    position: object.position,
                    depth: object.cell.depth,
                    shown: false,
                },
                Text2d::new(String::with_capacity(32)),
                TextLayout::justify(Justify::Center),
                TextFont {
                    font: font.clone().into(),
                    font_size: FontSize::Px(20.0),
                    ..default()
                },
                TextColor(palette_color(object.cell.color)),
                Transform::from_translation(
                    grid_to_world_f(object.position, width, height) + Vec3::Z,
                ),
                Visibility::Hidden,
            ))
            .id();
        world
            .resource_mut::<ObjectSprites>()
            .0
            .insert(object.entity, sprite);
    }
}

/// Draws one text sprite per visible object at its animated position.
pub(super) fn place_object_sprites(
    grid: Res<MapGrid>,
    buffers: Res<RenderBuffers>,
    index: Res<ObjectSprites>,
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
        let Some(&entity) = index.0.get(&object.entity) else {
            continue;
        };
        let Ok((mut sprite, mut text, mut color, ..)) = sprites.get_mut(entity) else {
            continue;
        };
        drawn.clear();
        push_glyph(&mut drawn, object.cell.ch, billboards);
        sprite.position = object.position;
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
        let translation = object_translation(&sprite, &grid);
        if transform.translation != translation {
            transform.translation = translation;
        }
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
fn object_translation(sprite: &ObjectSprite, grid: &MapGrid) -> Vec3 {
    grid_to_world_f(sprite.position, grid.width, grid.height)
        + Vec3::Z * (1.0 + sprite.depth as f32 * 0.1)
}

#[allow(clippy::too_many_arguments)]
fn update_hud(
    turn: Res<TurnState>,
    grid: Res<MapGrid>,
    extruded_walls: Option<Res<ExtrudedWalls>>,
    players: Query<(&Pos, &Tokens), With<Player>>,
    enemies: Query<Entity, (With<Enemy>, Without<DestroyRequested>)>,
    collisions: Res<CollisionBuffer>,
    motion: Option<Res<MotionStyle>>,
    mut hud: Query<&mut Text, With<HudText>>,
) {
    let (position, tokens) = players
        .iter()
        .next()
        .map(|(pos, tokens)| (pos.0, tokens.count))
        .unwrap_or((IVec2::ZERO, 0));
    for mut text in hud.iter_mut() {
        text.0.clear();
        write!(
            text.0,
            "PavEcsGame Lite Bevy port | arrows/WASD{} | M motion: {}\nTick {} | {} | map {}x{} | player ({},{}) | tokens {} | enemies {} | bumps {}",
            if extruded_walls.is_some() {
                " | Q/E orbit | wheel zoom"
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
    use std::time::Duration;

    use bevy::time::TimeUpdateStrategy;

    use super::*;
    use crate::animation::ObjectAnimationPlugin;
    use crate::app::GamePlugin;

    fn player_sprite(app: &mut App) -> (IVec2, Vec2) {
        let world = app.world_mut();
        let (player, pos) = world
            .query_filtered::<(Entity, &Pos), With<Player>>()
            .single(world)
            .unwrap();
        let pos = pos.0;
        let sprite = world.resource::<ObjectSprites>().0[&player];
        let sprite = world.get::<ObjectSprite>(sprite).expect("player sprite");
        (pos, sprite.position())
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
    fn sprites_sit_on_their_cells_without_animation() {
        let mut app = boot(false);
        step_right(&mut app);
        let (moved, shown) = player_sprite(&mut app);
        assert_eq!(shown, moved.as_vec2());
    }
}
