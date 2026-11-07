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

/// The cells of a shape in the order it is gone through, as offsets from
/// the one cell marked `origin`: each marked with a digit, `1` first, or
/// all with `x`, in the order they read (the art may not mix the two).
pub fn ordered(art: &str, origin: char) -> Result<Vec<IVec2>, String> {
    let digits: Vec<(char, IVec2)> = ('1'..='9')
        .filter_map(|digit| {
            cells(art, origin, digit)
                .ok()
                .map(|cells| cells.into_iter().map(move |cell| (digit, cell)))
        })
        .flatten()
        .collect();
    let crosses = cells(art, origin, 'x');
    match (digits.is_empty(), crosses) {
        (true, crosses) => crosses.map_err(|_| "the art marks no cell (`1`-`9` or `x`)".into()),
        (false, Ok(_)) => Err("the art mixes digits with `x`".into()),
        (false, Err(_)) => {
            if let Some(origin_error) = cells(art, origin, digits[0].0).err() {
                return Err(origin_error);
            }
            Ok(digits.into_iter().map(|(_, cell)| cell).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_give_the_order_and_crosses_the_reading_order() {
        assert_eq!(
            ordered("\n21\n3@\n", '@').unwrap(),
            vec![IVec2::new(0, -1), IVec2::new(-1, -1), IVec2::new(-1, 0)]
        );
        assert_eq!(ordered("x\n@", '@').unwrap(), vec![IVec2::new(0, -1)]);
        assert!(ordered("1x\n@.", '@').is_err());
        assert!(ordered("@", '@').is_err());
    }

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
