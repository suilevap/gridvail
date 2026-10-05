use bevy::prelude::*;

use super::DoorPolicy;
use crate::locomotion::tests::{boot_walking, place_enemy, status};
use crate::model::*;
use crate::navigation::{NavCell, NavMap};

#[test]
fn the_last_key_is_worth_the_most() {
    let policy = DoorPolicy::default();
    assert_eq!(policy.door_cost(0), None, "no key, no door");
    assert_eq!(policy.door_cost(1), Some(20));
    assert_eq!(policy.door_cost(2), Some(10));
    assert_eq!(policy.door_cost(3), Some(7));
    assert_eq!(policy.door_cost(10), Some(2));
    let thrifty = DoorPolicy { last_key_cost: 100 };
    assert_eq!(thrifty.door_cost(1), Some(100));
}

#[test]
fn door_costs_follow_the_keys_held() {
    let mut app = boot_walking();
    let enemy = place_enemy(&mut app, IVec2::new(3, 5));
    app.update();
    let door_cost = |app: &App| app.world().get::<TraversalPrefs>(enemy).unwrap().door_cost;
    assert_eq!(door_cost(&app), None);
    for (keys, expected) in [(1, 20), (2, 10), (4, 5)] {
        let world = app.world_mut();
        while world.get::<Inventory>(enemy).unwrap().0.len() < keys {
            let key = world.spawn((Active, Item, Key)).id();
            world.get_mut::<Inventory>(enemy).unwrap().0.push(key);
        }
        app.update();
        assert_eq!(door_cost(&app), Some(expected), "{keys} keys");
    }
}

#[test]
fn wandering_enemies_keep_moving_to_new_goals() {
    let mut app = boot_walking();
    let world = app.world_mut();
    let enemies: Vec<(Entity, IVec2)> = world
        .query_filtered::<(Entity, &Pos), With<Enemy>>()
        .iter(world)
        .map(|(entity, pos)| (entity, pos.0))
        .collect();
    let mut goals = vec![Vec::new(); enemies.len()];
    for _ in 0..300 {
        app.update();
        for (i, (enemy, _)) in enemies.iter().enumerate() {
            let goal = app.world().get::<Destination>(*enemy).unwrap().goal();
            if let Some(goal) = goal {
                if goals[i].last() != Some(&goal) {
                    goals[i].push(goal);
                }
            }
        }
    }
    let nav = app.world().resource::<NavMap>();
    for (i, (enemy, start)) in enemies.iter().enumerate() {
        assert!(goals[i].len() >= 2, "enemy {i} had goals {:?}", goals[i]);
        assert!(goals[i].iter().all(|goal| nav.at(*goal) == NavCell::Floor));
        assert_ne!(app.world().get::<Pos>(*enemy).unwrap().0, *start);
    }
}

#[test]
fn a_wanderer_picks_a_new_goal_when_its_walk_ends() {
    let mut app = boot_walking();
    let enemy = place_enemy(&mut app, IVec2::new(18, 2));
    app.world_mut().entity_mut(enemy).insert(Wander);
    // The closet behind its door: unreachable without a key.
    let closet = IVec2::new(18, 5);
    app.world_mut()
        .get_mut::<Destination>(enemy)
        .unwrap()
        .go_to(closet);
    let mut saw_unreachable = false;
    for _ in 0..20 {
        app.update();
        saw_unreachable |= status(&app, enemy) == WalkStatus::Unreachable;
        if app.world().get::<Destination>(enemy).unwrap().goal() != Some(closet) {
            break;
        }
    }
    assert!(saw_unreachable);
    let goal = app.world().get::<Destination>(enemy).unwrap().goal();
    assert!(goal.is_some_and(|goal| goal != closet), "new goal {goal:?}");
}

/// Behavior-tree decisions in a bare app: no locomotion, so these check what
/// the trees ask for (acts, steps, destinations), not where enemies end up.
mod trees {
    use bevy::prelude::*;
    use flatbt_bevy::prelude::Behavior;
    use rand::SeedableRng;

    use crate::ai::{enemy_tree, AiPlugin};
    use crate::model::*;
    use crate::navigation::NavMap;

    const ENEMY: IVec2 = IVec2::new(1, 1);
    const PLAYER: IVec2 = IVec2::new(5, 1);

