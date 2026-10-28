/// An sRGB colour, 8 bits per channel: what glyphs are drawn in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0, 0, 0);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// The colour `t / 255` of the way from `self` to `other`.
    pub const fn lerp(self, other: Rgb, t: u8) -> Rgb {
        const fn channel(a: u8, b: u8, t: u8) -> u8 {
            let (a, b, t) = (a as i32, b as i32, t as i32);
            (a + ((b - a) * t + 127) / 255) as u8
        }
        Rgb::new(
            channel(self.r, other.r, t),
            channel(self.g, other.g, t),
            channel(self.b, other.b, t),
        )
    }

    /// Its brightness (Rec. 601 luma) as a grey.
    pub const fn grey(self) -> Rgb {
        let luma = ((self.r as u32 * 299 + self.g as u32 * 587 + self.b as u32 * 114) / 1000) as u8;
        Rgb::new(luma, luma, luma)
    }

    /// `#rrggbb`.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

impl From<Rgb> for bevy::prelude::Color {
    fn from(c: Rgb) -> Self {
        bevy::prelude::Color::srgb_u8(c.r, c.g, c.b)
    }
}
