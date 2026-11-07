//! Effects: short-lived glyphs that are only shown, on top of the motion
//! (`animation`), for the feel of a fight. Blood splashes off what a blow
//! lands on, and a swing leaves a streak where the weapon passed. Messages
//! (`Splash`, `Swoosh`) ask for them; each becomes a few `Particle`s that
//! drift, slow down and fade. Nothing here touches the simulation, and its
//! random numbers are its own, so the game plays the same without it.

use bevy::prelude::*;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use crate::lighting::{DARK_RED, RED};
use crate::model::*;
use crate::schedule::GamePhase;

/// Drops a splash throws for each point of damage, and at least.
const DROPS_PER_DAMAGE: i32 = 4;
const MIN_DROPS: i32 = 4;
/// How fast drops fly off (cells a second), how far either side of the
/// blow they spread (radians), and how soon they stop.
const DROP_SPEED: (f32, f32) = (2.5, 7.0);
const DROP_SPREAD: f32 = 1.1;
const DROP_DRAG: f32 = 6.0;
/// How long a drop lies on the floor, in seconds, and how long it takes
/// to fade at the end.
const DROP_LIFE: (f32, f32) = (1.5, 3.0);
const DROP_FADE: f32 = 1.0;
const DROP_GLYPHS: [char; 4] = ['·', '.', ',', '\''];
const DROP_COLORS: [Rgb; 3] = [RED, DARK_RED, Rgb::new(200, 24, 24)];
/// The flash where a blow lands.
const SPARK_GLYPH: char = '*';
const SPARK_COLOR: Rgb = Rgb::new(255, 240, 200);
const SPARK_LIFE: f32 = 0.12;

/// How a swing's streak looks: its colour, how many glyphs make it, how
/// long each shows, and where it passes, in cells from the wielder.
const STREAK_COLOR: Rgb = Rgb::new(220, 230, 255);
const STREAK_GLYPHS: usize = 9;
const STREAK_LIFE: f32 = 0.22;
const STREAK_FADE: f32 = 0.18;
/// A thrust's streak runs from near the wielder to past the cell it
/// strikes; a sweep's arcs through this angle either side of straight
/// ahead, this far out.
const THRUST_STREAK: (f32, f32) = (0.3, 1.05);
const SWEEP_STREAK_ANGLE: f32 = 1.2;
const SWEEP_STREAK_RADIUS: f32 = 0.9;

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Splash>()
            .add_message::<Swoosh>()
            .insert_resource(EffectRng(StdRng::seed_from_u64(0x5eed)))
            .add_systems(
                Update,
                (splash_blood, streak_swings, advance_particles)
                    .chain()
                    .in_set(GamePhase::Animation),
            );
    }
}

/// The effects' own random numbers, apart from the game's.
#[derive(Resource)]
pub struct EffectRng(pub StdRng);

/// Where `entity` is shown now (its cell, if it is not animated).
fn shown_at(entity: Entity, shown: &Query<(Option<&AnimatedPos>, &Pos)>) -> Option<Vec2> {
    shown
        .get(entity)
        .ok()
        .map(|(animated, pos)| animated.map_or(pos.0.as_vec2(), |animated| animated.position))
}

fn spawn(commands: &mut Commands, particle: Particle, delay: f32) {
    if delay > 0.0 {
        commands.spawn((particle, ParticleDelay(delay)));
    } else {
        commands.spawn(particle);
    }
}

/// Each splash throws drops off its target, spread around the way the blow
/// went, that land and lie a while; and a flash where the blow struck.
pub fn splash_blood(
    mut commands: Commands,
    mut splashes: MessageReader<Splash>,
    mut rng: ResMut<EffectRng>,
    shown: Query<(Option<&AnimatedPos>, &Pos)>,
) {
    for splash in splashes.read() {
        let Some(at) = shown_at(splash.entity, &shown) else {
            continue;
        };
        let toward = splash.toward.normalize_or(Vec2::X);
        spawn(
            &mut commands,
            Particle {
                position: at - toward * 0.4,
                velocity: Vec2::ZERO,
                drag: 0.0,
                age: 0.0,
                life: SPARK_LIFE,
                fade: SPARK_LIFE,
                ch: SPARK_GLYPH,
                color: SPARK_COLOR,
            },
            splash.delay,
        );
        let drops = (splash.strength * DROPS_PER_DAMAGE).max(MIN_DROPS);
        for _ in 0..drops {
            let rng = &mut rng.0;
            let angle = rng.random_range(-DROP_SPREAD..=DROP_SPREAD);
            let speed = rng.random_range(DROP_SPEED.0..=DROP_SPEED.1);
            spawn(
                &mut commands,
                Particle {
                    position: at,
                    velocity: Vec2::from_angle(angle).rotate(toward) * speed,
                    drag: DROP_DRAG,
                    age: 0.0,
                    life: rng.random_range(DROP_LIFE.0..=DROP_LIFE.1),
                    fade: DROP_FADE,
                    ch: DROP_GLYPHS[rng.random_range(0..DROP_GLYPHS.len())],
                    color: DROP_COLORS[rng.random_range(0..DROP_COLORS.len())],
                },
                splash.delay,
            );
        }
    }
}

