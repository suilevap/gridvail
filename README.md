# PavEcsGame Lite Bevy Port

Bevy 0.19.1 Rust port of [PavEcsGame](https://github.com/suilevap/PavEcsGame),
commit `bc0b449` — specifically **PavEcsLiteGame**, the latest and most
complete variant (the legacy `PavEcsGame` demo is intentionally not the
target). Designed against `PavEcsGame_Rust_Bevy_migration_plan.md`, kept
next to this file.

## What is ported

- Six entity types from map glyphs: wall `X`, player `p`, enemy `e`,
  electricity `~`, light `i`, acid `%` (all five maps + four rule files
  ship under `assets/`; `map1.txt` loads at startup).
- Turn/token pipeline: each frame runs turn, simulation, animation, then
  rendering. Once every token is spent, the next turn starts when every
  unfinished animation (not just the player's) is within
  `TurnPacing::animation_lead` (20ms) of finishing, so an object moved in
  one turn arrives before it can move again, and held movement follows the
  animation. Decorations bound to an actor and objects marked
  `NonBlockingAnimation` never hold turns. Without an animation step
  (headless runs, the agent API without a window, tests) or with the `snap`
  style there is no delay at all. Idle turns advance after 1s. Recharges
  assign, never add. Keyboard and enemy commands remain token-gated.
- Two-phase movement resolution with the original conservative contract:
  swaps blocked, entering a vacated cell blocked in the same pass, one
  winner per cell in stable creation order, collisions recorded (the Lite
  container has no collision consumer — same as the original).
- Doors and keys (an addition, not in the original): `k` places a key
  (`♀`) and `D` a door (`+`). The player and enemies pick up every item on a
  cell they enter. Walking into a closed door while carrying a key opens it
  (`'`, no longer blocking movement or sight) and uses the key up. An actor
  that is destroyed drops everything it carries on its cell. `map1` has a
  key near the start, a door to a closet holding a second key, and another
  door further on. The HUD and the agent API's `player.keys` count the keys
  the player holds.
- Pathfinding (an addition): the workspace crate `crates/gridvail-path`
  (no dependencies) runs A* over any graph (a `Space`; a 4-connected grid is
  built in behind the default `grid` feature) with pluggable `Rules`. Rules price or forbid each
  step and may carry a small state, such as doors passed; the search keeps one
  layer of cells per state, so the same cell reached with a different state is
  searched separately; a rule may declare which states dominate others (more
  keys left beats fewer) so dominated arrivals are skipped. Rules may add
  jumps (teleports, ladders), searched by cost alone since they break
  distance estimates. Rules combine as tuples, and costs may be `Lex` pairs
  ("fewest doors, then shortest"). Besides the cheapest path, one search can
  stream the cheapest path for each other state the goal is reachable in, in
  order of cost: with doors counted, the way through the door and the long
  way round. `PathSearch` reuses its memory, so warmed-up searches allocate
  nothing. `navigation::NavMap` snapshots walls and closed
  doors, rebuilt only when static blockers change, and `navigation::Terrain`
  is the base rule (walls block; closed doors block or cost extra). Searches
  do not wrap around the map edges.
- Enemies decide with [FlatBT](https://github.com/suilevap/flatbt) behavior
  trees and walk along planned paths (a deviation: the original random-walks
  them one step at a time). Unaware, they stroll to nearby cells and rest. On
  spotting the player in their own field of view (the player's FOV, radius
  8) they freeze for a beat (a yellow `!`), then hunt and attack, search
  where it was last seen, and give up when it gets away. The trees only
  report an `EnemyAct`; `ai::carry_out` turns it into a step or a
  `Destination`. Locomotion (`locomotion/`) walks any actor to its
  `Destination`: it plans with the navigation planner using the actor's
  `TraversalPrefs` (what a closed door costs it, or that doors block), takes
  one step per action, and reports the outcome in `PathFollow::status`
  (walking, arrived, unreachable, or blocked after three failed steps); it
  never changes the destination. The AI (`ai/`) decides: wanderers pick a
  random floor cell, and a new one once a walk ends, and `DoorPolicy` prices
  doors by the keys held, more for the last key (one key 20 steps of detour,
  two 10, ten 2; no key, doors block). Enemies also pay a small
  `TraversalPrefs::crowd_cost` for cells other actors stand on, so they route
  around each other instead of queueing. Planning memory is sized to the
  map, so steady turns stay allocation-free.
- Player-bound `i` direction marker via relative position + rotation.
- Wall autotiling from `wall_rule.txt`, direction glyphs from the three
  direction rules (the file's Y-down inversion is inherited verbatim).
- Interval-based fractional FOV with obstacle caching, player
  Visible/Known layers (threshold 0.1, Known persists).
- CPU lightmaps: static layer with dirty-flag rebuild + dynamic layer,
  `(1 - sqD/radiusSq)` falloff, 255 saturation, same-kind sum,
  brighter-kind-wins, and the original fire/electricity/acid/gray palettes.
- Portals (an addition, in progress, see `PORTALS.md`): digits `1`-`9` in
  a map are portal walls, and the two walls with the same digit are paired.
  Seen from the floor in front of it, a portal wall shows what lies beyond
  the other one, through any number of portals; cells seen through a
  portal are not remembered. Walking into it comes out of the other one,
  with the step animated straight through. The `portals.txt` map has two
  rooms joined only by portals and a portal that leads from a room back
  into itself. The 3D renderer does not show portals yet.
- A view camera (an addition): both renderers draw through a
  renderer-neutral `ViewCamera` (centre, rotation, zoom, offset) that a
  camera operator keeps on the player's animated position, so the world
  glides past as they move. Q/E turn the view a quarter turn and Z/X zoom,
  both eased. While turning, the text view rotates as one picture, glyphs
  included; at rest it is upright again, with wall shapes and facing markers
  drawn turned so they keep matching the map. Arrow keys follow the turned
  view. `PORTALS.md`
  describes the portal view built on it.
- Renderer-neutral frame composition with depth merge, hex fill, `?` unknown
  borders, and previous-frame diffing. The default `TextRendererPlugin`
  writes changed cells onto one `Text2d` entity each. The optional hybrid
  renderer replaces walls with shared, extruded 3D autotile meshes and dots
  with light-mapped floor tiles while leaving actors and UI as text.

## Deliberate deviations (all commented at the site)

- One Bevy `World` + resources instead of `EcsUniverse` type-worlds.
- `TileSystem` computes masks two-phased; the original mutates neighbour
  masks mid-iteration and drops links asymmetrically.
- FOV caches key on a static-blocker revision, so changing walls invalidates
  shadows while moving actors do not invalidate every light and sensor.
- FOV output clamps floating-point roundoff to `[0, 1]`; circular shadow
  intervals are enabled through both constructors, including Bevy defaults.
- Bound decorations update after collision resolution, eliminating the
  reference's one-pass visual lag.
- Turn phase uses remaining executable work instead of the reference's
  per-system work flags. Tokenless commands never block fresh input.
- The static-light XOR version counter is an explicit dirty flag; source
  removal, FOV changes, and brightness/type edits invalidate the layer.
- The player has no direction tile (its Lite rule name is null, which
  throws in the original rule dictionary); the classic `@` stays.
- Keyboard input is read once per frame and dropped outside `TickUpdate`.
- Shared seeded RNG replaces the shared `Random(42)` (same sharing
  semantics; C# and Rust streams differ anyway).

## Layout

```text
src/lib.rs                 library root and public layers
src/main.rs                renderer selection, window, and screenshot harness
src/agent_api/             optional Bevy Remote control and state API
src/app/mod.rs             game plugin composition and phase ordering
src/app/map.rs             selected map, rules, and initial entity bundles
src/schedule.rs            shared startup and update phase contract
src/foundation/fov.rs      engine-independent interval FOV, also through portals
src/foundation/portal.rs   rigid grid transforms and portal faces
src/content/               map and symbol-rule parsers
src/debug_ui.rs             optional FPS and runtime performance panel
src/model/                 ECS data split by gameplay domain
src/simulation/            simulation plugin; control, motion, resolution, tiles
src/vision/                vision plugin; FOV cache and player visibility
src/navigation/            nav map snapshot, game rules and planner for pathfinding
src/ai/                    enemy decisions: behavior trees (FlatBT), wander goals, door prices
src/locomotion/            walking actors to their destination along planned paths
crates/gridvail-path/      grid pathfinding crate (A*, per-state paths, no deps)
src/lighting/              light math and palettes
src/presentation/          renderer-neutral lighting and frame composition
src/rendering/             swappable Text2d and extruded-wall output plugins
tests/full_map.rs  headless map1 boot + settle integration test
tests/gameplay.rs  timed input, movement, collision, and vision regressions
tests/enemy_behavior.rs  enemy behavior-tree scenarios on small ASCII maps
tests/allocations.rs  warmed Bevy baseline + full-turn allocation regression
tests/reference_parity.rs  independent C# FOV/light/palette fixtures
tools/generate_reference.py  regenerate fixtures from the pinned checkout
assets/maps/       map1/2/3, map1_test, lightTest
assets/rules/      wall + three direction rules
assets/fonts/      Unscii-8 square pixel font (public domain)
assets/shaders/    world-space procedural 3D surface material
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for layer boundaries, dependency
direction, and where new foundational versus game-specific code belongs.

## Run and verify

```sh
cargo run    # arrows or WASD to step the @ player
cargo run -- --renderer 3d-walls  # perspective 3D walls, text actors and HUD
cargo run -- --motion overshoot   # motion style; M cycles it in game
cargo run -- --map assets/maps/portals.txt  # two rooms joined only by portals
cargo test --workspace   # 168 tests, including allocation and independent C# comparisons
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
ENEMY_TRACE=1 cargo test --test enemy_behavior -- --nocapture --test-threads=1
```

The last command prints each enemy scenario turn by turn: `!` alert, `H`
hunting, `A` attacking, `S` searching, `p` patrolling, `z` resting, `k` a key, `+`/`'` a closed/open door.

Every object glides when it moves (`animation::ObjectAnimationPlugin`,
independent of the renderer): actors, and equally walls or decor that a
level moves, by any number of cells at once. A longer move takes
proportionally longer at the same speed; only a move across the wrapping
map edge teleports. Each animation is a `Move`: start and end (possibly the
same cell), a `Path` (straight, an out-and-back excursion, a sideways arc,
or a Hermite curve), its timing, and secondary motion such as a hop per
cell. Walking into a wall plays an excursion toward it and back, taking one
step's time, and animations pace the turns (see above).

`--motion` picks the style. The default, `locomotion`, gives each move
`step` (0.12s) per cell along a Hermite curve: the first step after a pause
starts slowly, a straight run flows at constant speed and gains momentum,
turns keep part of it, and stopping after a run coasts past the last cell
and settles back. Each cell crossed also hops slightly (`bob`), shown as a
lift up the screen or off the floor in the 3D view. The others are
`ease-out`, `linear`, `ease-in-out`, `overshoot`, and `snap`. `M` cycles the
styles at runtime. Code can insert a custom `animation::MotionStyle` before
adding `ObjectAnimationPlugin`; an `ObjectMotion` component overrides the
style for one entity, and a `MovePath` component gives its cell moves
another path (an arc, say). Children bound to an object (`BoundTo`, such as
the player's direction marker) ride on their parent's animation, hop
included: they animate their offset from the parent with the same moves,
along an orbit around it, so a turn swings them around the parent along the
shorter arc.

The debug performance panel is visible by default and toggles with `F3`. It
shows smoothed FPS/frame time, process and system CPU/RAM, entity count, text
cells updated by the renderer, and whether unfinished animations still hold
the next turn.

Note: if your cargo home is not writable, point it somewhere writable,
e.g. `CARGO_HOME=/tmp/cargo-home cargo run`.


The window uses a bundled Unicode font, console-shaped cells, and a camera
that fits the full map when resized. Capture the actual rendered window:

```sh
cargo run -- --screenshot screenshots/bevy-map1.png
cargo run -- --renderer 3d-walls --screenshot screenshots/bevy-map1-3d.png
cargo run -- --screenshot screenshots/bevy-explored.png --walk LLLUUURRRRRRRDDDDDDDDDDDDDDD
cargo run -- --record recordings/walk --walk "LLLLL.......RRRRUU......"
```

`--record DIR` saves every frame after warm-up as `DIR/frame_NNNNN.png`
(fixed 16 ms timestep) plus `DIR/trace.csv` with the player's logical cell,
drawn position, and the view camera's centre, rotation and zoom per frame, then exits. Its `--walk` holds each arrow key
for 8 frames, like a player holding the key, taps the camera keys `Q`/`E`
(turn) and `Z`/`X` (zoom) once, and `.` releases; turn into a
video with, for example,
`ffmpeg -framerate 60 -i DIR/frame_%05d.png -pix_fmt yuv420p walk.mp4`.

`--map FILE` plays another map instead of the bundled one, and `--reveal`
shows the whole map rather than what the player sees, for recording what
enemies do out of sight:

```sh
cargo run --release -- --map my_map.txt --reveal \
  --record recordings/enemies --walk "$(printf 'LR%.0s' $(seq 40))"
```

`cargo run -- --map assets/maps/chasers.txt` plays three enemies chasing
the player, stepping around each other on the way.

Without a GPU, run it under `xvfb-run` with Mesa's software Vulkan
(`mesa-vulkan-drivers`, `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).

Screenshot mode uses deterministic 16ms frames, optionally replays `UDLR`
(and the camera keys `QEZX`) through the real keyboard system, saves a PNG, then exits. It requires GPU
and window-server access. Capture mode renders to a fixed 1100x700 offscreen
target, allows Bevy's render pipelines to warm up, and returns failure if
saving fails. Normal play uses Bevy's real clock and window target. Screenshots
are local artifacts and are excluded from Git.

The 3D backend creates 16 combined wall meshes (one per neighbour mask), one
floor mesh, and fixed shared materials. Wall entities share those handles,
allowing Bevy's renderer to batch and instance matching mesh/material pairs.
The floor shader samples a map-sized GPU texture generated from the existing
CPU light and visibility results. It reconstructs those square-grid samples as
a smooth field around jittered Voronoi sites. A low-frequency domain warp bends
the sampling field across several cells, while stable world-space noise softens
the explored boundary. This removes axis-aligned light regions without changing
simulation resolution. The texture buffer is reused and uploaded only when its
bytes change.
In this mode the perspective camera orbits the view camera's centre: `Q`/`E`
turn it a quarter turn and `Z`/`X` or the mouse wheel zoom, all eased by the
camera operator; projected text cells remain attached
to their positions on the 3D ground plane. Humanoid actors expand into small
multiline symbol billboards without spawning extra runtime entities; compact
environmental symbols remain single-line billboards to avoid crowding.

## Agent runtime API

Start the game with Bevy Remote enabled on its loopback-only default address:

```sh
cargo run -- --remote                  # http://127.0.0.1:15702
cargo run -- --remote-port 15703       # choose another port
```

The server accepts JSON-RPC 2.0 POST requests. `gridvail/state` returns the
turn phase, whether input is currently accepted, player position and tokens,
enemy/collision counts, and the exact composed visual grid. `visual.rows`
contains Unicode glyph rows; `visual.colors` contains matching palette indices
and `visual.palette` names those indices.

```sh
curl http://127.0.0.1:15702 \
  -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":1,"method":"gridvail/state"}'
```

`gridvail/move` accepts `up`, `down`, `left`, `right`, or `wait`. It returns
`{"accepted":true}` when queued, or `accepted:false` with a reason when the
simulation is busy, another command is pending, or the player has no token.
Accepted commands use the normal turn pipeline.

```sh
curl http://127.0.0.1:15702 \
  -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":2,"method":"gridvail/move","params":{"direction":"right"}}'
```

The standard Bevy Remote methods remain available. Call `rpc.discover` to list
them alongside the Gridvail methods. The API plugin is not installed without a
remote flag, preserving the normal runtime allocation profile.

## Memory and steady-state allocation policy

The simulation follows the C# version's reuse-first design. The occupancy
map is a fixed dense array; movement claims, reservations, FOV rings, shadow
ranges, FOV results, visibility, light layers, render cells, and text strings
retain their capacity and are cleared or overwritten in place. Player and
enemy command/position state stays in stable ECS components, avoiding an
archetype move for every command and movement phase. Moving actors also leave
the static-blocker revision unchanged, so they do not trigger global FOV
recomputation.

Allocations are expected during startup, new entity/archetype creation, map
resizing, and destruction. Bevy 0.19's schedule executor makes a small fixed
number of allocations per `App::update()` even after warmup. The allocation
regression test measures that warmed framework envelope and verifies that a
forced player turn—movement, collision resolution, FOV, visibility, lighting,
frame composition, and changed-cell output—adds no allocations above the
corresponding steady frames. This is the practical Bevy equivalent of the C#
implementation's near-zero per-turn heap policy.

See [the comparison report](REFERENCE_COMPARISON.md) for what was verified
against upstream and what intentionally differs. Regenerate the C# fixtures
with .NET 10 and a clean checkout at the pinned commit:

```sh
python3 tools/generate_reference.py /path/to/PavEcsGame
```
