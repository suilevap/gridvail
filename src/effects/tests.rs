use std::time::Duration;

use bevy::time::TimeUpdateStrategy;

use super::*;

const FRAME: f32 = 0.016;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            FRAME,
        )))
        .add_plugins(EffectsPlugin);
    app.update();
    app
}

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        app.update();
    }
}

fn particles(app: &mut App) -> Vec<Particle> {
    let mut query = app
        .world_mut()
        .query_filtered::<&Particle, Without<ParticleDelay>>();
    query.iter(app.world()).copied().collect()
}

#[test]
fn a_splash_throws_drops_the_way_the_blow_went_that_lie_then_fade() {
    let mut app = app();
    let target = app.world_mut().spawn(Pos(IVec2::new(4, 4))).id();
    app.world_mut().write_message(Splash {
        entity: target,
        toward: Vec2::X,
        strength: 2,
        delay: 0.1,
    });
    frames(&mut app, 2);
    assert!(
        particles(&mut app).is_empty(),
        "splashed before the blow landed"
    );
    frames(&mut app, 30);
    let drops: Vec<_> = particles(&mut app)
        .into_iter()
        .filter(|drop| drop.ch != SPARK_GLYPH)
        .collect();
    assert_eq!(drops.len(), 8);
    assert!(
        drops.iter().all(|drop| drop.position.x > 4.0),
        "a drop flew back toward the blow"
    );
    assert!(
        drops.iter().all(|drop| drop.velocity.length() < 0.5),
        "the drops have not landed"
    );
    frames(&mut app, 250);
    assert!(particles(&mut app).is_empty(), "the drops never faded");
}

#[test]
fn a_sweep_streaks_from_the_side_it_starts_on_to_the_other() {
    let mut app = app();
    let wielder = app.world_mut().spawn(Pos(IVec2::new(4, 4))).id();
    app.world_mut().write_message(Swoosh {
        entity: wielder,
        shape: GestureShape::Sweep {
            toward: Vec2::X,
            across: Vec2::Y,
        },
        delay: 0.0,
        secs: 0.2,
    });
    frames(&mut app, 2);
    let first = particles(&mut app);
    assert!(!first.is_empty());
    assert!(first.iter().all(|streak| streak.position.y < 4.0));
    frames(&mut app, 12);
    assert!(particles(&mut app)
        .iter()
        .any(|streak| streak.position.y > 4.0));
}

#[test]
fn a_streak_is_drawn_the_way_it_goes() {
    assert_eq!(streak_glyph(Vec2::X), '-');
    assert_eq!(streak_glyph(Vec2::NEG_Y), '|');
    assert_eq!(streak_glyph(Vec2::new(1.0, 1.0)), '\\');
    assert_eq!(streak_glyph(Vec2::new(1.0, -1.0)), '/');
}