/// Points along a swing's streak, out from the wielder, at `t` from 0 (the
/// start of the swing) to 1, and the way it is going there. A shake leaves
/// none.
pub fn streak_at(shape: GestureShape, t: f32) -> Option<(Vec2, Vec2)> {
    match shape {
        GestureShape::Thrust { toward }
        | GestureShape::Hook { toward }
        | GestureShape::Reach { toward } => {
            let ahead = toward.normalize_or_zero();
            let out = THRUST_STREAK.0 + (THRUST_STREAK.1 - THRUST_STREAK.0) * t;
            Some((ahead * out, ahead))
        }
        GestureShape::Sweep { toward, across } => {
            let ahead = toward.normalize_or_zero();
            let angle = SWEEP_STREAK_ANGLE * (2.0 * t - 1.0);
            let (sin, cos) = angle.sin_cos();
            let point = (ahead * cos + across * sin) * SWEEP_STREAK_RADIUS;
            let going = across * cos - ahead * sin;
            Some((point, going))
        }
        GestureShape::Shake { .. } => None,
    }
}

/// The glyph of a streak going `way` (map y grows down the screen).
pub fn streak_glyph(way: Vec2) -> char {
    let angle = way.y.atan2(way.x).rem_euclid(std::f32::consts::PI);
    match (angle / std::f32::consts::FRAC_PI_4).round() as i32 {
        1 => '\\',
        2 => '|',
        3 => '/',
        _ => '-',
    }
}

/// Each swing leaves a streak where its weapon passes: glyphs that appear
/// one after another along its way while it strikes, then fade fast.
pub fn streak_swings(
    mut commands: Commands,
    mut swooshes: MessageReader<Swoosh>,
    shown: Query<(Option<&AnimatedPos>, &Pos)>,
) {
    for swoosh in swooshes.read() {
        let Some(at) = shown_at(swoosh.entity, &shown) else {
            continue;
        };
        for i in 0..STREAK_GLYPHS {
            let t = i as f32 / (STREAK_GLYPHS - 1) as f32;
            let Some((point, going)) = streak_at(swoosh.shape, t) else {
                break;
            };
            spawn(
                &mut commands,
                Particle {
                    position: at + point,
                    velocity: going * 0.6,
                    drag: 4.0,
                    age: 0.0,
                    life: STREAK_LIFE,
                    fade: STREAK_FADE,
                    ch: streak_glyph(going),
                    color: STREAK_COLOR,
                },
                swoosh.delay + t * swoosh.secs,
            );
        }
    }
}

/// Particles appear when their delay is up, drift and slow down, and go
/// once they have lived their life.
pub fn advance_particles(
    mut commands: Commands,
    time: Res<Time>,
    mut waiting: Query<(Entity, &mut ParticleDelay)>,
    mut particles: Query<(Entity, &mut Particle), Without<ParticleDelay>>,
) {
    let dt = time.delta_secs();
    for (entity, mut delay) in &mut waiting {
        delay.0 -= dt;
        if delay.0 <= 0.0 {
            commands.entity(entity).remove::<ParticleDelay>();
        }
    }
    for (entity, mut particle) in &mut particles {
        particle.age += dt;
        if particle.age >= particle.life {
            commands.entity(entity).despawn();
            continue;
        }
        let velocity = particle.velocity;
        particle.position += velocity * dt;
        particle.velocity = velocity * (-particle.drag * dt).exp();
    }
}

#[cfg(test)]
mod tests;
