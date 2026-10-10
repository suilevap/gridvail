use bevy::prelude::*;

#[derive(Component, Clone, Copy, Debug)]
pub struct Player(pub usize);

#[derive(Component, Clone, Copy, Debug)]
pub struct Enemy;

#[derive(Component, Clone, Copy, Debug)]
pub struct Wall;

#[derive(Component, Clone, Copy, Debug)]
pub struct Lamp;

#[derive(Component, Clone, Copy, Debug)]
pub struct AcidPool;

#[derive(Component, Clone, Copy, Debug)]
pub struct ElectroField;

/// Participates in the occupancy grid. Bound decorations do not.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Collider;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Active;

/// Action budget. `count` is assigned from `recharge`, never added.
#[derive(Component, Clone, Copy, Debug)]
pub struct Tokens {
    pub count: i32,
    pub recharge: i32,
}

impl Tokens {
    pub const fn new(recharge: i32) -> Self {
        Self { count: 0, recharge }
    }
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveCommand {
    pub target: IVec2,
    pub relative: bool,
    pub active: bool,
}

/// What the player means to do with the movement input they are holding:
/// the direction on screen they hold, and the map direction it meant when
/// they first held it.
///
/// The map direction is latched, as in games whose camera cuts between
/// views: while the same direction is held, the player keeps walking the
/// same way on the map even if the view turns (Q/E, a view easing back to
/// north after a portal). It changes only with the intent (a new direction
/// or a release), or with the player: a portal that turns them turns it
/// too, so a held step goes on through.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoveIntent {
    pub held: Option<HeldMove>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeldMove {
    /// The direction held, on screen (up is `(0, -1)`).
    pub screen: IVec2,
    /// The map direction it was latched to.
    pub map: IVec2,
}

impl MoveIntent {
    /// The map direction to walk for `screen` held now, or `None` once
    /// nothing is held; `map_direction` turns a newly held screen
    /// direction into a map direction.
    pub fn hold(
        &mut self,
        screen: Option<IVec2>,
        map_direction: impl FnOnce(IVec2) -> IVec2,
    ) -> Option<IVec2> {
        let Some(screen) = screen else {
            self.held = None;
            return None;
        };
        match self.held {
            Some(held) if held.screen == screen => Some(held.map),
            _ => {
                let map = map_direction(screen);
                self.held = Some(HeldMove { screen, map });
                Some(map)
            }
        }
    }
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct DestroyRequested;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct DirectionBasedOnSpeed;

#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct DirectionTile {
    pub rule: String,
}

#[derive(Component, Clone, Debug, Default)]
pub struct Tile {
    pub mask: u8,
    pub rule: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_direction_is_latched_until_the_intent_changes() {
        let mut intent = MoveIntent::default();
        let turned = |screen: IVec2| IVec2::new(-screen.y, screen.x);
        assert_eq!(intent.hold(Some(IVec2::NEG_Y), turned), Some(IVec2::X));
        // Held on: the latched direction, whatever the view says now.
        assert_eq!(intent.hold(Some(IVec2::NEG_Y), |s| s), Some(IVec2::X));
        // Another direction latches anew.
        assert_eq!(intent.hold(Some(IVec2::X), |s| s), Some(IVec2::X));
        assert_eq!(intent.hold(Some(IVec2::NEG_Y), |s| s), Some(IVec2::NEG_Y));
        // Let go, nothing is held.
        assert_eq!(intent.hold(None, turned), None);
        assert_eq!(intent.held, None);
    }
}
