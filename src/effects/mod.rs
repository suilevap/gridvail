//! Effects: short-lived glyphs that are only shown, on top of the motion
//! (`animation`), for the feel of a fight. Every effect is data, one file
//! in `assets/effects` (see `content::effects`), bundled into the game: a
//! new effect is a new file and needs no code. A `PlayEffect` names one;
//! each of its emitters throws off `Particle`s that drift, slow down, turn
//! colour and fade. Nothing here touches the simulation, and its random
//! numbers are its own, so the game plays the same without it.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use crate::content::data;
use crate::content::effects::{EffectSpec, EmitterSpec, GlyphSpec, SourceSpec};
use crate::model::*;
use crate::schedule::GamePhase;

include!(concat!(env!("OUT_DIR"), "/bundled_effects.rs"));

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        let library = EffectLibrary::bundled().unwrap_or_else(|error| panic!("{error}"));
        app.add_message::<PlayEffect>()
            .insert_resource(library)
            .insert_resource(EffectRng(StdRng::seed_from_u64(0x5eed)))
            .add_systems(
                Update,
                (play_effects, advance_particles)
                    .chain()
                    .in_set(GamePhase::Animation),
            );
    }
}

/// Every effect, by name. The bundled ones by default; change it (or insert
/// another) to change how effects look.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct EffectLibrary(pub HashMap<String, EffectSpec>);

impl EffectLibrary {
    /// Every effect in `assets/effects`.
    pub fn bundled() -> Result<Self, String> {
        Self::read(BUNDLED_EFFECTS.iter().copied())
    }

    /// The effects in `(name, text)` files.
    pub fn read<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> Result<Self, String> {
        files
            .into_iter()
            .map(|(name, text)| {
                let spec = data::read(&format!("effect `{name}`"), text)?;
                Ok((name.to_string(), spec))
            })
            .collect::<Result<_, String>>()
            .map(Self)
    }
}

/// The effects' own random numbers, apart from the game's.
#[derive(Resource)]
pub struct EffectRng(pub StdRng);

fn rgb((r, g, b): (u8, u8, u8)) -> Rgb {
    Rgb::new(r, g, b)
}

fn pick(rng: &mut StdRng, (low, high): (f32, f32)) -> f32 {
    if high > low {
        rng.random_range(low..=high)
    } else {
        low
    }
}

/// Where along a swing, out from the wielder, a stroke is at `t` from 0
/// (the start of the swing) to 1, and the way the swing goes there.
pub fn along_swing(swing: GestureShape, (near, far, arc): (f32, f32, f32), t: f32) -> (Vec2, Vec2) {
    match swing {
        GestureShape::Sweep { toward, across } => {
            let ahead = toward.normalize_or_zero();
            let (sin, cos) = (arc * (2.0 * t - 1.0)).sin_cos();
            (
                (ahead * cos + across * sin) * far,
                across * cos - ahead * sin,
            )
        }
        GestureShape::Thrust { toward }
        | GestureShape::Hook { toward }
        | GestureShape::Reach { toward }
        | GestureShape::Shake { toward } => {
            let ahead = toward.normalize_or_zero();
            (ahead * (near + (far - near) * t), ahead)
        }
    }
}

/// The stroke drawn for something going `way` (map y grows down the
/// screen).
pub fn stroke(way: Vec2) -> char {
    let angle = way.y.atan2(way.x).rem_euclid(std::f32::consts::PI);
    match (angle / std::f32::consts::FRAC_PI_4).round() as i32 {
        1 => '\\',
        2 => '|',
        3 => '/',
        _ => '-',
    }
}

/// The particles one emitter throws off for `request`, from `at`: each
/// with the seconds until it appears.
pub fn emit(
    emitter: &EmitterSpec,
    request: &PlayEffect,
    at: Vec2,
    rng: &mut StdRng,
) -> Vec<(Particle, f32)> {
    let toward = request.toward.normalize_or(Vec2::X);
    let count = emitter.count as f32 + emitter.per_strength * request.strength.max(0) as f32;
    let count = count.round() as usize;
    let mut particles = Vec::with_capacity(count);
    for i in 0..count {
        let t = if count > 1 {
            i as f32 / (count - 1) as f32
        } else {
            0.0
        };
        let (start, way, delay) = match emitter.from {
            SourceSpec::Point { ahead, spread } => {
                let angle = if spread > 0.0 {
                    rng.random_range(-spread..=spread)
                } else {
                    0.0
                };
                (
                    at + toward * ahead,
                    Vec2::from_angle(angle).rotate(toward),
                    0.0,
                )
            }
            SourceSpec::Along { near, far, arc } => {
                let Some(swing) = request.swing else {
                    return Vec::new();
                };
                let (point, way) = along_swing(swing, (near, far, arc), t);
                (at + point, way, t * request.secs)
            }
        };
        let ch = match &emitter.glyphs {
            GlyphSpec::Pick(glyphs) if !glyphs.is_empty() => {
                glyphs[rng.random_range(0..glyphs.len())]
            }
            GlyphSpec::Pick(_) => continue,
            GlyphSpec::Along => stroke(way),
        };
        let Some(&color) = emitter
            .colors
            .get(rng.random_range(0..emitter.colors.len().max(1)))
        else {
            continue;
        };
        let color = rgb(color);
        particles.push((
            Particle {
                position: start,
                velocity: way * pick(rng, emitter.speed),
                drag: emitter.drag,
                age: 0.0,
                life: pick(rng, emitter.life),
                hold: emitter.hold,
                ch,
                color,
                end_color: emitter.fade_to.map_or(color, rgb),
            },
            request.delay + emitter.delay + delay,
        ));
    }
    particles
}

/// Plays each requested effect from where its entity is shown: every
/// emitter's particles, each waiting for its moment. An effect nobody
/// wrote is skipped.
pub fn play_effects(
    mut commands: Commands,
    mut requests: MessageReader<PlayEffect>,
    library: Res<EffectLibrary>,
    mut rng: ResMut<EffectRng>,
    shown: Query<(Option<&AnimatedPos>, &Pos)>,
) {
    for request in requests.read() {
        let Some(effect) = library.0.get(&request.effect) else {
            warn_once!("no effect called `{}` in assets/effects", request.effect);
            continue;
        };
        let Ok((animated, pos)) = shown.get(request.entity) else {
            continue;
        };
        let at = animated.map_or(pos.0.as_vec2(), |animated| animated.position);
        for emitter in &effect.emitters {
            for (particle, delay) in emit(emitter, request, at, &mut rng.0) {
                if delay > 0.0 {
                    commands.spawn((particle, ParticleDelay(delay)));
                } else {
                    commands.spawn(particle);
                }
            }
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
