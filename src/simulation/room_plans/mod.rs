//! How a room's walls get from one layout to the next: replaceable
//! strategies ([`RoomPlanner`]) that turn a switch into a [`SwitchPlan`].
//!
//! `transform_rooms` asks the room's planner for a plan, checks it and
//! carries it out; it does not know how plans are made. To try another
//! strategy, implement [`RoomPlanner`] and add it to [`RoomPlanners`] under
//! a name. A room uses the planner its map names (`@room <x> <y> <name>`),
//! or the default one, so several strategies can run side by side in one
//! map. Each strategy lives in its own file here.

mod lines;
mod rise_sink;

use std::sync::Arc;

use bevy::prelude::*;

use crate::model::*;

pub use lines::LineSlides;
pub use rise_sink::RiseAndSink;

/// One wall's part in a switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallChange {
    /// A wall slides from one cell to another, keeping its entity.
    Slide { from: IVec2, to: IVec2 },
    /// A new wall rises out of the floor at its cell.
    Rise { at: IVec2 },
    /// A wall sinks into the floor at its cell and goes.
    Sink { at: IVec2 },
}

/// How a room's walls get from one layout to the next. The changes happen
/// together: every wall leaves its old cell before any reaches its new one,
/// so walls can slide into each other's places.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SwitchPlan {
    pub changes: Vec<WallChange>,
}

impl SwitchPlan {
    /// Every wall that goes sinks and every new one rises: always valid.
    pub fn rise_and_sink(switch: &SwitchContext) -> Self {
        Self {
            changes: switch
                .removed()
                .map(|at| WallChange::Sink { at })
                .chain(switch.added().map(|at| WallChange::Rise { at }))
                .collect(),
        }
    }

    /// Cells walls leave: where they slide from or sink.
    pub fn sources(&self) -> impl Iterator<Item = IVec2> + '_ {
        self.changes.iter().filter_map(|change| match *change {
            WallChange::Slide { from, .. } => Some(from),
            WallChange::Sink { at } => Some(at),
            WallChange::Rise { .. } => None,
        })
    }

    /// Cells walls reach: where they slide to or rise.
    pub fn targets(&self) -> impl Iterator<Item = IVec2> + '_ {
        self.changes.iter().filter_map(|change| match *change {
            WallChange::Slide { to, .. } => Some(to),
            WallChange::Rise { at } => Some(at),
            WallChange::Sink { .. } => None,
        })
    }

    /// Whether carrying the plan out turns the old layout into the new one:
    /// walls leave only cells that hold one (each once), reach each cell
    /// once, and what stays plus what arrives is exactly the new layout.
    pub fn check(&self, switch: &SwitchContext) -> Result<(), String> {
        let mut left = Vec::new();
        for source in self.sources() {
            if !switch.from.contains(&source) || !switch.holds_wall(source) {
                return Err(format!("no wall leaves {source}"));
            }
            if left.contains(&source) {
                return Err(format!("two walls leave {source}"));
            }
            left.push(source);
        }
        let mut walls: Vec<IVec2> = switch
            .from
            .iter()
            .copied()
            .filter(|p| !left.contains(p))
            .collect();
        for target in self.targets() {
            if walls.contains(&target) {
                return Err(format!("two walls reach {target}"));
            }
            walls.push(target);
        }
        if let Some(p) = walls.iter().find(|p| !switch.to.contains(p)) {
            return Err(format!("a wall ends at {p}, not in the new layout"));
        }
        if let Some(p) = switch.to.iter().find(|p| !walls.contains(p)) {
            return Err(format!("no wall ends at {p}"));
        }
        Ok(())
    }
}

/// What a planner sees of a switch.
pub struct SwitchContext<'a> {
    pub room: &'a Room,
    /// The walls of the layout on the map.
    pub from: &'a [IVec2],
    /// The walls of the layout asked for.
    pub to: &'a [IVec2],
    free: &'a dyn Fn(IVec2) -> bool,
    wall: &'a dyn Fn(IVec2) -> bool,
}

impl<'a> SwitchContext<'a> {
    pub fn new(
        room: &'a Room,
        from: &'a [IVec2],
        to: &'a [IVec2],
        free: &'a dyn Fn(IVec2) -> bool,
        wall: &'a dyn Fn(IVec2) -> bool,
    ) -> Self {
        Self {
            room,
            from,
            to,
            free,
            wall,
        }
    }

    /// Wall cells the new layout loses.
    pub fn removed(&self) -> impl Iterator<Item = IVec2> + '_ {
        self.from.iter().copied().filter(|p| !self.to.contains(p))
    }

    /// Wall cells the new layout gains.
    pub fn added(&self) -> impl Iterator<Item = IVec2> + '_ {
        self.to.iter().copied().filter(|p| !self.from.contains(p))
    }

    /// Whether nothing stands at `p`.
    pub fn is_free(&self, p: IVec2) -> bool {
        (self.free)(p)
    }

    /// Whether a wall entity stands at `p` (only those can slide or sink).
    pub fn holds_wall(&self, p: IVec2) -> bool {
        (self.wall)(p)
    }
}

/// A strategy for how a room's walls get from one layout to the next. A
/// plan that fails [`SwitchPlan::check`] is replaced by rising and sinking.
pub trait RoomPlanner: Send + Sync + 'static {
    fn plan(&self, switch: &SwitchContext) -> SwitchPlan;
}

/// The planners rooms can use, by name, and the one used when a room names
/// none. Add your own before the game starts (or at any time) to try it.
#[derive(Resource, Clone)]
pub struct RoomPlanners {
    planners: Vec<(String, Arc<dyn RoomPlanner>)>,
    /// The planner of rooms that name none.
    pub default: String,
}

