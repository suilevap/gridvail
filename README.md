# PavEcsGame Lite Bevy Port

Bevy Rust port of [PavEcsGame](https://github.com/suilevap/PavEcsGame),
commit `bc0b449` — specifically **PavEcsLiteGame**, the latest and most
complete variant (the legacy `PavEcsGame` demo is intentionally not the
target). Designed against `PavEcsGame_Rust_Bevy_migration_plan.md`, kept
next to this file.

## What is ported

- Six entity types from map glyphs: wall `X`, player `p`, enemy `e`,
  electricity `~`, light `i`, acid `%` (all five maps + four rule files
  ship under `assets/`; `map1.txt` loads at startup).
- Turn/token pipeline: 1s token recharge (assign, never add), keyboard and
  random-walk move commands gated by tokens, integer speed + friction,
  direction-from-speed.
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
- Frame composition with depth merge, hex fill, `?` unknown borders, and
  previous-frame diffing onto one `Text2d` cell entity each.

## Deliberate deviations (all commented at the site)

- One Bevy `World` + resources instead of `EcsUniverse` type-worlds.
- `TileSystem` computes masks two-phased; the original mutates neighbour
  masks mid-iteration and drops links asymmetrically.
- FOV caches also key on the occupancy revision so changing walls invalidates
  shadows. Moving actors have `Speed` and do not block sight in either version.
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
src/lib.rs         library root
src/main.rs        window bootstrap
src/game.rs        GamePlugin: setup + chained Lite pipeline
src/components.rs  ECS types and resources
src/map.rs         ASCII map parsing (BOM/CRLF-safe)
src/tiles.rs       wall + direction rule loaders
src/fov.rs         interval FOV algorithm
src/sim.rs         tokens, commands, movement, resolve, destroy, tiles
src/vision.rs      FOV requests, FOV fields, visibility layers
src/lighting.rs    light math + palettes
src/render.rs      light layers, frame composition, cells, HUD
tests/full_map.rs  headless map1 boot + settle integration test
tests/gameplay.rs  timed input, movement, collision, and vision regressions
tests/reference_parity.rs  independent C# FOV/light/palette fixtures
tools/generate_reference.py  regenerate fixtures from the pinned checkout
assets/maps/       map1/2/3, map1_test, lightTest
assets/rules/      wall + three direction rules
assets/fonts/      DejaVu Sans Mono + redistribution license
```

## Run and verify

```sh
cargo run    # arrows or WASD to step the @ player
cargo test   # 36 tests, including independent C# comparisons
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Note: if your cargo home is not writable, point it somewhere writable,
e.g. `CARGO_HOME=/tmp/cargo-home cargo run`.


The window uses a bundled Unicode font, console-shaped cells, and a camera
that fits the full map when resized. Capture the actual rendered window:

```sh
cargo run -- --screenshot screenshots/bevy-map1.png
cargo run -- --screenshot screenshots/bevy-explored.png --walk LLLUUURRRRRRRDDDDDDDDDDDDDDD
```

Screenshot mode uses deterministic 16ms frames, optionally replays `UDLR`
through the real keyboard system, saves a PNG, then exits. It requires GPU
and window-server access. Normal play uses Bevy's real clock. Screenshots
are local artifacts and are excluded from Git.

See [the comparison report](REFERENCE_COMPARISON.md) for what was verified
against upstream and what intentionally differs. Regenerate the C# fixtures
with .NET 10 and a clean checkout at the pinned commit:

```sh
python3 tools/generate_reference.py /path/to/PavEcsGame
```
