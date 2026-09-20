//! Rendering: light layers, frame composition, cell output, HUD.
//!
//! Mirrors `LightRenderSystem` (static/dynamic lightmaps) and
//! `PrepareForRenderSystem` + `ConsoleRenderSystem` (visibility-gated
//! composition with depth merge, light post-effect, `'?'` unknown borders,
//! and previous-frame diffing).
//!
//! Representation changes (same observable frame): the composed frame is a
//! `Vec<RenderCell>` resource instead of `MapData<RenderItem>`, and output
//! goes to one `Text2d` entity per cell instead of console cursor writes.
//! The static-layer version counter (XOR of entity ids, which the migration
//! plan flags as unreliable) is replaced with an explicit dirty flag plus
//! per-source FOV revisions.

use bevy::prelude::*;

use crate::components::*;
use crate::lighting::{light_to_palette, merge_light, LightContext, DARK_RED};

/// Dynamic light layer (static copy + moving sources).
#[derive(Resource, Debug)]
pub struct DynamicLight {
    pub data: Vec<LightCell>,
}

impl DynamicLight {
    pub fn sized(n: usize) -> Self {
        Self {
            data: vec![LightCell::default(); n],
        }
    }
}

/// Mirrors `LightRenderSystem`: rebuild the static layer when dirty or when
/// a static source changed, then blend it with the moving sources.
pub fn render_light_layers(
    grid: Res<MapGrid>,
    mut static_layer: ResMut<StaticLight>,
    mut dynamic: ResMut<DynamicLight>,
    static_sources: Query<(Entity, &Pos, Ref<LightSource>, &FovResult), Without<Speed>>,
    dynamic_sources: Query<(&Pos, &LightSource, &FovResult), With<Speed>>,
) {
    let n = (grid.width * grid.height) as usize;
    if static_layer.data.len() != n {
        static_layer.resize(grid.width, grid.height);
    }
    if dynamic.data.len() != n {
        dynamic.data = vec![LightCell::default(); n];
    }
    let seen: Vec<(Entity, u64)> = static_sources
        .iter()
        .map(|(e, _, _, fov)| (e, fov.revision))
        .collect();
    if static_layer.dirty
        || seen != static_layer.last_seen_revisions
        || static_sources
            .iter()
            .any(|(_, _, light, _)| light.is_changed())
    {
        static_layer.dirty = false;
        static_layer.last_seen_revisions = seen;
        static_layer.data.fill(LightCell {
            value: 1,
            kind: LightKind::None,
        });
        for (_, pos, light, fov) in static_sources.iter() {
            blend_source(&mut static_layer.data, &grid, pos.0, &light, fov);
        }
    }
    dynamic.data.clone_from(&static_layer.data);
    for (pos, light, fov) in dynamic_sources.iter() {
        blend_source(&mut dynamic.data, &grid, pos.0, light, fov);
    }
}

fn blend_source(
    layer: &mut [LightCell],
    grid: &MapGrid,
    center: IVec2,
    light: &LightSource,
    fov: &FovResult,
) {
    let ctx = LightContext::new((center.x, center.y), light.radius, light.value, light.kind);
    for (i, f) in fov.data.iter().enumerate() {
        if *f <= 0.0 || i >= layer.len() {
            continue;
        }
        let x = i as i32 % grid.width;
        let y = i as i32 / grid.width;
        merge_light(&mut layer[i], *f, x, y, &ctx);
    }
}

