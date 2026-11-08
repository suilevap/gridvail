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

fn request(effect: EffectId, entity: Entity) -> PlayEffect {
    PlayEffect {
        effect,
        entity,
        toward: Vec2::X,
        swing: None,
        secs: 0.0,
        strength: 1,
        delay: 0.0,
    }
}

#[test]
fn every_effect_file_is_an_id_and_reads() {
    let library = EffectLibrary::bundled().unwrap();
    for id in EffectId::ALL {
        assert!(library.get(id).is_some(), "{id:?}");
    }
}

#[test]
fn how_an_effect_looks_is_only_data() {
    // Blood that looks like dust: a library read from other text.
    let library = EffectLibrary::read([(
        EffectId::Blood,
        "(emitters: [(count: 3, from: Point(ahead: 0.0, spread: 3.14), \
          speed: (1.0, 2.0), life: (0.5, 0.5), glyphs: Pick(['.']), \
          colors: [(120, 120, 120)])])",
    )])
    .unwrap();
    let mut app = app();
    app.insert_resource(library);
    let crate_ = app.world_mut().spawn(Pos(IVec2::new(4, 4))).id();
    app.world_mut()
        .write_message(request(EffectId::Blood, crate_));
    frames(&mut app, 2);
    let dust = particles(&mut app);
    assert_eq!(dust.len(), 3);
    assert!(dust.iter().all(|mote| mote.ch == '.'));
    frames(&mut app, 40);
    assert!(particles(&mut app).is_empty(), "the dust never settled");
}

#[test]
fn blood_is_thrown_the_way_the_blow_went_and_lies_then_fades() {
    let mut app = app();
    let target = app.world_mut().spawn(Pos(IVec2::new(4, 4))).id();
    app.world_mut().write_message(PlayEffect {
        strength: 2,
        delay: 0.1,
        ..request(EffectId::Blood, target)
    });
    frames(&mut app, 2);
    assert!(
        particles(&mut app).is_empty(),
        "bled before the blow landed"
    );
    frames(&mut app, 50);
    let drops: Vec<_> = particles(&mut app)
        .into_iter()
        .filter(|drop| drop.ch != '*')
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
    app.world_mut().write_message(PlayEffect {
        swing: Some(GestureShape::Sweep {
            toward: Vec2::X,
            across: Vec2::Y,
        }),
        secs: 0.2,
        ..request(EffectId::Swoosh, wielder)
    });
    frames(&mut app, 2);
    let first = particles(&mut app);
    assert!(!first.is_empty());
    assert!(first.iter().all(|stroke| stroke.position.y < 4.0));
    frames(&mut app, 12);
    assert!(particles(&mut app)
        .iter()
        .any(|stroke| stroke.position.y > 4.0));
}

#[test]
fn strokes_are_drawn_the_way_they_go() {
    assert_eq!(stroke_glyph(Vec2::X), '-');
    assert_eq!(stroke_glyph(Vec2::NEG_Y), '|');
    assert_eq!(stroke_glyph(Vec2::new(1.0, 1.0)), '\\');
    assert_eq!(stroke_glyph(Vec2::new(1.0, -1.0)), '/');
}

#[test]
fn a_new_map_clears_every_particle() {
    let mut app = app();
    let target = app.world_mut().spawn(Pos(IVec2::new(4, 4))).id();
    app.world_mut().write_message(PlayEffect {
        delay: 0.5,
        ..request(EffectId::Blood, target)
    });
    app.world_mut()
        .write_message(request(EffectId::Blood, target));
    frames(&mut app, 2);
    assert!(!particles(&mut app).is_empty());
    app.world_mut().write_message(MapLoaded::default());
    frames(&mut app, 2);
    let mut all = app.world_mut().query::<&Particle>();
    assert_eq!(
        all.iter(app.world()).count(),
        0,
        "a particle outlived its map"
    );
}

#[test]
fn a_particle_holds_then_fades_and_turns_colour() {
    let mut particle = Particle {
        position: Vec2::ZERO,
        velocity: Vec2::ZERO,
        drag: 0.0,
        age: 0.0,
        life: 1.0,
        hold: 0.5,
        ch: '.',
        stroke: None,
        color: Rgb::new(200, 0, 0),
        end_color: Rgb::new(100, 0, 0),
    };
    particle.age = 0.4;
    assert_eq!(particle.alpha(), 1.0);
    particle.age = 0.75;
    assert!((particle.alpha() - 0.5).abs() < 1e-4);
    assert_eq!(particle.shown_color(), Rgb::new(125, 0, 0));
}
