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

use crate::ai::{enemy_tree, hunter_tree};
use crate::content::map::{parse_map, parse_next, parse_rooms, portal_links, SpawnKind};
use crate::content::tile_rules::{DirectionTileRule, TileRule};
use crate::lighting::{CYAN, GRAY, RED, WHITE, YELLOW};
use crate::model::*;
use crate::schedule::{GamePhase, StartupPhase};
use crate::simulation::{wall_bundle, Rules};

use super::intro::{intro_arrival, Intro};
use super::transition::{begin_transition, Arrival};

const MAP_TEXT: &str = include_str!("../../assets/maps/map1.txt");
const WALL_RULE_TEXT: &str = include_str!("../../assets/rules/wall_rule.txt");
const TRIANGLE_RULE_TEXT: &str = include_str!("../../assets/rules/direction_triangle_rule.txt");
const V_RULE_TEXT: &str = include_str!("../../assets/rules/direction_v_rule.txt");

// The generated table of bundled maps.
include!(concat!(env!("OUT_DIR"), "/bundled_maps.rs"));

/// The bundled map called `name` (with or without `assets/maps/` and
/// `.txt`).
pub fn bundled_map(name: &str) -> Option<&'static str> {
    bundled_index(name).map(|index| BUNDLED_MAPS[index].1)
}

/// Where the map called `name` is among [`BUNDLED_MAPS`].
pub fn bundled_index(name: &str) -> Option<usize> {
    let name = name.strip_prefix("assets/maps/").unwrap_or(name);
    let name = name.strip_suffix(".txt").unwrap_or(name);
    BUNDLED_MAPS.iter().position(|(map, _)| *map == name)
}

/// The map being played, and the map its exits lead to (its `@next`, a
/// bundled map).
#[derive(Resource, Clone, Copy, Debug)]
pub struct CurrentMap {
    pub text: &'static str,
    pub next: Option<&'static str>,
}

/// The map layout to construct. Defaults to the bundled `map1`; insert another
/// before startup to play a different one.
#[derive(Resource, Clone, Copy, Debug)]
pub struct MapText(pub &'static str);

impl Default for MapText {
    fn default() -> Self {
        Self(MAP_TEXT)
    }
}

/// Asks for another map to be played, at any time: the current one goes and
/// the player carries over to the new one's start (see `load_map`).
#[derive(Message, Clone, Copy, Debug)]
pub struct LoadMap(pub &'static str);

/// Constructs the selected map, its rules, and its initial entities, and
/// loads another on [`LoadMap`].
pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MapText>()
            .add_message::<LoadMap>()
            .add_systems(Startup, construct_map.in_set(StartupPhase::Content))
            // Before the frame's game systems, in a schedule of its own, so
            // the old level is wholly gone and the new one wholly there
            // when they run.
            .add_systems(PreUpdate, load_map.run_if(on_message::<LoadMap>))
            .add_systems(Update, take_exits.in_set(GamePhase::Finalize));
    }
}

/// Spawns the first map, coming together from the intro's with an [`Intro`].
fn construct_map(mut commands: Commands, map: Res<MapText>, intro: Option<Res<Intro>>) {
    let mut grid = MapGrid::new(1, 1);
    let arrival = intro.map(|intro| intro_arrival(intro.0, map.0));
    build_map(&mut commands, &mut grid, map.0, None, arrival);
    commands.insert_resource(grid);
    commands.insert_resource(Rules {
        wall: TileRule::parse(WALL_RULE_TEXT),
        triangle: DirectionTileRule::parse(TRIANGLE_RULE_TEXT).expect("triangle rule"),
        v: DirectionTileRule::parse(V_RULE_TEXT).expect("v rule"),
    });
}

