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
- Turn/token pipeline: actions fast-forward token recharge after a configurable
  100ms minimum turn interval; idle turns advance after 1s. Recharges assign,
  never add. Keyboard and random-walk commands remain token-gated.
- Two-phase movement resolution with the original conservative contract:
  swaps blocked, entering a vacated cell blocked in the same pass, one
  winner per cell in stable creation order, collisions recorded (the Lite
  container has no collision consumer — same as the original).
- Player-bound `i` direction marker via relative position + rotation.
- Wall autotiling from `wall_rule.txt`, direction glyphs from the three
  direction rules (the file's Y-down inversion is inherited verbatim).
- Interval-based fractional FOV with obstacle caching, player
  Visible/Known layers (threshold 0.1, Known persists).
- CPU lightmaps: static layer with dirty-flag rebuild + dynamic layer,
  `(1 - sqD/radiusSq)` falloff, 255 saturation, same-kind sum,
  brighter-kind-wins, and the original fire/electricity/acid/gray palettes.
- Renderer-neutral frame composition with depth merge, hex fill, `?` unknown
  borders, and previous-frame diffing. The default `TextRendererPlugin`
  writes changed cells onto one `Text2d` entity each. The optional hybrid
  renderer replaces walls with shared, extruded 3D autotile meshes while
  leaving actors and UI as text.

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
src/foundation/fov.rs      engine-independent interval FOV algorithm
src/content/               map and symbol-rule parsers
src/debug_ui.rs             optional FPS and runtime performance panel
src/model/                 ECS data split by gameplay domain
src/simulation/            simulation plugin; control, motion, resolution, tiles
src/vision/                vision plugin; FOV cache and player visibility
src/lighting/              light math and palettes
src/presentation/          renderer-neutral lighting and frame composition
src/rendering/             swappable Text2d and extruded-wall output plugins
tests/full_map.rs  headless map1 boot + settle integration test
tests/gameplay.rs  timed input, movement, collision, and vision regressions
tests/allocations.rs  warmed Bevy baseline + full-turn allocation regression
tests/reference_parity.rs  independent C# FOV/light/palette fixtures
tools/generate_reference.py  regenerate fixtures from the pinned checkout
assets/maps/       map1/2/3, map1_test, lightTest
assets/rules/      wall + three direction rules
assets/fonts/      DejaVu Sans Mono + redistribution license
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for layer boundaries, dependency
direction, and where new foundational versus game-specific code belongs.

## Run and verify

```sh
cargo run    # arrows or WASD to step the @ player
cargo run -- --renderer 3d-walls  # extruded 3D walls, text actors and HUD
cargo test   # 41 tests, including allocation and independent C# comparisons
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

The debug performance panel is visible by default and toggles with `F3`. It
shows smoothed FPS/frame time, process and system CPU/RAM, entity count, text
cells updated by the renderer, and current turn pacing.

Note: if your cargo home is not writable, point it somewhere writable,
e.g. `CARGO_HOME=/tmp/cargo-home cargo run`.


The window uses a bundled Unicode font, console-shaped cells, and a camera
that fits the full map when resized. Capture the actual rendered window:

```sh
cargo run -- --screenshot screenshots/bevy-map1.png
cargo run -- --renderer 3d-walls --screenshot screenshots/bevy-map1-3d.png
cargo run -- --screenshot screenshots/bevy-explored.png --walk LLLUUURRRRRRRDDDDDDDDDDDDDDD
```

Screenshot mode uses deterministic 16ms frames, optionally replays `UDLR`
through the real keyboard system, saves a PNG, then exits. It requires GPU
and window-server access. Capture mode renders to a fixed 1100x700 offscreen
target, allows Bevy's render pipelines to warm up, and returns failure if
saving fails. Normal play uses Bevy's real clock and window target. Screenshots
are local artifacts and are excluded from Git.

The 3D wall backend creates only 16 combined meshes (one per wall-neighbour
mask) and 16 palette materials. Wall entities share those handles, allowing
Bevy's renderer to batch and instance matching mesh/material pairs. Its PBR
materials use the already-computed CPU light palette as unlit base colors, so
walls receive the same fire, electricity, acid, and visibility lighting as the
symbol renderer without paying for a second lighting calculation.

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
