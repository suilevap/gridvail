//! Game assembly: startup spawn (mirrors `LoadMapSystem` + `SpawnEntitySystem`
//! end states) and composition of the domain plugins.
//!
//! Shared schedule phases preserve the container order: tiles → tokens → input/AI → commands →
//! direction → movement → friction → resolve → bindings → verify → destroy →
//! direction tiles → light requests → FOV → light layers → visibility →
//! compose → flush → HUD → turn update.
//!
//! Deliberate deviations from the Lite sources:
//! - entities spawn directly with `Pos` (the Lite `NewPosition` +
//!   `SpawnRequest` bootstrap converges to the same state on the first pass);
//! - the player has no `DirectionTile` (the Lite player carries a null rule
//!   name, which would throw in its rule dictionary; the classic `@` stays);
//! - one Bevy `World` + resources replaces `EcsUniverse` worlds-per-type.

use crate::content::map::{parse_map, SpawnKind};
use crate::content::tile_rules::{DirectionTileRule, TileRule};
use crate::lighting::{GRAY, RED, WHITE};
use crate::model::*;
use crate::presentation::{DynamicLight, PresentationPlugin};
use crate::schedule::{GamePhase, StartupPhase};
use crate::simulation::{self as sim, Rules, SimulationPlugin};
use crate::vision::VisionPlugin;
use bevy::prelude::*;

const MAP_TEXT: &str = include_str!("../../assets/maps/map1.txt");
const WALL_RULE_TEXT: &str = include_str!("../../assets/rules/wall_rule.txt");
const TRIANGLE_RULE_TEXT: &str = include_str!("../../assets/rules/direction_triangle_rule.txt");
const V_RULE_TEXT: &str = include_str!("../../assets/rules/direction_v_rule.txt");

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Startup,
            (StartupPhase::Content, StartupPhase::Derive).chain(),
        )
        .configure_sets(
            Update,
            (
                GamePhase::Simulation,
                GamePhase::FieldOfView,
                GamePhase::Lighting,
                GamePhase::Visibility,
                GamePhase::Presentation,
                GamePhase::Finalize,
            )
                .chain(),
        )
        .add_plugins((SimulationPlugin::new(42), VisionPlugin, PresentationPlugin))
        .add_systems(Startup, setup.in_set(StartupPhase::Content));
    }
}