    fn headless() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AiPlugin))
            .insert_resource(MapGrid::new(8, 8))
            .init_resource::<NavMap>()
            .init_resource::<TurnState>()
            .insert_resource(SharedRng(rand::rngs::StdRng::seed_from_u64(42)));
        app.world_mut().spawn((Active, Player(0), Pos(PLAYER)));
        app
    }

    fn spawn_enemy(app: &mut App, tokens: i32) -> Entity {
        app.world_mut()
            .spawn((
                (Active, Enemy, Pos(ENEMY)),
                Tokens {
                    count: tokens,
                    recharge: 1,
                },
                MoveCommand::default(),
                (Destination::default(), PathFollow::default()),
                EnemyMind::default(),
                Behavior::for_tree(enemy_tree),
            ))
            .id()
    }

    fn wall_at(app: &mut App, pos: IVec2) {
        let wall = app.world_mut().spawn((Wall, Pos(pos))).id();
        app.world_mut()
            .resource_mut::<MapGrid>()
            .set_with_blocking(pos, wall, true);
    }

    fn act_of(app: &App, enemy: Entity) -> Option<EnemyAct> {
        app.world().get::<EnemyAct>(enemy).copied()
    }

    fn goal_of(app: &App, enemy: Entity) -> Option<IVec2> {
        app.world().get::<Destination>(enemy).unwrap().goal()
    }

    fn is_idle(act: Option<EnemyAct>) -> bool {
        matches!(act, Some(EnemyAct::Patrol(_) | EnemyAct::Rest))
    }

    // Nothing spends tokens in this app, so every update is another turn.

    #[test]
    fn a_sighting_freezes_for_a_beat_then_walks_at_the_player() {
        let mut app = headless();
        let enemy = spawn_enemy(&mut app, 1);
        app.update();
        assert_eq!(act_of(&app, enemy), Some(EnemyAct::Alert));
        let command = app.world().get::<MoveCommand>(enemy).unwrap();
        assert!(command.active && command.target == IVec2::ZERO);
        assert_eq!(goal_of(&app, enemy), None);

        app.update();
        assert_eq!(act_of(&app, enemy), Some(EnemyAct::Hunt(PLAYER)));
        assert_eq!(goal_of(&app, enemy), Some(PLAYER), "a walk for locomotion");
    }

    #[test]
    fn a_wall_between_hides_the_player() {
        let mut app = headless();
        wall_at(&mut app, IVec2::new(3, 1));
        let enemy = spawn_enemy(&mut app, 1);
        for _ in 0..10 {
            app.update();
            assert!(is_idle(act_of(&app, enemy)), "{:?}", act_of(&app, enemy));
        }
    }

    #[test]
    fn a_lost_player_is_searched_for_without_a_second_alert() {
        let mut app = headless();
        let enemy = spawn_enemy(&mut app, 1);
        app.update();
        app.update();
        wall_at(&mut app, IVec2::new(3, 1));
        app.update();
        assert_eq!(act_of(&app, enemy), Some(EnemyAct::Search(PLAYER)));
        assert_eq!(goal_of(&app, enemy), Some(PLAYER));
    }

    #[test]
    fn a_search_locomotion_cannot_finish_is_given_up() {
        let mut app = headless();
        let enemy = spawn_enemy(&mut app, 1);
        app.update();
        app.update();
        wall_at(&mut app, IVec2::new(3, 1));
        app.update();
        // Locomotion reports there is no way to the last sighting.
        app.world_mut().get_mut::<PathFollow>(enemy).unwrap().status = WalkStatus::Unreachable;
        app.update();
        assert!(is_idle(act_of(&app, enemy)), "{:?}", act_of(&app, enemy));
        assert_eq!(app.world().get::<EnemyMind>(enemy).unwrap().last_seen, None);
        assert_eq!(goal_of(&app, enemy), None, "the walk was called off");
    }

    #[test]
    fn an_enemy_without_a_token_is_not_ticked() {
        let mut app = headless();
        let enemy = spawn_enemy(&mut app, 0);
        app.update();
        assert_eq!(act_of(&app, enemy), None);
        assert!(!app.world().get::<MoveCommand>(enemy).unwrap().active);
    }
}
