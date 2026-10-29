# Architecture

The code is arranged from reusable foundations toward the concrete game:

```text
foundation  content  layout  model  schedule
     ↑        ↑        ↑       ↑       ↑
     simulation  vision  navigation  lighting  animation  presentation
          ↑               ↑                  ↑
          ai  →  locomotion   selection  combat  damage   transition
             ↑       ↑       ↑
                    app       rendering  agent_api  debug_ui
                      \         |         /         /
                         main
```

Arrows point toward dependencies. `selection`, `combat` and `damage` sit
on `model`, `schedule` and `simulation` and import none of each other; they
meet only in messages and components. `transition` sits on `simulation`,
`camera` and `model`. `camera` sits on top of `animation` (it
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
  (`portal::CellTransform`), and small grid geometry: an `Oval` of cells
  and the nearest free cell to a point (`grid::nearest_cell`). Grid
  pathfinding is one level lower still, in its own workspace crate,
  `crates/gridvail-path`, with no dependencies: its core searches any graph
  (`Space`), for the cheapest path to one goal (`PathSearch::find`) or to
  each of many, nearest first (`find_each`, which the `along-walls` room
  planner uses), and its `grid` feature adds the grid the game uses.
- `content/` parses external map and tile-rule formats (and the `@room`
  layout blocks and `@next` line after a map). It does not spawn ECS
  entities or decide which map is active.
- `layout/` holds the ways to lay out walls, as plain data: a
  `LayoutSource` turns a `LayoutRequest` (a map's size and a point to build
  around) into a `Layout` (walls and the `Oval` they fill), one source per
  file: text art (`ArtLayout`, a picture drawn with `X` walls), cells
  scattered by seed (`ScatterLayout`), and `Fallback`, which tries one and
  then another. It knows nothing of entities or how walls appear; a new way
  to lay out walls is a new file implementing the trait.
- `model/` contains ECS data, split into actors, spatial state, vision,
  lighting, world resources, and presentation buffers. It contains no systems.
- `schedule.rs` defines startup and update phase sets. It contains no systems
  and lets plugins declare ordering without depending on `app`.
- `service.rs` is for work that may take longer than a frame. A service
  answers with flatbt-bevy's `Promise`, a plain `Future` its asker keeps
  and polls on later frames; dropping it cancels the work. How a service
  runs its work is its own flatbt-bevy `Runner`: inline (deterministic:
  tests) or in the background on the async compute pool (paths, in the
  game). `Delayed` holds an answer back a fixed number of polls, for
  testing slow work (`PathService::delayed`). `Services` is the one shared handle agents keep to
  reach every service; `ServicesPlugin` provides it and keeps what the
  services know of the world current.
- `simulation/` contains reusable gameplay systems and `SimulationPlugin`.
  Control, turn budgeting, motion, conflict resolution, lifecycle, and tile
  updates are separate files.
  `rooms.rs` switches `Room`s to the layout asked for (`Room::advance`).
  It asks the room's planner how (`room_plans/`: the `RoomPlanner` trait,
  one strategy per file, chosen by name from the `RoomPlanners` resource;
  `along-walls`, the default, finds its paths with `gridvail-path`;
  strategies that weigh possible slides share `SwitchPlan::from_candidates`,
  which takes the best ones that do not overlap)
  for a `SwitchPlan` of `WallMove`s (every wall slides, `from` to `to`;
  its `MoveKind` says whether it is a new wall that rises first or one
  that goes and sinks after), checks that the plan gives the new layout
  (falling back to rising and sinking in place), and carries it out.
  Every wall is marked `Sliding`: one that moves keeps its entity, a new
  one is spawned at its cell `Rising` from where its move starts, and one
  that goes leaves the map `Sinking`, its `Pos` set to where its move
  ends (despawned at once without the animation step). It marks their neighbours with `Tile`,
  which `tile_system` autotiles again. It does not know what asked:
  `plates.rs` is a stand-in trigger (`PressurePlate`) until a trigger/action
  system, where a room switch is one kind of action. Rails (`Rail`, floor
  that planners prefer walls to glide along) reach planners through
  `SwitchContext::has_rail`, and `tile_system` autotiles them with the wall
  rule, flat and dimmed.