fn setup(mut commands: Commands, fonts: Option<ResMut<Assets<Font>>>) {
    let (width, height, cells) = parse_map(MAP_TEXT);
    let cell_count = (width * height) as usize;
    commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::render::camera::ScalingMode::AutoMin {
                min_width: (width + 2) as f32 * CELL_SIZE.x,
                min_height: (height + 4) as f32 * CELL_SIZE.y,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
    // The default Bevy font is a subset without the box-drawing and marker
    // glyphs. Embed a licensed complete font; headless tests need no assets.
    let font = fonts
        .map(|mut fonts| {
            fonts.add(
                Font::try_from_bytes(
                    include_bytes!("../../assets/fonts/DejaVuSansMono.ttf").to_vec(),
                )
                .expect("bundled DejaVu font"),
            )
        })
        .unwrap_or_default();
    let mut grid = MapGrid::new(width, height);

    for cell in cells {
        let entity = match cell.kind {
            SpawnKind::Wall => commands
                .spawn((
                    Active,
                    Collider,
                    Wall,
                    Pos(cell.pos),
                    Glyph::new('#', 1, GRAY),
                    Tile {
                        mask: 0,
                        rule: "wall_rule".to_string(),
                    },
                ))
                .id(),
            SpawnKind::Player => {
                let player = commands
                    .spawn((
                        Active,
                        Collider,
                        Pos(cell.pos),
                        (
                            Speed::default(),
                            MoveCommand::default(),
                            PendingPos::default(),
                            PrevPos(cell.pos),
                        ),
                        Glyph::new('@', 1, WHITE),
                        Player(0),
                        Tokens::new(1),
                        Friction(1),
                        Facing::default(),
                        DirectionBasedOnSpeed,
                        VisualSensor { radius: 16 },
                        LightSource {
                            radius: 16,
                            kind: LightKind::None,
                            value: 32,
                        },
                        (
                            initial_fov(cell_count),
                            VisibilityMap {
                                revision: u64::MAX,
                                data: vec![Vis::empty(); cell_count],
                            },
                        ),
                    ))
                    .id();
                // Bound 'i' direction marker, one step ahead of the player
                // (the original light source belongs to the player).
                commands.spawn((
                    Active,
                    BoundTo {
                        parent: player,
                        offset: IVec2::new(1, 0),
                        offset_dir: IVec2::new(1, 0),
                    },
                    Pos(cell.pos + IVec2::X),
                    PrevPos(cell.pos + IVec2::X),
                    Facing::default(),
                    DirectionTile {
                        rule: "direction_triangle_rule".to_string(),
                    },
                    Glyph::new('i', 0, WHITE),
                ));
                player
            }
            SpawnKind::Enemy => commands
                .spawn((
                    Active,
                    Collider,
                    Enemy,
                    Pos(cell.pos),
                    Speed::default(),
                    MoveCommand::default(),
                    PendingPos::default(),
                    PrevPos(cell.pos),
                    Glyph::new('☺', 1, RED),
                    Tokens::new(1),
                    Friction(1),
                    Facing::default(),
                    DirectionBasedOnSpeed,
                    DirectionTile {
                        rule: "direction_v_rule".to_string(),
                    },
                ))
                .id(),
            SpawnKind::Electricity => commands
                .spawn((
                    Active,
                    ElectroField,
                    Pos(cell.pos),
                    Glyph::new('Ω', 0, WHITE),
                    LightSource {
                        radius: 3,
                        kind: LightKind::Electricity,
                        value: 32,
                    },
                    initial_fov(cell_count),
                ))
                .id(),
            SpawnKind::Light => commands
                .spawn((
                    Active,
                    Lamp,
                    Pos(cell.pos),
                    Glyph::new('i', 0, WHITE),
                    LightSource {
                        radius: 16,
                        kind: LightKind::Fire,
                        value: 196,
                    },
                    initial_fov(cell_count),
                ))
                .id(),
            SpawnKind::Acid => commands
                .spawn((
                    Active,
                    AcidPool,
                    Pos(cell.pos),
                    Glyph::new('▒', 0, WHITE),
                    LightSource {
                        radius: 4,
                        kind: LightKind::Acid,
                        value: 32,
                    },
                    initial_fov(cell_count),
                ))
                .id(),
        };
        if commands.get_entity(entity).is_ok() {
            // Colliders own their cells (mirrors the first UpdatePosition pass).
            if matches!(
                cell.kind,
                SpawnKind::Wall | SpawnKind::Player | SpawnKind::Enemy
            ) {
                grid.set_with_blocking(cell.pos, entity, matches!(cell.kind, SpawnKind::Wall));
            }
        }
    }
    commands.insert_resource(grid);
    commands.insert_resource(sim::CommitBuffer::sized(width, height));

    commands.insert_resource(Rules {
        wall: TileRule::parse(WALL_RULE_TEXT),
        triangle: DirectionTileRule::parse(TRIANGLE_RULE_TEXT).expect("triangle rule"),
        v: DirectionTileRule::parse(V_RULE_TEXT).expect("v rule"),
    });
    commands.insert_resource(RenderBuffers::new(width, height));
    let mut static_light = StaticLight::new();
    static_light.resize(width, height);
    commands.insert_resource(static_light);
    commands.insert_resource(DynamicLight::sized((width * height) as usize));

    // One text entity per cell; only diffs are rewritten each frame.
    for y in 0..height {
        for x in 0..width {
            let p = IVec2::new(x, y);
            let mut cell_text = String::with_capacity(4);
            cell_text.push(' ');
            commands.spawn((
                MapCell(p),
                Text2d::new(cell_text),
                TextFont {
                    font: font.clone(),
                    font_size: 20.0,
                    ..default()
                },
                TextColor(crate::lighting::palette_color(GRAY)),
                Transform::from_translation(grid_to_world(p, width, height)),
            ));
        }
    }

    let hud_text = String::with_capacity(192);
    commands.spawn((
        HudText,
        Text::new(hud_text),
        TextFont {
            font,
            font_size: 15.0,
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

fn initial_fov(cell_count: usize) -> FovResult {
    FovResult {
        revision: 0,
        obstacle_revision: u64::MAX,
        pos: IVec2::splat(i32::MIN),
        radius: -1,
        data: vec![0.0; cell_count],
    }
}
