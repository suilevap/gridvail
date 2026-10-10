# Architecture

The code is arranged from reusable foundations toward the concrete game:

```text
foundation  content  model  schedule
     ↑        ↑       ↑       ↑
     simulation  vision  navigation  lighting  animation  presentation
          ↑               ↑
          ai  →  locomotion
             ↑       ↑       ↑
                    app       rendering  agent_api  debug_ui
                      \         |         /         /
                         main
```

Arrows point toward dependencies. `camera` sits on top of `animation` (it
eases the view with the same curves) and below `app` and `rendering`.
`schedule` is the small, neutral contract
shared by the domain plugins and output backends. `app` is the game composition
root: it chooses the bundled map, configures phase order, and installs game
logic. The executable selects a renderer independently. Code that exists only
to reproduce PavEcsLiteGame belongs in `app`; reusable algorithms, systems,
and renderers must not import it.

## Layers

- `foundation/` contains engine-independent algorithms. The fractional FOV
  implementation and its ring/range scratch storage live here, with its
  portal-aware variant (`PortalFovComputer`: windows through portal faces,
  nested to any depth) and the rigid grid transforms portals are made of
  (`portal::CellTransform`). Grid
  pathfinding is one level lower still, in its own workspace crate,
  `crates/gridvail-path`, with no dependencies: its core searches any graph
  (`Space`), and its `grid` feature adds the grid the game uses.
- `content/` parses external map and tile-rule formats. It does not spawn ECS
  entities or decide which map is active.
- `model/` contains ECS data, split into actors, spatial state, vision,
  lighting, world resources, and presentation buffers. It contains no systems.
- `schedule.rs` defines startup and update phase sets. It contains no systems
  and lets plugins declare ordering without depending on `app`.
- `service.rs` is for work that may take longer than a frame. A service
  answers with a `Promise`, a plain `Future` its asker keeps and polls on
  later frames; dropping it cancels the work. How a service runs its work
  is its own `Runner`: inline (deterministic: tests), in the background on
  the async compute pool (paths, in the game), or handed over a fixed number
  of polls later (tests). `Services` is the one shared handle agents keep to
  reach every service; `ServicesPlugin` provides it and keeps what the
  services know of the world current.
- `simulation/` contains reusable gameplay systems and `SimulationPlugin`.
  Control, turn budgeting, motion, conflict resolution, lifecycle, and tile
  updates are separate files.
- `vision/` converts sensors and blocker state into cached FOV and visibility
  components. Lights use the plain field of view; the player's
  `PlayerView` sees through portals, and Visible/Known come from it (only
  cells seen directly become Known). `VisionPlugin` owns their resources
  and phase registration; the
  underlying algorithm remains in `foundation/`.
- `navigation/` keeps `NavMap`, a walls/closed-doors snapshot of the map
  rebuilt when its static blockers change, and the game's base `Terrain`
  rules for the `gridvail-path` crate. New pathfinding rules (enemy sight,
  door limits, places to avoid) are `Rules` combined with `Terrain`;
  `PathPlanner` plans on the map with reused memory. `PathService` plans
  paths, in the background by default, over its own copies of the map (kept
  current by `share_paths`), into pooled buffers: a `Path` is a cheap shared
  handle, and its buffer goes back to the pool when the last handle drops.
- `locomotion/` walks actors to their `Destination` (its input, which
  anything may write): it plans with the navigation planner using the
  actor's `TraversalPrefs`, keeps progress and the outcome in `PathFollow`,
  and writes ordinary `MoveCommand`s during the simulation phase, so
  movement still goes through the turn pipeline. It reports, and never
  changes the destination.
- `ai/` decides what actors want: random steps for enemies without a
  destination, random floor cells for wanderers (and a new one when a walk
  ends), and what doors cost given the keys held (`DoorPolicy` into
  `TraversalPrefs`). It writes intentions, never moves anything itself.
  Enemies decide with FlatBT behavior trees: `perceive` fills the
  `EnemyMind` blackboard (sight through the enemy's own `VisualSensor`
  field of view, last sighting, a stroll cell) and hands it the `Services`
  handle once. A walk is a `scope!` over two locals: a node picks the
  `target` (here from perception; anything can), flatbt's `await_future` (generic:
  any `Future`, held in the node's state, so leaving the branch cancels it)
  plans from `target` into `path`, then `Follow` reports
  `EnemyAct::Move(mood, path)` a turn at a time until it arrives, the goal
  moves away, or a step fails (someone in the way), after which the tree
  plans again.
  `carry_out` turns each act into a single step. A thinking enemy
  (`EnemyAct::Think`) keeps its turn open for a few frames for its path,
  then waits the turn out. Out-of-turn enemies are skipped with
  `Tick::Skip`.
  Hunters (`hunter_tree`) follow an `Order` with flatbt's goal stack: with
  no way to it (a path task that lands no path) and no key, `Unlock(here)`
  needs `GetKey`, which needs `FetchKey(key)` for each known key in turn
  and fails only when every one has; with a key, door pricing lets the next
  plan through the door.
- `lighting/` contains light blending and palette conversion. It does not know
  about Bevy text entities or the application schedule.
