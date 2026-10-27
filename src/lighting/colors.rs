//! Named colours for glyphs and markers that are not lit (portals, the fog
//! edge, enemy moods) and for glyphs' own colours.

use crate::model::Rgb;

pub const BLACK: Rgb = Rgb::BLACK;
pub const DARK_BLUE: Rgb = Rgb::new(0, 0, 140);
pub const DARK_GREEN: Rgb = Rgb::new(0, 128, 0);
pub const DARK_CYAN: Rgb = Rgb::new(0, 128, 128);
pub const DARK_RED: Rgb = Rgb::new(140, 0, 0);
pub const DARK_MAGENTA: Rgb = Rgb::new(128, 0, 128);
pub const DARK_YELLOW: Rgb = Rgb::new(153, 128, 0);
pub const GRAY: Rgb = Rgb::new(140, 140, 140);
pub const DARK_GRAY: Rgb = Rgb::new(64, 64, 64);
pub const BLUE: Rgb = Rgb::new(51, 77, 255);
pub const GREEN: Rgb = Rgb::new(51, 230, 51);
pub const CYAN: Rgb = Rgb::new(51, 230, 230);
pub const RED: Rgb = Rgb::new(255, 64, 64);
pub const MAGENTA: Rgb = Rgb::new(230, 51, 230);
pub const YELLOW: Rgb = Rgb::new(255, 230, 51);
pub const WHITE: Rgb = Rgb::new(255, 255, 255);
