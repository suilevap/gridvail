//! How a room's walls get from one layout to the next: replaceable
//! strategies ([`RoomPlanner`]) that turn a switch into a [`SwitchPlan`].
//!
//! `transform_rooms` asks the room's planner for a plan, checks it and
//! carries it out; it does not know how plans are made. To try another
//! strategy, implement [`RoomPlanner`] and add it to [`RoomPlanners`] under
//! a name. A room uses the planner its map names (`@room <x> <y> <name>`),
//! or the default one, so several strategies can run side by side in one
//! map. Each strategy lives in its own file here.

mod along_walls;
mod lines;
mod rise_sink;

use std::collections::HashSet;
use std::sync::Arc;

use bevy::prelude::*;

use crate::model::*;

pub use along_walls::AlongWalls;
pub use lines::LineSlides;
pub use rise_sink::RiseAndSink;

/// What happens at the ends of a [`WallMove`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveKind {
    /// A wall on the map slides from its cell to its new one.
    Slide,
    /// A new wall rises out of the floor at `from`, then slides to `to`,
    /// where it is in the world from the start. `from == to` rises in place.
    Rise,
    /// A wall that goes slides from its cell to `to`, then sinks there; it
    /// is out of the world from the start, so `to` may be anywhere (inside
    /// another wall, say). `from == to` sinks in place.
    Sink,
}

/// One wall's part in a switch: every wall slides, from `from` to `to`, and
/// `kind` says whether it rises first or sinks after.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WallMove {
    pub from: IVec2,
    pub to: IVec2,
    pub kind: MoveKind,
}

impl WallMove {
    pub fn slide(from: IVec2, to: IVec2) -> Self {
        Self {
            from,
            to,
            kind: MoveKind::Slide,
        }
    }

    /// A new wall that rises at `from` and slides to its cell, `to`.
    pub fn rise(from: IVec2, to: IVec2) -> Self {
        Self {
            from,
            to,
            kind: MoveKind::Rise,
        }
    }

    /// A wall that goes: it slides from its cell, `from`, to `to` and sinks.
    pub fn sink(from: IVec2, to: IVec2) -> Self {
        Self {
            from,
            to,
            kind: MoveKind::Sink,
        }
    }

    pub fn rise_at(at: IVec2) -> Self {
        Self::rise(at, at)
    }

    pub fn sink_at(at: IVec2) -> Self {
        Self::sink(at, at)
    }

    /// The move from `from` to `to`, its kind read from the layouts: a wall
    /// of the old layout slides, and sinks after if `to` is not in the new
    /// one; anything else rises first. It cannot tell a new wall coming out
    /// of a wall that stays from that wall moving: say so with
    /// [`WallMove::rise`] or [`WallMove::sink`].
    pub fn between(switch: &SwitchContext, from: IVec2, to: IVec2) -> Self {
        match (switch.from.contains(&from), switch.to.contains(&to)) {
            (true, true) => Self::slide(from, to),
            (true, false) => Self::sink(from, to),
            (false, _) => Self::rise(from, to),
        }
    }

    /// The map cell a wall leaves, if it is on the map before.
    pub fn source(&self) -> Option<IVec2> {
        (self.kind != MoveKind::Rise).then_some(self.from)
    }

    /// The map cell a wall takes, if it is on the map after.
    pub fn target(&self) -> Option<IVec2> {
        (self.kind != MoveKind::Sink).then_some(self.to)
    }
}

/// How a room's walls get from one layout to the next. The moves happen
/// together: every wall leaves its old cell before any reaches its new one,
/// so walls can slide into each other's places.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SwitchPlan {
    pub moves: Vec<WallMove>,
}

impl SwitchPlan {
    /// Every wall that goes sinks and every new one rises, in place: always
    /// valid.
    pub fn rise_and_sink(switch: &SwitchContext) -> Self {
        Self {
            moves: switch
                .removed()
                .map(WallMove::sink_at)
                .chain(switch.added().map(WallMove::rise_at))
                .collect(),
        }
    }

