use bevy::prelude::*;

use crate::model::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MoveCommit {
    entity: Entity,
    from: Option<IVec2>,
    to: IVec2,
}

#[derive(Resource, Debug, Default)]
pub struct CommitBuffer {
    commits: Vec<MoveCommit>,
    order: Vec<(Entity, Option<IVec2>, IVec2)>,
    reserved: Vec<Option<Entity>>,
}

impl CommitBuffer {
    pub fn sized(width: i32, height: i32) -> Self {
        let cells = (width * height).max(0) as usize;
        Self {
            commits: Vec::with_capacity(cells),
            order: Vec::with_capacity(cells),
            reserved: vec![None; cells],
        }
    }

    fn prepare(&mut self, cells: usize) {
        self.commits.clear();
        self.order.clear();
        if self.reserved.len() != cells {
            self.reserved.resize(cells, None);
        } else {
            self.reserved.fill(None);
        }
    }
}

pub fn resolve_collect(
    grid: Res<MapGrid>,
    mut collisions: ResMut<CollisionBuffer>,
    mut commits: ResMut<CommitBuffer>,
    mut claimants: Query<
        (Entity, Option<&Pos>, &mut PendingPos),
        (With<Collider>, Without<DestroyRequested>),
    >,
) {
    collisions.0.clear();
    commits.prepare((grid.width * grid.height) as usize);
    for (entity, pos, pending) in claimants.iter_mut() {
        if let PendingPos::MoveTo(dest) = *pending {
            commits
                .order
                .push((entity, pos.map(|pos| pos.0), grid.safe_pos(dest)));
        }
    }
    commits
        .order
        .sort_by_key(|(entity, _, _)| (entity.index(), entity.generation()));
    for i in 0..commits.order.len() {
        let (entity, from, to) = commits.order[i];
        let index = grid.idx(to).expect("wrapped destination");
        match grid.get(to) {
            Some(other) if other != entity => {
                collisions.0.push(CollisionEvent {
                    source: entity,
                    target: other,
                });
                *claimants.get_mut(entity).expect("claimant").2 = PendingPos::None;
            }
            _ if commits.reserved[index].is_some() => {
                let winner = commits.reserved[index].unwrap_or(entity);
                if winner != entity {
                    collisions.0.push(CollisionEvent {
                        source: entity,
                        target: winner,
                    });
                }
                *claimants.get_mut(entity).expect("claimant").2 = PendingPos::None;
            }
            _ => {
                commits.reserved[index] = Some(entity);
                *claimants.get_mut(entity).expect("claimant").2 = PendingPos::MoveTo(to);
                commits.commits.push(MoveCommit { entity, from, to });
            }
        }
    }
}

pub fn resolve_unmap(
    mut grid: ResMut<MapGrid>,
    movers: Query<(Entity, Option<&Pos>, &PendingPos), With<Collider>>,
) {
    for (entity, pos, pending) in movers.iter() {
        if *pending != PendingPos::None {
            if let Some(pos) = pos {
                if grid.get(pos.0) == Some(entity) {
                    grid.clear(pos.0);
                }
            }
        }
    }
}

pub fn resolve_commit(
    mut grid: ResMut<MapGrid>,
    mut movers: Query<(
        Entity,
        &mut Pos,
        &mut PendingPos,
        Option<&Collider>,
        Option<&Speed>,
        Option<&mut PrevPos>,
    )>,
) {
    for (entity, mut pos, mut pending, collider, speed, previous) in movers.iter_mut() {
        if *pending == PendingPos::None {
            continue;
        }
        if let Some(mut previous) = previous {
            previous.0 = pos.0;
        }
        if let PendingPos::MoveTo(dest) = *pending {
            pos.0 = dest;
            if collider.is_some() {
                grid.set_with_blocking(dest, entity, speed.is_none());
            }
        }
        *pending = PendingPos::None;
    }
}

pub fn resolve_missing_positions(
    mut commands: Commands,
    mut grid: ResMut<MapGrid>,
    movers: Query<
        (Entity, &PendingPos, Option<&Speed>),
        (With<Collider>, Without<Pos>, Without<DestroyRequested>),
    >,
) {
    for (entity, pending, speed) in movers.iter() {
        if let PendingPos::MoveTo(dest) = *pending {
            commands.entity(entity).insert((Pos(dest), PrevPos(dest)));
            grid.set_with_blocking(dest, entity, speed.is_none());
        }
    }
}

pub fn verify_map(
    grid: Res<MapGrid>,
    bodies: Query<(Entity, &Pos), (With<Collider>, Without<DestroyRequested>)>,
) {
    if cfg!(debug_assertions) {
        for (entity, pos) in bodies.iter() {
            debug_assert_eq!(grid.get(pos.0), Some(entity));
        }
    }
}
