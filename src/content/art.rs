//! Shapes drawn as text art in data files: one character marks the origin,
//! another each cell of the shape.

use bevy::math::IVec2;

/// The cells marked `mark` in `art`, as offsets from the one cell marked
/// `origin` (`x` to the right, `y` down the art). Blank lines around the
/// art are ignored, so it can sit on lines of its own in a string.
pub fn cells(art: &str, origin: char, mark: char) -> Result<Vec<IVec2>, String> {
    let rows: Vec<&str> = art.lines().filter(|line| !line.trim().is_empty()).collect();
    let at = |ch: char| {
        rows.iter().enumerate().flat_map(move |(y, line)| {
            line.chars()
                .enumerate()
                .filter(move |&(_, c)| c == ch)
                .map(move |(x, _)| IVec2::new(x as i32, y as i32))
        })
    };
    let mut origins = at(origin);
    let start = origins
        .next()
        .ok_or_else(|| format!("the art needs its origin `{origin}`"))?;
    if origins.next().is_some() {
        return Err(format!("the art has more than one `{origin}`"));
    }
    let cells: Vec<_> = at(mark).map(|cell| cell - start).collect();
    if cells.is_empty() {
        return Err(format!("the art marks no `{mark}`"));
    }
    Ok(cells)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_are_offsets_from_the_origin() {
        let art = "\nxxx\n.@.\n";
        assert_eq!(
            cells(art, '@', 'x').unwrap(),
            vec![IVec2::new(-1, -1), IVec2::new(0, -1), IVec2::new(1, -1)]
        );
    }

    #[test]
    fn art_without_its_origin_or_marks_is_refused() {
        assert!(cells("xx", '@', 'x').unwrap_err().contains('@'));
        assert!(cells("@", '@', 'x').unwrap_err().contains('x'));
        assert!(cells("@@x", '@', 'x').is_err());
    }
}