    /// Takes `candidates` best first (lowest rank), each
    /// only if none of its cells is part of one already taken. What is left
    /// rises or sinks in place.
    pub(crate) fn from_candidates(switch: &SwitchContext, mut candidates: Vec<Candidate>) -> Self {
        candidates.sort_by_key(|candidate| candidate.rank);
        let mut plan = Self::default();
        let mut used: HashSet<IVec2> = HashSet::new();
        for candidate in candidates {
            if candidate.cells.iter().any(|p| used.contains(p)) {
                continue;
            }
            used.extend(&candidate.cells);
            plan.moves.extend(
                candidate
                    .moves
                    .into_iter()
                    .map(|(from, to)| WallMove::slide(from, to)),
            );
        }
        let to: HashSet<IVec2> = switch.to.iter().copied().collect();
        let from: HashSet<IVec2> = switch.from.iter().copied().collect();
        plan.moves.extend(
            switch
                .removed_in(&to)
                .filter(|p| !used.contains(p))
                .map(WallMove::sink_at),
        );
        plan.moves.extend(
            switch
                .added_in(&from)
                .filter(|p| !used.contains(p))
                .map(WallMove::rise_at),
        );
        plan
    }

    /// Map cells walls leave: where walls on the map slide or sink from.
    pub fn sources(&self) -> impl Iterator<Item = IVec2> + '_ {
        self.moves.iter().filter_map(WallMove::source)
    }

    /// Map cells walls take: where they slide or rise to.
    pub fn targets(&self) -> impl Iterator<Item = IVec2> + '_ {
        self.moves.iter().filter_map(WallMove::target)
    }

    /// Whether carrying the plan out turns the old layout into the new one:
    /// walls leave only cells that hold one (each once), reach each cell
    /// once, and what stays plus what arrives is exactly the new layout.
    /// Where a rising wall starts and a sinking one ends are only shown, so
    /// they may be anywhere.
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