impl RoomPlanners {
    /// The name of [`LineSlides`].
    pub const LINES: &'static str = "lines";
    /// The name of [`RiseAndSink`].
    pub const RISE_SINK: &'static str = "rise-sink";

    /// No planners; rooms rise and sink.
    pub fn empty(default: &str) -> Self {
        Self {
            planners: Vec::new(),
            default: default.to_owned(),
        }
    }

    /// Uses the planner named `name` for rooms that name none.
    pub fn with_default(mut self, name: &str) -> Self {
        self.default = name.to_owned();
        self
    }

    /// Adds `planner` under `name`, replacing any planner of that name.
    pub fn add(&mut self, name: &str, planner: impl RoomPlanner) -> &mut Self {
        self.planners.retain(|(known, _)| known != name);
        self.planners.push((name.to_owned(), Arc::new(planner)));
        self
    }

    pub fn get(&self, name: &str) -> Option<&dyn RoomPlanner> {
        self.planners
            .iter()
            .find(|(known, _)| known == name)
            .map(|(_, planner)| planner.as_ref())
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.planners.iter().map(|(name, _)| name.as_str())
    }

    /// The planner `room` names, else the default one, else rising and
    /// sinking. A name nobody added is reported once per switch.
    pub fn for_room(&self, room: &Room) -> &dyn RoomPlanner {
        let name = room.planner.as_deref().unwrap_or(&self.default);
        self.get(name).unwrap_or_else(|| {
            warn!("no room planner named `{name}`; walls rise and sink");
            &RiseAndSink
        })
    }
}

impl Default for RoomPlanners {
    /// [`LineSlides`] (the default) and [`RiseAndSink`].
    fn default() -> Self {
        let mut planners = Self::empty(Self::LINES);
        planners
            .add(Self::LINES, LineSlides)
            .add(Self::RISE_SINK, RiseAndSink);
        planners
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn cells(list: &[(i32, i32)]) -> Vec<IVec2> {
        list.iter().map(|&(x, y)| IVec2::new(x, y)).collect()
    }

    /// Plans with `planner`, with every cell free and every old wall cell
    /// holding a wall, and checks the plan.
    pub(super) fn plan_with(
        planner: &dyn RoomPlanner,
        from: &[(i32, i32)],
        to: &[(i32, i32)],
    ) -> SwitchPlan {
        let (from, to) = (cells(from), cells(to));
        let room = Room::new(IVec2::ZERO, IVec2::splat(8), vec![from.clone(), to.clone()]);
        let wall = |p: IVec2| from.contains(&p);
        let switch = SwitchContext::new(&room, &from, &to, &|_| true, &wall);
        let plan = planner.plan(&switch);
        plan.check(&switch).unwrap();
        plan
    }

    fn check(from: &[(i32, i32)], to: &[(i32, i32)], changes: &[WallChange]) -> Result<(), String> {
        let (from, to) = (cells(from), cells(to));
        let room = Room::new(IVec2::ZERO, IVec2::splat(8), vec![]);
        let wall = |p: IVec2| from.contains(&p);
        let switch = SwitchContext::new(&room, &from, &to, &|_| true, &wall);
        SwitchPlan {
            changes: changes.to_vec(),
        }
        .check(&switch)
    }

    #[test]
    fn plans_are_checked_against_both_layouts() {
        let (a, b, c) = (IVec2::new(0, 0), IVec2::new(1, 0), IVec2::new(2, 0));
        let slide = |from, to| WallChange::Slide { from, to };
        // Walls trading places along a line.
        assert!(check(
            &[(0, 0), (1, 0)],
            &[(1, 0), (2, 0)],
            &[slide(b, c), slide(a, b)]
        )
        .is_ok());
        assert!(check(&[(0, 0)], &[(1, 0)], &[]).is_err(), "nothing done");
        assert!(
            check(&[(0, 0)], &[(1, 0)], &[slide(c, b)]).is_err(),
            "slides from no wall"
        );
        assert!(
            check(
                &[(0, 0)],
                &[(1, 0)],
                &[slide(a, b), WallChange::Sink { at: a }]
            )
            .is_err(),
            "one wall leaves twice"
        );
        assert!(
            check(&[(0, 0)], &[(0, 0), (1, 0)], &[WallChange::Rise { at: a }]).is_err(),
            "rises onto a wall that stays"
        );
        assert!(
            check(&[(0, 0)], &[(1, 0)], &[slide(a, c)]).is_err(),
            "ends outside the new layout"
        );
    }

    #[test]
    fn rooms_use_their_own_planner_or_the_default() {
        let planners = RoomPlanners::default();
        let from = cells(&[(0, 0)]);
        let to = cells(&[(1, 0)]);
        let mut room = Room::new(IVec2::ZERO, IVec2::splat(2), vec![from.clone(), to.clone()]);
        let wall = |p: IVec2| from.contains(&p);
        let slides = |room: &Room| {
            let switch = SwitchContext::new(room, &from, &to, &|_| true, &wall);
            planners
                .for_room(room)
                .plan(&switch)
                .changes
                .iter()
                .any(|change| matches!(change, WallChange::Slide { .. }))
        };
        assert!(slides(&room), "the default slides");
        room.planner = Some(RoomPlanners::RISE_SINK.into());
        assert!(!slides(&room), "its own rises and sinks");
        room.planner = Some("nobody".into());
        assert!(!slides(&room), "an unknown one rises and sinks");
    }
}