- `animation/` turns cell moves of any length into continuous motion for
  every positioned object (actors, walls, decor). Each object plays one
  `Move` at a time (start, end, `Path` shape, timing, secondary motion such as
  a hop); a cell change plays a
  straight move and a blocked move (a simulation collision) plays an
  excursion out and back to the same cell. Children (`BoundTo`) are shown at
  their parent's shown position plus their animated offset from it: the
  same `ObjectAnimation` and per-frame step as any object, but following
  `rotate(offset, facing)` instead of the cell, along `Path::Orbit` around
  the parent, so a turn swings them around it rather than through it. `ObjectAnimationPlugin` picks the
  timing from `MotionStyle` (per-entity `ObjectMotion` overrides), writes the
  shown position to `AnimatedPos`, and reports how long unfinished moves
  still run to `TurnPacing`, which holds the next turn until they have nearly
  finished. It needs no renderer; without it the simulation runs with no
  delay.
- `presentation/` builds light maps and renderer-neutral composed cell frames.
  Frame cells are in the player's map coordinates; a cell the player sees
  shows the map cell it looks onto (`RenderBuffers::seen`), which behind a
  portal is elsewhere on the map, and objects get one instance per place
  they are seen. Cells not seen show the map as remembered, dimmed.
  Renderers draw each frame as two layers: `ground` (per-cell floor and fog,
  blank under objects) and `objects` (every visible glyph entity, at its
  animated position), so any object can move.
- `camera/` directs the renderer-neutral `ViewCamera` (model data: the map
  point at the screen centre, rotation, zoom, offset, and the conversion
  between map and view cells). `CameraOperator` follows a target (the
  player by default), either locked on its animated position or trailing
  it with its own moves, and eases every change of rotation, zoom, offset
  or target with a `Tween` (a retargetable value on Bevy's `EasingCurve`;
  all easing uses Bevy's `EaseFunction`). Q/E turn the view a quarter turn
  and Z/X zoom;
  movement keys are read as screen directions, latched when pressed
  (`MoveIntent`): a held key keeps its map direction while the view turns,
  and a portal that turns the player turns it too. Renderers only read the
  camera. `PORTALS.md` builds the portal view on it: when the target goes
  through a portal that turns, the operator turns the view with it so the
  picture stays, and with `PortalTurn::KeepNorth` (N) then eases it back.
  The frame is a window of `FrameSize` cells, not the map's size: it
  follows the player, and each map position keeps a slot (its coordinates
  modulo the window size), so steps only rewrite the slots scrolling in.
  It is 80x80 headless; the text renderer grows it to cover its window at
  the furthest zoom-out, turned any way (the window's diagonal), up to 192
  cells a side, past which the camera operator's zoom floor rises instead.
  Changing `FrameSize` rebuilds the frame. `PresentationPlugin` owns those
  systems and the frame and light-map resources.
- `rendering/` contains replaceable output plugins. `TextRendererPlugin` owns
  the font, `Text2d` ground cells, HUD, and one `Text2d` sprite per object
  at its animated position, each placed where the view camera shows it. Its
  2D camera never moves: a turning view turns the whole picture, glyphs
  included. Cells are square (the Unscii-8 pixel font, drawn at 16 px with
  one world unit per screen pixel), so the turn is rigid; for a non-square
  cell font the spacing would ease between cell width and height. Glyphs
  are drawn for the nearest quarter turn (wall and facing
  glyphs swapped for their turned versions, plus the turn of any portal an
  object is seen through, `ObjectCell::quarters`) and tilted by the rest, so at
  rest they are upright and crisp. Glyphs sample nearest (pixel-exact)
  at rest and linearly while tilted, which smooths their edges mid-turn.
  Wall
  meshes in the 3D backend follow their objects the same way, and its
  perspective camera orbits and zooms to match the view camera. The optional
  `ExtrudedWallRendererPlugin` adds a perspective camera plus shared wall and
  floor meshes beneath that text layer. A small GPU texture carries the same
  visibility and CPU-light results to the procedural floor shader.
- `agent_api/` contains the optional loopback Bevy Remote transport and custom
  control/state methods. It reads the composed frame and writes normal player
  command components, keeping agent actions inside the gameplay pipeline.
- `debug_ui.rs` contains optional runtime diagnostics and their UI. It reads
  public game/renderer statistics without owning simulation or rendering.
- `app/` wires the concrete Gridvail/PavEcsLiteGame port together. The bundled
  map choice and exact spawn bundles live in `app/map.rs`; domain composition,
  phase ordering, and compatibility decisions live in `app/mod.rs`.
- `main.rs` installs the animation step, selects the text or hybrid 3D-wall
  renderer, installs
  `DebugPerformancePlugin`, optionally installs `AgentApiPlugin`, and owns the
  window/screenshot harness.

## Performance boundary

Stable gameplay entities receive their movement, FOV, and visibility storage
when spawned. Systems overwrite or clear those components and resource buffers
in place. Startup parsing/autotiling and exceptional spawn/destruction paths may
allocate. `tests/allocations.rs` protects the warmed steady-turn boundary.

## Placement rule

When adding code, place it in the lowest layer that can own it:

1. Pure algorithm or container: `foundation`.
2. File-format parser: `content`.
3. Data with no behavior: `model`.
4. Reusable world mutation and its registration: the matching domain plugin.
5. Renderer-neutral motion over time: `animation`; visual composition:
   `presentation`.
6. Concrete screen, terminal, or tile output: a plugin in `rendering`.
7. Runtime automation transport: `agent_api`.
8. Runtime diagnostics that observe other layers: `debug_ui`.
9. A rule specific to this port's map, entity bundle, or schedule: `app`.

This keeps the C# project's useful separation between common algorithms,
components, and game systems without reproducing its custom ECS infrastructure.
