//! Bundled map selection and construction of the initial ECS world.
//!
//! Deliberate deviations from the Lite sources:
//! - entities spawn directly with `Pos` (the Lite `NewPosition` and
//!   `SpawnRequest` bootstrap converges to the same state on the first pass);
//! - the player has no `DirectionTile` (the Lite player carries a null rule
//!   name, which would throw in its rule dictionary; the classic `@` stays);
//! - one Bevy `World` plus resources replaces `EcsUniverse` worlds-per-type.

use bevy::prelude::*;

use flatbt_bevy::prelude::Behavior;

use crate::ai::enemy_tree;
use crate::content::map::{parse_map, SpawnKind};
use crate::content::tile_rules::{DirectionTileRule, TileRule};
use crate::lighting::{GRAY, RED, WHITE};
use crate::model::*;
use crate::schedule::StartupPhase;
use crate::simulation::Rules;

const MAP_TEXT: &str = include_str!("../../assets/maps/map1.txt");
const WALL_RULE_TEXT: &str = include_str!("../../assets/rules/wall_rule.txt");
const TRIANGLE_RULE_TEXT: &str = include_str!("../../assets/rules/direction_triangle_rule.txt");
const V_RULE_TEXT: &str = include_str!("../../assets/rules/direction_v_rule.txt");

/// The map layout to construct. Defaults to the bundled `map1`; insert another
/// before startup to play a different one.
#[derive(Resource, Clone, Copy, Debug)]
pub struct MapText(pub &'static str);

impl Default for MapText {
    fn default() -> Self {
        Self(MAP_TEXT)
    }
}

/// Constructs the selected map, its rules, and its initial entities.
pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MapText>()
            .add_systems(Startup, construct_map.in_set(StartupPhase::Content));
    }
}

fn construct_map(mut commands: Commands, map: Res<MapText>) {
    let (width, height, cells) = parse_map(map.0);
    let cell_count = (width * height) as usize;
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
                    (EnemyMind::default(), Behavior::for_tree(enemy_tree)),
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
        if commands.get_entity(entity).is_ok()
            && matches!(
                cell.kind,
                SpawnKind::Wall | SpawnKind::Player | SpawnKind::Enemy
            )
        {
            // Colliders own their cells (mirrors the first UpdatePosition pass).
            grid.set_with_blocking(cell.pos, entity, matches!(cell.kind, SpawnKind::Wall));
        }
    }

    commands.insert_resource(grid);
    commands.insert_resource(Rules {
        wall: TileRule::parse(WALL_RULE_TEXT),
        triangle: DirectionTileRule::parse(TRIANGLE_RULE_TEXT).expect("triangle rule"),
        v: DirectionTileRule::parse(V_RULE_TEXT).expect("v rule"),
    });
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