/// Mirrors `PrepareForRenderSystem`: swap buffers, forward-render glyphs by
/// visibility + depth, apply the light post-effect.
pub fn compose_frame(
    dynamic: Res<DynamicLight>,
    visibility: Query<&VisibilityMap, With<Player>>,
    glyphs: Query<(&Pos, &Glyph, Option<&Speed>)>,
    mut buffers: ResMut<RenderBuffers>,
) {
    let Ok(vis) = visibility.single() else {
        return;
    };
    buffers.swap();
    buffers.current.fill(RenderCell::default());

    // ForwardRender: visible actors always; static (speedless) decor also
    // when merely known. Deeper glyphs win ties-or-better.
    for (pos, glyph, speed) in glyphs.iter() {
        let Some(idx) = buffers.idx(pos.0) else {
            continue;
        };
        let v = vis.data.get(idx).copied().unwrap_or_default();
        if v.contains(Vis::VISIBLE) || (speed.is_none() && v.contains(Vis::KNOWN)) {
            let cell = &mut buffers.current[idx];
            if cell.depth <= glyph.depth {
                cell.ch = glyph.ch;
                cell.color = glyph.color;
                cell.depth = glyph.depth;
            }
        }
    }

    // PostEffects: light known cells; mark unknown cells that border the
    // known world with '?'.
    for y in 0..buffers.height {
        for x in 0..buffers.width {
            let idx = (y * buffers.width + x) as usize;
            let v = vis.data.get(idx).copied().unwrap_or_default();
            if v.contains(Vis::KNOWN) {
                let light = dynamic.data.get(idx).copied().unwrap_or_default();
                let cell = &mut buffers.current[idx];
                if cell.ch == ' ' || cell.ch == '\0' {
                    if v.contains(Vis::VISIBLE) || is_hex_pos(IVec2::new(x, y)) {
                        cell.ch = '.';
                        cell.color = light_to_palette(&light);
                    }
                } else {
                    cell.color = light_to_palette(&light);
                }
            } else {
                let borders_known = [(0, -1), (-1, 0), (1, 0), (0, 1)].iter().any(|(dx, dy)| {
                    let n = IVec2::new(x + dx, y + dy);
                    let Some(ni) = buffers.idx(n) else {
                        return false;
                    };
                    vis.data
                        .get(ni)
                        .copied()
                        .unwrap_or_default()
                        .contains(Vis::KNOWN)
                        && buffers.current[ni].depth == 0
                });
                if borders_known {
                    buffers.current[idx] = RenderCell {
                        ch: '?',
                        color: DARK_RED,
                        depth: 0,
                    };
                }
            }
        }
    }
}

/// Push changed cells to their `Text2d` entities (mirrors the render-command
/// diffing; one entity per cell instead of console writes).
pub fn flush_cells(
    buffers: Res<RenderBuffers>,
    mut cells: Query<(&MapCell, &mut Text2d, &mut TextColor)>,
) {
    for (cell, mut text, mut color) in cells.iter_mut() {
        let Some(idx) = buffers.idx(cell.0) else {
            continue;
        };
        if buffers.current[idx] != buffers.previous[idx] {
            let c = buffers.current[idx];
            text.0 = if c.ch == '\0' {
                " ".to_string()
            } else {
                c.ch.to_string()
            };
            color.0 = crate::lighting::palette_color(c.color);
        }
    }
}

