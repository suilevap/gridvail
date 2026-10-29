use bevy::prelude::*;

use crate::model::*;

/// Sub-steps a frame's bodies may take at most, so a resolution that keeps
/// passing motion on still ends.
const MAX_STEPS: usize = 32;

/// The moving bodies of a frame, kept between frames so moving allocates
/// nothing once warmed up.
#[derive(Default)]
pub struct Bodies(Vec<Body>);

/// A body in motion and how its next cell resolved.
#[derive(Clone, Copy, Debug)]
struct Body {
    entity: Entity,
    /// Where its motion began this frame.
    start: IVec2,
    pos: IVec2,
    /// A unit direction times the speed left, in cells.
    velocity: IVec2,
    forced: bool,
    friction: i32,
    state: Step,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Not moving any more.
    Stopped,
    Pending,
    Moves,
    /// Stays, blocked by this.
    Blocked(Entity),
}

impl Body {
    fn speed(&self) -> i32 {
        self.velocity.x.abs().max(self.velocity.y.abs())
    }

    fn dir(&self) -> IVec2 {
        self.velocity.signum()
    }
}

/// Moves every body the frame's `Impulse`s set moving, a cell per sub-step,
/// all bodies together, until all have stopped.
///
/// In each sub-step a body goes ahead when its next cell is free, or is
/// left by another body that goes ahead (one follows another). A ring of
/// bodies each moving into the next one's cell (two swapping) goes ahead
/// only if one of them steps there on purpose (unforced): bodies knocked
/// into each other collide instead of passing through. A body whose cell
/// stays taken stops; a forced one sends a `Collision` with the speed it
/// had left, and a `Pushable` body it ran into carries that speed on. A
/// body that moved loses its friction of speed.
#[allow(clippy::type_complexity)]
pub fn move_bodies(
    mut grid: ResMut<MapGrid>,
    mut impulses: MessageReader<Impulse>,
    mut collisions: MessageWriter<Collision>,
    mut placed: Query<
        (
            &mut Pos,
            Option<&mut PrevPos>,
            Has<Speed>,
            Option<&Friction>,
        ),
        Without<DestroyRequested>,
    >,
    pushable: Query<(), (With<Pushable>, Without<DestroyRequested>)>,
    mut bodies: Local<Bodies>,
) {
    let bodies = &mut bodies.0;
    bodies.clear();
    for impulse in impulses.read() {
        let Ok((pos, _, _, friction)) = placed.get(impulse.entity) else {
            continue;
        };
        let friction = friction.map_or(1, |friction| friction.0.max(1));
        push(
            bodies,
            impulse.entity,
            pos.0,
            impulse.velocity,
            impulse.forced,
            friction,
        );
    }
    for _ in 0..MAX_STEPS {
        if !bodies.iter().any(|body| body.velocity != IVec2::ZERO) {
            break;
        }
        resolve(&grid, bodies);
        for body in bodies.iter().filter(|body| body.state == Step::Moves) {
            if grid.get(body.pos) == Some(body.entity) {
                grid.clear(body.pos);
            }
        }
        for i in 0..bodies.len() {
            let body = bodies[i];
            match body.state {
                Step::Moves => {
                    let to = grid.safe_pos(body.pos + body.dir());
                    let Ok((mut pos, previous, walks, _)) = placed.get_mut(body.entity) else {
                        continue;
                    };
                    if let Some(mut previous) = previous {
                        previous.0 = body.start;
                    }
                    pos.0 = to;
                    grid.set_with_blocking(to, body.entity, !walks);
                    let left = (body.speed() - body.friction).max(0);
                    bodies[i].pos = to;
                    bodies[i].velocity = body.dir() * left;
                }
                Step::Blocked(obstacle) => {
                    bodies[i].velocity = IVec2::ZERO;
                    if !body.forced {
                        continue;
                    }
                    collisions.write(Collision {
                        mover: body.entity,
                        obstacle,
                        speed: body.speed(),
                    });
                    if pushable.contains(obstacle) {
                        if let Ok((pos, _, _, friction)) = placed.get(obstacle) {
                            let friction = friction.map_or(1, |friction| friction.0.max(1));
                            push(bodies, obstacle, pos.0, body.velocity, true, friction);
                        }
                    }
                }
                Step::Stopped | Step::Pending => {}
            }
        }
    }
}

