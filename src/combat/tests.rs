use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

use super::*;
use crate::simulation::TimeSet;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(MapGrid::new(8, 8))
        .init_resource::<TurnState>()
        .add_message::<DebtPaid>()
        .add_message::<Delay>()
        .add_message::<Knock>()
        .add_message::<Follow>()
        .add_message::<Turn>()
        .add_message::<Collision>()
        .add_message::<Damage>()
        .add_plugins(CombatPlugin)
        // As in the game: what falls due is released before it happens, and
        // actors decide before what they decided plays out.
        .configure_sets(Update, TimeSet::Release.before(ActionStage::Ongoing))
        .configure_sets(Update, (ActionStage::Decide, ActionStage::Effects).chain());
    app
}

fn sent<M: Message + Copy>(app: &App) -> Vec<M> {
    let messages = app.world().resource::<Messages<M>>();
    messages.iter_current_update_messages().copied().collect()
}

fn blade() -> Maneuvers {
    let make = |name: &str, cells: &[IVec2], hit, stance, turn| Maneuver {
        name: name.to_string(),
        area: Area(cells.to_vec()),
        stop: false,
        hit,
        knock: 1,
        stance,
        turn,
    };
    let ahead_left = IVec2::new(1, -1);
    let ahead_right = IVec2::new(1, 1);
    Maneuvers {
        forward: make("lunge", &[IVec2::X], 1, Stance::Follow, false),
        left: make(
            "sweep left",
            &[IVec2::X, ahead_left, IVec2::NEG_Y],
            1,
            Stance::Step,
            true,
        ),
        right: make(
            "sweep right",
            &[IVec2::X, ahead_right, IVec2::Y],
            1,
            Stance::Step,
            true,
        ),
        back: Maneuver {
            knock: 0,
            ..make("back off", &[IVec2::X], 0, Stance::Step, false)
        },
    }
}

/// A body that can be attacked at `at`.
fn target(app: &mut App, at: IVec2) -> Entity {
    let target = app
        .world_mut()
        .spawn((Health::full(2), Movable, Pos(at)))
        .id();
    app.world_mut().resource_mut::<MapGrid>().set(at, target);
    target
}

/// An actor with the blade moves at (2, 2), facing east, moving `dir`.
fn actor(app: &mut App, dir: IVec2) -> Entity {
    let at = IVec2::new(2, 2);
    let actor = app
        .world_mut()
        .spawn((
            blade(),
            Pos(at),
            Facing(IVec2::X),
            Tokens {
                count: 1,
                recharge: 1,
            },
            MoveCommand {
                target: dir,
                relative: true,
                active: true,
                ..default()
            },
        ))
        .id();
    app.world_mut().resource_mut::<MapGrid>().set(at, actor);
    actor
}

#[test]
fn a_move_into_something_ahead_lunges_carrying_its_step_and_takes_the_turn() {
    let mut app = app();
    let target = target(&mut app, IVec2::new(3, 2));
    let actor = actor(&mut app, IVec2::X);
    app.update();
    assert_eq!(
        sent::<Hit>(&app),
        vec![Hit {
            source: actor,
            target,
            damage: 1
        }]
    );
    assert_eq!(
        sent::<Knock>(&app),
        vec![Knock {
            entity: target,
            velocity: IVec2::new(2, 0)
        }],
        "the swing's push and the step it carries"
    );
    assert_eq!(
        sent::<Follow>(&app),
        vec![Follow {
            follower: actor,
            into: IVec2::new(3, 2)
        }]
    );
    let command = app.world().get::<MoveCommand>(actor).unwrap();
    assert_eq!(command.target, IVec2::ZERO, "the move still walks");
}

#[test]
fn a_move_that_meets_nothing_is_left_to_walk() {
    let mut app = app();
    let actor = actor(&mut app, IVec2::X);
    app.update();
    assert!(sent::<Attack>(&app).is_empty());
    assert_eq!(
        app.world().get::<MoveCommand>(actor).unwrap().target,
        IVec2::X
    );
}

#[test]
fn a_side_move_sweeps_what_is_ahead_that_way_steps_and_turns() {
    let mut app = app();
    let target = target(&mut app, IVec2::new(3, 2));
    // Facing east, north is to the left.
    let actor = actor(&mut app, IVec2::NEG_Y);
    app.update();
    assert_eq!(
        sent::<Knock>(&app),
        vec![Knock {
            entity: target,
            velocity: IVec2::new(0, -2)
        }]
    );
    assert_eq!(
        sent::<Follow>(&app),
        vec![Follow {
            follower: actor,
            into: IVec2::new(2, 1)
        }]
    );
    assert_eq!(
        sent::<Turn>(&app),
        vec![Turn {
            entity: actor,
            dir: IVec2::NEG_Y
        }]
    );
}

#[test]
fn a_sweep_hits_each_target_in_order_unless_it_stops_at_the_first() {
    for stop in [false, true] {
        let mut app = app();
        let ahead = target(&mut app, IVec2::new(3, 2));
        let beside = target(&mut app, IVec2::new(2, 1));
        let actor = actor(&mut app, IVec2::NEG_Y);
        app.world_mut()
            .get_mut::<Maneuvers>(actor)
            .unwrap()
            .left
            .stop = stop;
        app.update();
        let hit: Vec<_> = sent::<Hit>(&app).iter().map(|hit| hit.target).collect();
        let expected = if stop {
            vec![ahead]
        } else {
            vec![ahead, beside]
        };
        assert_eq!(hit, expected, "stop: {stop}");
    }
}

