//! Simulation systems. Mirrors the Lite `GameMainContainer` control + tick +
//! sim groups, in the same order:
//!
//! control: tokens → player input → enemy AI → move commands
//! tick: direction-from-speed → movement → friction
//! sim: resolve (collect → unmap → commit) → bindings → destroy → tiles
//!
//! Deliberate deviations (all documented where they occur):
//! - systems touching `DestroyRequested` skip it: nothing in the Lite
//!   container sets the tag, so the guarded paths are unobservable;
//! - `TileSystem` computes masks two-phased (the original mutates neighbour
//!   masks while iterating, which drops links asymmetrically);
//! - pending keyboard input is dropped outside `TickUpdate`, same as the
//!   original's read-before-phase-check, but read exactly once per frame.

// Bevy system params are inherently long; readability beats the lint here.
#![allow(clippy::type_complexity)]

use bevy::prelude::*;
use rand::Rng;

use crate::components::*;
use crate::tiles::{DirectionTileRule, TileRule};

/// Loaded rules shared by tile systems.
#[derive(Resource, Debug)]
pub struct Rules {
    pub wall: TileRule,
    pub triangle: DirectionTileRule,
    pub v: DirectionTileRule,
}

/// Committed winner of the collect phase (mirrors a reserved map cell).
/// `from` is empty for entities standing nowhere yet (first placement).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MoveCommit {
    entity: Entity,
    from: Option<IVec2>,
    to: IVec2,
}

#[derive(Resource, Debug, Default)]
pub struct CommitBuffer(Vec<MoveCommit>);

/// Advance the turn counter on quiet frames (mirrors `TurnManager.Run`).
pub fn turn_tick(mut turn: ResMut<TurnState>) {
    if !turn.simulation {
        turn.tick += 1;
    }
}

/// Mirrors `CommandTokenDistributionSystem`: drop spent tokens, then assign
/// the recharge value to every waiter when nobody holds a token or the
/// 1s timer fires.
pub fn recharge_tokens(
    time: Res<Time>,
    mut timer: ResMut<TokenTimer>,
    mut holders: Query<&mut Tokens, Or<(With<Player>, With<Enemy>)>>,
) {
    timer.0.tick(time.delta());
    let any_left = holders.iter().any(|t| t.count > 0);
    if timer.0.just_finished() || !any_left {
        for mut t in holders.iter_mut() {
            t.count = t.recharge;
        }
        timer.0.reset();
    }
}

/// Mirrors `KeyboardMoveSystem`: arrows/WASD become relative move commands.
/// Only in `TickUpdate`; the key is read once per frame.
pub fn player_input(
    keys: Res<ButtonInput<KeyCode>>,
    turn: Res<TurnState>,
    mut players: Query<(Entity, &Player), (With<Active>, Without<DestroyRequested>)>,
    mut commands: Commands,
) {
    if turn.simulation {
        return;
    }
    let dir = if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
        Some(IVec2::new(0, -1))
    } else if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) {
        Some(IVec2::new(0, 1))
    } else if keys.any_just_pressed([KeyCode::ArrowLeft, KeyCode::KeyA]) {
        Some(IVec2::new(-1, 0))
    } else if keys.any_just_pressed([KeyCode::ArrowRight, KeyCode::KeyD]) {
        Some(IVec2::new(1, 0))
    } else {
        None
    };
    if let Some(d) = dir {
        for (e, _) in players.iter_mut() {
            commands.entity(e).insert(MoveCommand {
                target: d,
                relative: true,
            });
        }
    }
}

/// Mirrors `RandomMoveSystem`: each enemy picks stay/4-neighbour from the
/// shared RNG (the original shares one `Random(42)` across enemies).
pub fn enemy_ai(
    turn: Res<TurnState>,
    mut rng: ResMut<SharedRng>,
    enemies: Query<Entity, (With<Enemy>, With<Active>, Without<DestroyRequested>)>,
    mut commands: Commands,
) {
    if turn.simulation {
        return;
    }
    const MOVES: [IVec2; 5] = [
        IVec2::ZERO,
        IVec2::new(1, 0),
        IVec2::new(-1, 0),
        IVec2::new(0, 1),
        IVec2::new(0, -1),
    ];
    for e in enemies.iter() {
        let m = MOVES[rng.0.gen_range(0..MOVES.len())];
        commands.entity(e).insert(MoveCommand {
            target: m,
            relative: true,
        });
    }
}

