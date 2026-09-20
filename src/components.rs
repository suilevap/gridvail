//! ECS types for the Lite-variant port.
//!
//! Mirrors `PavEcsGame.Components` + `PavEcsLiteGame` spawn sets.
//! Game coordinates are integer grid cells, Y grows downward; only the
//! renderer flips Y for Bevy's Y-up world space.

use bevy::prelude::*;

/// Console-shaped cells match the bundled monospace font's advance at 20px.
pub const CELL_SIZE: Vec2 = Vec2::new(12.0, 20.0);
/// Token recharge interval, mirrors `CommandTokenDistributionSystem(1s)`.
pub const TOKEN_RECHARGE_SECS: f32 = 1.0;
/// FOV visibility threshold, mirrors `PlayerFieldOfViewSystem` (`> 0.1`).
pub const VISIBILITY_THRESHOLD: f32 = 0.1;

/// Grid position (mirrors `PositionComponent`; wraps toroidally for colliders).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Pos(pub IVec2);

/// Cells moved per pass, not m/s (mirrors `SpeedComponent`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Speed(pub IVec2);

/// Rendered glyph. `depth` mirrors `Depth` (Back < Foreground wins).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyph {
    pub ch: char,
    pub depth: u8,
    /// Palette index 0..16 (mirrors `ConsoleColor`).
    pub color: u8,
}

impl Glyph {
    pub const fn new(ch: char, depth: u8, color: u8) -> Self {
        Self { ch, depth, color }
    }
}

/// Player marker with controller index.
#[derive(Component, Clone, Copy, Debug)]
pub struct Player(pub usize);

/// Enemy marker (random-walk actor).
#[derive(Component, Clone, Copy, Debug)]
pub struct Enemy;

/// Wall marker (static autotiled collider).
#[derive(Component, Clone, Copy, Debug)]
pub struct Wall;

/// Light-fixture markers (`i` / `%` / `~` map cells).
#[derive(Component, Clone, Copy, Debug)]
pub struct Lamp;
#[derive(Component, Clone, Copy, Debug)]
pub struct AcidPool;
#[derive(Component, Clone, Copy, Debug)]
pub struct ElectroField;

/// Participates in the occupancy grid (mirrors `ColliderComponent`).
/// Only colliders occupy cells; bound decorations do not.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Collider;

/// Marks entities simulated this run (mirrors `IsActiveTag`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Active;

/// Action budget. `recharge` mirrors `WaitCommandTokenComponent`,
/// `count` mirrors `CommandTokenComponent` (assigned, never added).
#[derive(Component, Clone, Copy, Debug)]
pub struct Tokens {
    pub count: i32,
    pub recharge: i32,
}

impl Tokens {
    pub const fn new(recharge: i32) -> Self {
        Self { count: 0, recharge }
    }
}

/// Friction applied to [`Speed`] every simulation pass.
#[derive(Component, Clone, Copy, Debug)]
pub struct Friction(pub i32);

/// Movement intent with three states (mirrors `NewPositionComponent?
/// — absent / move-to / remove-from-map`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PendingPos {
    #[default]
    None,
    MoveTo(IVec2),
    RemoveFromMap,
}

/// Position before the latest move (mirrors `PreviousPositionComponent`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrevPos(pub IVec2);

/// Queued step from input or AI (mirrors `MoveCommandComponent`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveCommand {
    pub target: IVec2,
    pub relative: bool,
    pub active: bool,
}

/// Marked for destruction (mirrors `DestroyRequestTag`).
/// Map removal and ECS despawn are separate stages.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct DestroyRequested;

/// Facing vector (mirrors `DirectionComponent`; Y grows downward).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Facing(pub IVec2);

/// Picks a glyph from a direction rule file (mirrors `DirectionTileComponent`).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct DirectionTile {
    pub rule: String,
}

/// Facing follows velocity (mirrors `DirectionBasedOnSpeed` tag).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct DirectionBasedOnSpeed;

/// Autotile state (mirrors `TileComponent`).
#[derive(Component, Clone, Debug, Default)]
pub struct Tile {
    pub mask: u8,
    pub rule: String,
}

