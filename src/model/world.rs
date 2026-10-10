use bevy::prelude::*;

use crate::foundation::portal::{CellTransform, PortalFace};

pub const TOKEN_RECHARGE_SECS: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionEvent {
    pub source: Entity,
    pub target: Entity,
}

#[derive(Resource, Debug, Default)]
pub struct CollisionBuffer(pub Vec<CollisionEvent>);

/// A step through a portal: `entity` stepped into a portal face and
/// should arrive at `arrival`, the floor in front of the exit. `through`
/// takes positions in front of the portal to where they continue beyond
/// its exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortalCrossing {
    pub entity: Entity,
    pub through: CellTransform,
    pub arrival: IVec2,
}

/// The steps through portals of the current simulation pass. A step can
/// still be blocked at the exit, so a crossing happened only if the entity
/// now stands on its `arrival`.
#[derive(Resource, Debug, Default)]
pub struct PortalCrossings(pub Vec<PortalCrossing>);

impl PortalCrossings {
    /// The crossing `entity` made this pass, if it arrived.
    pub fn arrived(&self, entity: Entity, at: IVec2) -> Option<CellTransform> {
        self.0
            .iter()
            .find(|crossing| crossing.entity == entity && crossing.arrival == at)
            .map(|crossing| crossing.through)
    }
}

#[derive(Resource, Debug)]
pub struct MapGrid {
    pub width: i32,
    pub height: i32,
    pub revision: u64,
    pub blocker_revision: u64,
    /// Bumped whenever a portal face appears, disappears or changes, so
    /// views through portals are recomputed.
    pub portal_revision: u64,
    cells: Vec<Option<MapOccupant>>,
    portals: Vec<Option<PortalFace>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MapOccupant {
    entity: Entity,
    blocks_vision: bool,
}

impl MapGrid {
    pub fn new(width: i32, height: i32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        Self {
            width,
            height,
            revision: 0,
            blocker_revision: 0,
            portal_revision: 0,
            cells: vec![None; (width * height) as usize],
            portals: vec![None; (width * height) as usize],
        }
    }

    pub fn is_valid(&self, p: IVec2) -> bool {
        p.x >= 0 && p.y >= 0 && p.x < self.width && p.y < self.height
    }

    pub fn safe_pos(&self, p: IVec2) -> IVec2 {
        IVec2::new(p.x.rem_euclid(self.width), p.y.rem_euclid(self.height))
    }

    pub fn get(&self, p: IVec2) -> Option<Entity> {
        self.idx(p)
            .and_then(|i| self.cells[i].map(|occupant| occupant.entity))
    }

    pub fn set(&mut self, p: IVec2, entity: Entity) {
        self.set_with_blocking(p, entity, true);
    }

    pub fn set_with_blocking(&mut self, p: IVec2, entity: Entity, blocks_vision: bool) {
        let Some(i) = self.idx(p) else { return };
        let next = Some(MapOccupant {
            entity,
            blocks_vision,
        });
        if self.cells[i] != next {
            let old_blocks = self.cells[i].is_some_and(|cell| cell.blocks_vision);
            self.cells[i] = next;
            self.revision += 1;
            if old_blocks != blocks_vision {
                self.blocker_revision += 1;
            }
        }
    }

    pub fn clear(&mut self, p: IVec2) {
        let Some(i) = self.idx(p) else { return };
        if let Some(old) = self.cells[i].take() {
            self.revision += 1;
            if old.blocks_vision {
                self.blocker_revision += 1;
            }
        }
    }

    pub fn blocks_vision(&self, p: IVec2) -> bool {
        self.idx(p)
            .and_then(|i| self.cells[i])
            .is_some_and(|cell| cell.blocks_vision)
    }

    /// The portal face of the wall at `p`, if it has one.
    pub fn portal_at(&self, p: IVec2) -> Option<PortalFace> {
        self.idx(p).and_then(|i| self.portals[i])
    }

    /// Every portal face, with the position of its wall.
    pub fn portal_faces(&self) -> impl Iterator<Item = (IVec2, PortalFace)> + '_ {
        let width = self.width.max(1);
        self.portals
            .iter()
            .enumerate()
            .filter_map(move |(i, face)| {
                face.map(|face| (IVec2::new(i as i32 % width, i as i32 / width), face))
            })
    }

    /// Sets or removes the portal face at `p`.
    pub fn set_portal(&mut self, p: IVec2, face: Option<PortalFace>) {
        let Some(i) = self.idx(p) else { return };
        if self.portals[i] != face {
            self.portals[i] = face;
            self.portal_revision += 1;
        }
    }

    /// Removes every portal face.
    pub fn clear_portals(&mut self) {
        if self.portals.iter().any(Option::is_some) {
            self.portals.fill(None);
            self.portal_revision += 1;
        }
    }

    pub fn idx(&self, p: IVec2) -> Option<usize> {
        self.is_valid(p)
            .then_some((p.y * self.width + p.x) as usize)
    }
}

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TurnState {
    pub tick: u64,
    pub simulation: bool,
}

impl TurnState {
    pub fn phase_name(&self) -> &'static str {
        if self.simulation {
            "Simulation"
        } else {
            "TickUpdate"
        }
    }
}

#[derive(Resource, Debug)]
pub struct TokenTimer(pub Timer);

impl Default for TokenTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(TOKEN_RECHARGE_SECS, TimerMode::Once))
    }
}

/// Decides when a completed turn may refill action tokens.
///
/// The simulation itself never waits: without an animation step the next
/// turn starts as soon as every token is spent. An animation system reports
/// how long its unfinished animations still run (every moving object, not
/// just the player), and the next turn then starts once they are all within
/// `animation_lead` of finishing. Like the simulation within a turn, the
/// animation of one turn completes before the next begins, so a box pushed
/// twice arrives before it is pushed again.
///
/// Insert a custom value before adding `GamePlugin` to tune the lead.
#[derive(Resource, Debug)]
pub struct TurnPacing {
    pub animation_lead: std::time::Duration,
    animation: Option<std::time::Duration>,
    started: bool,
}

impl TurnPacing {
    /// About one frame: the turn that follows is noticed a frame later.
    pub const DEFAULT_ANIMATION_LEAD: std::time::Duration = std::time::Duration::from_millis(20);

    pub const fn new(animation_lead: std::time::Duration) -> Self {
        Self {
            animation_lead,
            animation: None,
            started: false,
        }
    }

    /// Time until every blocking animation finishes, as last reported by an
    /// animation system; `None` when nothing reports.
    pub fn animation(&self) -> Option<std::time::Duration> {
        self.animation
    }

    /// Called by an animation system every frame with the longest time any
    /// blocking animation still runs (zero when all are done).
    pub fn report_animation(&mut self, remaining: std::time::Duration) {
        self.animation = Some(remaining);
    }

    pub(crate) fn can_advance(&self) -> bool {
        !self.started
            || self
                .animation
                .is_none_or(|remaining| remaining <= self.animation_lead)
    }

    pub(crate) fn started(&mut self) {
        self.started = true;
    }

    pub(crate) fn action_committed(&mut self) {
        // The animation step reports again after this turn's moves.
        self.animation = None;
    }
}

impl Default for TurnPacing {
    fn default() -> Self {
        Self::new(Self::DEFAULT_ANIMATION_LEAD)
    }
}

#[derive(Resource, Debug)]
pub struct SharedRng(pub rand::rngs::StdRng);