pub fn update_hud(
    turn: Res<TurnState>,
    grid: Res<MapGrid>,
    players: Query<(&Pos, &Tokens), With<Player>>,
    enemies: Query<Entity, (With<Enemy>, Without<DestroyRequested>)>,
    collisions: Res<CollisionBuffer>,
    mut hud: Query<&mut Text, With<HudText>>,
) {
    let (ppos, ptok) = players
        .iter()
        .next()
        .map(|(p, t)| (p.0, t.count))
        .unwrap_or((IVec2::ZERO, 0));
    for mut text in hud.iter_mut() {
        text.0 = format!(
            "PavEcsGame Lite Bevy port | arrows/WASD\nTick {} | {} | map {}x{} | player ({},{}) | tokens {} | enemies {} | bumps {}",
            turn.tick,
            turn.phase_name(),
            grid.width,
            grid.height,
            ppos.x,
            ppos.y,
            ptok,
            enemies.iter().count(),
            collisions.0.len(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::GRAY;
    use crate::sim::test_app;
    use crate::vision::FovShared;
    use bevy::ecs::system::RunSystemOnce;

    fn vis_map_with(w: i32, h: i32, cells: &[(IVec2, Vis)]) -> VisibilityMap {
        let mut data = vec![Vis::empty(); (w * h) as usize];
        for (p, v) in cells {
            data[(p.y * w + p.x) as usize] = *v;
        }
        VisibilityMap { revision: 0, data }
    }

    #[test]
    fn static_decor_renders_when_known_but_unseen() {
        let mut app = test_app::headless();
        app.init_resource::<FovShared>()
            .insert_resource(DynamicLight::sized(64))
            .insert_resource(RenderBuffers::new(8, 8));
        // Player with a hand-built visibility layer (result path covered
        // in vision tests).
        let all = Vis::VISIBLE | Vis::KNOWN;
        let known_only = Vis::KNOWN;
        let vismap = vis_map_with(
            8,
            8,
            &[
                (IVec2::new(0, 0), all),
                (IVec2::new(1, 0), known_only),
                (IVec2::new(2, 0), known_only),
            ],
        );
        app.world_mut().spawn((
            Active,
            Player(0),
            Pos(IVec2::ZERO),
            Speed::default(),
            vismap,
        ));
        // Speedless decor on a known-but-unseen cell renders...
        app.world_mut()
            .spawn((Active, Pos(IVec2::new(1, 0)), Glyph::new('#', 1, GRAY)));
        // ...a moving actor there does not...
        app.world_mut().spawn((
            Active,
            Pos(IVec2::new(2, 0)),
            Speed::default(),
            Glyph::new('e', 1, 12),
        ));
        // ...and nothing renders on unknown cells.
        app.world_mut()
            .spawn((Active, Pos(IVec2::new(3, 0)), Glyph::new('#', 1, GRAY)));
        app.update();
        // compose_frame is not in the headless chain; run it directly.
        app.world_mut().run_system_once(compose_frame).unwrap();
        let buffers = app.world().resource::<RenderBuffers>();
        assert_eq!(buffers.current[0].ch, '.');
        assert_eq!(buffers.current[1].ch, '#');
        // Known hex cells fill with '.' even when the actor there is unseen...
        assert_eq!(buffers.current[2].ch, '.');
        // ...and unknown cells bordering the known world show '?'.
        assert_eq!(buffers.current[3].ch, '?');
    }

    #[test]
    fn static_light_rebuilds_only_on_change() {
        let mut app = test_app::headless();
        app.insert_resource(StaticLight::new())
            .insert_resource(DynamicLight::sized(64));
        // Static lamp with a flat FOV field.
        let lamp = app
            .world_mut()
            .spawn((
                Active,
                Pos(IVec2::new(4, 4)),
                LightSource {
                    radius: 2,
                    kind: LightKind::Fire,
                    value: 100,
                },
                FovResult {
                    revision: 0,
                    obstacle_revision: 0,
                    pos: IVec2::new(4, 4),
                    radius: 2,
                    data: vec![1.0; 64],
                },
            ))
            .id();
        app.update();
        app.world_mut()
            .run_system_once(render_light_layers)
            .unwrap();
        let lit = app.world().resource::<DynamicLight>().data[(4 * 8 + 4) as usize];
        assert_eq!(lit.value, 100);
        assert_eq!(lit.kind, LightKind::Fire);
        // Second run: no rebuild (dirty flag cleared, revisions match).
        app.world_mut()
            .run_system_once(render_light_layers)
            .unwrap();
        // Bumping the source revision forces a rebuild.
        app.world_mut().get_mut::<FovResult>(lamp).unwrap().revision = 7;
        app.world_mut()
            .run_system_once(render_light_layers)
            .unwrap();
        assert_eq!(
            app.world().resource::<StaticLight>().last_seen_revisions,
            vec![(lamp, 7)]
        );
        // Brightness/type changes do not change FOV revisions, but must
        // still refresh the cached static layer.
        *app.world_mut().get_mut::<LightSource>(lamp).unwrap() = LightSource {
            radius: 2,
            kind: LightKind::Acid,
            value: 200,
        };
        app.world_mut()
            .run_system_once(render_light_layers)
            .unwrap();
        assert_eq!(
            app.world().resource::<DynamicLight>().data[36],
            LightCell {
                value: 200,
                kind: LightKind::Acid
            }
        );
    }
}
