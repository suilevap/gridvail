use bevy::prelude::*;
use pav_ecs_game_bevy_port::components::*;
use pav_ecs_game_bevy_port::game::GamePlugin;

fn snap(w: &mut World, label: &str) {
    let pt = w.query_filtered::<&Tokens, With<Player>>().single(w).unwrap().count;
    let etoks: Vec<i32> = w.query_filtered::<&Tokens, With<Enemy>>().iter(w).map(|t| t.count).collect();
    let cmds = w.query::<&MoveCommand>().iter(w).count();
    let epos: Vec<IVec2> = w.query_filtered::<&Pos, With<Enemy>>().iter(w).map(|p| p.0).collect();
    let ppos = w.query_filtered::<&Pos, With<Player>>().single(w).unwrap().0;
    eprintln!("{label}: player={ppos:?} ptok={pt} enemies={epos:?} etoks={etoks:?} cmds={cmds}");
}

#[test]
fn release_scenario() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<ButtonInput<KeyCode>>()
        .add_plugins(GamePlugin);
    app.update();
    app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::ArrowRight);
    app.update();
    app.world_mut().resource_mut::<ButtonInput<KeyCode>>().release(KeyCode::ArrowRight);
    for i in 0..4 {
        app.update();
        let w = app.world_mut();
        snap(w, &format!("pass {i}"));
    }
}