- `vision/` converts sensors and blocker state into cached FOV and visibility
  components. Every field of view sees through portals: lights' and
  sensors' `FovResult` (its samples for lights, which fade by the way the
  light came, and its per-cell grid for enemy sight), and the player's
  `PlayerView`, which Visible/Known come from (only cells seen directly
  become Known). `VisionPlugin` owns their resources
  and phase registration; the
  underlying algorithm remains in `foundation/`.
- `navigation/` keeps `NavMap`, a walls/closed-doors snapshot of the map
  rebuilt when its static blockers or portals change (portal faces become
  `gridvail-path` portal links, searched as single moves with a
  portal-aware estimate for walkers whose `TraversalPrefs::portals` is
  set; `NavMap::step_toward` turns a path's next cell into a step,
  including into a portal face), and the game's base `Terrain`
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
- Fights are put together from small modules that only share data and
  messages, on general mechanisms that other features can use too
  (`COMBAT.md` has the design, `PRINCIPLES.md` the mechanisms). A turn's
  actions go through `schedule::ActionStage`, in order, within the
  simulation before the moves: `Due` (time passes), `Ongoing` (what fell
  due happens), `PlayerCommands` (input), `Decide`, `Effects`, `Costs`,
  `Movement`, `Hits`, `Damage`. Each system names its stage, not its
  neighbours.
  - `simulation/time.rs`, time: an actor in `Debt` has each of its turns
    pay one off instead (its token taken before anyone decides, so
    deciders such as the AI need no check) until `DebtPaid`; anything adds
    debt with a `Delay` (a wind-up, later a stun). A message can be
    `Scheduled` on an entity and is sent when its debt is paid; a message
    type opts in with `add_scheduled_message`. An action with a cast time
    is a delay plus its effect scheduled.
  - `simulation/physics.rs`, physics for bodies moved outside walking: an
    `Impulse` sets a body moving at a speed in cells; every moving body
    takes a cell per sub-step, all together (so one can follow another,
    and a ring of bodies moves only if one steps on purpose), losing its
    `Friction` of speed per cell. A forced body that is blocked sends a
    `Collision` with the speed it had left, and a `Pushable` body it ran
    into carries that speed on. Walking still has its own resolver (see
    `PRINCIPLES.md`, Backfill).
  - `model::Area`: a set of cells around an origin, in the frame of a
    facing (what a weapon strikes).
  - `selection/`: a `Selector` moving into something `Selectable` picks it
    (`Selected`), free; its next fresh move is a `ContextAction`, its
    action for the turn, which anything may answer; a `CancelIntent` (a
    tap, `C`, the agent API) drops the pick. Input layers mark held-on
    commands (`MoveCommand::repeat`) so a held key never acts twice.
  - `combat/`: `perform_maneuvers` answers a context action by an actor
    with `Maneuvers` (one move per `Relation` of the second direction to
    the first) with a `Hit` and `Impulse`s; `ready_weapons` answers a
    `UseWeapon` by aiming the `Weapon` (`Telegraph`, a direction from the
    wielder), then owing its wind-up with the `Strike` scheduled, and
    `strike` hits every cell of the weapon's `Area`; `hits_deal_damage`,
    `critical_hits` (on `Exposed` targets, which the AI writes) and
    `collisions_deal_damage` (by the collision's speed) turn what landed
    into `Damage`.
  - `damage/`: `Damage` comes off `Health`; what runs out has `Died` and is
    destroyed, unless it `KeepOnDeath` (the player: the app plays the map
    again).
  - Cells to show the player (strikes, picks) are listed each frame into
    `CellMarks` by `MarkCells` systems; the frame composer tints whatever
    stands there, or draws a mark on the floor.
  - Who is who comes from data. `content::data::read` reads any RON file
    into any type that derives `Deserialize`, so a new data file is a new
    type, with no parser; `content::art` reads shapes drawn as text art.
    The roster (`assets/rules/actors.ron`: actor kinds by map glyph, their
    behaviour, hit points, weapon or moves; weapons with their area drawn
    as art; move sets) is read that way into `content::actors` types, and
    `app::ActorKinds` equips each actor spawned from a map glyph with what
    its kind names.
- `lighting/` contains light blending and light colour: a `LightCell` keeps
  a level per kind of light, summed over every light of that kind; each
  kind has a gradient baked into a 256-entry lookup table (`LightLut`), and
  a cell's colour mixes every kind's gradient by its share. It also holds the
  named colours glyphs are given (`Rgb`, in `model/`). It does not know
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
  the parent, so a turn swings them around it rather than through it.
  `Sliding` objects move at a steady pace instead of in the actors'
  style. `Rising` objects come up out of the floor where they start and
  then move to their cell; `Sinking` ones move to their `Pos` and then go
  down into the floor (`AnimatedPos::height`, eased by a `Tween`); sunk
  objects are despawned. Renderers lower a wall's mesh into the floor or fade its
  glyph, and the frame shows the floor under an object partly sunk. `ObjectAnimationPlugin` picks the
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
- `model::settings` holds settings the player changes in game
  (`CompassMode`, `WallVolume`) and the open state of the settings menu (`SettingsMenu`),
  which player input and camera controls check so the menu's keys do not
  also move them. The menu and the compass are drawn by the text renderer
  (`rendering/settings.rs`).
- `rendering/volume.rs` (`WallVolumePlugin`) draws objects with height for
  the text renderer when `WallVolume` is not flat, seen at a slant. How
  tall an object stands is renderer-neutral model data: its own `Stature`
  component, else its glyph's height in `GlyphStatures` (loaded from
  `assets/rules/statures.txt` by `content::statures`; walls and closed
  doors by default), else flat. A second camera draws each standing
  object's glyph at its animated position into a wall map (8 texels per
  cell, height as alpha), and one full-screen shader (`wall_volume.wgsl`)
  extrudes it up the screen, writing the depth the text sprites are sorted
  by, so what stands hides actors behind it.
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
- `transition/` makes a map come together from what was there before. On
  `MapLoaded` it takes an `Arrival`: what `NextArrival` asks for (walls
  shown first in an area), else the walls of the map before. The map's
  plain walls give way to the arriving ones, and a room covering the map
  switches back to them, as a `Cutscene` (player input and new turns wait)
  with an area in sight (`RevealArea`, which the frame marks as seen like
  `RevealAll` but only within it, and for shown walls instead of what the
  player sees or remembers). Each phase is a component on the room and a
  system of its own: `Showing` (shown walls stand, with the camera on
  them), `Focusing` (their area narrows onto the player as the camera
  returns) and `Forming` (the room switches, the player's sight comes back,
  explored cells not in sight are forgotten, and the circle left around
  them closes).
