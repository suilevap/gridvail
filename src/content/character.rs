//! Exact pixel frames, read through the shared RON reader. The text is the
//! editable source of truth, with palette keys instead of image compression.

use serde::Deserialize;

pub const FRAMES_PER_DIRECTION: usize = 10;

#[derive(Deserialize)]
pub struct CharacterSheet {
    pub width: u32,
    pub height: u32,
    pub world_height: f32,
    pub idle_seconds: f32,
    pub walk_frames_per_cell: f32,
    pub palette: Vec<(char, (u8, u8, u8, u8))>,
    pub frames: Vec<String>,
}

impl CharacterSheet {
    pub fn bundled() -> Self {
        crate::content::data::read(
            "characters/trespasser.ron",
            include_str!("../../assets/characters/trespasser.ron"),
        )
        .expect("valid trespasser sheet")
    }

    /// Four rows of ten frames, tightly packed with transparent padding in
    /// each frame. Invalid source data fails at startup rather than clipping.
    pub fn pixels(&self) -> Result<Vec<u8>, String> {
        if self.width == 0
            || self.height == 0
            || self.frames.len() != 4 * FRAMES_PER_DIRECTION
            || !self.world_height.is_finite()
            || self.world_height <= 0.0
            || !self.idle_seconds.is_finite()
            || self.idle_seconds <= 0.0
            || !self.walk_frames_per_cell.is_finite()
            || self.walk_frames_per_cell <= 0.0
        {
            return Err("invalid character dimensions, frame count or timing".into());
        }
        let width = self.width as usize * FRAMES_PER_DIRECTION;
        let mut pixels = vec![0; width * self.height as usize * 4 * 4];
        for (index, frame) in self.frames.iter().enumerate() {
            let rows: Vec<_> = frame.lines().collect();
            if rows.len() != self.height as usize {
                return Err(format!("frame {index}: wrong height"));
            }
            for (y, row) in rows.into_iter().enumerate() {
                if row.chars().count() != self.width as usize {
                    return Err(format!("frame {index}: wrong width"));
                }
                for (x, key) in row.chars().enumerate() {
                    let (_, (r, g, b, a)) = self
                        .palette
                        .iter()
                        .find(|(k, _)| *k == key)
                        .ok_or_else(|| format!("frame {index}: unknown pixel {key}"))?;
                    let atlas_x = index % FRAMES_PER_DIRECTION * self.width as usize + x;
                    let atlas_y = index / FRAMES_PER_DIRECTION * self.height as usize + y;
                    let offset = (atlas_y * width + atlas_x) * 4;
                    pixels[offset..offset + 4].copy_from_slice(&[*r, *g, *b, *a]);
                }
            }
        }
        Ok(pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_frames_are_complete_distinct_and_have_transparent_edges() {
        let sheet = CharacterSheet::bundled();
        let pixels = sheet.pixels().unwrap();
        assert_eq!(pixels.len(), 240 * 128 * 4);
        for frames in sheet.frames.chunks_exact(FRAMES_PER_DIRECTION) {
            assert_ne!(frames[0], frames[2], "walk differs from rest");
            assert_ne!(frames[2], frames[4], "opposite foot contacts differ");
            assert_ne!(frames[6], frames[7], "preparation differs from strike");
            for frame in frames {
                assert!(frame.lines().next().unwrap().chars().all(|c| c == '.'));
                assert!(frame.lines().last().unwrap().chars().all(|c| c == '.'));
            }
        }
        for a in 0..4 {
            for b in a + 1..4 {
                assert_ne!(sheet.frames[a * 10], sheet.frames[b * 10]);
            }
        }
    }

    #[test]
    fn malformed_frames_fail_instead_of_silently_clipping() {
        let mut sheet = CharacterSheet::bundled();
        sheet.frames[0].push('!');
        assert!(sheet.pixels().is_err());
    }
}
