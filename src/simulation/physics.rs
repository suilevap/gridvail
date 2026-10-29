//! Bodies moved by more than their own walking, on the simulation's own
//! movement passes: a knocked body is given a `Speed` and moves, slows
//! (`Friction`) and is blocked like anything that walks, pass after pass
//! until everything has settled (`turn_update` keeps the passes going). The
//! systems here only start motion and report on it:
//!
//! - `knock`: a `Knock` sets a body's speed and marks it `Knocked`;
//! - `report_collisions`: a knocked body that was blocked is a `Collision`;
//! - `settle_knocked`: a knocked body that has stopped is not knocked any
//!   more;
//! - `follow`: a `Follow` steps into a cell once it has been left;
//! - `swap_places`: a `Swap` trades two bodies' places, which walking
//!   cannot do.

use bevy::prelude::*;

use crate::model::*;

/// Sets each knocked body moving: its speed becomes the knock's, and it is
/// `Knocked` until it stops. Runs after each mover's own step is chosen,
/// so a knock wins over it.
pub fn knock(
    mut commands: Commands,
    mut knocks: MessageReader<Knock>,
    mut bodies: Query<&mut Speed, Without<DestroyRequested>>,
) {
    for knock in knocks.read().filter(|knock| knock.velocity != IVec2::ZERO) {
        let Ok(mut speed) = bodies.get_mut(knock.entity) else {
            continue;
        };
        speed.0 = knock.velocity;
        commands.entity(knock.entity).insert(Knocked);
    }
}

/// A knocked body blocked on this pass ran into what blocked it.
pub fn report_collisions(
    blocked: Res<CollisionBuffer>,
    mut collisions: MessageWriter<Collision>,
    knocked: Query<(), With<Knocked>>,
) {
    for bump in blocked
        .0
        .iter()
        .filter(|bump| knocked.contains(bump.source))
    {
        collisions.write(Collision {
            mover: bump.source,
            obstacle: bump.target,
        });
    }
}

/// A knocked body that has stopped is no longer knocked.
pub fn settle_knocked(mut commands: Commands, knocked: Query<(Entity, &Speed), With<Knocked>>) {
    for (body, speed) in &knocked {
        if speed.0 == IVec2::ZERO {
            commands.entity(body).remove::<Knocked>();
        }
    }
}

/// Each follower steps into its cell once the cell has been left: it is
/// given the step as its speed, which the next movement pass carries out.
pub fn follow(
    grid: Res<MapGrid>,
    mut follows: MessageReader<Follow>,
    mut followers: Query<(&Pos, &mut Speed), Without<DestroyRequested>>,
) {
    for follow in follows.read() {
        if grid.get(follow.into).is_some() {
            continue;
        }
        let Ok((pos, mut speed)) = followers.get_mut(follow.follower) else {
            continue;
        };
        let step = (follow.into - pos.0).clamp(IVec2::NEG_ONE, IVec2::ONE);
        speed.0 = step;
    }
}

