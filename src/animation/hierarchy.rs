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
//! When the root of a chain steps through a portal that turns, every offset
//! in the chain is turned with it, so the child stays where it was shown.
//!
//! No glyph rotates: renderers still draw a glyph at a position, so text
//! terminals are unaffected.

use bevy::prelude::*;

use crate::model::*;

use super::plugin::{facing_of, follow, style_of};
use super::{MotionStyle, MovePath, ObjectAnimation, ObjectMotion, Path};
use crate::foundation::portal::CellTransform;

/// Longest `BoundTo` chain followed when ordering parents before children.
const MAX_DEPTH: u8 = 8;

/// Places every child at its parent's shown position plus its animated
/// offset. Parents are placed before their children.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn animate_children(
    time: Res<Time>,
    style: Res<MotionStyle>,
    crossings: Option<Res<PortalCrossings>>,
    mut order: Local<Vec<(u8, Entity)>>,
    chain: Query<&BoundTo>,
    positions: Query<&Pos>,
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
        if let Some(turn) = crossings
            .as_ref()
            .and_then(|crossings| root_crossing(entity, &chain, &positions, crossings))
        {
            animation.state.carry(&turn);
        }
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

/// The turn of the portal the root of `child`'s chain stepped through this
/// frame, if it turns: offsets turn with it but do not move.
fn root_crossing(
    child: Entity,
    chain: &Query<&BoundTo>,
    positions: &Query<&Pos>,
    crossings: &PortalCrossings,
) -> Option<CellTransform> {
    let mut root = chain.get(child).ok()?.parent;
    for _ in 0..MAX_DEPTH {
        let Ok(next) = chain.get(root) else { break };
        root = next.parent;
    }
    let through = crossings.arrived(root, positions.get(root).ok()?.0)?;
    (through.quarters != 0).then_some(CellTransform {
        quarters: through.quarters,
        offset: IVec2::ZERO,
    })
}
