use bevy::prelude::*;

use crate::model::*;

/// The displacements of a frame being resolved, kept between frames so
/// resolving allocates nothing once warmed up.
#[derive(Default)]
pub struct DisplaceBatch(Vec<Request>);

/// One displacement of a frame and how it resolved.
#[derive(Clone, Copy, Debug)]
struct Request {
    entity: Entity,
    from: IVec2,
    to: IVec2,
    forced: bool,
    state: State,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Pending,
    Moves,
    /// Stays, blocked by this.
    Blocked(Entity),
}

/// Carries out the frame's `Displace`s together. A displacement goes ahead
/// when its cell is free, or is left by another one that goes ahead (chains
/// follow on, and a cycle, such as a swap, moves as one). One whose cell
/// stays taken is blocked: a forced one is an `Impact` with what is there.
/// Two into the same free cell: the first wins.
pub fn displace(
    mut grid: ResMut<MapGrid>,
    mut requests: MessageReader<Displace>,
    mut impacts: MessageWriter<Impact>,
    mut bodies: Query<(&mut Pos, Option<&mut PrevPos>, Has<Speed>), Without<DestroyRequested>>,
    mut batch: Local<DisplaceBatch>,
) {
    let batch = &mut batch.0;
    batch.clear();
    for request in requests.read() {
        let Ok((pos, ..)) = bodies.get(request.entity) else {
            continue;
        };
        batch.push(Request {
            entity: request.entity,
            from: pos.0,
            to: grid.safe_pos(request.to),
            forced: request.forced,
            state: State::Pending,
        });
    }
    if batch.is_empty() {
        return;
    }
    resolve(&grid, batch);
    for request in batch.iter().filter(|request| request.state == State::Moves) {
        if grid.get(request.from) == Some(request.entity) {
            grid.clear(request.from);
        }
    }
    for request in batch.iter() {
        match request.state {
            State::Moves => {
                let Ok((mut pos, previous, walks)) = bodies.get_mut(request.entity) else {
                    continue;
                };
                if let Some(mut previous) = previous {
                    previous.0 = pos.0;
                }
                pos.0 = request.to;
                grid.set_with_blocking(request.to, request.entity, !walks);
            }
            State::Blocked(obstacle) if request.forced => {
                impacts.write(Impact {
                    mover: request.entity,
                    obstacle,
                });
            }
            _ => {}
        }
    }
}

/// Settles every request: moves, or blocked by what keeps its cell.
fn resolve(grid: &MapGrid, batch: &mut [Request]) {
    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..batch.len() {
            if batch[i].state != State::Pending {
                continue;
            }
            let request = batch[i];
            let claimed = batch.iter().find(|other| {
                other.entity != request.entity
                    && other.to == request.to
                    && other.state == State::Moves
            });
            let state = match (claimed, grid.get(request.to)) {
                (Some(winner), _) => State::Blocked(winner.entity),
                (None, None) => State::Moves,
                (None, Some(occupant)) if occupant == request.entity => State::Moves,
                (None, Some(occupant)) => {
                    match batch.iter().find(|other| other.entity == occupant) {
                        Some(leaving) if leaving.state == State::Moves => State::Moves,
                        Some(leaving) if leaving.state == State::Pending => continue,
                        _ => State::Blocked(occupant),
                    }
                }
            };
            batch[i].state = state;
            changed = true;
        }
    }
    // What is still pending waits on itself round a cycle: it all moves.
    for request in batch.iter_mut() {
        if request.state == State::Pending {
            request.state = State::Moves;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(MapGrid::new(6, 6))
            .add_message::<Displace>()
            .add_message::<Impact>()
            .add_systems(Update, displace);
        app
    }

    fn body(app: &mut App, at: IVec2) -> Entity {
        let entity = app
            .world_mut()
            .spawn((Pos(at), PrevPos(at), Speed::default()))
            .id();
        app.world_mut().resource_mut::<MapGrid>().set(at, entity);
        entity
    }

    fn at(app: &App, entity: Entity) -> IVec2 {
        app.world().get::<Pos>(entity).unwrap().0
    }

    fn impacts(app: &App) -> Vec<Impact> {
        let messages = app.world().resource::<Messages<Impact>>();
        messages.iter_current_update_messages().copied().collect()
    }

    fn displace_to(app: &mut App, entity: Entity, to: IVec2, forced: bool) {
        app.world_mut()
            .write_message(Displace { entity, to, forced });
    }

    #[test]
    fn two_can_swap() {
        let mut app = app();
        let (a, b) = (
            body(&mut app, IVec2::new(1, 1)),
            body(&mut app, IVec2::new(2, 1)),
        );
        displace_to(&mut app, a, IVec2::new(2, 1), true);
        displace_to(&mut app, b, IVec2::new(1, 1), false);
        app.update();
        assert_eq!(
            (at(&app, a), at(&app, b)),
            (IVec2::new(2, 1), IVec2::new(1, 1))
        );
        let grid = app.world().resource::<MapGrid>();
        assert_eq!(grid.get(IVec2::new(2, 1)), Some(a));
        assert_eq!(grid.get(IVec2::new(1, 1)), Some(b));
    }

    #[test]
    fn a_follower_moves_into_the_cell_left_for_it() {
        let mut app = app();
        let (a, b) = (
            body(&mut app, IVec2::new(1, 1)),
            body(&mut app, IVec2::new(2, 1)),
        );
        // Listed first, the follower waits for the cell to be left.
        displace_to(&mut app, a, IVec2::new(2, 1), false);
        displace_to(&mut app, b, IVec2::new(3, 1), true);
        app.update();
        assert_eq!(
            (at(&app, a), at(&app, b)),
            (IVec2::new(2, 1), IVec2::new(3, 1))
        );
    }

    #[test]
    fn a_blocked_forced_move_is_an_impact_and_its_follower_stays() {
        let mut app = app();
        let (a, b) = (
            body(&mut app, IVec2::new(1, 1)),
            body(&mut app, IVec2::new(2, 1)),
        );
        let wall = body(&mut app, IVec2::new(3, 1));
        displace_to(&mut app, b, IVec2::new(3, 1), true);
        displace_to(&mut app, a, IVec2::new(2, 1), false);
        app.update();
        assert_eq!(
            (at(&app, a), at(&app, b)),
            (IVec2::new(1, 1), IVec2::new(2, 1))
        );
        assert_eq!(
            impacts(&app),
            vec![Impact {
                mover: b,
                obstacle: wall
            }],
            "only the forced move is an impact"
        );
    }

    #[test]
    fn the_first_into_a_free_cell_wins() {
        let mut app = app();
        let (a, b) = (
            body(&mut app, IVec2::new(1, 1)),
            body(&mut app, IVec2::new(3, 1)),
        );
        displace_to(&mut app, a, IVec2::new(2, 1), true);
        displace_to(&mut app, b, IVec2::new(2, 1), true);
        app.update();
        assert_eq!(
            (at(&app, a), at(&app, b)),
            (IVec2::new(2, 1), IVec2::new(3, 1))
        );
        assert_eq!(
            impacts(&app),
            vec![Impact {
                mover: b,
                obstacle: a
            }]
        );
    }
}
