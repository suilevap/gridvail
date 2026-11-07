//! Loader for the figures file (`assets/rules/figures.txt`).

use bevy::math::Vec2;
use bevy::platform::collections::HashMap;

use crate::model::{FigurePart, FigureShape, FigureView, PartGlyph, Rgb};

/// The size of a figure's glyphs unless a part says otherwise: half a cell,
/// the cell font's own 8 px.
pub const DEFAULT_PART_SCALE: f32 = 0.5;
/// Font pixels a cell is wide at the default scale: lengths given in `px`
/// are in these, so parts line up with the glyphs' own pixels.
pub const FONT_PIXELS: f32 = 16.0;

/// Parses figures: a `figure NAME` line, then for each way it can be seen a
/// `view front|back|left|right` line (parts before any are the front), then
/// a line per part, back to front:
///
/// ```text
/// LABEL GLYPH [x=X] [y=Y] [scale=S] [turn=DEGREES] [shade=S | tint=#rrggbb] [only=GLYPHS] [noflip]
/// ```
///
/// `x`, `y` and `scale` are in cells, or in font pixels with `px` (16 a
/// cell; `scale=8px` is the default size). The label only names the part
/// for whoever reads the file. `GLYPH` is a single character, or `own` for
/// the object's own glyph. Blank lines and lines starting with `#` are
/// skipped.
pub fn parse_figures(text: &str) -> Result<HashMap<String, FigureShape>, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut figures: HashMap<String, FigureShape> = HashMap::default();
    let mut current: Option<(String, FigureView)> = None;
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let error = |message: String| format!("line {}: {message}", number + 1);
        if let Some(name) = line.strip_prefix("figure ") {
            let name = name.trim().to_string();
            figures.insert(name.clone(), FigureShape::default());
            current = Some((name, FigureView::Front));
            continue;
        }
        let Some((name, view)) = current.as_mut() else {
            return Err(error("a part before any `figure NAME` line".into()));
        };
        if let Some(named) = line.strip_prefix("view ") {
            *view = FigureView::from_name(named.trim())
                .ok_or_else(|| error(format!("`{named}` is not front, back, left or right")))?;
            continue;
        }
        let mut words = line.split_whitespace();
        let (Some(_label), Some(glyph)) = (words.next(), words.next()) else {
            return Err(error("expected a label and a glyph".into()));
        };
        let glyph = match glyph {
            "own" => PartGlyph::Own,
            _ => {
                let mut chars = glyph.chars();
                match (chars.next(), chars.next()) {
                    (Some(ch), None) => PartGlyph::Char(ch),
                    _ => return Err(error(format!("`{glyph}` is not one glyph or `own`"))),
                }
            }
        };
        let mut part = FigurePart {
            glyph,
            offset: Vec2::ZERO,
            scale: DEFAULT_PART_SCALE,
            turn: 0.0,
            tint: Rgb::new(255, 255, 255),
            only: Vec::new(),
            no_flip: false,
        };
        for word in words {
            // A length in cells, or in font pixels with `px`.
            let length = |value: &str| match value.strip_suffix("px") {
                Some(pixels) => pixels.parse::<f32>().map(|pixels| pixels / FONT_PIXELS),
                None => value.parse::<f32>(),
            };
            let length = |value: &str| {
                length(value)
                    .ok()
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| error(format!("`{word}`: `{value}` is not a length")))
            };
            let number = |value: &str| {
                value
                    .parse::<f32>()
                    .ok()
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| error(format!("`{word}`: `{value}` is not a number")))
            };
            match word.split_once('=') {
                None if word == "noflip" => part.no_flip = true,
                Some(("x", value)) => part.offset.x = length(value)?,
                Some(("y", value)) => part.offset.y = length(value)?,
                Some(("turn", value)) => part.turn = number(value)?.to_radians(),
                Some(("scale", value)) => {
                    part.scale = length(value)?;
                    if part.scale <= 0.0 {
                        return Err(error(format!("`{word}`: a scale is above 0")));
                    }
                }
                Some(("shade", value)) => {
                    let grey = (number(value)?.clamp(0.0, 1.0) * 255.0).round() as u8;
                    part.tint = Rgb::new(grey, grey, grey);
                }
                Some(("tint", value)) => {
                    part.tint = parse_hex(value)
                        .ok_or_else(|| error(format!("`{word}`: expected #rrggbb")))?;
                }
                Some(("only", value)) if !value.is_empty() => {
                    part.only = value.chars().collect();
                }
                _ => return Err(error(format!("`{word}` is not a part option"))),
            }
        }
        figures
            .get_mut(name.as_str())
            .expect("the current figure is listed")
            .views
            .entry(*view)
            .or_default()
            .push(part);
    }
    Ok(figures)
}

fn parse_hex(value: &str) -> Option<Rgb> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok();
    Some(Rgb::new(channel(0)?, channel(2)?, channel(4)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_go_to_the_view_they_are_listed_under() {
        let figures = parse_figures(
            "# a test\nfigure post\nfoot ▪\nhead • y=0.25 scale=1 turn=90 shade=0.5\n\
             view left\nhead < x=-0.1 noflip\n\
             figure mark\nsign own tint=#ff8000 only=!?\n",
        )
        .unwrap();
        let post = &figures["post"];
        let (front, _) = post.parts(FigureView::Front);
        assert_eq!(front.len(), 2);
        assert_eq!(front[0].glyph, PartGlyph::Char('▪'));
        assert_eq!(front[0].scale, DEFAULT_PART_SCALE);
        assert_eq!(front[1].offset, Vec2::new(0.0, 0.25));
        assert!((front[1].turn - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert_eq!(front[1].tint, Rgb::new(128, 128, 128));
        let (right, mirrored) = post.parts(FigureView::Right);
        assert!(mirrored && right[0].glyph == PartGlyph::Char('<') && right[0].no_flip);
        let sign = &figures["mark"].parts(FigureView::Front).0[0];
        assert_eq!(sign.glyph, PartGlyph::Own);
        assert_eq!(sign.tint, Rgb::new(255, 128, 0));
        assert!(sign.shown_for('!') && !sign.shown_for('x'));
    }

    #[test]
    fn lengths_may_be_font_pixels() {
        let figures = parse_figures("figure a\neye █ x=-3px y=4px scale=1px\n").unwrap();
        let eye = &figures["a"].parts(FigureView::Front).0[0];
        assert_eq!(eye.offset, Vec2::new(-3.0, 4.0) / FONT_PIXELS);
        assert_eq!(eye.scale, 1.0 / FONT_PIXELS);
        assert!(parse_figures("figure a\neye █ x=3pt").is_err());
    }

    #[test]
    fn mistakes_are_errors() {
        assert!(parse_figures("foot ▪").is_err());
        assert!(parse_figures("figure a\nfoot ab").is_err());
        assert!(parse_figures("figure a\nview up").is_err());
        assert!(parse_figures("figure a\nfoot ▪ wobble").is_err());
        assert!(parse_figures("figure a\nfoot ▪ tint=red").is_err());
        assert!(parse_figures("figure a\nfoot ▪ scale=0").is_err());
    }

    #[test]
    fn the_bundled_file_parses() {
        let figures = parse_figures(include_str!("../../assets/rules/figures.txt")).unwrap();
        for name in ["person", "rifleman", "officer", "lamp"] {
            assert!(
                !figures[name].parts(FigureView::Front).0.is_empty(),
                "{name}"
            );
        }
    }
}
