//! Gestures (moves out and back that keep the cell) and held moves, timed
//! against each other so one exchange plays in order: an attacker's thrust,
//! the blow landing, the target's reaction.

use bevy::prelude::*;

use crate::model::*;

use super::{ObjectAnimation, Path};

/// Gestures waiting for their delay, each with the seconds still to wait.
#[derive(Component, Debug, Default)]
pub struct PendingGestures(pub Vec<(f32, Gesture)>);

impl PendingGestures {
    /// Seconds until the last one has finished.
    pub fn remaining(&self) -> f32 {
        self.0
            .iter()
            .map(|(wait, gesture)| wait + gesture.secs)
            .fold(0.0, f32::max)
    }
}

/// Moves to new cells held back for the seconds left.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct HeldMoves(pub f32);

/// A gesture being made: an offset over time on top of the object's
/// motion, so a body can slide and shake at once, and what follows the body
/// (the camera) is not shaken by it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    path: Option<Path>,
    elapsed: f32,
    secs: f32,
}

impl Pose {
    /// Starts `path` (out and back from zero) over `secs`, in place of any
    /// gesture under way.
    pub fn start(&mut self, path: Path, secs: f32) {
        *self = Self {
            path: Some(path),
            elapsed: 0.0,
            secs: secs.max(f32::EPSILON),
        };
    }

    pub fn advance(&mut self, dt: f32) {
        if self.path.is_some() {
            self.elapsed += dt;
            if self.elapsed >= self.secs {
                *self = Self::default();
            }
        }
    }

    /// How far the gesture takes the object off its position now.
    pub fn offset(&self) -> Vec2 {
        self.path.map_or(Vec2::ZERO, |path| {
            path.sample(Vec2::ZERO, Vec2::ZERO, self.elapsed / self.secs)
        })
    }

    pub fn remaining(&self) -> f32 {
        if self.path.is_some() {
            (self.secs - self.elapsed).max(0.0)
        } else {
            0.0
        }
    }
}

/// The path a gesture plays.
pub fn gesture_path(shape: GestureShape) -> Path {
    match shape {
        GestureShape::Reach { toward } => Path::Excursion { toward },
        GestureShape::Thrust { toward } => Path::Thrust { toward },
        GestureShape::Shake { toward } => Path::Shake { toward },
    }
}

/// Plays each new gesture now, or queues it on its object until its delay
/// is up; holds an object's moves for as long as the longest hold asked.
pub fn take_gestures(
    mut commands: Commands,
    mut gestures: MessageReader<Gesture>,
    mut holds: MessageReader<HoldMoves>,
    mut objects: Query<(
        &mut ObjectAnimation,
        Option<&mut PendingGestures>,
        Option<&mut HeldMoves>,
    )>,
) {
    for hold in holds.read() {
        let Ok((_, _, held)) = objects.get_mut(hold.entity) else {
            continue;
        };
        match held {
            Some(mut held) => held.0 = held.0.max(hold.secs),
            None => {
                commands.entity(hold.entity).insert(HeldMoves(hold.secs));
            }
        }
    }
    for gesture in gestures.read() {
        let Ok((mut animation, pending, _)) = objects.get_mut(gesture.entity) else {
            continue;
        };
        if gesture.delay <= 0.0 {
            animation
                .pose
                .start(gesture_path(gesture.shape), gesture.secs);
            continue;
        }
        match pending {
            Some(mut pending) => pending.0.push((gesture.delay, *gesture)),
            None => {
                commands
                    .entity(gesture.entity)
                    .insert(PendingGestures(vec![(gesture.delay, *gesture)]));
            }
        }
    }
}

/// Counts queued gestures and holds down, playing each gesture when its
/// delay is up and letting moves go when a hold is over.
pub fn advance_gestures(
    mut commands: Commands,
    time: Res<Time>,
    mut pending: Query<(Entity, &mut ObjectAnimation, &mut PendingGestures)>,
    mut held: Query<(Entity, &mut HeldMoves)>,
) {
    let dt = time.delta_secs();
    for (entity, mut animation, mut queue) in &mut pending {
        for (wait, gesture) in queue.0.iter_mut() {
            *wait -= dt;
            if *wait <= 0.0 {
                animation
                    .pose
                    .start(gesture_path(gesture.shape), gesture.secs);
            }
        }
        queue.0.retain(|(wait, _)| *wait > 0.0);
        if queue.0.is_empty() {
            commands.entity(entity).remove::<PendingGestures>();
        }
    }
    for (entity, mut hold) in &mut held {
        hold.0 -= dt;
        if hold.0 <= 0.0 {
            commands.entity(entity).remove::<HeldMoves>();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::time::TimeUpdateStrategy;

    use super::*;
    use crate::animation::{MotionStyle, ObjectAnimationPlugin};

    const FRAME: f32 = 0.016;

    fn app() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                FRAME,
            )))
            .insert_resource(MapGrid::new(8, 8))
            .init_resource::<TurnPacing>()
            .init_resource::<CollisionBuffer>()
            .insert_resource(MotionStyle::Tween {
                duration: 0.1,
                easing: EaseFunction::Linear,
            })
            .add_plugins(ObjectAnimationPlugin);
        let entity = app.world_mut().spawn(Pos(IVec2::new(2, 2))).id();
        app.update();
        app.update();
        (app, entity)
    }

    fn shown(app: &App, entity: Entity) -> Vec2 {
        app.world().get::<AnimatedPos>(entity).unwrap().drawn()
    }

    fn frames(app: &mut App, n: usize) {
        for _ in 0..n {
            app.update();
        }
    }

    #[test]
    fn a_held_object_stays_until_the_hold_is_over_then_moves() {
        let (mut app, entity) = app();
        app.world_mut()
            .write_message(HoldMoves { entity, secs: 0.1 });
        app.world_mut().get_mut::<Pos>(entity).unwrap().0 = IVec2::new(3, 2);
        frames(&mut app, 4);
        assert_eq!(shown(&app, entity), Vec2::new(2.0, 2.0), "moved while held");
        let pacing = app.world().resource::<TurnPacing>();
        assert!(
            pacing.animation().unwrap() > Duration::ZERO,
            "the turn did not wait"
        );
        frames(&mut app, 20);
        assert_eq!(shown(&app, entity), Vec2::new(3.0, 2.0));
    }

    #[test]
    fn a_delayed_gesture_waits_for_its_delay() {
        let (mut app, entity) = app();
        app.world_mut().write_message(Gesture {
            entity,
            shape: GestureShape::Reach { toward: Vec2::X },
            secs: 0.2,
            delay: 0.1,
        });
        frames(&mut app, 4);
        assert_eq!(shown(&app, entity), Vec2::new(2.0, 2.0), "played early");
        frames(&mut app, 8);
        assert!(shown(&app, entity).x > 2.1, "never played");
        frames(&mut app, 20);
        assert_eq!(
            shown(&app, entity),
            Vec2::new(2.0, 2.0),
            "did not come back"
        );
    }

    #[test]
    fn a_thrust_pulls_back_first_and_reaches_furthest_at_its_middle() {
        let path = gesture_path(GestureShape::Thrust { toward: Vec2::X });
        let at = |s: f32| path.sample(Vec2::ZERO, Vec2::ZERO, s).x;
        assert!(at(0.2) < 0.0, "no pull back");
        assert!((at(0.5) - 1.0).abs() < 1e-4, "contact is not at the middle");
        assert!(at(0.4) < at(0.5) && at(0.6) < at(0.5));
        assert!(at(1.0).abs() < 1e-4);
    }
}
