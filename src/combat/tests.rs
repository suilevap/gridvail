use bevy::prelude::*;

use super::*;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(MapGrid::new(8, 8))
        .add_message::<ChannelDone>()
        .add_message::<ContextAction>()
        .add_message::<Displace>()
        .add_message::<Impact>()
        .add_message::<Damage>()
        .add_plugins(CombatPlugin);
    app
}

fn sent<M: Message + Copy>(app: &App) -> Vec<M> {
    let messages = app.world().resource::<Messages<M>>();
    messages.iter_current_update_messages().copied().collect()
}

fn blade() -> Maneuvers {
    let make = |name: &str, follow| Maneuver {
        name: name.to_string(),
        hit: 1,
        knock: true,
        follow,
    };
    Maneuvers {
        forward: make("lunge", true),
        side: make("push", false),
        back: make("yank", true),
    }
}

/// An actor with the blade moves at (2, 2), a movable target right of it,
/// and a context action with second direction `dir`.
fn maneuver(dir: IVec2) -> (App, Entity, Entity) {
    let mut app = app();
    let actor = app.world_mut().spawn((blade(), Pos(IVec2::new(2, 2)))).id();
    let target = app.world_mut().spawn((Movable, Pos(IVec2::new(3, 2)))).id();
    app.world_mut().write_message(ContextAction {
        actor,
        target,
        picked: IVec2::X,
        dir,
    });
    app.update();
    (app, actor, target)
}

#[test]
fn a_lunge_hits_knocks_the_target_on_and_follows() {
    let (app, actor, target) = maneuver(IVec2::X);
    assert_eq!(
        sent::<Hit>(&app),
        vec![Hit {
            source: actor,
            target,
            damage: 1
        }]
    );
    assert_eq!(
        sent::<Displace>(&app),
        vec![
            Displace {
                entity: target,
                to: IVec2::new(4, 2),
                forced: true
            },
            Displace {
                entity: actor,
                to: IVec2::new(3, 2),
                forced: false
            },
        ]
    );
}

#[test]
fn a_push_knocks_the_target_aside_and_stays() {
    let (app, _, target) = maneuver(IVec2::Y);
    assert_eq!(
        sent::<Displace>(&app),
        vec![Displace {
            entity: target,
            to: IVec2::new(3, 3),
            forced: true
        }]
    );
}

#[test]
fn a_yank_pulls_the_target_into_the_actors_cell_and_swaps() {
    let (app, actor, target) = maneuver(IVec2::NEG_X);
    let displaced: Vec<_> = sent::<Displace>(&app)
        .iter()
        .map(|d| (d.entity, d.to))
        .collect();
    assert_eq!(
        displaced,
        vec![(target, IVec2::new(2, 2)), (actor, IVec2::new(3, 2))]
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
fn an_impact_hurts_both_sides() {
    let mut app = app();
    let (mover, obstacle) = (
        app.world_mut().spawn_empty().id(),
        app.world_mut().spawn_empty().id(),
    );
    app.world_mut().write_message(Impact { mover, obstacle });
    app.update();
    let targets: Vec<_> = sent::<Damage>(&app).iter().map(|d| d.target).collect();
    assert_eq!(targets, vec![mover, obstacle]);
}

#[test]
fn a_weapon_is_aimed_and_wound_up_then_strikes_its_shape() {
    let mut app = app();
    let sweep = Weapon {
        shape: StrikeShape(vec![IVec2::new(1, -1), IVec2::X, IVec2::new(1, 1)]),
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
    let world = app.world();
    assert_eq!(
        world.get::<Telegraph>(wielder).map(|t| t.dir),
        Some(IVec2::NEG_X)
    );
    assert_eq!(
        world.get::<Channeling>(wielder),
        Some(&Channeling { turns: 1 })
    );

    app.world_mut().write_message(ChannelDone(wielder));
    app.update();
    let struck: Vec<_> = sent::<Hit>(&app).iter().map(|hit| hit.target).collect();
    assert!(struck.contains(&front) && struck.contains(&beside));
    assert!(!struck.contains(&behind));
    assert!(app.world().get::<Telegraph>(wielder).is_none());
}
