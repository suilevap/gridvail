//! Loader for the glyph heights file (`assets/rules/statures.txt`).

use std::collections::HashMap;

/// Parses lines of a height followed by the glyphs that stand that tall.
/// Blank lines and lines starting with `#` are skipped; a glyph listed
/// twice takes its last height.
pub fn parse_statures(text: &str) -> Result<HashMap<char, f32>, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut statures = HashMap::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (height, glyphs) = line
            .split_once(char::is_whitespace)
            .ok_or_else(|| format!("line {}: expected a height and glyphs", number + 1))?;
        let height: f32 = height
            .parse()
            .ok()
            .filter(|height: &f32| height.is_finite() && *height >= 0.0)
            .ok_or_else(|| format!("line {}: `{height}` is not a height", number + 1))?;
        for glyph in glyphs.chars().filter(|ch| !ch.is_whitespace()) {
            statures.insert(glyph, height);
        }
    }
    Ok(statures)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights_apply_to_every_glyph_on_their_line() {
        let statures = parse_statures("# walls\n0.5 ═║\n\n1 @ \n0.25 ║\n").unwrap();
        assert_eq!(statures[&'═'], 0.5);
        assert_eq!(statures[&'@'], 1.0);
        // Listed again, it takes the later height.
        assert_eq!(statures[&'║'], 0.25);
        assert_eq!(statures.len(), 3);
    }

    #[test]
    fn a_line_without_a_height_is_an_error() {
        assert!(parse_statures("═║").is_err());
        assert!(parse_statures("-1 ═").is_err());
    }

    #[test]
    fn the_bundled_file_parses() {
        let statures = parse_statures(include_str!("../../assets/rules/statures.txt")).unwrap();
        assert!(statures[&'╬'] > 0.0);
    }
}