/// Mirrors `MoveCommandSystem`: relative commands become speed, spending one
/// token. Requires a held token; tokenless commands wait.
pub fn move_commands(
    mut commands: Commands,
    mut movers: Query<(Entity, &MoveCommand, &mut Speed, &mut Tokens), Without<DestroyRequested>>,
) {
    for (e, cmd, mut speed, mut tokens) in movers.iter_mut() {
        if tokens.count <= 0 {
            continue;
        }
        if cmd.relative {
            speed.0 = cmd.target;
        }
        commands.entity(e).remove::<MoveCommand>();
        tokens.count -= 1;
    }
}

/// Mirrors `UpdateDirectionBasedOnSpeedSystem`.
pub fn update_direction(
    mut facings: Query<(&Speed, &mut Facing), (With<DirectionBasedOnSpeed>, With<Active>)>,
) {
    for (speed, mut facing) in facings.iter_mut() {
        if speed.0 != IVec2::ZERO {
            facing.0 = speed.0;
        }
    }
}

/// Mirrors `MovementSystem`: nonzero speed becomes a move intent.
/// Entities mid-destruction are skipped (nothing in the Lite container
/// destroys moving entities, so this path is unobservable there).
pub fn movement(
    mut commands: Commands,
    grid: Res<MapGrid>,
    movers: Query<
        (Entity, &Pos, &Speed),
        (With<Active>, Without<DestroyRequested>, Without<PendingPos>),
    >,
) {
    for (e, pos, speed) in movers.iter() {
        if speed.0 != IVec2::ZERO {
            commands
                .entity(e)
                .insert(PendingPos::MoveTo(grid.safe_pos(pos.0 + speed.0)));
        }
    }
}

/// Bound decorations follow the parent's committed position. Unlike the
/// reference's pre-resolve binding pass, this runs after collision resolution
/// so the marker stays ahead of the player in the rendered frame.
/// A missing parent (or one without position/facing) leaves the child
/// untouched instead of crashing on a stale link.
pub fn relative_position(
    mut commands: Commands,
    children: Query<
        (Entity, &BoundTo, Option<&Pos>, Option<&Facing>),
        (Without<Collider>, Without<DestroyRequested>),
    >,
    parents: Query<(&Pos, &Facing)>,
) {
    for (e, bound, pos, facing) in children.iter() {
        let Ok((parent_pos, parent_facing)) = parents.get(bound.parent) else {
            continue;
        };
        let new_pos = parent_pos.0 + rotate(bound.offset, parent_facing.0);
        if pos.map(|p| p.0) != Some(new_pos) {
            if let Some(pos) = pos {
                commands.entity(e).insert(PrevPos(pos.0));
            }
            commands.entity(e).insert(Pos(new_pos));
        }
        let new_dir = rotate(bound.offset_dir, parent_facing.0);
        if facing.map(|f| f.0) != Some(new_dir) {
            commands.entity(e).insert(Facing(new_dir));
        }
    }
}

/// Resolve phase A: collect move claims in stable entity order against a
/// snapshot of the grid (mirrors `PlaceToNewPos`, where old cells stay
/// occupied for the whole pass).
///
/// Conservative contract, like the original: swaps blocked, entering a
/// vacated cell blocked this pass, one winner per cell, losers keep their
/// old cell and emit a [`CollisionEvent`] (recorded only — the Lite
/// container has no collision consumer).
pub fn resolve_collect(
    mut commands: Commands,
    grid: Res<MapGrid>,
    mut collisions: ResMut<CollisionBuffer>,
    mut commits: ResMut<CommitBuffer>,
    claimants: Query<
        (Entity, Option<&Pos>, &PendingPos),
        (With<Collider>, Without<DestroyRequested>),
    >,
) {
    collisions.0.clear();
    commits.0.clear();
    let mut order: Vec<(Entity, Option<IVec2>, IVec2)> = Vec::new();
    for (e, pos, pending) in claimants.iter() {
        if let PendingPos::MoveTo(dest) = pending {
            order.push((e, pos.map(|p| p.0), grid.safe_pos(*dest)));
        }
    }
    // Stable creation-order processing (stands in for EcsLite filter order).
    order.sort_by_key(|(e, _, _)| (e.index(), e.generation()));
    let mut reserved = std::collections::HashSet::new();
    for (e, from, to) in order {
        match grid.get(to) {
            Some(other) if other != e => {
                collisions.0.push(CollisionEvent {
                    source: e,
                    target: other,
                });
                commands.entity(e).remove::<PendingPos>();
            }
            _ if reserved.contains(&to) => {
                // Lost the race for a free cell; the winner already owns it.
                let winner = commits
                    .0
                    .iter()
                    .find(|c| c.to == to)
                    .map(|c| c.entity)
                    .unwrap_or(e);
                if winner != e {
                    collisions.0.push(CollisionEvent {
                        source: e,
                        target: winner,
                    });
                }
                commands.entity(e).remove::<PendingPos>();
            }
            _ => {
                reserved.insert(to);
                // Commit exactly the wrapped destination whose occupancy
                // was checked, including first placements and direct intents.
                commands.entity(e).insert(PendingPos::MoveTo(to));
                commits.0.push(MoveCommit {
                    entity: e,
                    from,
                    to,
                });
            }
        }
    }
}

