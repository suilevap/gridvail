//! Planning paths away from the frame, for walkers with a [`Planning`]
//! component when [`ServiceMode`] is not `Inline`.
//!
//! A plan is a service [`Job`] over shared copies of the map ([`SharedNav`]),
//! so it may take as many frames as it needs. Until it lands the walker keeps
//! to its old path when that still leads near the goal, or waits, with
//! [`WalkStatus::Planning`] reported to whoever set its destination. Path
//! buffers move between the walker and its job instead of being reallocated.

use std::cell::RefCell;
use std::sync::Arc;

use bevy::prelude::*;

use crate::model::*;
use crate::navigation::{cell_of, Crowd, NavMap, OccupancyMap, PathPlanner, Terrain};
use crate::service::{Job, ServiceMode, Ticket};

/// Copies of the map that planning jobs share: the nav map, retaken when
/// walls or doors change, and occupancy, refreshed in place each frame unless
/// a job still holds the previous copy.
#[derive(Resource, Default)]
pub struct SharedNav {
    pub map: Arc<NavMap>,
    pub occupancy: Arc<OccupancyMap>,
}

/// Keeps [`SharedNav`] current. Only runs when plans leave the frame.
pub fn share_nav(nav: Res<NavMap>, grid: Res<MapGrid>, mut shared: ResMut<SharedNav>) {
    if shared.map.revision() != nav.revision() {
        shared.map = Arc::new(nav.clone());
    }
    Arc::make_mut(&mut shared.occupancy).refresh(&grid);
}

pub fn plans_leave_the_frame(mode: Res<ServiceMode>) -> bool {
    *mode != ServiceMode::Inline
}

/// A walker's plan on its way, or landed and not yet taken.
#[derive(Component, Default)]
pub struct Planning {
    ticket: Option<Ticket<PlanResult>>,
    landed: Option<PlanResult>,
    /// The buffer the next plan fills.
    spare: Vec<IVec2>,
}

impl Planning {
    /// Room for paths up to `cells` long, so planning never allocates.
    pub fn with_capacity(cells: usize) -> Self {
        Self {
            spare: Vec::with_capacity(cells),
            ..Self::default()
        }
    }

    pub fn is_planning(&self) -> bool {
        self.ticket.is_some()
    }

    /// Drops a plan on its way (cancelling its job) or landed.
    pub fn cancel(&mut self) {
        self.ticket = None;
        if let Some(result) = self.landed.take() {
            self.spare = result.path;
        }
    }

    pub(super) fn start(&mut self, request: PlanRequest, shared: &SharedNav, mode: ServiceMode) {
        let job = PlanJob {
            request,
            nav: shared.map.clone(),
            occupancy: shared.occupancy.clone(),
            path: std::mem::take(&mut self.spare),
        };
        self.ticket = Some(Ticket::start(job, mode));
    }

    pub(super) fn take_landed(&mut self) -> Option<PlanResult> {
        self.landed.take()
    }

    pub(super) fn recycle(&mut self, path: Vec<IVec2>) {
        self.spare = path;
    }
}

/// What a plan is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PlanRequest {
    pub from: IVec2,
    pub goal: IVec2,
    /// The destination request and door cost, as in `PathFollow::planned_for`.
    pub wanted: Option<(u32, Option<u32>)>,
    pub door_cost: Option<u32>,
    pub crowd_cost: Option<u32>,
}

#[derive(Default)]
pub(super) struct PlanResult {
    pub request: PlanRequest,
    /// From `request.from` to `request.goal`; empty when there is no way.
    pub path: Vec<IVec2>,
}

struct PlanJob {
    request: PlanRequest,
    nav: Arc<NavMap>,
    occupancy: Arc<OccupancyMap>,
    path: Vec<IVec2>,
}

thread_local! {
    static PLANNER: RefCell<PathPlanner> = RefCell::default();
}

impl Job for PlanJob {
    type Output = PlanResult;

    fn run(mut self) -> PlanResult {
        let PlanRequest {
            from,
            goal,
            door_cost,
            crowd_cost,
            ..
        } = self.request;
        let terrain = Terrain {
            nav: &self.nav,
            door_cost,
        };
        let found = PLANNER.with_borrow_mut(|planner| match crowd_cost {
            None => planner.plan_with(&self.nav, &terrain, from, goal, &mut self.path),
            Some(cost) => {
                let crowd = Crowd {
                    terrain,
                    occupied: &*self.occupancy,
                    goal: cell_of(goal),
                    cost,
                };
                planner.plan_with(&self.nav, &crowd, from, goal, &mut self.path)
            }
        });
        if !found {
            self.path.clear();
        }
        PlanResult {
            request: self.request,
            path: self.path,
        }
    }
}

/// Picks up finished plans on whichever frame they finish; the walker takes
/// one on its next turn.
pub fn collect_plans(mut walkers: Query<&mut Planning>) {
    for mut planning in &mut walkers {
        let Some(ticket) = planning.ticket.as_mut() else {
            continue;
        };
        if let Some(result) = ticket.poll() {
            planning.ticket = None;
            if let Some(stale) = planning.landed.replace(result) {
                planning.spare = stale.path;
            }
        }
    }
}
