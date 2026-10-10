# Portals: design

Portals let the player see and walk from one place on the map to another
as if the two places were joined. What is on screen is derived from the
player's position and field of view, not drawn straight from the map grid.

This document records the agreed design. The steps at the end are built in
order; each one keeps the game unchanged on maps without portals.

## Decisions

1. **The view is a camera that follows the player.** `ViewCamera` holds the
   map point at the screen centre, a rotation, a zoom and an offset;
   `CameraOperator` keeps it on the player's shown (animated) position and
   eases every change (step 0, done). The player stays still and the world
   moves around them. With portals the camera becomes the origin of the
   player's view frame, so stepping through a portal causes no jump on
   screen, and a rotating portal turns the camera with the player.
2. **Portals live in walls.** A portal is one face of a wall cell. Seen
   from the floor in front of that face, the wall shows what lies beyond the
   paired face instead. The rest of a wall's angular slice still occludes:
   the part of a cell that is not seen through the portal counts as hidden.
3. **Portals are dynamic.** They are components on entities and can be
   created, removed, retargeted or moved at runtime, like doors. The grid
   keeps a portal revision next to `blocker_revision`, and every FOV cache
   is keyed on it.
4. **Moves through a portal are smooth.** Walking into a portal face moves
   the actor to the floor in front of the paired face and continues the
   same animation there, so it emerges from the exit face instead of
   jumping. The camera follows it.
5. **Remembered cells are dimmer.** Cells known but not visible are drawn
   with a darker palette and come from the world around the player in the
   current view frame. Wherever something is visible (directly or through a
   portal) the live view replaces the memory. Only what is seen directly is
   remembered: cells seen through a portal are visible while in sight, but
   are not marked known.
6. **Translation first, then 90° rotations.** The two faces of a portal may
   point in different directions, so going through can turn the view by
   0°, 90°, 180° or 270°. The first version supports only face pairs that
   need no rotation; rotation follows, built on the same transform.

## Portal transform

A portal face is a wall cell `a` and its open side `n_a`: the unit vector
from the wall towards the floor a viewer stands on. It pairs with a face
`b`, `n_b`. Looking into `a` means looking along `-n_a`; the view comes
out of `b` along `n_b`. The rotation `R` is the quarter turn with
`R(-n_a) = n_b`, and a cell `c` seen through the face maps to the world
cell

```text
T(c) = b + n_b + R(c - a)
```

so the wall cell itself (`c = a`) shows the floor in front of `b`, and
cells further in continue from there. The floor in front of `a` maps onto
`b` itself: `T(a + n_a) = b`, which is where an actor stepping through
starts its emerging animation. Translation-only portals are the case
`n_b = -n_a` (`R` is the identity). Facing and other direction vectors
rotate by `R`; transforms of nested portals compose.

## Field of view

`foundation::fov` walks square rings and tracks occluded angle ranges.
Portals add a list of **windows**: an angular range plus the transform to
use inside it.

- A ring cell whose slice falls in a window is the cell `T(origin + delta)`;
  `is_obstacle` is asked about that world cell, so walls behind the portal
  cast shadows.
- A visible portal face inside a window opens a nested window whose
  transform is the composition. Each ring is still visited once, so the
  radius bounds the nesting (a portal seen in a portal costs nothing extra).
- The part of a slice outside the window is occluded by the portal's wall.
  A slice split between a window and unoccluded direct space (only at a
  portal's edge) uses whichever covers more of it.
- A sample carries both its view `delta` and its `world` cell.

Lights and other sensors keep world-indexed results; using the portal-aware
FOV for them lets light shine through portals, as an optional later step.

## Player view

The player's vision output becomes **view-indexed**: for each cell around
the camera, the world cell it shows, the transform it is seen through, and
how visible it is. One world cell can appear more than once. Known cells
(`Vis::KNOWN`) stay world-indexed, as now.

`compose_frame` then fills a view-sized frame from that map instead of
writing each glyph at its world position. Objects get an instance per place
they are seen, keyed by entity and view instance, placed at the transform of
their animated position. Wall and direction glyphs are turned by the
transform they are seen through (the same lookup the turned view uses), so
rotated views show the right shapes. Wall shapes keep the masks computed
from their map neighbours, so a wall next to a portal may not join up with
what is seen through it; whether that needs fixing is decided once portals
can be seen (optional step 8).

## Steps

0. A renderer-neutral `ViewCamera` (position, rotation, zoom, offset)
   moved by a `CameraOperator` that follows the player and eases changes;
   both renderers draw from it and movement keys follow the turned view.
   Wall shapes and facing markers are drawn turned with the view (each
   rule glyph maps to the glyph of its turned mask or direction). **Done.**
1. Portal model: `Portal` components on walls (open side, exit), synced to
   the grid's portal faces with a `portal_revision`; digits `1`-`9` in maps
   pair portal walls; `assets/maps/portals.txt` shows them
   (`--map assets/maps/portals.txt`). **Done.**
2. Portal-aware FOV (`PortalFovComputer`) with windows and transforms,
   including portals seen through portals; maps without portals produce
   identical samples. **Done.**
3. Moving through a portal, with the animation continuing at the exit face.
4. The player's view through portals (`PlayerView`) and a frame whose seen
   cells show the map cells they look onto; objects drawn once per place
   they are seen. Remembered cells dimmer. **Done.**

   Built differently from the plan above: the frame stays map-sized in
   the player's map coordinates instead of becoming view-indexed, so
   renderers needed no change. Cells seen through a portal that would fall
   outside the map are not drawn yet (keep portal rooms away from the map
   edge).
5. The 3D renderer in view space.
6. 90° rotations: rotated transforms, facing, and a view frame that turns
   with the player.
7. Optional: light through portals.
8. Optional, after trying portals on screen: wall shapes joined across
   portal edges, by autotiling walls from the neighbours seen in the view
   instead of their map neighbours. Skipped unless the seams look wrong.
