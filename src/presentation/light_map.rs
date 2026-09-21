use bevy::prelude::*;

use crate::lighting::{merge_light, LightContext};
use crate::model::*;

#[derive(Resource, Debug)]
pub struct DynamicLight {
    pub data: Vec<LightCell>,
}

impl DynamicLight {
    pub fn sized(size: usize) -> Self {
        Self {
            data: vec![LightCell::default(); size],
        }
    }
}

pub fn render_light_layers(
    grid: Res<MapGrid>,
    mut static_layer: ResMut<StaticLight>,
    mut dynamic: ResMut<DynamicLight>,
    static_sources: Query<(Entity, &Pos, Ref<LightSource>, &FovResult), Without<Speed>>,
    dynamic_sources: Query<(&Pos, &LightSource, &FovResult), With<Speed>>,
) {
    let size = (grid.width * grid.height) as usize;
    if static_layer.data.len() != size {
        static_layer.resize(grid.width, grid.height);
    }
    if dynamic.data.len() != size {
        dynamic.data = vec![LightCell::default(); size];
    }
    static_layer.current_revisions.clear();
    static_layer.current_revisions.extend(
        static_sources
            .iter()
            .map(|(entity, _, _, fov)| (entity, fov.revision)),
    );
    let revisions_changed = static_layer.current_revisions != static_layer.last_seen_revisions;
    if static_layer.dirty
        || revisions_changed
        || static_sources
            .iter()
            .any(|(_, _, light, _)| light.is_changed())
    {
        static_layer.dirty = false;
        let StaticLight {
            last_seen_revisions,
            current_revisions,
            ..
        } = &mut *static_layer;
        std::mem::swap(last_seen_revisions, current_revisions);
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
    let context = LightContext::new((center.x, center.y), light.radius, light.value, light.kind);
    for (index, visibility) in fov.data.iter().enumerate() {
        if *visibility <= 0.0 || index >= layer.len() {
            continue;
        }
        let x = index as i32 % grid.width;
        let y = index as i32 / grid.width;
        merge_light(&mut layer[index], *visibility, x, y, &context);
    }
}