/// Plays the map last asked for with [`LoadMap`]. The level goes: everything
/// placed on the map (anything with a `Pos`), what those carry, and its
/// rooms. The player stays, with what they carry and anything bound to
/// them, and moves to the new map's start, seeing it afresh: they remember
/// nothing of it yet. The new map comes together from the walls of the old
/// one, where they stood (see `transition`). The grid keeps counting its
/// revisions, so sight, paths and the rest are built again for the new map.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn load_map(
    mut commands: Commands,
    mut requests: MessageReader<LoadMap>,
    mut grid: ResMut<MapGrid>,
    mut crossings: ResMut<PortalCrossings>,
    placed: Query<(Entity, Option<&BoundTo>, Option<&Inventory>), With<Pos>>,
    rooms: Query<Entity, With<Room>>,
    players: Query<(Entity, &Pos), With<Player>>,
    walls: Query<(Entity, &Pos), (With<Wall>, Without<Portal>)>,
) {
    let Some(&LoadMap(text)) = requests.read().last() else {
        return;
    };
    let (player, was_at) = players.single().ok().unzip();
    // The old map's plain walls stay, for the new one to come together from.
    let old_walls: Vec<(Entity, IVec2)> = walls.iter().map(|(wall, pos)| (wall, pos.0)).collect();
    let mut kept = Vec::new();
    for (entity, bound, carried) in &placed {
        if Some(entity) == player || walls.contains(entity) {
            continue;
        }
        if let Some(bound) = bound.filter(|bound| Some(bound.parent) == player) {
            kept.push((entity, bound.offset));
            continue;
        }
        for &item in carried.iter().flat_map(|carried| &carried.0) {
            if let Ok(mut item) = commands.get_entity(item) {
                item.despawn();
            }
        }
        commands.entity(entity).despawn();
    }
    for room in &rooms {
        commands.entity(room).despawn();
    }
    crossings.0.clear();
    let arrival = Some(Arrival::Walls(old_walls));
    let start = build_map(&mut commands, &mut grid, text, player, arrival);
    if let (Some(player), Some(was_at)) = (player, was_at) {
        // A map without a start keeps the player where they were, or as
        // near as is free.
        let start = start.unwrap_or_else(|| {
            let at = free_cell_near(&grid, was_at.0);
            grid.set(at, player);
            at
        });
        let cell_count = (grid.width * grid.height) as usize;
        // Taking `Pos` off and putting it back places them anew, so they are
        // shown there at once rather than moving there.
        commands.entity(player).remove::<Pos>().insert((
            Pos(start),
            PrevPos(start),
            PendingPos::default(),
            MoveCommand::default(),
            Speed::default(),
            VisibilityMap {
                revision: u64::MAX,
                data: vec![Vis::empty(); cell_count],
            },
        ));
        for (entity, offset) in kept {
            let at = grid.safe_pos(start + offset);
            commands
                .entity(entity)
                .remove::<Pos>()
                .insert((Pos(at), PrevPos(at)));
        }
    }
}

/// Leaves for the next map as soon as the player steps onto an exit, even
/// mid-run: waiting for the step to finish would let a held key carry them
/// over it.
fn take_exits(
    current: Option<Res<CurrentMap>>,
    players: Query<(&Pos, &PrevPos), With<Player>>,
    exits: Query<&Pos, With<Exit>>,
    mut load: MessageWriter<LoadMap>,
) {
    let Some(next) = current.and_then(|current| current.next) else {
        return;
    };
    let Ok((pos, previous)) = players.single() else {
        return;
    };
    // Stepped onto it (a player placed on a new map has not stepped yet).
    if pos.0 != previous.0 && exits.iter().any(|exit| exit.0 == pos.0) {
        load.write(LoadMap(next));
    }
}

/// The free cell of `grid` nearest `p` (wrapped into it), or `p` itself if
/// none is free.
fn free_cell_near(grid: &MapGrid, p: IVec2) -> IVec2 {
    let p = grid.safe_pos(p);
    let reach = grid.width.max(grid.height);
    (0..=reach)
        .flat_map(|r| {
            (-r..=r)
                .flat_map(move |dy| (-r..=r).map(move |dx| IVec2::new(dx, dy)))
                .filter(move |d| d.abs().max_element() == r)
        })
        .map(|d| grid.safe_pos(p + d))
        .find(|&cell| grid.get(cell).is_none())
        .unwrap_or(p)
}

