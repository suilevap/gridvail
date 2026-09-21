# Architecture

The code is arranged from reusable foundations toward the concrete game:

```text
foundation  content
     \        /
        model
      /   |    \
simulation vision lighting
      \   |    /
      presentation
           |
          app
           |
         main
```

Dependencies should point downward in this diagram. `app` is the composition
root: it chooses the bundled map, defines entity bundles, registers resources,
and fixes system order. Code that exists only to reproduce PavEcsLiteGame
belongs there. Reusable algorithms and systems must not import `app`.

## Layers

- `foundation/` contains engine-independent algorithms. The fractional FOV
  implementation and its ring/range scratch storage live here.
- `content/` parses external map and tile-rule formats. It does not spawn ECS
  entities or decide which map is active.
- `model/` contains ECS data, split into actors, spatial state, vision,
  lighting, world resources, and presentation buffers. It contains no systems.
- `simulation/` contains reusable gameplay systems. Control, turn budgeting,
  motion, conflict resolution, lifecycle, and tile updates are separate files.
- `vision/` converts sensors and blocker state into cached FOV and visibility
  components. The underlying algorithm remains in `foundation/`.
- `lighting/` contains light blending and palette conversion. It does not know
  about Bevy text entities or the application schedule.
- `presentation/` builds light maps and composed frames, then writes frame
  changes to Bevy text and HUD entities.
- `app/` wires the concrete Gridvail/PavEcsLiteGame port together. The bundled
  map choice, exact spawn bundles, schedule ordering, font, and compatibility
  decisions belong here.
- `main.rs` is only the executable/window and screenshot harness.

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
4. Reusable world mutation: the matching gameplay system module.
5. Visual composition or Bevy output: `presentation`.
6. A rule specific to this port's map, entity bundle, or schedule: `app`.

This keeps the C# project's useful separation between common algorithms,
components, and game systems without reproducing its custom ECS infrastructure.