#[test]
fn backing_off_from_something_ahead_steps_back_still_facing_it() {
    let mut app = app();
    target(&mut app, IVec2::new(3, 2));
    let actor = actor(&mut app, IVec2::NEG_X);
    app.update();
    assert!(sent::<Hit>(&app).is_empty() && sent::<Turn>(&app).is_empty());
    assert_eq!(
        sent::<Follow>(&app),
        vec![Follow {
            follower: actor,
            into: IVec2::new(1, 2)
        }]
    );
}

#[test]
fn a_hit_on_an_exposed_target_is_critical() {
    let mut app = app();
    let source = app.world_mut().spawn_empty().id();
    let wary = app.world_mut().spawn(Exposed(false)).id();
    let exposed = app.world_mut().spawn(Exposed(true)).id();
    for target in [wary, exposed] {
        app.world_mut().write_message(Hit {
            source,
            target,
            damage: 1,
        });
    }
    app.update();
    let total = |target| {
        sent::<Damage>(&app)
            .iter()
            .filter(|d| d.target == target)
            .map(|d| d.amount)
            .sum::<i32>()
    };
    assert_eq!((total(wary), total(exposed)), (1, 2));
}

#[test]
fn a_collision_hurts_both_sides() {
    let mut app = app();
    let (mover, obstacle) = (
        app.world_mut().spawn_empty().id(),
        app.world_mut().spawn_empty().id(),
    );
    app.world_mut().write_message(Collision { mover, obstacle });
    app.update();
    let damage: Vec<_> = sent::<Damage>(&app)
        .iter()
        .map(|d| (d.target, d.amount))
        .collect();
    assert_eq!(damage, vec![(mover, 1), (obstacle, 1)]);
}

#[test]
fn a_weapon_is_aimed_owes_its_windup_then_strikes_its_area() {
    let mut app = app();
    let sweep = Weapon {
        area: Area(vec![IVec2::new(1, -1), IVec2::X, IVec2::new(1, 1)]),
        damage: 1,
        windup: 1,
    };
    let wielder = app.world_mut().spawn((sweep, Pos(IVec2::new(3, 3)))).id();
    // Two in the row in front (to the west), one behind.
    let victim = |app: &mut App, at: IVec2| {
        let entity = app.world_mut().spawn((Health::full(2), Pos(at))).id();
        app.world_mut().resource_mut::<MapGrid>().set(at, entity);
        entity
    };
    let front = victim(&mut app, IVec2::new(2, 3));
    let beside = victim(&mut app, IVec2::new(2, 4));
    let behind = victim(&mut app, IVec2::new(4, 3));
    app.world_mut().write_message(UseWeapon {
        user: wielder,
        dir: IVec2::NEG_X,
    });
    app.update();
    assert_eq!(
        sent::<Delay>(&app),
        vec![Delay {
            entity: wielder,
            turns: 1
        }]
    );
    let world = app.world();
    assert_eq!(
        world.get::<Telegraph>(wielder).map(|t| t.dir),
        Some(IVec2::NEG_X)
    );
    assert!(world.get::<Scheduled<Strike>>(wielder).is_some());

    app.world_mut().write_message(DebtPaid(wielder));
    app.update();
    app.update();
    let messages = app.world().resource::<Messages<Hit>>();
    let struck: Vec<_> = messages
        .iter_current_update_messages()
        .map(|hit| hit.target)
        .collect();
    let struck = if struck.is_empty() {
        let mut cursor = messages.get_cursor();
        cursor.read(messages).map(|hit| hit.target).collect()
    } else {
        struck
    };
    assert!(struck.contains(&front) && struck.contains(&beside));
    assert!(!struck.contains(&behind));
    assert!(app.world().get::<Telegraph>(wielder).is_none());
}

#[test]
fn strike_marks_are_planted_in_the_order_the_weapon_swings() {
    let mut world = World::new();
    world.insert_resource(MapGrid::new(8, 8));
    world.init_resource::<CellMarks>();
    let sweep = Weapon {
        area: Area(vec![IVec2::new(1, 1), IVec2::X, IVec2::new(1, -1)]),
        damage: 1,
        windup: 1,
    };
    // Facing east, the area goes from south to north.
    world.spawn((Pos(IVec2::new(3, 3)), sweep, Telegraph { dir: IVec2::X }));
    world.run_system_once(mark_strikes).unwrap();
    let mut marks = world.resource::<CellMarks>().0.clone();
    marks.sort_by(|a, b| a.1.delay.total_cmp(&b.1.delay));
    let cells: Vec<_> = marks.iter().map(|(cell, _)| *cell).collect();
    assert_eq!(
        cells,
        vec![IVec2::new(4, 4), IVec2::new(4, 3), IVec2::new(4, 2)],
        "not in the area's order"
    );
    assert!(marks[0].1.delay < marks[1].1.delay);
}