/// Spawns the map `text` into `grid`, reset to its size. With `player`, that
/// player takes the map's start instead of a new one being spawned; with an
/// `arrival`, the map's plain walls come together from it rather than
/// standing at once. Returns the start, if the map has one.
fn build_map(
    commands: &mut Commands,
    grid: &mut MapGrid,
    text: &'static str,
    player: Option<Entity>,
    arrival: Option<Arrival>,
) -> Option<IVec2> {
    let (width, height, cells) = parse_map(text);
    let links = portal_links(width, height, &cells).unwrap_or_else(|error| panic!("map: {error}"));
    let rooms = parse_rooms(text, &cells).unwrap_or_else(|error| panic!("map: {error}"));
    let next = parse_next(text, &cells)
        .unwrap_or_else(|error| panic!("map: {error}"))
        .map(|name| {
            bundled_map(&name).unwrap_or_else(|| panic!("map: no bundled map called `{name}`"))
        });
    commands.insert_resource(CurrentMap { text, next });
    let cell_count = (width * height) as usize;
    grid.reset(width, height);
    let mut portal_walls = Vec::with_capacity(links.len());
    let mut start = None;
    let mut map_walls = Vec::new();

    for &cell in &cells {
        if arrival.is_some() && cell.kind == SpawnKind::Wall {
            map_walls.push(cell.pos);
            continue;
        }
        let entity = match cell.kind {
            // A portal is a wall; only its open face differs.
            SpawnKind::Wall | SpawnKind::Portal(_) => commands.spawn(wall_bundle(cell.pos)).id(),
            SpawnKind::Player => {
                start = Some(cell.pos);
                player.unwrap_or_else(|| spawn_player(commands, cell.pos, cell_count))
            }
            SpawnKind::Enemy => commands
                .spawn((
                    enemy_body(cell.pos, cell_count),
                    EnemyMind::default(),
                    Behavior::for_tree(enemy_tree),
                ))
                .id(),
            SpawnKind::Hunter => commands
                .spawn((
                    enemy_body(cell.pos, cell_count),
                    (Hunter, Order::default()),
                    EnemyMind::default(),
                    Behavior::for_tree(hunter_tree),
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
            SpawnKind::Key => commands
                .spawn((Active, Item, Key, Pos(cell.pos), Glyph::new('♀', 0, YELLOW)))
                .id(),
            SpawnKind::Plate => commands
                .spawn((
                    Active,
                    PressurePlate::default(),
                    Pos(cell.pos),
                    Glyph::new(PressurePlate::GLYPH, 0, YELLOW),
                ))
                .id(),
            SpawnKind::Exit => commands
                .spawn((
                    Active,
                    Exit,
                    Pos(cell.pos),
                    Glyph::new(Exit::GLYPH, 0, CYAN),
                ))
                .id(),
            SpawnKind::Door => commands
                .spawn((
                    Active,
                    Collider,
                    Door::default(),
                    Pos(cell.pos),
                    Glyph::new(Door::CLOSED_GLYPH, 1, GRAY),
                ))
                .id(),
        };
        if commands.get_entity(entity).is_ok()
            && matches!(
                cell.kind,
                SpawnKind::Wall
                    | SpawnKind::Portal(_)
                    | SpawnKind::Player
                    | SpawnKind::Enemy
                    | SpawnKind::Hunter
                    | SpawnKind::Door
            )
        {
            // Colliders own their cells (mirrors the first UpdatePosition pass).
            grid.set_with_blocking(
                cell.pos,
                entity,
                matches!(
                    cell.kind,
                    SpawnKind::Wall | SpawnKind::Portal(_) | SpawnKind::Door
                ),
            );
        }
        if matches!(cell.kind, SpawnKind::Portal(_)) {
            portal_walls.push((cell.pos, entity));
        }
    }
    let wall_at = |pos: IVec2| {
        portal_walls
            .iter()
            .find(|(wall, _)| *wall == pos)
            .map(|(_, entity)| *entity)
            .expect("every portal link names a portal wall")
    };
    for link in links {
        commands.entity(wall_at(link.pos)).insert(Portal {
            side: link.side,
            exit: wall_at(link.exit),
        });
    }

    for room in rooms {
        commands.spawn(Room::new(room.origin, room.size, room.layouts).with_planner(room.planner));
    }
    if let Some(arrival) = arrival {
        begin_transition(commands, grid, arrival, &cells, map_walls);
    }
    start
}

/// A new player at `at`, with the direction marker bound to them.
fn spawn_player(commands: &mut Commands, at: IVec2, cell_count: usize) -> Entity {
    let player = commands
        .spawn((
            Active,
            Collider,
            Pos(at),
            (
                Speed::default(),
                MoveCommand::default(),
                MoveIntent::default(),
                PendingPos::default(),
                PrevPos(at),
            ),
            Glyph::new('@', 1, WHITE),
            Player(0),
            Inventory::default(),
            Tokens::new(1),
            Friction(1),
            Facing::default(),
            DirectionBasedOnSpeed,
            VisualSensor { radius: 16 },
            PlayerView::with_radius(16),
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
        Pos(at + IVec2::X),
        PrevPos(at + IVec2::X),
        Facing::default(),
        DirectionTile {
            rule: "direction_triangle_rule".to_string(),
        },
        Glyph::new('i', 0, WHITE),
    ));
    player
}

/// Steps of detour an enemy takes rather than queue behind another actor.
const ENEMY_CROWD_COST: u32 = 4;

/// What every kind of enemy is made of: a walking, carrying, token-taking
/// body. Its role adds a mind and a tree.
fn enemy_body(pos: IVec2, cell_count: usize) -> impl Bundle {
    (
        (Active, Collider, Enemy, Pos(pos)),
        (
            Speed::default(),
            MoveCommand::default(),
            PendingPos::default(),
            PrevPos(pos),
        ),
        Glyph::new('☺', 1, RED),
        Inventory::default(),
        (
            Destination::default(),
            TraversalPrefs {
                door_cost: None,
                crowd_cost: Some(ENEMY_CROWD_COST),
                // `PortalPolicy` decides, once the enemy is in play.
                portals: false,
            },
            PathFollow::with_capacity(cell_count),
        ),
        (
            VisualSensor {
                radius: ENEMY_SIGHT_RADIUS,
            },
            initial_fov(cell_count),
        ),
        Tokens::new(1),
        Friction(1),
        Facing::default(),
        DirectionBasedOnSpeed,
        DirectionTile {
            rule: "direction_v_rule".to_string(),
        },
    )
}

fn initial_fov(cell_count: usize) -> FovResult {
    FovResult::empty(cell_count)
}
