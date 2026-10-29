//! Bundled map selection and construction of the initial ECS world.
//!
//! Deliberate deviations from the Lite sources:
//! - entities spawn directly with `Pos` (the Lite `NewPosition` and
//!   `SpawnRequest` bootstrap converges to the same state on the first pass);
//! - the player has no `DirectionTile` (the Lite player carries a null rule
//!   name, which would throw in its rule dictionary; the classic `@` stays);
//! - one Bevy `World` plus resources replaces `EcsUniverse` worlds-per-type.

use bevy::prelude::*;

use super::ActorKinds;
use crate::content::map::{parse_map, parse_next, parse_rooms, portal_links, SpawnKind};
use crate::content::map::{PortalLink, RoomSpec, SpawnCell};
use crate::content::statures::parse_statures;
use crate::content::tile_rules::{DirectionTileRule, TileRule};
use crate::foundation::grid::nearest_cell;
use crate::lighting::{CYAN, GRAY, RED, WHITE, YELLOW};
use crate::model::*;
use crate::schedule::{GamePhase, StartupPhase};
use crate::simulation::room_plans::RoomPlanners;
use crate::simulation::{rail_bundle, wall_bundle, Rules};

const MAP_TEXT: &str = include_str!("../../assets/maps/map1.txt");
const WALL_RULE_TEXT: &str = include_str!("../../assets/rules/wall_rule.txt");
const TRIANGLE_RULE_TEXT: &str = include_str!("../../assets/rules/direction_triangle_rule.txt");
const V_RULE_TEXT: &str = include_str!("../../assets/rules/direction_v_rule.txt");
const STATURES_TEXT: &str = include_str!("../../assets/rules/statures.txt");

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
            .init_resource::<ActorKinds>()
            .add_message::<LoadMap>()
            .add_message::<MapLoaded>()
            .add_systems(Startup, construct_map.in_set(StartupPhase::Content))
            // Before the frame's game systems, in a schedule of its own, so
            // the old level is wholly gone and the new one wholly there
            // when they run.
            .add_systems(PreUpdate, load_map.run_if(on_message::<LoadMap>))
            .add_systems(Update, take_exits.in_set(GamePhase::Finalize));
    }
}

/// Spawns the first map.
fn construct_map(
    mut commands: Commands,
    map: Res<MapText>,
    planners: Option<Res<RoomPlanners>>,
    kinds: Res<ActorKinds>,
    mut loaded: MessageWriter<MapLoaded>,
) {
    let mut grid = MapGrid::new(1, 1);
    build_map(
        &mut commands,
        &mut grid,
        map.0,
        None,
        planners.as_deref(),
        &kinds,
    );
    commands.insert_resource(grid);
    commands.insert_resource(Rules {
        wall: TileRule::parse(WALL_RULE_TEXT),
        triangle: DirectionTileRule::parse(TRIANGLE_RULE_TEXT).expect("triangle rule"),
        v: DirectionTileRule::parse(V_RULE_TEXT).expect("v rule"),
    });
    commands.insert_resource(GlyphStatures(
        parse_statures(STATURES_TEXT)
            .expect("statures")
            .into_iter()
            .collect(),
    ));
    loaded.write(MapLoaded::default());
}