/// Vision radius (mirrors `VisualSensorComponent`).
#[derive(Component, Clone, Copy, Debug)]
pub struct VisualSensor {
    pub radius: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LightKind {
    #[default]
    None = 0,
    Fire = 1,
    Electricity = 2,
    Acid = 4,
}

impl LightKind {
    /// Shared-type test, mirrors `(a.LightType & b.LightType) != 0`.
    pub fn overlaps(self, other: LightKind) -> bool {
        (self as u8) & (other as u8) != 0
    }
}

/// Light emitter (mirrors `LightSourceComponent`).
#[derive(Component, Clone, Copy, Debug)]
pub struct LightSource {
    pub radius: i32,
    pub kind: LightKind,
    pub value: u8,
}

/// Cached fractional-visibility field (mirrors `AreaResultComponent<float>`).
#[derive(Component, Clone, Debug)]
pub struct FovResult {
    pub revision: u64,
    pub obstacle_revision: u64,
    pub pos: IVec2,
    pub radius: i32,
    pub data: Vec<f32>,
}

bitflags::bitflags! {
    /// Mirrors `VisibilityType` flags.
    #[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Vis: u8 {
        const VISIBLE = 1;
        const KNOWN = 2;
    }
}

/// Per-observer visibility layer (mirrors `AreaResultComponent<VisibilityType>`).
#[derive(Component, Clone, Debug)]
pub struct VisibilityMap {
    pub revision: u64,
    pub data: Vec<Vis>,
}

/// Child bound to a parent entity (mirrors `LinkToEntityComponent` +
/// `RelativePositionComponent`).
#[derive(Component, Clone, Copy, Debug)]
pub struct BoundTo {
    pub parent: Entity,
    pub offset: IVec2,
    pub offset_dir: IVec2,
}

/// Collision pair of one pass (mirrors `CollisionEvent<Entity>`).
/// The Lite container only records these; nothing destroys on collision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionEvent {
    pub source: Entity,
    pub target: Entity,
}

/// Per-pass collision buffer (cleared after consumers run).
#[derive(Resource, Debug, Default)]
pub struct CollisionBuffer(pub Vec<CollisionEvent>);

