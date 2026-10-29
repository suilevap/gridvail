use bevy::prelude::*;

use crate::animation::NonBlockingAnimation;
use crate::lighting::{DARK_GRAY, RED, WHITE};
use crate::model::*;

use super::mark;

/// How far a target recoils from a blow, in cells.
const RECOIL: f32 = 0.3;
/// How long a struck target flashes, in seconds...
const FLASH_SECS: f32 = 0.45;
/// ...switching between red and white this many times a second.
const FLASH_RATE: f32 = 12.0;
/// How long the hearts show after a hit, in seconds.
const HEALTH_SECS: f32 = 1.5;
/// How long the burst where something died shows, in seconds.
const BURST_SECS: f32 = 0.5;

/// Flashing for a moment, having just been hit.
#[derive(Component, Debug)]
pub struct Flash(pub Timer);

/// Showing its hearts for a moment, having just taken damage.
#[derive(Component, Debug)]
pub struct ShowHealth(pub Timer);

/// A mark left for a moment where something happened (a death), with no
/// one there to carry it.
#[derive(Component, Debug)]
pub struct Popup(pub Timer);

/// A target hit recoils away from the blow and flashes.
pub fn recoil_on_hits(
    mut commands: Commands,
    mut hits: MessageReader<Hit>,
    mut gestures: MessageWriter<Gesture>,
    positions: Query<&Pos, Without<DestroyRequested>>,
) {
    for hit in hits.read() {
        let (Ok(from), Ok(at)) = (positions.get(hit.source), positions.get(hit.target)) else {
            continue;
        };
        gestures.write(Gesture {
            entity: hit.target,
            toward: (at.0 - from.0).clamp(IVec2::NEG_ONE, IVec2::ONE).as_vec2() * RECOIL,
        });
        commands
            .entity(hit.target)
            .insert(Flash(Timer::from_seconds(FLASH_SECS, TimerMode::Once)));
    }
}

/// Whatever took damage and lives shows its hearts for a moment, and
/// flashes too (a knock into a wall hurts without a hit).
pub fn show_health_on_damage(
    mut commands: Commands,
    mut damage: MessageReader<Damage>,
    alive: Query<(), (With<Health>, Without<DestroyRequested>)>,
) {
    for hurt in damage.read().filter(|hurt| hurt.amount > 0) {
        if alive.contains(hurt.target) {
            commands.entity(hurt.target).insert((
                ShowHealth(Timer::from_seconds(HEALTH_SECS, TimerMode::Once)),
                Flash(Timer::from_seconds(FLASH_SECS, TimerMode::Once)),
            ));
        }
    }
}

/// Where something dies, a burst shows for a moment. Runs before the dead
/// are removed, while they still have a place.
pub fn burst_on_death(
    mut commands: Commands,
    mut died: MessageReader<Died>,
    positions: Query<&Pos>,
) {
    for &Died(entity) in died.read() {
        if let Ok(pos) = positions.get(entity) {
            commands.spawn((
                Popup(Timer::from_seconds(BURST_SECS, TimerMode::Once)),
                NonBlockingAnimation,
                Pos(pos.0),
                Glyph::new('*', 3, WHITE),
            ));
        }
    }
}

/// Flashes, hearts and popups wear off.
pub fn wear_off(
    mut commands: Commands,
    time: Res<Time>,
    mut flashes: Query<(Entity, &mut Flash)>,
    mut health: Query<(Entity, &mut ShowHealth)>,
    mut popups: Query<(Entity, &mut Popup)>,
) {
    for (entity, mut flash) in &mut flashes {
        if flash.0.tick(time.delta()).is_finished() {
            commands.entity(entity).remove::<Flash>();
        }
    }
    for (entity, mut shown) in &mut health {
        if shown.0.tick(time.delta()).is_finished() {
            commands.entity(entity).remove::<ShowHealth>();
        }
    }
    for (entity, mut popup) in &mut popups {
        if popup.0.tick(time.delta()).is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

/// A flashing target switches between red and white; a burst keeps its
/// own colour.
pub fn mark_hits(
    marks: Option<ResMut<CellMarks>>,
    flashes: Query<(&Pos, &Flash)>,
    popups: Query<(&Pos, &Glyph), With<Popup>>,
) {
    let Some(mut marks) = marks else {
        return;
    };
    for (pos, flash) in &flashes {
        let on = ((flash.0.elapsed_secs() * FLASH_RATE) as u32).is_multiple_of(2);
        marks
            .0
            .push((pos.0, mark(' ', if on { RED } else { WHITE })));
    }
    for (pos, glyph) in &popups {
        marks.0.push((pos.0, mark(glyph.ch, glyph.color)));
    }
}

/// Hearts in a row above whatever shows its health, on screen: red for
/// each hit point left, grey for each lost. Hearts go on free floor only,
/// never on whoever stands there. The player's own show in the HUD, where
/// nothing stands in their way.
#[allow(clippy::type_complexity)]
pub fn mark_health(
    grid: Res<MapGrid>,
    camera: Option<Res<ViewCamera>>,
    marks: Option<ResMut<CellMarks>>,
    shown: Query<(&Pos, &Health), (With<ShowHealth>, Without<Player>)>,
) {
    let Some(mut marks) = marks else {
        return;
    };
    let (up, right) = camera.map_or((IVec2::NEG_Y, IVec2::X), |camera| {
        (
            camera.map_direction(IVec2::NEG_Y),
            camera.map_direction(IVec2::X),
        )
    });
    for (pos, health) in &shown {
        let first = -(health.max - 1) / 2;
        for i in 0..health.max.max(0) {
            let cell = grid.safe_pos(pos.0 + up + right * (first + i));
            if grid.get(cell).is_some() {
                continue;
            }
            let heart = if i < health.hp {
                mark('♥', RED)
            } else {
                mark('♡', DARK_GRAY)
            };
            marks.0.push((cell, heart));
        }
    }
}
