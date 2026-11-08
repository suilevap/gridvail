//! Cues: what is shown waits for its moment in seconds on screen, as
//! actions wait for theirs in turns (`simulation::time`). A `Later<M>` is
//! sent as its `M` once its seconds are up (a target's shake at the blow's
//! contact, blood when it lands, a streak as the swing strikes), so
//! whatever shows `M` plays it when it arrives and never counts time down
//! itself.

use bevy::prelude::*;

use crate::model::{Later, MapLoaded, Waiting};
use crate::schedule::GamePhase;

/// Lets messages of type `M` wait to be shown: a `Later<M>` is sent as an
/// `M` once its seconds are up, in `GamePhase::Cues`. Asking for the same
/// type again (its producer and its reader both may) changes nothing.
pub trait ShowLater {
    fn add_later_message<M: Message + Clone>(&mut self) -> &mut Self;
}

impl ShowLater for App {
    fn add_later_message<M: Message + Clone>(&mut self) -> &mut Self {
        self.add_message::<M>().add_message::<Later<M>>();
        if !self.world().contains_resource::<Waiting<M>>() {
            self.init_resource::<Waiting<M>>()
                .add_message::<MapLoaded>()
                .add_systems(Update, release_later::<M>.in_set(GamePhase::Cues));
        }
        self
    }
}

/// Counts what waits down by the frame's time and sends what is due (a
/// `Later` asked this frame for no time at all goes at once). A new map
/// drops everything that waited.
pub fn release_later<M: Message + Clone>(
    time: Res<Time>,
    mut loaded: MessageReader<MapLoaded>,
    mut later: MessageReader<Later<M>>,
    mut waiting: ResMut<Waiting<M>>,
    mut out: MessageWriter<M>,
) {
    if loaded.read().count() > 0 {
        waiting.clear();
    }
    waiting.age(time.delta_secs());
    for asked in later.read() {
        waiting.push(asked.secs, asked.message.clone());
    }
    waiting.release(|message| {
        out.write(message);
    });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::time::TimeUpdateStrategy;

    use super::*;

    #[derive(Message, Clone, Copy, Debug, PartialEq)]
    struct Ping(u8);

    const FRAME: f32 = 0.01;

    /// What each frame was sent, a list a frame.
    #[derive(Resource, Default)]
    struct Sent(Vec<Vec<Ping>>);

    fn collect(mut pings: MessageReader<Ping>, mut sent: ResMut<Sent>) {
        sent.0.push(pings.read().copied().collect());
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                FRAME,
            )))
            .init_resource::<Sent>()
            // Asked for twice, by a producer and a reader: still one queue.
            .add_later_message::<Ping>()
            .add_later_message::<Ping>()
            .add_systems(Update, collect.after(GamePhase::Cues));
        app.update();
        app
    }

    /// What was sent each frame since the first.
    fn sent(app: &App) -> &[Vec<Ping>] {
        &app.world().resource::<Sent>().0[1..]
    }

    #[test]
    fn what_waits_is_sent_once_when_its_seconds_are_up_and_now_at_once() {
        let mut app = app();
        app.world_mut().write_message(Later::after(0.035, Ping(1)));
        app.world_mut().write_message(Later::now(Ping(0)));
        for _ in 0..10 {
            app.update();
        }
        let sent = sent(&app);
        assert_eq!(sent[0], [Ping(0)]);
        // Three frames of 0.01s after the first leave 0.005s; the fourth
        // sends it.
        assert_eq!(sent[4], [Ping(1)]);
        assert_eq!(sent.concat(), [Ping(0), Ping(1)], "sent other than once");
    }

    #[test]
    fn a_new_map_drops_what_waited() {
        let mut app = app();
        app.world_mut().write_message(Later::after(0.02, Ping(1)));
        app.update();
        app.world_mut().write_message(MapLoaded::default());
        for _ in 0..5 {
            app.update();
        }
        assert!(sent(&app).concat().is_empty(), "outlived its map");
    }
}