/// Resolve phase B: vacate old cells of movers and removals
/// (mirrors `RemoveFromMap`; losers already lost `PendingPos` above,
/// and the position-less have nothing to vacate).
pub fn resolve_unmap(
    mut grid: ResMut<MapGrid>,
    movers: Query<(Entity, Option<&Pos>, &PendingPos), With<Collider>>,
) {
    for (e, pos, _) in movers.iter() {
        if let Some(p) = pos {
            if grid.get(p.0) == Some(e) {
                grid.clear(p.0);
            }
        }
    }
}

/// Resolve phase C: record previous positions and apply intents
/// (mirrors `UpdatePrevPos` + `MoveToNewPos`; position is optional, so
/// first-time placements land here too).
pub fn resolve_commit(
    mut commands: Commands,
    mut grid: ResMut<MapGrid>,
    movers: Query<(Entity, Option<&Pos>, &PendingPos, Option<&Collider>)>,
) {
    for (e, pos, pending, collider) in movers.iter() {
        if let Some(p) = pos {
            commands.entity(e).insert(PrevPos(p.0));
        }
        match pending {
            PendingPos::MoveTo(dest) => {
                commands.entity(e).insert(Pos(*dest));
                if collider.is_some() {
                    grid.set(*dest, e);
                }
            }
            PendingPos::RemoveFromMap => {
                commands.entity(e).remove::<Pos>();
            }
        }
        commands.entity(e).remove::<PendingPos>();
    }
}

/// One axis of `FrictionSystem` decay.
fn decay_axis(v: i32, friction: i32) -> i32 {
    if v.abs() > friction {
        v - v.signum() * friction
    } else {
        0
    }
}

/// Mirrors `FrictionSystem`: integer decay of speed toward zero.
pub fn friction(mut movers: Query<(&mut Speed, &Friction), With<Active>>) {
    for (mut speed, friction) in movers.iter_mut() {
        if speed.0 == IVec2::ZERO || friction.0 == 0 {
            continue;
        }
        let f = friction.0;
        speed.0.x = decay_axis(speed.0.x, f);
        speed.0.y = decay_axis(speed.0.y, f);
    }
}

/// Mirrors `DestroyEntitySystem` stage 1: positioned victims are scheduled
/// for map removal (overwriting any move intent, like `Ensure` does).
pub fn destroy_unmap(
    mut commands: Commands,
    victims: Query<Entity, (With<Pos>, With<DestroyRequested>, Without<PendingPos>)>,
) {
    for e in victims.iter() {
        commands.entity(e).insert(PendingPos::RemoveFromMap);
    }
}

/// Mirrors `DestroyEntitySystem` stage 2: entities off the map despawn.
pub fn destroy_despawn(
    mut commands: Commands,
    gone: Query<Entity, (With<DestroyRequested>, Without<Pos>, Without<PendingPos>)>,
) {
    for e in gone.iter() {
        commands.entity(e).despawn();
    }
}

/// Wall autotiling (mirrors `TileSystem`, but two-phased: the original
/// mutates neighbour masks while iterating and deletes `TileComponent`
/// mid-pass, which asymmetrically drops links).
pub fn tile_system(
    mut commands: Commands,
    grid: Res<MapGrid>,
    rules: Res<Rules>,
    tiles: Query<(Entity, &Pos, &Tile)>,
) {
    // Phase 1: snapshot the tiled set, compute every mask immutably.
    let present: std::collections::HashSet<IVec2> = tiles.iter().map(|(_, p, _)| p.0).collect();
    let mut masks: Vec<(Entity, u8)> = Vec::new();
    for (e, pos, tile) in tiles.iter() {
        if tile.rule != "wall_rule" {
            continue;
        }
        let at = |d: IVec2| present.contains(&grid.safe_pos(pos.0 + d));
        let mut mask = 0u8;
        if at(IVec2::new(1, 0)) {
            mask |= 1 << 0;
        }
        if at(IVec2::new(0, 1)) {
            mask |= 1 << 1;
        }
        if at(IVec2::new(-1, 0)) {
            mask |= 1 << 2;
        }
        if at(IVec2::new(0, -1)) {
            mask |= 1 << 3;
        }
        masks.push((e, mask));
    }
    // Phase 2: apply symbols, retire the tile state.
    for (e, mask) in masks {
        commands.entity(e).insert(Glyph {
            ch: rules.wall.symbol(mask),
            depth: 1,
            color: crate::lighting::GRAY,
        });
        commands.entity(e).remove::<Tile>();
    }
}