/// Plays the map last asked for with [`LoadMap`]. The level goes: everything
/// placed on the map (anything with a `Pos`), what those carry, and its
/// rooms. The player stays, with what they carry and anything bound to
/// them, and moves to the new map's start, seeing it afresh: they remember
/// nothing of it yet. [`MapLoaded`] tells what the old map's plain walls
/// were, for anything that reacts to a new map. The grid keeps counting its
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
    walls: Query<(&Pos, &Glyph), (With<Wall>, Without<Portal>)>,
    planners: Option<Res<RoomPlanners>>,
    kinds: Res<ActorKinds>,
    mut loaded: MessageWriter<MapLoaded>,
) {
    let Some(&LoadMap(text)) = requests.read().last() else {
        return;
    };
    let (player, was_at) = players.single().ok().unzip();
    let previous_walls = walls.iter().map(|(pos, glyph)| (pos.0, *glyph)).collect();
    let mut kept = Vec::new();
    for (entity, bound, carried) in &placed {
        if Some(entity) == player {
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
    let start = build_map(
        &mut commands,
        &mut grid,
        text,
        player,
        planners.as_deref(),
        &kinds,
    );
    loaded.write(MapLoaded { previous_walls });
    if let (Some(player), Some(was_at)) = (player, was_at) {
        // A map without a start keeps the player where they were, or as
        // near as is free.
        let start = start.unwrap_or_else(|| {
            let size = IVec2::new(grid.width, grid.height);
            let at = nearest_cell(was_at.0, size, |cell| grid.get(cell).is_none())
                .unwrap_or_else(|| grid.safe_pos(was_at.0));
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

/// A defeated player plays the map again from its start, at full health,
/// with nothing picked.
pub(super) fn restart_on_defeat(
    mut commands: Commands,
    mut died: MessageReader<Died>,
    current: Option<Res<CurrentMap>>,
    mut players: Query<(Entity, &mut Health), With<Player>>,
    mut load: MessageWriter<LoadMap>,
) {
    let mut restart = false;
    for &Died(entity) in died.read() {
        if let Ok((player, mut health)) = players.get_mut(entity) {
            health.hp = health.max;
            commands.entity(player).remove::<Selected>();
            restart = true;
        }
    }
    if let (true, Some(current)) = (restart, current) {
        load.write(LoadMap(current.text));
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

/// A map file, read and checked: a map that cannot be played stops the game
/// with what is wrong with it.
struct MapPlan {
    width: i32,
    height: i32,
    cells: Vec<SpawnCell>,
    links: Vec<PortalLink>,
    rooms: Vec<RoomSpec>,
    next: Option<&'static str>,
}

impl MapPlan {
    fn read(text: &str, planners: Option<&RoomPlanners>) -> Self {
        let fail = |error: String| -> ! { panic!("map: {error}") };
        let (width, height, cells) = parse_map(text);
        let links = portal_links(width, height, &cells).unwrap_or_else(|e| fail(e));
        let rooms = parse_rooms(text, &cells).unwrap_or_else(|e| fail(e));
        let next = parse_next(text, &cells)
            .unwrap_or_else(|e| fail(e))
            .map(|name| {
                bundled_map(&name)
                    .unwrap_or_else(|| fail(format!("no bundled map called `{name}`")))
            });
        if let Some(planners) = planners {
            for room in &rooms {
                if let Some(name) = room
                    .planner
                    .as_deref()
                    .filter(|name| planners.get(name).is_none())
                {
                    let known: Vec<_> = planners.names().collect();
                    fail(format!(
                        "room at {} names no known planner `{name}` (known: {})",
                        room.origin,
                        known.join(", ")
                    ));
                }
            }
        }
        Self {
            width,
            height,
            cells,
            links,
            rooms,
            next,
        }
    }
}

/// Spawns the map `text` into `grid`, reset to its size. With `player`, that
/// player takes the map's start instead of a new one being spawned. Returns
/// the start, if the map has one.
fn build_map(
    commands: &mut Commands,
    grid: &mut MapGrid,
    text: &'static str,
    player: Option<Entity>,
    planners: Option<&RoomPlanners>,
    kinds: &ActorKinds,
) -> Option<IVec2> {
    let plan = MapPlan::read(text, planners);
    commands.insert_resource(CurrentMap {
        text,
        next: plan.next,
    });
    grid.reset(plan.width, plan.height);
    let cell_count = (plan.width * plan.height) as usize;
    let mut start = None;
    let mut portal_walls = Vec::with_capacity(plan.links.len());
    for &cell in &plan.cells {
        let entity = match cell.kind {
            SpawnKind::Player => {
                start = Some(cell.pos);
                player.unwrap_or_else(|| spawn_player(commands, cell.pos, cell_count, kinds))
            }
            kind => spawn_cell(commands, kind, cell.pos, cell_count, kinds),
        };
        // Colliders own their cells (mirrors the first UpdatePosition pass).
        if let Some(blocks_vision) = occupies(cell.kind) {
            grid.set_with_blocking(cell.pos, entity, blocks_vision);
        }
        if matches!(cell.kind, SpawnKind::Portal(_)) {
            portal_walls.push((cell.pos, entity));
        }
    }
    link_portals(commands, &portal_walls, plan.links);
    for room in plan.rooms {
        commands.spawn(Room::new(room.origin, room.size, room.layouts).with_planner(room.planner));
    }
    start
}

/// Whether a cell of this kind holds its cell on the grid, and if so,
/// whether it blocks sight there.
fn occupies(kind: SpawnKind) -> Option<bool> {
    match kind {
        SpawnKind::Wall | SpawnKind::Portal(_) | SpawnKind::Door => Some(true),
        SpawnKind::Player | SpawnKind::Actor(_) => Some(false),
        _ => None,
    }
}

/// Gives each portal wall its face onto the wall it leads to.
fn link_portals(commands: &mut Commands, walls: &[(IVec2, Entity)], links: Vec<PortalLink>) {
    let wall_at = |pos: IVec2| {
        walls
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
}

/// Spawns what a map cell of `kind` (anything but the player) holds at `at`.
fn spawn_cell(
    commands: &mut Commands,
    kind: SpawnKind,
    at: IVec2,
    cell_count: usize,
    kinds: &ActorKinds,
) -> Entity {
    let marker = |glyph: char, color: Rgb| (Active, Pos(at), Glyph::new(glyph, 0, color));
    let light = |kind: LightKind, radius: i32, value: u8| {
        (
            LightSource {
                radius,
                kind,
                value,
            },
            initial_fov(cell_count),
        )
    };
    match kind {
        // A portal is a wall; only its open face differs.
        SpawnKind::Wall | SpawnKind::Portal(_) => commands.spawn(wall_bundle(at)),
        SpawnKind::Player => unreachable!("the player is placed by build_map"),
        SpawnKind::Actor(glyph) => {
            let mut actor = commands.spawn(actor_body(at, cell_count));
            kinds.get(glyph).equip(&mut actor);
            actor
        }
        SpawnKind::Electricity => commands.spawn((
            ElectroField,
            marker('Ω', WHITE),
            light(LightKind::Electricity, 3, 32),
        )),
        SpawnKind::Light => {
            commands.spawn((Lamp, marker('i', WHITE), light(LightKind::Fire, 16, 196)))
        }
        SpawnKind::Acid => {
            commands.spawn((AcidPool, marker('▒', WHITE), light(LightKind::Acid, 4, 32)))
        }
        SpawnKind::Key => commands.spawn((Item, Key, marker('♀', YELLOW))),
        SpawnKind::Plate => commands.spawn((
            PressurePlate::default(),
            marker(PressurePlate::GLYPH, YELLOW),
        )),
        SpawnKind::Exit => commands.spawn((Exit, marker(Exit::GLYPH, CYAN))),
        SpawnKind::Rail => commands.spawn(rail_bundle(at)),
        SpawnKind::Door => commands.spawn((
            Active,
            Collider,
            Door::default(),
            Pos(at),
            Glyph::new(Door::CLOSED_GLYPH, 1, GRAY),
        )),
    }
    .id()
}

/// A new player at `at`, with the direction marker bound to them.
fn spawn_player(
    commands: &mut Commands,
    at: IVec2,
    cell_count: usize,
    kinds: &ActorKinds,
) -> Entity {
    let mut player = commands.spawn((
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
        (Player(0), Selector, KeepOnDeath),
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
    ));
    kinds.get('p').equip(&mut player);
    let player = player.id();
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

/// What every actor from the roster is made of: a walking, carrying,
/// token-taking body that can be picked and knocked about. Its kind adds
/// the rest (`ActorKind::equip`).
fn actor_body(pos: IVec2, cell_count: usize) -> impl Bundle {
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
        (Selectable, Movable, Exposed::default()),
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

#[cfg(test)]
mod tests {
    use super::*;

    const ROOM: &str = "\
XXXXX
X.p.X
XXXXX
@room 1 1 PLANNER
.
";

    #[test]
    fn a_room_may_name_a_known_planner() {
        let text = ROOM.replace("PLANNER", RoomPlanners::RISE_SINK);
        let plan = MapPlan::read(&text, Some(&RoomPlanners::default()));
        assert_eq!(
            plan.rooms[0].planner.as_deref(),
            Some(RoomPlanners::RISE_SINK)
        );
    }

    #[test]
    #[should_panic(expected = "no known planner `wobble`")]
    fn a_room_naming_an_unknown_planner_stops_the_game() {
        let text = ROOM.replace("PLANNER", "wobble");
        MapPlan::read(&text, Some(&RoomPlanners::default()));
    }
}
