use bevy::prelude::*;

pub const TOKEN_RECHARGE_SECS: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionEvent {
    pub source: Entity,
    pub target: Entity,
}

#[derive(Resource, Debug, Default)]
pub struct CollisionBuffer(pub Vec<CollisionEvent>);

#[derive(Resource, Debug)]
pub struct MapGrid {
    pub width: i32,
    pub height: i32,
    pub revision: u64,
    pub blocker_revision: u64,
    cells: Vec<Option<MapOccupant>>,
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
            cells: vec![None; (width * height) as usize],
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
        Self(Timer::from_seconds(
            TOKEN_RECHARGE_SECS,
            TimerMode::Repeating,
        ))
    }
}

#[derive(Resource, Debug)]
pub struct SharedRng(pub rand::rngs::StdRng);