/// One way some walls could slide, for [`SwitchPlan::from_candidates`]:
/// the cells it takes up (no other candidate taken may touch them), the
/// slides, and its rank (lower is taken first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    /// Lower is better, compared in order: a planner's own measures (a
    /// tier, a cost), then a length.
    pub rank: (i32, i32, i32),
    pub cells: Vec<IVec2>,
    pub moves: Vec<(IVec2, IVec2)>,
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
    rail: &'a dyn Fn(IVec2, IVec2) -> bool,
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
            rail: &|_, _| false,
        }
    }

    /// Where the floor has rails (`Rail`): whether a cell has one running
    /// along a step. None without this.
    pub fn with_rails(mut self, rail: &'a dyn Fn(IVec2, IVec2) -> bool) -> Self {
        self.rail = rail;
        self
    }

    /// Wall cells the new layout loses.
    pub fn removed(&self) -> impl Iterator<Item = IVec2> + '_ {
        self.from.iter().copied().filter(|p| !self.to.contains(p))
    }

    /// Wall cells the new layout gains.
    pub fn added(&self) -> impl Iterator<Item = IVec2> + '_ {
        self.to.iter().copied().filter(|p| !self.from.contains(p))
    }

    /// [`Self::removed`], with the new layout as a set (for big rooms).
    pub fn removed_in<'s>(&'s self, to: &'s HashSet<IVec2>) -> impl Iterator<Item = IVec2> + 's {
        self.from.iter().copied().filter(|p| !to.contains(p))
    }

    /// [`Self::added`], with the old layout as a set (for big rooms).
    pub fn added_in<'s>(&'s self, from: &'s HashSet<IVec2>) -> impl Iterator<Item = IVec2> + 's {
        self.to.iter().copied().filter(|p| !from.contains(p))
    }

    /// Whether nothing stands at `p`.
    pub fn is_free(&self, p: IVec2) -> bool {
        (self.free)(p)
    }

    /// Whether the floor at `p` has a rail running along `step`.
    pub fn has_rail(&self, p: IVec2, step: IVec2) -> bool {
        (self.rail)(p, step)
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
    /// The name of [`AlongWalls`].
    pub const ALONG_WALLS: &'static str = "along-walls";
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
    /// [`AlongWalls`] (the default), [`LineSlides`] and [`RiseAndSink`].
    fn default() -> Self {
        let mut planners = Self::empty(Self::ALONG_WALLS);
        planners
            .add(Self::LINES, LineSlides)
            .add(Self::ALONG_WALLS, AlongWalls)
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

    fn check(from: &[(i32, i32)], to: &[(i32, i32)], moves: &[WallMove]) -> Result<(), String> {
        let (from, to) = (cells(from), cells(to));
        let room = Room::new(IVec2::ZERO, IVec2::splat(8), vec![]);
        let wall = |p: IVec2| from.contains(&p);
        let switch = SwitchContext::new(&room, &from, &to, &|_| true, &wall);
        SwitchPlan {
            moves: moves.to_vec(),
        }
        .check(&switch)
    }

    #[test]
    fn plans_are_checked_against_both_layouts() {
        let (a, b, c) = (IVec2::new(0, 0), IVec2::new(1, 0), IVec2::new(2, 0));
        let slide = WallMove::slide;
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
            check(&[(0, 0)], &[(1, 0)], &[slide(a, b), WallMove::sink_at(a)]).is_err(),
            "one wall leaves twice"
        );
        assert!(
            check(&[(0, 0)], &[(0, 0), (1, 0)], &[WallMove::rise_at(a)]).is_err(),
            "rises onto a wall that stays"
        );
        assert!(
            check(&[(0, 0)], &[(1, 0)], &[slide(a, c)]).is_err(),
            "ends outside the new layout"
        );
        // Where a rising wall starts and a sinking one ends are only shown:
        // here out of and into the wall at (0, 0), which stays.
        assert!(check(&[(0, 0)], &[(0, 0), (1, 0)], &[WallMove::rise(a, b)]).is_ok());
        assert!(check(&[(0, 0), (1, 0)], &[(0, 0)], &[WallMove::sink(b, a)]).is_ok());
    }

    #[test]
    fn moves_read_their_kind_from_the_layouts() {
        let (from, to) = (cells(&[(0, 0), (1, 0)]), cells(&[(1, 0), (2, 0)]));
        let room = Room::new(IVec2::ZERO, IVec2::splat(4), vec![]);
        let wall = |p: IVec2| from.contains(&p);
        let switch = SwitchContext::new(&room, &from, &to, &|_| true, &wall);
        let kind = |a: (i32, i32), b: (i32, i32)| {
            WallMove::between(&switch, IVec2::from(a), IVec2::from(b)).kind
        };
        assert_eq!(
            kind((0, 0), (2, 0)),
            MoveKind::Slide,
            "old wall to a new cell"
        );
        assert_eq!(
            kind((0, 0), (0, 3)),
            MoveKind::Sink,
            "old wall out of the room"
        );
        assert_eq!(
            kind((0, 3), (2, 0)),
            MoveKind::Rise,
            "from nowhere to a new cell"
        );
        assert_eq!(kind((0, 0), (0, 0)), MoveKind::Sink, "sinking in place");
        assert_eq!(kind((2, 0), (2, 0)), MoveKind::Rise, "rising in place");
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
                .moves
                .iter()
                .any(|step| step.kind == MoveKind::Slide)
        };
        assert!(slides(&room), "the default slides");
        room.planner = Some(RoomPlanners::RISE_SINK.into());
        assert!(!slides(&room), "its own rises and sinks");
        room.planner = Some("nobody".into());
        assert!(!slides(&room), "an unknown one rises and sinks");
    }
}