/// Mirrors `DirectionTileSystem`: glyph follows facing through the rule.
pub fn direction_tiles(
    rules: Res<Rules>,
    mut glyphs: Query<(&Facing, &DirectionTile, &mut Glyph)>,
) {
    for (facing, tile, mut glyph) in glyphs.iter_mut() {
        let rule = match tile.rule.as_str() {
            "direction_triangle_rule" => &rules.triangle,
            "direction_v_rule" => &rules.v,
            _ => continue,
        };
        glyph.ch = rule.symbol(crate::components::Direction::of(facing.0));
    }
}

/// Recompute the turn phase from remaining work (mirrors the
/// `SystemHasMoreWorkTag` aggregate behind `TurnManager.CurrentPhase`).
/// Only commands on token-holding entities count: the original
/// `MoveCommandSystem` filter requires the token, so tokenless intents
/// wait without holding the simulation open.
pub fn turn_update(
    mut turn: ResMut<TurnState>,
    commands_q: Query<&Tokens, (With<MoveCommand>, With<Speed>, Without<DestroyRequested>)>,
    speeds: Query<&Speed, (With<Active>, Without<DestroyRequested>)>,
    pendings: Query<(), With<PendingPos>>,
    doomed: Query<(), With<DestroyRequested>>,
) {
    turn.simulation = commands_q.iter().any(|t| t.count > 0)
        || speeds.iter().any(|s| s.0 != IVec2::ZERO)
        || !pendings.is_empty()
        || !doomed.is_empty();
}

/// Map/ECS consistency check (mirrors `VerifyMapSystem`; debug builds only).
/// Every collider with a position must own its cell, and every occupied
/// cell must resolve to a live entity standing on it.
pub fn verify_map(
    grid: Res<MapGrid>,
    bodies: Query<(Entity, &Pos), (With<Collider>, Without<DestroyRequested>)>,
) {
    if !cfg!(debug_assertions) {
        return;
    }
    for (e, pos) in bodies.iter() {
        debug_assert_eq!(
            grid.get(pos.0),
            Some(e),
            "collider {e:?} at {:?} does not own its cell",
            pos.0
        );
    }
}

/// Test helper: build a headless app running the sim chain (no window).
#[cfg(test)]
pub mod test_app {
    use super::*;
    use crate::vision;
    use rand::SeedableRng;