/// Adds `velocity` to a body's motion, or sets a new body moving.
fn push(
    bodies: &mut Vec<Body>,
    entity: Entity,
    pos: IVec2,
    velocity: IVec2,
    forced: bool,
    friction: i32,
) {
    if let Some(body) = bodies.iter_mut().find(|body| body.entity == entity) {
        body.velocity += velocity;
        body.forced |= forced;
        return;
    }
    bodies.push(Body {
        entity,
        start: pos,
        pos,
        velocity,
        forced,
        friction,
        state: Step::Pending,
    });
}

/// Settles each moving body's next cell: it moves, or what blocks it.
fn resolve(grid: &MapGrid, bodies: &mut [Body]) {
    for body in bodies.iter_mut() {
        body.state = if body.velocity == IVec2::ZERO {
            Step::Stopped
        } else {
            Step::Pending
        };
    }
    let target = |body: &Body| grid.safe_pos(body.pos + body.dir());
    loop {
        settle_chains(grid, bodies, &target);
        // What is still pending waits on itself round a ring: the ring
        // moves if anyone in it steps on purpose, else it is blocked.
        let Some(start) = bodies.iter().position(|body| body.state == Step::Pending) else {
            return;
        };
        let mut ring = vec![start];
        let mut next = start;
        loop {
            let occupant = grid.get(target(&bodies[next]));
            let Some(after) = bodies.iter().position(|body| Some(body.entity) == occupant) else {
                break;
            };
            if ring.contains(&after) {
                ring.drain(..ring.iter().position(|&i| i == after).unwrap_or(0));
                break;
            }
            ring.push(after);
            next = after;
        }
        let on_purpose = ring.iter().any(|&i| !bodies[i].forced);
        for &i in &ring {
            bodies[i].state = if on_purpose {
                Step::Moves
            } else {
                let occupant = grid.get(target(&bodies[i]));
                Step::Blocked(occupant.unwrap_or(bodies[i].entity))
            };
        }
    }
}

