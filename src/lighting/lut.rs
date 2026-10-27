//! Light colour: each light kind has a gradient from a dim tone to its
//! brightest colour, baked into a 256-entry lookup table indexed by the
//! cell's light value. Value 0 (no light) is black, and the least light is
//! still dimly visible.

use crate::model::{LightCell, LightKind, Rgb};

/// A light value's colour for every value from 0 to 255.
pub struct LightLut([Rgb; 256]);

impl LightLut {
    /// Bakes a gradient through `stops`, `(value, colour)` pairs in
    /// increasing value from 0 to 255, interpolated between them.
    pub const fn bake(stops: &[(u8, Rgb)]) -> Self {
        assert!(stops.len() >= 2, "a gradient needs two stops");
        assert!(stops[0].0 == 0 && stops[stops.len() - 1].0 == 255);
        let mut table = [Rgb::BLACK; 256];
        let mut stop = 0;
        let mut value = 0;
        while value < 256 {
            while stops[stop + 1].0 < value as u8 {
                stop += 1;
            }
            let (from, low) = stops[stop];
            let (to, high) = stops[stop + 1];
            assert!(from < to, "stops must increase");
            let t = ((value as u32 - from as u32) * 255 / (to as u32 - from as u32)) as u8;
            table[value] = low.lerp(high, t);
            value += 1;
        }
        Self(table)
    }

    pub const fn get(&self, value: u8) -> Rgb {
        self.0[value as usize]
    }
}

/// Light with no kind: grey, white at its brightest.
pub const PLAIN: LightLut = LightLut::bake(&[
    (0, Rgb::BLACK),
    (1, Rgb::new(40, 40, 44)),
    (24, Rgb::new(100, 100, 104)),
    (96, Rgb::new(150, 150, 155)),
    (200, Rgb::new(210, 210, 210)),
    (255, Rgb::new(255, 255, 255)),
]);
/// Fire: embers through amber and yellow to white.
pub const FIRE: LightLut = LightLut::bake(&[
    (0, Rgb::BLACK),
    (1, Rgb::new(56, 24, 6)),
    (24, Rgb::new(120, 70, 10)),
    (100, Rgb::new(200, 150, 30)),
    (170, Rgb::new(255, 215, 70)),
    (235, Rgb::new(255, 240, 170)),
    (255, Rgb::new(255, 255, 255)),
]);
/// Electricity: deep blue through blue to pale cyan.
pub const ELECTRICITY: LightLut = LightLut::bake(&[
    (0, Rgb::BLACK),
    (1, Rgb::new(8, 12, 80)),
    (24, Rgb::new(10, 40, 150)),
    (90, Rgb::new(20, 120, 170)),
    (170, Rgb::new(70, 130, 255)),
    (255, Rgb::new(150, 255, 255)),
]);
/// Acid: dark green to bright green.
pub const ACID: LightLut = LightLut::bake(&[
    (0, Rgb::BLACK),
    (1, Rgb::new(0, 50, 6)),
    (24, Rgb::new(0, 100, 10)),
    (128, Rgb::new(30, 190, 40)),
    (255, Rgb::new(110, 255, 90)),
]);

/// The table a kind of light is coloured from.
pub const fn light_lut(kind: LightKind) -> &'static LightLut {
    match kind {
        LightKind::None => &PLAIN,
        LightKind::Fire => &FIRE,
        LightKind::Electricity => &ELECTRICITY,
        LightKind::Acid => &ACID,
    }
}

/// The colour a cell with this light is drawn in; black without light.
pub fn light_color(cell: &LightCell) -> Rgb {
    light_lut(cell.kind).get(cell.value)
}

/// The dimmest a remembered cell that had any light is drawn in.
const REMEMBERED: Rgb = Rgb::new(40, 40, 44);

/// The darker colour a remembered (not currently seen) cell is drawn in:
/// half way to grey, and well under half as bright, but never too dark to
/// see. Black (no light) stays black.
pub fn dimmed(color: Rgb) -> Rgb {
    if color == Rgb::BLACK {
        return Rgb::BLACK;
    }
    REMEMBERED.lerp(color.lerp(color.grey(), 128), 110)
}
