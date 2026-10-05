use bevy::prelude::*;

use super::DoorPolicy;
use crate::locomotion::tests::{boot_walking, place_enemy};
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
    let door_cost = |app: &App| app.world().get::<Destination>(enemy).unwrap().door_cost;
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
            let goal = app.world().get::<Destination>(*enemy).unwrap().goal;
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