/// Settles every body whose next cell is free, taken by something that
/// does not move, or left by a body already settled.
fn settle_chains(grid: &MapGrid, bodies: &mut [Body], target: &impl Fn(&Body) -> IVec2) {
    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..bodies.len() {
            if bodies[i].state != Step::Pending {
                continue;
            }
            let to = target(&bodies[i]);
            let claimed = bodies.iter().find(|other| {
                other.entity != bodies[i].entity
                    && other.state == Step::Moves
                    && target(other) == to
            });
            let state = match (claimed, grid.get(to)) {
                (Some(winner), _) => Step::Blocked(winner.entity),
                (None, None) => Step::Moves,
                (None, Some(occupant)) => {
                    match bodies.iter().find(|other| other.entity == occupant) {
                        Some(leaving) if leaving.state == Step::Moves => Step::Moves,
                        Some(leaving) if leaving.state == Step::Pending => continue,
                        _ => Step::Blocked(occupant),
                    }
                }
            };
            bodies[i].state = state;
            changed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(MapGrid::new(8, 4))
            .add_message::<Impulse>()
            .add_message::<Collision>()
            .add_systems(Update, move_bodies);
        app
    }

    fn body(app: &mut App, at: IVec2) -> Entity {
        let entity = app
            .world_mut()
            .spawn((Pos(at), PrevPos(at), Speed::default(), Friction(1)))
            .id();
        app.world_mut().resource_mut::<MapGrid>().set(at, entity);
        entity
    }

    fn at(app: &App, entity: Entity) -> IVec2 {
        app.world().get::<Pos>(entity).unwrap().0
    }

    fn collisions(app: &App) -> Vec<Collision> {
        let messages = app.world().resource::<Messages<Collision>>();
        messages.iter_current_update_messages().copied().collect()
    }

    fn impulse(app: &mut App, entity: Entity, velocity: IVec2, forced: bool) {
        app.world_mut().write_message(Impulse {
            entity,
            velocity,
            forced,
        });
    }

    #[test]
    fn speed_is_distance_less_friction_per_cell() {
        let mut app = app();
        let a = body(&mut app, IVec2::new(0, 1));
        impulse(&mut app, a, IVec2::new(3, 0), true);
        app.update();
        assert_eq!(at(&app, a), IVec2::new(3, 1));
        let grid = app.world().resource::<MapGrid>();
        assert_eq!(grid.get(IVec2::new(3, 1)), Some(a));
        assert_eq!(grid.get(IVec2::new(0, 1)), None);
    }

    #[test]
    fn a_body_stops_at_what_blocks_it_and_collides_with_the_speed_left() {
        let mut app = app();
        let a = body(&mut app, IVec2::new(0, 1));
        let wall = app.world_mut().spawn(Pos(IVec2::new(2, 1))).id();
        app.world_mut()
            .resource_mut::<MapGrid>()
            .set(IVec2::new(2, 1), wall);
        impulse(&mut app, a, IVec2::new(3, 0), true);
        app.update();
        assert_eq!(at(&app, a), IVec2::new(1, 1));
        assert_eq!(
            collisions(&app),
            vec![Collision {
                mover: a,
                obstacle: wall,
                speed: 2
            }]
        );
    }

    #[test]
    fn a_pushable_body_carries_the_motion_on() {
        let mut app = app();
        let a = body(&mut app, IVec2::new(0, 1));
        let b = body(&mut app, IVec2::new(2, 1));
        app.world_mut().entity_mut(b).insert(Pushable);
        impulse(&mut app, a, IVec2::new(3, 0), true);
        app.update();
        // `a` moves one cell and runs into `b` with 2 left; `b` moves 2.
        assert_eq!(
            (at(&app, a), at(&app, b)),
            (IVec2::new(1, 1), IVec2::new(4, 1))
        );
        assert_eq!(collisions(&app).len(), 1);
    }

    #[test]
    fn a_follower_steps_into_the_cell_left_for_it() {
        let mut app = app();
        let (a, b) = (
            body(&mut app, IVec2::new(1, 1)),
            body(&mut app, IVec2::new(2, 1)),
        );
        impulse(&mut app, a, IVec2::X, false);
        impulse(&mut app, b, IVec2::X, true);
        app.update();
        assert_eq!(
            (at(&app, a), at(&app, b)),
            (IVec2::new(2, 1), IVec2::new(3, 1))
        );
    }

    #[test]
    fn a_step_on_purpose_swaps_but_knocked_bodies_collide() {
        let mut app = app();
        let (a, b) = (
            body(&mut app, IVec2::new(1, 1)),
            body(&mut app, IVec2::new(2, 1)),
        );
        impulse(&mut app, a, IVec2::X, true);
        impulse(&mut app, b, IVec2::NEG_X, false);
        app.update();
        assert_eq!(
            (at(&app, a), at(&app, b)),
            (IVec2::new(2, 1), IVec2::new(1, 1))
        );

        let mut app = self::app();
        let (a, b) = (
            body(&mut app, IVec2::new(1, 1)),
            body(&mut app, IVec2::new(2, 1)),
        );
        impulse(&mut app, a, IVec2::X, true);
        impulse(&mut app, b, IVec2::NEG_X, true);
        app.update();
        assert_eq!(
            (at(&app, a), at(&app, b)),
            (IVec2::new(1, 1), IVec2::new(2, 1))
        );
        assert_eq!(collisions(&app).len(), 2);
    }

    #[test]
    fn a_blocked_follower_stops_without_a_collision() {
        let mut app = app();
        let (a, b) = (
            body(&mut app, IVec2::new(1, 1)),
            body(&mut app, IVec2::new(2, 1)),
        );
        let wall = app.world_mut().spawn(Pos(IVec2::new(3, 1))).id();
        app.world_mut()
            .resource_mut::<MapGrid>()
            .set(IVec2::new(3, 1), wall);
        impulse(&mut app, b, IVec2::X, true);
        impulse(&mut app, a, IVec2::X, false);
        app.update();
        assert_eq!(
            (at(&app, a), at(&app, b)),
            (IVec2::new(1, 1), IVec2::new(2, 1))
        );
        assert_eq!(
            collisions(&app),
            vec![Collision {
                mover: b,
                obstacle: wall,
                speed: 1
            }]
        );
    }
}
