//! Children shown relative to their animated parent.
//!
//! A child (`BoundTo`, such as the player's direction marker) is an ordinary
//! animated object whose motion plays in its parent's frame: its
//! `ObjectAnimation` follows its offset from the parent (`rotate(offset,
//! facing)`, as the simulation places it) instead of its cell, with the same
//! shared step as every other object. It is shown at its parent's shown
//! position plus that animated offset, so everything the parent does on
//! screen (gliding, overshooting, hopping, bumping, teleporting across the
//! map edge) carries over, through chains of any depth.
//!
//! By default the offset moves along [`Path::Orbit`] around the parent, so a
//! turn sweeps the child around it along the shorter arc instead of cutting
//! across or through it, in the child's style made [`MotionStyle::steady`].
//! A `MovePath` on the child overrides the path.
//!
//! No glyph rotates: renderers still draw a glyph at a position, so text
//! terminals are unaffected.

use bevy::prelude::*;

use crate::model::*;

use super::plugin::{facing_of, follow, style_of};
use super::{MotionStyle, MovePath, ObjectAnimation, ObjectMotion, Path};

/// Longest `BoundTo` chain followed when ordering parents before children.
const MAX_DEPTH: u8 = 8;

/// Places every child at its parent's shown position plus its animated
/// offset. Parents are placed before their children.
#[allow(clippy::type_complexity)]
pub fn animate_children(
    time: Res<Time>,
    style: Res<MotionStyle>,
    mut order: Local<Vec<(u8, Entity)>>,
    chain: Query<&BoundTo>,
    facings: Query<&Facing>,
    mut children: Query<(
        Entity,
        &BoundTo,
        Option<&ObjectMotion>,
        Option<&MovePath>,
        &mut ObjectAnimation,
    )>,
    mut shown: Query<&mut AnimatedPos>,
) {
    let dt = time.delta_secs();
    order.clear();
    for (entity, bound, ..) in &children {
        let mut depth = 0;
        let mut parent = bound.parent;
        while depth < MAX_DEPTH {
            let Ok(next) = chain.get(parent) else { break };
            parent = next.parent;
            depth += 1;
        }
        order.push((depth, entity));
    }
    order.sort_unstable();

    for &(_, entity) in order.iter() {
        let Ok((_, bound, own_style, path, mut animation)) = children.get_mut(entity) else {
            continue;
        };
        let Ok(parent) = shown.get(bound.parent).copied() else {
            continue;
        };
        let offset = rotate(bound.offset, facing_of(bound.parent, &facings));
        let path = path.map_or(Path::Orbit { center: Vec2::ZERO }, |path| path.0);
        follow(
            &mut animation.state,
            offset.as_vec2(),
            Some(path),
            style_of(own_style, *style).steady(),
            |_, _| false,
            dt,
        );
        if let Ok(mut shown) = shown.get_mut(entity) {
            shown.set_if_neq(AnimatedPos {
                position: parent.position + animation.state.position,
                lift: parent.lift + animation.state.lift,
            });
        }
    }
}