- `app/` wires the concrete Gridvail/PavEcsLiteGame port together. The bundled
  map choice and exact spawn bundles live in `app/map.rs`, which also loads
  another map on `LoadMap` (in `PreUpdate`, so the frame's systems see one
  whole map): the level (everything with a `Pos`, what it carries, its
  rooms) is despawned, the player kept and moved to the new start, and
  `MapGrid::reset` empties the grid for the new size while its revisions
  move on, so sight, paths and light rebuild themselves; renderers follow
  (the text renderer drops sprites of objects that are gone, the 3D floor
  is rebuilt for a new size). It knows the bundled maps by name
  (`BUNDLED_MAPS`, generated by `build.rs`), keeps the map being played and
  where its exits lead (`CurrentMap`), and loads that map when the player
  steps onto an `Exit`. A map file is read and checked first (`MapPlan`:
  portal links, rooms, `@next`, and that every room names a known room
  planner), then spawned cell by cell (`spawn_cell`). Every map built,
  the first included, sends `MapLoaded` with the plain walls of the map it
  replaced; map loading knows nothing of what reacts to it.
  `app/intro.rs` is the game's intro (`Intro`): it picks a layout source
  (the title as text art falling back to noise, or noise) and asks, with a
  `NextArrival`, for the first map to come together from its walls.
  Domain composition,
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

`PRINCIPLES.md` describes how to shape code once it is placed: one job per
system, entities composed of small components, systems meeting in messages,
stats as data.


When adding code, place it in the lowest layer that can own it:

1. Pure algorithm or container: `foundation`; a way to lay out walls:
   `layout`.
2. File-format parser: `content`.
3. Data with no behavior: `model`.
4. Reusable world mutation and its registration: the matching domain plugin.
5. Renderer-neutral motion over time: `animation`; visual composition:
   `presentation`; how one map gives way to the next: `transition`.
6. Concrete screen, terminal, or tile output: a plugin in `rendering`.
7. Runtime automation transport: `agent_api`.
8. Runtime diagnostics that observe other layers: `debug_ui`.
9. A rule specific to this port's map, entity bundle, or schedule: `app`.

This keeps the C# project's useful separation between common algorithms,
components, and game systems without reproducing its custom ECS infrastructure.