    pub fn headless() -> bevy::prelude::App {
        let mut app = bevy::prelude::App::new();
        app.add_plugins(bevy::prelude::MinimalPlugins)
            .insert_resource(MapGrid::new(8, 8))
            .init_resource::<vision::FovShared>()
            .init_resource::<TurnState>()
            .init_resource::<TokenTimer>()
            .init_resource::<CollisionBuffer>()
            .init_resource::<CommitBuffer>()
            .insert_resource(SharedRng(rand::rngs::StdRng::seed_from_u64(42)))
            .add_systems(
                bevy::prelude::Update,
                (
                    turn_tick,
                    recharge_tokens,
                    enemy_ai,
                    move_commands,
                    update_direction,
                    movement,
                    resolve_collect,
                    resolve_unmap,
                    resolve_commit,
                    relative_position,
                    friction,
                    destroy_unmap,
                    destroy_despawn,
                    turn_update,
                    vision::ensure_fov_requests,
                    vision::compute_fov,
                    vision::player_visibility,
                )
                    .chain(),
            );
        app
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_intents_wrap_before_commit_and_first_placement() {
        let mut app = test_app::headless();
        let mover = app
            .world_mut()
            .spawn((
                Active,
                Collider,
                Pos(IVec2::new(1, 1)),
                PendingPos::MoveTo(IVec2::new(-1, 9)),
            ))
            .id();
        app.world_mut()
            .resource_mut::<MapGrid>()
            .set(IVec2::new(1, 1), mover);
        let newcomer = app
            .world_mut()
            .spawn((Active, Collider, PendingPos::MoveTo(IVec2::new(8, 0))))
            .id();
        app.update();
        let grid = app.world().resource::<MapGrid>();
        assert_eq!(app.world().get::<Pos>(mover).unwrap().0, IVec2::new(7, 1));
        assert_eq!(grid.get(IVec2::new(7, 1)), Some(mover));
        assert_eq!(grid.get(IVec2::new(1, 1)), None);
        assert_eq!(app.world().get::<Pos>(newcomer).unwrap().0, IVec2::ZERO);
        assert_eq!(grid.get(IVec2::ZERO), Some(newcomer));
    }

    #[test]
    fn token_recharge_assigns_instead_of_adding() {
        let mut app = test_app::headless();
        let e = app
            .world_mut()
            .spawn((
                Player(0),
                Tokens {
                    count: 0,
                    recharge: 1,
                },
            ))
            .id();
        // Force the timeout path off; all-spent path triggers recharge.
        app.update();
        let tokens = app.world().get::<Tokens>(e).unwrap();
        assert_eq!(tokens.count, 1, "recharge assigns the waiter value");
    }

    #[test]
    fn swap_and_follow_are_blocked_conservatively() {
        let mut app = test_app::headless();
        let a = app
            .world_mut()
            .spawn((
                Active,
                Collider,
                Pos(IVec2::new(1, 1)),
                PendingPos::MoveTo(IVec2::new(2, 1)),
            ))
            .id();
        let b = app
            .world_mut()
            .spawn((
                Active,
                Collider,
                Pos(IVec2::new(2, 1)),
                PendingPos::MoveTo(IVec2::new(1, 1)),
            ))
            .id();
        {
            let mut grid = app.world_mut().resource_mut::<MapGrid>();
            grid.set(IVec2::new(1, 1), a);
            grid.set(IVec2::new(2, 1), b);
        }
        // Run only the resolve trio.
        app.update();
        // Both blocked: positions unchanged, intents consumed, cells kept.
        assert_eq!(app.world().get::<Pos>(a).unwrap().0, IVec2::new(1, 1));
        assert_eq!(app.world().get::<Pos>(b).unwrap().0, IVec2::new(2, 1));
        assert!(app.world().get::<PendingPos>(a).is_none());
        let grid = app.world().resource::<MapGrid>();
        assert_eq!(grid.get(IVec2::new(1, 1)), Some(a));
        assert_eq!(grid.get(IVec2::new(2, 1)), Some(b));
        // ...and two collision events were recorded.
        assert_eq!(app.world().resource::<CollisionBuffer>().0.len(), 2);
    }

    #[test]
    fn single_winner_for_one_free_cell() {
        let mut app = test_app::headless();
        let first = app
            .world_mut()
            .spawn((
                Active,
                Collider,
                Pos(IVec2::new(0, 0)),
                PendingPos::MoveTo(IVec2::new(1, 0)),
            ))
            .id();
        {
            let mut grid = app.world_mut().resource_mut::<MapGrid>();
            grid.set(IVec2::new(0, 0), first);
        }
        let second = app
            .world_mut()
            .spawn((
                Active,
                Collider,
                Pos(IVec2::new(2, 0)),
                PendingPos::MoveTo(IVec2::new(1, 0)),
            ))
            .id();
        {
            let mut grid = app.world_mut().resource_mut::<MapGrid>();
            grid.set(IVec2::new(2, 0), second);
        }
        app.update();
        let grid = app.world().resource::<MapGrid>();
        let winner = grid
            .get(IVec2::new(1, 0))
            .expect("a winner occupies the cell");
        assert!(
            winner == first || winner == second,
            "winner must be one claimant"
        );
        let loser = if winner == first { second } else { first };
        // Loser kept its old cell.
        let loser_home = if loser == first {
            IVec2::new(0, 0)
        } else {
            IVec2::new(2, 0)
        };
        assert_eq!(grid.get(loser_home), Some(loser));
        assert_eq!(app.world().resource::<CollisionBuffer>().0.len(), 1);
    }

    #[test]
    fn destroy_removes_from_map_before_despawn() {
        let mut app = test_app::headless();
        let e = app
            .world_mut()
            .spawn((Active, Collider, Pos(IVec2::new(3, 3)), DestroyRequested))
            .id();
        app.world_mut()
            .resource_mut::<MapGrid>()
            .set(IVec2::new(3, 3), e);
        // Pass 1 schedules map removal; pass 2 unmaps and despawns.
        app.update();
        app.update();
        assert_eq!(
            app.world().resource::<MapGrid>().get(IVec2::new(3, 3)),
            None
        );
        assert!(app.world().get_entity(e).is_err());
    }
}
