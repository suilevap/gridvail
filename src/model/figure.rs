use bevy::{platform::collections::HashMap, prelude::*};

use super::Rgb;

/// Drawn as a figure (`Figures`, by name) by renderers that show height, in
/// place of its glyph: a few small upright glyphs placed together so they
/// read as a shape, such as `@` over `/|\` over `/\`. Others draw its glyph
/// as before.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Figure(pub String);

impl Figure {
    pub fn named(name: &str) -> Self {
        Self(name.to_string())
    }
}

/// What a figure part draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartGlyph {
    Char(char),
    /// The object's own glyph, so what changes it (a mood, a door opening)
    /// shows on the figure too.
    Own,
}

/// One glyph of a figure, upright on screen.
#[derive(Clone, Debug, PartialEq)]
pub struct FigurePart {
    pub glyph: PartGlyph,
    /// Where the glyph's centre is, on screen, in cells from the centre of
    /// the object's cell: x to the right, y up.
    pub offset: Vec2,
    /// The glyph's size, 1 for a whole cell.
    pub scale: f32,
    /// How far the glyph is turned, counter-clockwise, in radians.
    pub turn: f32,
    /// Multiplies the light it is in.
    pub tint: Rgb,
    /// Drawn only while the object's glyph is one of these; always if
    /// empty. Shows an object's state, such as a mood.
    pub only: Vec<char>,
    /// When its side is drawn mirrored, the glyph keeps its own way round
    /// (its place still mirrors): for a glyph read as itself, such as `@`.
    pub no_flip: bool,
}

impl FigurePart {
    pub fn shown_for(&self, glyph: char) -> bool {
        self.only.is_empty() || self.only.contains(&glyph)
    }
}

/// Which way a figure is seen facing, on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FigureView {
    /// Toward the viewer (down the screen); also anything that faces no way.
    Front,
    /// Away from the viewer (up the screen).
    Back,
    Left,
    Right,
}

impl FigureView {
    pub const ALL: [Self; 4] = [Self::Front, Self::Back, Self::Left, Self::Right];

    pub fn name(self) -> &'static str {
        match self {
            Self::Front => "front",
            Self::Back => "back",
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|view| view.name() == name)
    }

    /// The view of something facing `screen`, a direction on screen (y
    /// down); `Front` for none.
    pub fn of(screen: Vec2) -> Self {
        if screen.length_squared() < 1e-6 {
            Self::Front
        } else if screen.x.abs() > screen.y.abs() {
            if screen.x > 0.0 {
                Self::Right
            } else {
                Self::Left
            }
        } else if screen.y > 0.0 {
            Self::Front
        } else {
            Self::Back
        }
    }
}

/// A figure's parts for each way it can be seen, drawn back to front in
/// the order listed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FigureShape {
    pub views: HashMap<FigureView, Vec<FigurePart>>,
}

impl FigureShape {
    /// The parts to draw for `view`, and whether to mirror them (left to
    /// right). A side not drawn is the other side mirrored, and a view not
    /// drawn falls back to the front.
    pub fn parts(&self, view: FigureView) -> (&[FigurePart], bool) {
        let mirror = match view {
            FigureView::Left => Some(FigureView::Right),
            FigureView::Right => Some(FigureView::Left),
            _ => None,
        };
        if let Some(parts) = self.views.get(&view) {
            (parts, false)
        } else if let Some(parts) = mirror.and_then(|mirror| self.views.get(&mirror)) {
            (parts, true)
        } else {
            (
                self.views
                    .get(&FigureView::Front)
                    .map_or(&[], Vec::as_slice),
                false,
            )
        }
    }
}

/// Every figure by name.
#[derive(Resource, Clone, Debug, Default)]
pub struct Figures(pub HashMap<String, FigureShape>);

impl Figures {
    pub fn get(&self, figure: &Figure) -> Option<&FigureShape> {
        self.0.get(&figure.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(ch: char) -> FigurePart {
        FigurePart {
            glyph: PartGlyph::Char(ch),
            offset: Vec2::ZERO,
            scale: 0.5,
            turn: 0.0,
            tint: Rgb::new(255, 255, 255),
            only: Vec::new(),
            no_flip: false,
        }
    }

    #[test]
    fn screen_directions_pick_views() {
        assert_eq!(FigureView::of(Vec2::Y), FigureView::Front);
        assert_eq!(FigureView::of(Vec2::NEG_Y), FigureView::Back);
        assert_eq!(FigureView::of(Vec2::X), FigureView::Right);
        assert_eq!(FigureView::of(Vec2::NEG_X), FigureView::Left);
        assert_eq!(FigureView::of(Vec2::ZERO), FigureView::Front);
    }

    #[test]
    fn missing_views_mirror_the_other_side_or_show_the_front() {
        let mut shape = FigureShape::default();
        shape.views.insert(FigureView::Front, vec![part('@')]);
        shape.views.insert(FigureView::Left, vec![part('<')]);
        assert_eq!(shape.parts(FigureView::Left).0[0], part('<'));
        let (right, mirrored) = shape.parts(FigureView::Right);
        assert!(mirrored && right[0] == part('<'));
        assert_eq!(shape.parts(FigureView::Back), (&[part('@')][..], false));
    }
}
