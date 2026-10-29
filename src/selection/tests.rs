use bevy::prelude::*;

use super::*;

struct Scene {
    app: App,
    selector: Entity,
    target: Entity,
}

/// A selector at (2, 2) with a turn to spend, and something selectable
/// right of it.
fn scene() -> Scene {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(MapGrid::new(6, 6))
        .init_resource::<TurnState>()
        .add_plugins(SelectionPlugin);
    let selector = app
        .world_mut()
        .spawn((
            Selector,
            Pos(IVec2::new(2, 2)),
            MoveCommand::default(),
            Tokens {
                count: 1,
                recharge: 1,
            },
        ))
        .id();
    let target = app
        .world_mut()
        .spawn((Selectable, Pos(IVec2::new(3, 2))))
        .id();
    let mut grid = app.world_mut().resource_mut::<MapGrid>();
    grid.set(IVec2::new(2, 2), selector);
    grid.set(IVec2::new(3, 2), target);
    Scene {
        app,
        selector,
        target,
    }
}

impl Scene {
    /// One frame with the selector moving `dir`; `repeat` if the input is
    /// held on. Returns the command as left for the moves.
    fn input(&mut self, dir: IVec2, repeat: bool) -> MoveCommand {
        *self
            .app
            .world_mut()
            .get_mut::<MoveCommand>(self.selector)
            .unwrap() = MoveCommand {
            target: dir,
            relative: true,
            active: true,
            repeat,
        };
        self.app.update();
        *self.app.world().get::<MoveCommand>(self.selector).unwrap()
    }

    fn selected(&self) -> Option<Selected> {
        self.app.world().get::<Selected>(self.selector).copied()
    }

    fn actions(&self) -> Vec<ContextAction> {
        let messages = self.app.world().resource::<Messages<ContextAction>>();
        messages.iter_current_update_messages().copied().collect()
    }
}

#[test]
fn moving_into_something_selectable_picks_it_for_free() {
    let mut scene = scene();
    let command = scene.input(IVec2::X, false);
    assert!(!command.active, "the move was not dropped");
    assert_eq!(
        scene.selected(),
        Some(Selected {
            target: scene.target,
            dir: IVec2::X
        })
    );
}

#[test]
fn the_picking_input_held_on_does_nothing() {
    let mut scene = scene();
    scene.input(IVec2::X, false);
    let command = scene.input(IVec2::X, true);
    assert!(!command.active);
    assert!(scene.actions().is_empty());
    assert!(scene.selected().is_some());
}

#[test]
fn a_fresh_input_acts_on_the_pick_and_takes_the_turn() {
    let mut scene = scene();
    scene.input(IVec2::X, false);
    let command = scene.input(IVec2::Y, false);
    assert_eq!(
        scene.actions(),
        vec![ContextAction {
            actor: scene.selector,
            target: scene.target,
            picked: IVec2::X,
            dir: IVec2::Y,
        }]
    );
    assert!(
        command.active && command.target == IVec2::ZERO,
        "not a wait"
    );
    assert_eq!(scene.selected(), None);
}

#[test]
fn a_cancel_or_a_target_that_moved_drops_the_pick() {
    let mut scene = scene();
    scene.input(IVec2::X, false);
    scene.app.world_mut().write_message(CancelIntent);
    scene.app.update();
    assert_eq!(scene.selected(), None);

    scene.input(IVec2::X, false);
    scene
        .app
        .world_mut()
        .get_mut::<Pos>(scene.target)
        .unwrap()
        .0 = IVec2::new(4, 2);
    scene.app.update();
    assert_eq!(scene.selected(), None);
}
