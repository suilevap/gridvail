//! Light bounced off lit cells and spread over the grid, one cell a step.
//!
//! Every directly lit cell gives back a share of its light (walls more than
//! floors). That light spreads through open cells, each passing on the
//! brightest it gets less a fixed share, and lights the walls it reaches, so it creeps round corners and down
//! corridors the light itself cannot see. Each kind of light spreads
//! separately, so light bounced off a fire-lit wall can tint a cell lit by
//! another kind.

use crate::model::{BounceLight, LightCell, LightKind};

/// Light kinds, one channel each.
const KINDS: [LightKind; 4] = [
    LightKind::None,
    LightKind::Fire,
    LightKind::Electricity,
    LightKind::Acid,
];

type Energy = [f32; 4];

fn channel(kind: LightKind) -> usize {
    match kind {
        LightKind::None => 0,
        LightKind::Fire => 1,
        LightKind::Electricity => 2,
        LightKind::Acid => 3,
    }
}

/// The base value of every cell, lit by no light.
const AMBIENT: u8 = 1;

/// Bounced light dimmer than this (on the 0-255 scale) is left out: any
/// light at all gives a cell the colour of its kind, and a faint trace of
/// it would colour the dark far beyond where it can be told apart.
const FAINTEST: f32 = 3.0;

/// No neighbour on that side.
pub const NO_NEIGHBOUR: u32 = u32::MAX;

/// The cells bounced light spreads over: whether each lets light through,
/// and its four neighbours (through a portal face, the floor beyond it).
#[derive(Debug, Default)]
pub struct BounceTopology {
    pub open: Vec<bool>,
    pub neighbours: Vec<[u32; 4]>,
}

impl BounceTopology {
    pub fn clear(&mut self) {
        self.open.clear();
        self.neighbours.clear();
    }
}

/// Scratch storage for [`Bounce::apply`], reused between calls.
#[derive(Debug, Default)]
pub struct Bounce {
    lit: Vec<bool>,
    source: Vec<Energy>,
    energy: Vec<Energy>,
    next: Vec<Energy>,
}

impl Bounce {
    /// Fills `layer`'s shadows with the light bounced off its lit cells: a
    /// cell takes the brightest bounced light on it where that is brighter
    /// than its direct light. Adding the two instead would push lit cells
    /// past the brightest colour of their palette.
    pub fn apply(
        &mut self,
        layer: &mut [LightCell],
        topology: &BounceTopology,
        light: &BounceLight,
    ) {
        let cells = layer.len();
        debug_assert_eq!(topology.open.len(), cells);
        debug_assert_eq!(topology.neighbours.len(), cells);
        self.lit.clear();
        self.source.clear();
        for (cell, &open) in layer.iter().zip(&topology.open) {
            let lit = cell.value > AMBIENT;
            let mut source = [0.0; 4];
            if lit {
                let share = if open { light.floor } else { light.wall };
                source[channel(cell.kind)] = cell.value as f32 / 255.0 * share;
            }
            self.lit.push(lit);
            self.source.push(source);
        }
        self.energy.clear();
        self.energy.resize(cells, [0.0; 4]);
        self.next.clear();
        self.next.resize(cells, [0.0; 4]);

        for _ in 0..light.steps {
            for cell in 0..cells {
                let open = topology.open[cell];
                let mut brightest = [0.0f32; 4];
                for &neighbour in &topology.neighbours[cell] {
                    if neighbour == NO_NEIGHBOUR {
                        continue;
                    }
                    let neighbour = neighbour as usize;
                    if topology.open[neighbour] {
                        // An open cell passes on what it gives back or what
                        // reached it, whichever is brighter.
                        brighten(&mut brightest, &self.source[neighbour]);
                        brighten(&mut brightest, &self.energy[neighbour]);
                    } else if open && self.lit[cell] {
                        // A wall is lit on one side only, but a cell does
                        // not know which: it gives light back only to the
                        // lit cells beside it, so it does not shine
                        // through into the dark on its other side.
                        brighten(&mut brightest, &self.source[neighbour]);
                    }
                }
                self.next[cell] = brightest.map(|value| value * light.spread);
            }
            std::mem::swap(&mut self.energy, &mut self.next);
        }

        for (cell, bounced) in layer.iter_mut().zip(&self.energy) {
            let brightest = (0..4)
                .max_by(|&a, &b| bounced[a].total_cmp(&bounced[b]))
                .unwrap_or(0);
            let value = bounced[brightest] * 255.0;
            if value >= FAINTEST && value > cell.value as f32 {
                *cell = LightCell {
                    value: value.round().min(255.0) as u8,
                    kind: KINDS[brightest],
                };
            }
        }
    }
}

fn brighten(brightest: &mut Energy, value: &Energy) {
    for (brightest, value) in brightest.iter_mut().zip(value) {
        *brightest = brightest.max(*value);
    }
}