/// Occupancy grid: colliders only (mirrors `MapData<PackedEntity>`).
#[derive(Resource, Debug)]
pub struct MapGrid {
    pub width: i32,
    pub height: i32,
    /// Bumped on every occupancy change.
    pub revision: u64,
    /// Bumped only when the static obstacle layout changes. Moving actors do
    /// not invalidate every cached FOV in the world.
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
        Self {
            width: width.max(1),
            height: height.max(1),
            revision: 0,
            blocker_revision: 0,
            cells: vec![None; (width.max(1) * height.max(1)) as usize],
        }
    }

    pub fn is_valid(&self, p: IVec2) -> bool {
        p.x >= 0 && p.y >= 0 && p.x < self.width && p.y < self.height
    }

    /// Toroidal wrap, mirrors `MapData.GetSafePos` (via `rem_euclid`,
    /// which also covers large negative offsets).
    pub fn safe_pos(&self, p: IVec2) -> IVec2 {
        IVec2::new(p.x.rem_euclid(self.width), p.y.rem_euclid(self.height))
    }

    pub fn get(&self, p: IVec2) -> Option<Entity> {
        self.idx(p)
            .and_then(|i| self.cells[i].map(|occupant| occupant.entity))
    }

    /// Insert an obstacle. Kept as the convenient default for walls and for
    /// compatibility with small tests that build grids directly.
    pub fn set(&mut self, p: IVec2, e: Entity) {
        self.set_with_blocking(p, e, true);
    }

    pub fn set_with_blocking(&mut self, p: IVec2, e: Entity, blocks_vision: bool) {
        let Some(i) = self.idx(p) else { return };
        let next = Some(MapOccupant {
            entity: e,
            blocks_vision,
        });
        if self.cells[i] != next {
            let old_blocker = self.cells[i].is_some_and(|cell| cell.blocks_vision);
            self.cells[i] = next;
            self.revision += 1;
            if old_blocker != blocks_vision {
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

/// Turn state. `simulation` while movement/destruction work remains,
/// otherwise `TickUpdate` (mirrors `TurnManager.CurrentPhase`).
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

/// Repeating 1s timer that recharges command tokens.
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

/// Shared deterministic RNG (mirrors the single `Random(42)` all enemies draw
/// from; per-enemy RNGs would change the action sequence).
#[derive(Resource, Debug)]
pub struct SharedRng(pub rand::rngs::StdRng);

/// One composed cell for the renderer (`depth` mirrors `Depth`;
/// `Back` = 0 wins nothing, `Foreground` = 1 overwrites).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderCell {
    pub ch: char,
    pub color: u8,
    pub depth: u8,
}

/// Double-buffered composed frame (mirrors `PrepareForRenderSystem` buffers).
#[derive(Resource, Debug)]
pub struct RenderBuffers {
    pub width: i32,
    pub height: i32,
    pub current: Vec<RenderCell>,
    pub previous: Vec<RenderCell>,
}

impl RenderBuffers {
    pub fn new(width: i32, height: i32) -> Self {
        let n = (width.max(1) * height.max(1)) as usize;
        Self {
            width: width.max(1),
            height: height.max(1),
            current: vec![RenderCell::default(); n],
            previous: vec![RenderCell::default(); n],
        }
    }

    pub fn idx(&self, p: IVec2) -> Option<usize> {
        if p.x >= 0 && p.y >= 0 && p.x < self.width && p.y < self.height {
            Some((p.y * self.width + p.x) as usize)
        } else {
            None
        }
    }

    pub fn swap(&mut self) {
        std::mem::swap(&mut self.current, &mut self.previous);
    }
}

/// Static light layer + validity flag (mirrors `_lightMapStatic`; the
/// original XOR version counter is replaced with an explicit dirty flag).
#[derive(Resource, Debug)]
pub struct StaticLight {
    pub width: i32,
    pub height: i32,
    pub data: Vec<LightCell>,
    pub dirty: bool,
    pub last_seen_revisions: Vec<(Entity, u64)>,
    pub current_revisions: Vec<(Entity, u64)>,
}

impl StaticLight {
    pub fn new() -> Self {
        Self {
            width: 1,
            height: 1,
            data: vec![LightCell::default()],
            dirty: true,
            last_seen_revisions: Vec::new(),
            current_revisions: Vec::new(),
        }
    }

    pub fn resize(&mut self, w: i32, h: i32) {
        self.width = w.max(1);
        self.height = h.max(1);
        self.data = vec![LightCell::default(); (self.width * self.height) as usize];
        self.dirty = true;
        self.last_seen_revisions.clear();
        self.current_revisions.clear();
    }
}

impl Default for StaticLight {
    fn default() -> Self {
        Self::new()
    }
}

/// Accumulated light value of one cell (mirrors `LightValueComponent`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightCell {
    pub value: u8,
    pub kind: LightKind,
}

/// HUD marker for the status line.
#[derive(Component)]
pub struct HudText;

/// Marker for per-cell glyph entities spawned at startup.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapCell(pub IVec2);

/// Convert a grid cell to world coordinates (map centered on origin,
/// Y flipped: game Y grows downward).
pub fn grid_to_world(p: IVec2, w: i32, h: i32) -> Vec3 {
    Vec3::new(
        (p.x as f32 - w as f32 / 2.0 + 0.5) * CELL_SIZE.x,
        (h as f32 / 2.0 - p.y as f32 - 0.5) * CELL_SIZE.y,
        0.0,
    )
}

/// Rotate `v` by cardinal direction `dir` (mirrors `Int2Extensions.Rotate`).
/// Asserts in debug builds for non-unit directions, like the original.
pub fn rotate(v: IVec2, dir: IVec2) -> IVec2 {
    if dir == IVec2::ZERO {
        return v;
    }
    debug_assert_eq!(
        dir.x * dir.x + dir.y * dir.y,
        1,
        "only normalized direction supported"
    );
    IVec2::new(v.x * dir.x + v.y * dir.y, -v.y * dir.x + v.x * dir.y)
}

/// Dominant-axis direction name (mirrors `DirectionExtension.ToDirection`;
/// note game Y grows downward yet positive Y maps to "up", kept as-is).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    None,
    Right,
    Up,
    Left,
    Down,
}

impl Direction {
    pub fn of(v: IVec2) -> Self {
        if v == IVec2::ZERO {
            return Self::None;
        }
        if v.x.abs() > v.y.abs() {
            if v.x > 0 {
                Self::Right
            } else {
                Self::Left
            }
        } else if v.y > 0 {
            Self::Up
        } else {
            Self::Down
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::None => 0,
            Self::Right => 1,
            Self::Up => 2,
            Self::Left => 3,
            Self::Down => 4,
        }
    }
}

/// `(x + y%2) % 2 == 0` checkerboard (mirrors `IsHexPos`).
pub fn is_hex_pos(p: IVec2) -> bool {
    (p.x + p.y % 2) % 2 == 0
}
