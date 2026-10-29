//! Loader for the figures file (`assets/rules/figures.txt`).

use bevy::math::Vec2;
use bevy::platform::collections::HashMap;

use crate::model::{FigurePart, FigureShape, PartGlyph, Rgb};

/// Parses figures: a `figure NAME` line, then a line per part, bottom to
/// top:
///
/// ```text
/// LABEL GLYPH FROM TO [x=X] [y=Y] [scale=S] [shade=S | tint=#rrggbb] [upright] [only=GLYPHS]
/// ```
///
/// The label only names the part for whoever reads the file. `GLYPH` is a
/// single character, or `own` for the object's own glyph. Blank lines and
/// lines starting with `#` are skipped; a figure named twice takes its last
/// parts.
pub fn parse_figures(text: &str) -> Result<HashMap<String, FigureShape>, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut figures = HashMap::default();
    let mut current: Option<(String, FigureShape)> = None;
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let error = |message: String| format!("line {}: {message}", number + 1);
        let mut words = line.split_whitespace();
        if let Some(name) = line.strip_prefix("figure ") {
            if let Some((name, shape)) = current.take() {
                figures.insert(name, shape);
            }
            current = Some((name.trim().to_string(), FigureShape::default()));
            continue;
        }
        let Some((_, shape)) = current.as_mut() else {
            return Err(error("a part before any `figure NAME` line".into()));
        };
        let (Some(_label), Some(glyph), Some(bottom), Some(top)) =
            (words.next(), words.next(), words.next(), words.next())
        else {
            return Err(error("expected a label, a glyph and two heights".into()));
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
        let height = |word: &str| {
            word.parse::<f32>()
                .ok()
                .filter(|height| height.is_finite() && *height >= 0.0)
                .ok_or_else(|| error(format!("`{word}` is not a height")))
        };
        let (bottom, top) = (height(bottom)?, height(top)?);
        if top < bottom {
            return Err(error(format!("its top {top} is below its bottom {bottom}")));
        }
        let mut part = FigurePart {
            glyph,
            bottom,
            top,
            offset: Vec2::ZERO,
            scale: 1.0,
            tint: Rgb::new(255, 255, 255),
            upright: false,
            only: Vec::new(),
        };
        for word in words {
            let number = |value: &str| {
                value
                    .parse::<f32>()
                    .ok()
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| error(format!("`{word}`: `{value}` is not a number")))
            };
            match word.split_once('=') {
                None if word == "upright" => part.upright = true,
                Some(("x", value)) => part.offset.x = number(value)?,
                Some(("y", value)) => part.offset.y = number(value)?,
                Some(("scale", value)) => {
                    part.scale = number(value)?;
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
        shape.parts.push(part);
    }
    if let Some((name, shape)) = current {
        figures.insert(name, shape);
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
    fn parts_stack_in_the_order_listed() {
        let figures = parse_figures(
            "# a test\nfigure post\nfoot ▪ 0 0.1\n\nhead • 0.3 0.5 y=-0.25 shade=0.5 upright\n\
             figure mark\nsign own 0 0.2 tint=#ff8000 only=!?\n",
        )
        .unwrap();
        let post = &figures["post"];
        assert_eq!(post.parts.len(), 2);
        assert_eq!(post.parts[0].glyph, PartGlyph::Char('▪'));
        assert_eq!(post.parts[1].offset, Vec2::new(0.0, -0.25));
        assert_eq!(post.parts[1].tint, Rgb::new(128, 128, 128));
        assert!(post.parts[1].upright && !post.parts[0].upright);
        assert_eq!(post.top(), 0.5);
        let sign = &figures["mark"].parts[0];
        assert_eq!(sign.glyph, PartGlyph::Own);
        assert_eq!(sign.tint, Rgb::new(255, 128, 0));
        assert!(sign.shown_for('!') && !sign.shown_for('x'));
    }

    #[test]
    fn mistakes_are_errors() {
        assert!(parse_figures("foot ▪ 0 0.1").is_err());
        assert!(parse_figures("figure a\nfoot ab 0 0.1").is_err());
        assert!(parse_figures("figure a\nfoot ▪ 0.2 0.1").is_err());
        assert!(parse_figures("figure a\nfoot ▪ 0 0.1 wobble").is_err());
        assert!(parse_figures("figure a\nfoot ▪ 0 0.1 tint=red").is_err());
    }

    #[test]
    fn the_bundled_file_parses() {
        let figures = parse_figures(include_str!("../../assets/rules/figures.txt")).unwrap();
        for name in ["person", "guard", "lamp"] {
            assert!(!figures[name].parts.is_empty(), "{name}");
        }
    }
}
