# Architecture

The code is arranged from reusable foundations toward the concrete game:

```text
foundation  content  model  schedule
     ↑        ↑       ↑       ↑
     simulation  vision  navigation  lighting  animation  presentation
             ↑       ↑       ↑
                    app       rendering  agent_api  debug_ui
                      \         |         /         /
                         main
```

Arrows point toward dependencies. `schedule` is the small, neutral contract
shared by the domain plugins and output backends. `app` is the game composition
root: it chooses the bundled map, configures phase order, and installs game
logic. The executable selects a renderer independently. Code that exists only
to reproduce PavEcsLiteGame belongs in `app`; reusable algorithms, systems,
and renderers must not import it.

## Layers

- `foundation/` contains engine-independent algorithms. The fractional FOV
  implementation and its ring/range scratch storage live here. Grid
  pathfinding is one level lower still, in its own workspace crate,
  `crates/gridvail-path`, with no dependencies: its core searches any graph
  (`Space`), and its `grid` feature adds the grid the game uses.
- `content/` parses external map and tile-rule formats. It does not spawn ECS
  entities or decide which map is active.
- `model/` contains ECS data, split into actors, spatial state, vision,
  lighting, world resources, and presentation buffers. It contains no systems.
- `schedule.rs` defines startup and update phase sets. It contains no systems
  and lets plugins declare ordering without depending on `app`.
- `simulation/` contains reusable gameplay systems and `SimulationPlugin`.
  Control, turn budgeting, motion, conflict resolution, lifecycle, and tile
  updates are separate files.
- `vision/` converts sensors and blocker state into cached FOV and visibility
  components. `VisionPlugin` owns their resources and phase registration; the
  underlying algorithm remains in `foundation/`.
- `navigation/` keeps `NavMap`, a walls/closed-doors snapshot of the map
  rebuilt when its static blockers change, and the game's base `Terrain`
  rules for the `gridvail-path` crate. New pathfinding rules (enemy sight,
  door limits, places to avoid) are `Rules` combined with `Terrain`.
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
  Renderers draw each frame as two layers: `ground` (per-cell floor and fog,
  blank under objects) and `objects` (every visible glyph entity, at its
  animated position), so any object can move.
  `PresentationPlugin` owns those systems and all map-sized frame resources.
- `rendering/` contains replaceable output plugins. `TextRendererPlugin` owns
  the font, `Text2d` ground cells, HUD, and one `Text2d` sprite per object
  at its animated position. Wall meshes in the 3D backend follow their
  objects the same way. The optional
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
