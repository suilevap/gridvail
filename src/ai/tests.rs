use bevy::prelude::*;
use flatbt_bevy::prelude::Behavior;
use rand::SeedableRng;

use super::*;
use crate::model::*;

const ENEMY: IVec2 = IVec2::new(1, 1);
const PLAYER: IVec2 = IVec2::new(5, 1);

fn headless() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AiPlugin))
        .insert_resource(MapGrid::new(8, 8))
        .init_resource::<TurnState>()
        .insert_resource(SharedRng(rand::rngs::StdRng::seed_from_u64(42)));
    app.world_mut().spawn((Active, Player(0), Pos(PLAYER)));
    app
}

fn spawn_enemy(app: &mut App, tokens: i32) -> Entity {
    app.world_mut()
        .spawn((
            Active,
            Enemy,
            Pos(ENEMY),
            Tokens {
                count: tokens,
                recharge: 1,
            },
            MoveCommand::default(),
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

#[test]
fn a_visible_player_is_hunted() {
    let mut app = headless();
    let enemy = spawn_enemy(&mut app, 1);
    app.update();
    assert_eq!(act_of(&app, enemy), Some(EnemyAct::Hunt(IVec2::X)));
    let command = app.world().get::<MoveCommand>(enemy).unwrap();
    assert!(command.active && command.relative);
    assert_eq!(command.target, IVec2::X);
}

#[test]
fn a_wall_between_hides_the_player() {
    let mut app = headless();
    wall_at(&mut app, IVec2::new(3, 1));
    let enemy = spawn_enemy(&mut app, 1);
    app.update();
    assert!(matches!(act_of(&app, enemy), Some(EnemyAct::Wander(_))));
}

#[test]
fn a_lost_player_is_searched_for_where_last_seen() {
    let mut app = headless();
    let enemy = spawn_enemy(&mut app, 1);
    app.update();
    wall_at(&mut app, IVec2::new(3, 1));
    app.update();
    assert_eq!(act_of(&app, enemy), Some(EnemyAct::Search(IVec2::X)));
    assert_eq!(
        app.world().get::<EnemyMind>(enemy).unwrap().last_seen,
        Some(PLAYER)
    );
}

#[test]
fn an_enemy_without_a_token_is_not_ticked() {
    let mut app = headless();
    let enemy = spawn_enemy(&mut app, 0);
    app.update();
    assert_eq!(act_of(&app, enemy), None);
    assert!(!app.world().get::<MoveCommand>(enemy).unwrap().active);
}
