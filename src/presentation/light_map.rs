use bevy::prelude::*;

use crate::lighting::{
    lit_points, merge_light, Bounce, BounceTopology, LightContext, FULLY_LIT, NO_NEIGHBOUR,
};
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

/// Per map cell, which part of it its brightest light reaches
/// ([`lit_points`]), for drawing shadow edges finer than the grid.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LightEdge {
    /// How much light the brightest light gives the cell.
    pub amount: f32,
    /// How much it would give the cell seen in full: what the part of the
    /// cell it reaches gets.
    pub reach: f32,
    /// The points of the cell it reaches.
    pub lit: u16,
}

impl LightEdge {
    /// Whether the cell is on the edge of a shadow: lit only in part.
    pub fn is_edge(&self) -> bool {
        self.lit != 0 && self.lit != FULLY_LIT
    }
}

/// [`LightEdge`]s of the static lights (kept while they are) and of all
/// the lights, as `StaticLight` and `DynamicLight` keep their light.
#[derive(Resource, Debug, Default)]
pub struct LightEdges {
    static_cells: Vec<LightEdge>,
    pub cells: Vec<LightEdge>,
}

/// The map as bounced light sees it, rebuilt when its blockers or portals
/// change, and the bounce's scratch storage.
#[derive(Default)]
pub struct BounceState {
    topology: BounceTopology,
    bounce: Bounce,
    /// The map size and `MapGrid` revisions `topology` was built from.
    built: Option<(i32, i32, u64, u64)>,
}

#[allow(clippy::too_many_arguments)]
pub fn render_light_layers(
    grid: Res<MapGrid>,
    mut static_layer: ResMut<StaticLight>,
    mut dynamic: ResMut<DynamicLight>,
    edges: Option<ResMut<LightEdges>>,
    mut no_edges: Local<LightEdges>,
    bounce_light: Option<Res<BounceLight>>,
    mut bounce: Local<BounceState>,
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
    let edges = match edges {
        Some(edges) => edges.into_inner(),
        None => &mut *no_edges,
    };
    if edges.static_cells.len() != size {
        edges.static_cells = vec![LightEdge::default(); size];
        static_layer.dirty = true;
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
        edges.static_cells.fill(LightEdge::default());
        for (_, pos, light, fov) in static_sources.iter() {
            blend_source(
                &mut static_layer.data,
                &mut edges.static_cells,
                &grid,
                pos.0,
                &light,
                fov,
            );
        }
    }
    dynamic.data.clone_from(&static_layer.data);
    let LightEdges {
        static_cells,
        cells,
    } = edges;
    cells.clone_from(static_cells);
    for (pos, light, fov) in dynamic_sources.iter() {
        blend_source(&mut dynamic.data, cells, &grid, pos.0, light, fov);
    }
    if let Some(light) = bounce_light.filter(|light| light.enabled) {
        let built = Some((
            grid.width,
            grid.height,
            grid.blocker_revision,
            grid.portal_revision,
        ));
        let BounceState {
            topology,
            bounce,
            built: last,
        } = &mut *bounce;
        if *last != built {
            *last = built;
            build_topology(&grid, topology);
        }
        bounce.apply(&mut dynamic.data, topology, &light);
    }
}

/// Which cells let light through, and each one's four neighbours: stepping
/// into a portal face from the floor in front of it leads to the floor in
/// front of its exit, so bounced light goes through portals as sight does.
fn build_topology(grid: &MapGrid, topology: &mut BounceTopology) {
    topology.clear();
    for y in 0..grid.height {
        for x in 0..grid.width {
            let at = IVec2::new(x, y);
            topology.open.push(!grid.blocks_vision(at));
            let mut neighbours = [NO_NEIGHBOUR; 4];
            for (slot, step) in [IVec2::NEG_Y, IVec2::X, IVec2::Y, IVec2::NEG_X]
                .into_iter()
                .enumerate()
            {
                let mut next = at + step;
                if let Some(face) = grid.portal_at(next).filter(|face| face.side == -step) {
                    next = face.through.apply(next);
                }
                if let Some(index) = grid.idx(next) {
                    neighbours[slot] = index as u32;
                }
            }
            topology.neighbours.push(neighbours);
        }
    }
}

/// Adds one light to `layer` along what it sees. Through a portal the light
/// fades with the distance it travelled (`delta`), not the straight one
/// between its cell and the lit map cell, so a lamp lights the far side of a
/// portal as it would the cells just past it.
///
/// It also notes in `edges` which part of each cell it reaches where it is
/// the brightest light there: where it reaches a cell only in part, which
/// points of the cell it sees directly.
fn blend_source(
    layer: &mut [LightCell],
    edges: &mut [LightEdge],
    grid: &MapGrid,
    center: IVec2,
    light: &LightSource,
    fov: &FovResult,
) {
    let context = LightContext::new((center.x, center.y), light.radius, light.value, light.kind);
    for sample in &fov.samples {
        if sample.value <= 0.0 {
            continue;
        }
        let Some(index) = grid.idx(sample.world).filter(|&index| index < layer.len()) else {
            continue;
        };
        let apparent = center + sample.delta;
        let edge = &mut edges[index];
        let reach = context.reach(apparent.x, apparent.y);
        let amount = sample.value * reach;
        if amount > edge.amount {
            *edge = LightEdge {
                amount,
                reach,
                lit: if sample.value >= 0.999 || !sample.transform.is_identity() {
                    FULLY_LIT
                } else {
                    lit_points(center, sample.world, &|p| grid.blocks_vision(p))
                },
            };
        }
        merge_light(
            &mut layer[index],
            sample.value,
            apparent.x,
            apparent.y,
            &context,
        );
    }
}