/// Trades the places of each pair to swap.
pub fn swap_places(
    mut grid: ResMut<MapGrid>,
    mut swaps: MessageReader<Swap>,
    mut bodies: Query<(&mut Pos, &mut PrevPos, Has<Speed>), Without<DestroyRequested>>,
) {
    for swap in swaps.read() {
        let Ok([(mut a, mut a_prev, a_walks), (mut b, mut b_prev, b_walks)]) =
            bodies.get_many_mut([swap.a, swap.b])
        else {
            continue;
        };
        (a_prev.0, b_prev.0) = (a.0, b.0);
        (a.0, b.0) = (b.0, a.0);
        grid.set_with_blocking(a.0, swap.a, !a_walks);
        grid.set_with_blocking(b.0, swap.b, !b_walks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::test_app::headless;
    use crate::simulation::{friction, movement, resolve_collect, resolve_commit, turn_update};

    fn app() -> App {
        let mut app = headless();
        app.add_message::<Knock>()
            .add_message::<Collision>()
            .add_message::<Follow>()
            .add_message::<Swap>()
            .add_systems(
                Update,
                (
                    (knock, swap_places).before(movement),
                    (report_collisions, settle_knocked)
                        .chain()
                        .after(resolve_collect)
                        .before(turn_update),
                    follow
                        .after(resolve_commit)
                        .after(friction)
                        .before(turn_update),
                ),
            );
        app
    }

    fn body(app: &mut App, at: IVec2) -> Entity {
        let entity = app
            .world_mut()
            .spawn((
                Active,
                Collider,
                Pos(at),
                PrevPos(at),
                PendingPos::default(),
                Speed::default(),
                Friction(1),
            ))
            .id();
        app.world_mut().resource_mut::<MapGrid>().set(at, entity);
        entity
    }

    fn wall(app: &mut App, at: IVec2) -> Entity {
        let entity = app.world_mut().spawn((Collider, Pos(at))).id();
        app.world_mut().resource_mut::<MapGrid>().set(at, entity);
        entity
    }

    fn at(app: &App, entity: Entity) -> IVec2 {
        app.world().get::<Pos>(entity).unwrap().0
    }

    fn settle(app: &mut App) {
        for _ in 0..5 {
            app.update();
        }
    }

    fn collisions(app: &App) -> Vec<Collision> {
        let messages = app.world().resource::<Messages<Collision>>();
        messages.iter_current_update_messages().copied().collect()
    }

    #[test]
    fn a_knocked_body_moves_on_the_movement_passes_and_settles() {
        let mut app = app();
        let a = body(&mut app, IVec2::new(1, 1));
        app.world_mut().write_message(Knock {
            entity: a,
            velocity: IVec2::X,
        });
        settle(&mut app);
        assert_eq!(at(&app, a), IVec2::new(2, 1));
        assert!(app.world().get::<Knocked>(a).is_none(), "still knocked");
    }

    #[test]
    fn a_knocked_body_that_is_blocked_collides() {
        let mut app = app();
        let a = body(&mut app, IVec2::new(1, 1));
        let w = wall(&mut app, IVec2::new(2, 1));
        app.world_mut().write_message(Knock {
            entity: a,
            velocity: IVec2::X,
        });
        app.update();
        assert_eq!(
            collisions(&app),
            vec![Collision {
                mover: a,
                obstacle: w
            }]
        );
        assert_eq!(at(&app, a), IVec2::new(1, 1));
    }

    #[test]
    fn a_follower_steps_in_once_the_cell_is_left() {
        let mut app = app();
        let (a, b) = (
            body(&mut app, IVec2::new(1, 1)),
            body(&mut app, IVec2::new(2, 1)),
        );
        app.world_mut().write_message(Knock {
            entity: b,
            velocity: IVec2::X,
        });
        app.world_mut().write_message(Follow {
            follower: a,
            into: IVec2::new(2, 1),
        });
        settle(&mut app);
        assert_eq!(
            (at(&app, a), at(&app, b)),
            (IVec2::new(2, 1), IVec2::new(3, 1))
        );
    }

    #[test]
    fn a_follower_stays_when_the_cell_is_not_left() {
        let mut app = app();
        let (a, b) = (
            body(&mut app, IVec2::new(1, 1)),
            body(&mut app, IVec2::new(2, 1)),
        );
        wall(&mut app, IVec2::new(3, 1));
        app.world_mut().write_message(Knock {
            entity: b,
            velocity: IVec2::X,
        });
        app.world_mut().write_message(Follow {
            follower: a,
            into: IVec2::new(2, 1),
        });
        settle(&mut app);
        assert_eq!(
            (at(&app, a), at(&app, b)),
            (IVec2::new(1, 1), IVec2::new(2, 1))
        );
    }

    #[test]
    fn two_bodies_swap_places() {
        let mut app = app();
        let (a, b) = (
            body(&mut app, IVec2::new(1, 1)),
            body(&mut app, IVec2::new(2, 1)),
        );
        app.world_mut().write_message(Swap { a, b });
        app.update();
        assert_eq!(
            (at(&app, a), at(&app, b)),
            (IVec2::new(2, 1), IVec2::new(1, 1))
        );
        let grid = app.world().resource::<MapGrid>();
        assert_eq!(grid.get(IVec2::new(2, 1)), Some(a));
        assert_eq!(grid.get(IVec2::new(1, 1)), Some(b));
    }
}
