use bevy::prelude::*;

use crate::animation::NonBlockingAnimation;
use crate::lighting::WHITE;
use crate::model::*;

use super::mark;

/// How far a target recoils from a blow, in cells.
const RECOIL: f32 = 0.3;
/// How long a struck target flashes, in seconds.
const FLASH_SECS: f32 = 0.25;
/// How long a damage number shows, in seconds.
const POPUP_SECS: f32 = 0.8;
/// Damage numbers' colour: apart from the red of danger.
pub const DAMAGE_COLOR: Rgb = Rgb::new(255, 150, 40);

/// Flashing white for a moment, having just been hit.
#[derive(Component, Debug)]
pub struct Flash(pub Timer);

/// A damage number shown for a moment above what took the damage.
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

/// Shows the damage each thing took this frame, all of it in one number,
/// on the cell above it on screen.
pub fn pop_up_damage(
    mut commands: Commands,
    grid: Res<MapGrid>,
    camera: Option<Res<ViewCamera>>,
    mut damage: MessageReader<Damage>,
    positions: Query<&Pos>,
    mut totals: Local<Vec<(Entity, i32)>>,
) {
    totals.clear();
    for hurt in damage.read().filter(|hurt| hurt.amount > 0) {
        match totals.iter_mut().find(|(target, _)| *target == hurt.target) {
            Some((_, total)) => *total += hurt.amount,
            None => totals.push((hurt.target, hurt.amount)),
        }
    }
    let up = camera.map_or(IVec2::NEG_Y, |camera| camera.map_direction(IVec2::NEG_Y));
    for &(target, total) in totals.iter() {
        let Ok(pos) = positions.get(target) else {
            continue;
        };
        let digit = char::from_digit(total.clamp(1, 9) as u32, 10).unwrap_or('9');
        commands.spawn((
            Popup(Timer::from_seconds(POPUP_SECS, TimerMode::Once)),
            NonBlockingAnimation,
            Pos(grid.safe_pos(pos.0 + up)),
            Glyph::new(digit, 3, DAMAGE_COLOR),
        ));
    }
}

/// Flashes and popups wear off.
pub fn wear_off(
    mut commands: Commands,
    time: Res<Time>,
    mut flashes: Query<(Entity, &mut Flash)>,
    mut popups: Query<(Entity, &mut Popup)>,
) {
    for (entity, mut flash) in &mut flashes {
        if flash.0.tick(time.delta()).is_finished() {
            commands.entity(entity).remove::<Flash>();
        }
    }
    for (entity, mut popup) in &mut popups {
        if popup.0.tick(time.delta()).is_finished() {
            commands.entity(entity).despawn();
        }
    }
}

/// Flashing targets show white; damage numbers keep their own colour.
pub fn mark_hits(
    marks: Option<ResMut<CellMarks>>,
    flashes: Query<&Pos, With<Flash>>,
    popups: Query<(&Pos, &Glyph), With<Popup>>,
) {
    let Some(mut marks) = marks else {
        return;
    };
    for pos in &flashes {
        marks.0.push((pos.0, mark(' ', WHITE)));
    }
    for (pos, glyph) in &popups {
        marks.0.push((pos.0, mark(glyph.ch, glyph.color)));
    }
}
