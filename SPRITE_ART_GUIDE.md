# Gridvail sprite art guide

Use this brief for future player and enemy concepts. The direction combines
minimal pixel silhouettes, Victorian industrial decay and occult machinery.
Bleak Sword is a reference for economy, readability and presentation; create
original characters rather than copying its designs.

## Visual rules

- Build deliberate square pixel clusters. Target roughly 24×32 native pixels
  for a standard humanoid; treat generated images as enlarged concepts until
  their actual pixel grid has been cleaned up.
- Use warm ivory, muted grey and charcoal, with sparse rusty-red accents.
  Keep colours flat inside sprites; add atmospheric lighting in the renderer.
- Recognise roles by silhouette: lean player, tall watchman, hunched scavenger,
  broad furnace guard, low mechanical hound, narrow occult surveyor.
- Suggest worn coats, masks, weapons and machinery with a few large shapes.
  Keep negative space between limbs, equipment and torso. Remove details that
  disappear at gameplay size.
- Aim for about 16–32 simple block or glyph-like components per character.
  This is a simplification budget, not a requirement to use literal text glyphs.
- Keep animation expressive through poses, anticipation and clear timing.
  Preserve body volume and costume between frames.

## Views and sheet layout

Use four directions: left, right, away and toward the camera, viewed from a
slightly elevated three-quarter angle. Match the actual game camera when
preparing production assets. Keep equipment handedness consistent across views;
blindly mirroring an asymmetric character can swap its weapon hand.

Use one character per row and the four directions as columns. Keep equal cell
sizes, consistent scale and a shared foot anchor. Leave padding for weapons.
Use a plain dark background for concept review; request transparency for asset
exports. No labels, scenery, ground shadows, bloom or baked lighting.

The current game uses multiline symbol billboards in its 3D renderer. This guide
sets the art direction for sprite concepts; it does not introduce a texture
atlas loader or replace that renderer. Keep simulation coordinates, collisions,
FOV and actor behaviour independent of visual assets.

## Starting roster

| Role | Defining shapes and equipment |
| --- | --- |
| Player / trespasser | Lean cloak, scarf, small mask, short sword |
| Watchman | Tall coat, peaked helmet, baton, red armband |
| Scavenger | Hunched shoulders, worn hood, hook |
| Furnace guard | Wide armour, hammer, small red furnace slit |
| Mechanical hound | Low body, angular legs, narrow mechanical head |
| Occult surveyor | Thin silhouette, tall narrow hat, long instrument staff |

These are visual concepts, not a commitment to six gameplay actor types.

## Reusable generation prompt

> Create a sprite concept sheet for GRIDVAIL, a Victorian industrial occult
> game. Use minimalist pixel art with crisp square clusters, strong silhouettes
> and economical detail. Characters should feel designed at approximately
> 24×32 native pixels, enlarged with nearest-neighbour scaling.
>
> Palette: warm ivory, muted grey and charcoal, with sparse rusty-red accents.
> Use flat colours within sprites.
>
> Character: [ROLE, BODY SHAPE, CLOTHING AND EQUIPMENT].
>
> Show four matching views: left, right, back and front, suitable for a
> square-grid game viewed from a slightly elevated three-quarter camera.
> Keep character scale, equipment, handedness, proportions and foot anchor
> consistent. Separate limbs clearly from the torso.
>
> Present in evenly spaced cells on a solid dark background. No text, scenery,
> shadows, bloom, gradients, antialiasing or painterly texture. Prioritise
> readability at small size. Create original character designs.

For continuity, attach the approved roster and append:

> Match the reference's palette, pixel density, proportions and simplification.
> Change only [REQUESTED CHANGE].

## Generated reference sheet

![Gridvail player and enemy concepts in four directions](docs/art/gridvail-sprite-roster.png)

The generated concept sheet above is included in the repository. Rows show the
player, watchman, scavenger, furnace guard, mechanical hound and occult surveyor;
columns show left, right, back and front. Attach this PNG when generating new
concepts. It is a visual reference, not a production-ready animation atlas;
apply the cleanup checks below before runtime use.

## Production cleanup and acceptance

1. Review at actual gameplay size on both dark and light floors. Every role
   should remain recognisable without its red accent.
2. Rebuild or clean onto an exact pixel grid and fixed palette. Do not assume
   the generator followed the requested native resolution or equal cell sizes.
3. Check all directions for matching proportions, costume and weapon hand.
4. Export isolated frames with transparent backgrounds, equal dimensions and
   an identical foot anchor; use nearest-neighbour filtering.
5. Check animation for sliding feet, changing volume and equipment flicker.
6. Review with the game's lighting and camera. Keep readability before adding
   bloom, depth of field or other presentation effects.
