use super::*;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(MapGrid::new(8, 8))
        .add_message::<Attack>()
        .add_message::<Strike>()
        .add_message::<Hit>()
        .add_message::<Collision>()
        .add_message::<Died>()
        .add_plugins(ChoreographyPlugin);
    app
}

fn sent<M: Message + Clone>(app: &App) -> Vec<M> {
    let messages = app.world().resource::<Messages<M>>();
    messages.iter_current_update_messages().cloned().collect()
}

fn timing(app: &App) -> Timing {
    app.world().resource::<Timing>().clone()
}

fn guard(app: &mut App, at: IVec2, windup: u8) -> Entity {
    app.world_mut()
        .spawn((
            Pos(at),
            Weapon {
                area: Area(vec![IVec2::X]),
                damage: 1,
                windup,
            },
            WeaponLook(SwingLook {
                swing: Some(Swing::Sweep { reach: 0.6 }),
                streak: Some("swoosh".into()),
            }),
            Bleeds("blood".into()),
        ))
        .id()
}

#[test]
fn the_timings_are_data() {
    let timing = Timing::default();
    assert!(timing.0.swing_secs > 0.0);
    assert!(timing.release_contact() < timing.contact());
}

#[test]
fn an_aimed_weapon_winds_up_as_it_looks_and_a_dropped_aim_unwinds() {
    let mut app = app();
    let wielder = guard(&mut app, IVec2::new(3, 3), 1);
    app.world_mut()
        .entity_mut(wielder)
        .insert(Telegraph { dir: IVec2::X });
    app.update();
    let wind_ups = sent::<WindUp>(&app);
    assert_eq!(wind_ups.len(), 1);
    assert!(matches!(wind_ups[0].shape, GestureShape::Sweep { .. }));
    app.world_mut().entity_mut(wielder).remove::<Telegraph>();
    app.update();
    let releases = sent::<Release>(&app);
    assert_eq!(releases.len(), 1);
    assert!(!releases[0].strike, "a dropped aim struck");
}

#[test]
fn a_wound_up_strike_releases_and_streaks_at_once() {
    let mut app = app();
    let wielder = guard(&mut app, IVec2::new(3, 3), 1);
    app.world_mut().write_message(Strike {
        wielder,
        dir: IVec2::X,
    });
    app.update();
    let releases = sent::<Release>(&app);
    assert!(releases.len() == 1 && releases[0].strike);
    assert!(
        sent::<Gesture>(&app).is_empty(),
        "swung whole over the wind-up"
    );
    let streaks = sent::<PlayEffect>(&app);
    assert_eq!(streaks.len(), 1);
    assert_eq!(
        (streaks[0].effect.as_str(), streaks[0].delay),
        ("swoosh", 0.0)
    );
}

/// Moves that hit what is ahead, sweeping round to the left.
fn sweeping_moves() -> Maneuvers {
    let sweep = Maneuver {
        name: "sweep left".into(),
        area: Area(vec![IVec2::X, IVec2::new(1, -1), IVec2::NEG_Y]),
        stop: false,
        hit: 1,
        knock: 1,
        stance: Stance::Stay,
        turn: true,
    };
    Maneuvers {
        forward: sweep.clone(),
        left: sweep.clone(),
        right: sweep.clone(),
        back: sweep,
    }
}

#[test]
fn an_attack_swings_round_the_way_its_cells_go_and_holds_its_actor() {
    let mut app = app();
    let actor = app
        .world_mut()
        .spawn((
            Pos(IVec2::new(2, 2)),
            sweeping_moves(),
            ManeuverLooks {
                left: SwingLook {
                    swing: Some(Swing::Sweep { reach: 0.5 }),
                    streak: None,
                },
                ..default()
            },
        ))
        .id();
    app.world_mut().write_message(Attack {
        actor,
        relation: Relation::Left,
        facing: IVec2::X,
        dir: IVec2::NEG_Y,
    });
    app.update();
    let gestures = sent::<Gesture>(&app);
    assert_eq!(gestures.len(), 1);
    match gestures[0].shape {
        // Facing east, its cells go round to the north.
        GestureShape::Sweep { across, .. } => assert_eq!(across, Vec2::NEG_Y),
        shape => panic!("a sweep swung {shape:?}"),
    }
    let holds = sent::<HoldMoves>(&app);
    assert!(holds.iter().any(|hold| hold.entity == actor));
    assert!(sent::<PlayEffect>(&app).is_empty(), "a streak nobody named");
}

#[test]
fn an_attack_that_looks_like_nothing_makes_no_gesture() {
    let mut app = app();
    let actor = app
        .world_mut()
        .spawn((
            Pos(IVec2::new(2, 2)),
            sweeping_moves(),
            ManeuverLooks::default(),
        ))
        .id();
    app.world_mut().write_message(Attack {
        actor,
        relation: Relation::Back,
        facing: IVec2::X,
        dir: IVec2::NEG_X,
    });
    app.update();
    assert!(sent::<Gesture>(&app).is_empty());
    assert!(sent::<HoldMoves>(&app)
        .iter()
        .any(|hold| hold.entity == actor));
}

#[test]
fn a_blow_lands_at_contact_and_its_target_is_held_shaken_and_bleeds() {
    let mut app = app();
    let attacker = app.world_mut().spawn(Pos(IVec2::new(2, 2))).id();
    let target = guard(&mut app, IVec2::new(3, 2), 1);
    app.world_mut().write_message(Hit {
        source: attacker,
        target,
        damage: 1,
    });
    app.update();
    let contact = timing(&app).contact();
    let blows = sent::<BlowLands>(&app);
    assert_eq!(blows.len(), 1);
    assert_eq!((blows[0].contact, blows[0].toward), (contact, Vec2::X));
    assert!(sent::<HoldMoves>(&app)
        .iter()
        .any(|hold| hold.entity == target && hold.secs == contact));
    assert!(sent::<Gesture>(&app)
        .iter()
        .any(|shake| shake.entity == target && shake.delay == contact));
    let bled = sent::<PlayEffect>(&app);
    assert_eq!(bled.len(), 1);
    assert_eq!((bled[0].effect.as_str(), bled[0].strength), ("blood", 1));
}

#[test]
fn a_released_strike_lands_sooner_than_a_whole_swing() {
    let mut app = app();
    let wielder = guard(&mut app, IVec2::new(2, 2), 1);
    let target = app.world_mut().spawn(Pos(IVec2::new(3, 2))).id();
    app.world_mut().write_message(Strike {
        wielder,
        dir: IVec2::X,
    });
    app.world_mut().write_message(Hit {
        source: wielder,
        target,
        damage: 1,
    });
    app.update();
    assert_eq!(
        sent::<BlowLands>(&app)[0].contact,
        timing(&app).release_contact()
    );
}

#[test]
fn a_blow_across_the_map_edge_drives_its_target_on_across_it() {
    let mut app = app();
    let attacker = app.world_mut().spawn(Pos(IVec2::new(0, 2))).id();
    let target = guard(&mut app, IVec2::new(7, 2), 1);
    app.world_mut().write_message(Hit {
        source: attacker,
        target,
        damage: 1,
    });
    app.update();
    assert_eq!(sent::<BlowLands>(&app)[0].toward, Vec2::NEG_X);
}

#[test]
fn a_slam_plays_after_the_blow_where_the_body_meets_what_it_hit() {
    let mut app = app();
    let mover = guard(&mut app, IVec2::new(2, 2), 1);
    let wall = app.world_mut().spawn(Pos(IVec2::new(3, 2))).id();
    app.world_mut().write_message(Collision {
        mover,
        obstacle: wall,
    });
    app.update();
    let timing = timing(&app);
    assert!(timing.impact() > timing.contact(), "not after the blow");
    let played = sent::<PlayEffect>(&app);
    let slam = played
        .iter()
        .find(|effect| Some(&effect.effect) == timing.0.impact_effect.as_ref())
        .expect("no slam");
    assert_eq!((slam.entity, slam.toward), (mover, Vec2::X));
    assert_eq!(slam.delay, timing.impact());
    assert!(
        played.iter().any(|effect| effect.effect == "blood"),
        "no blood"
    );
    assert!(sent::<Gesture>(&app)
        .iter()
        .any(|shake| shake.entity == wall && shake.delay == timing.impact()));
}

#[test]
fn the_killed_bleed_through_their_remains_and_more() {
    let mut app = app();
    let attacker = app.world_mut().spawn(Pos(IVec2::new(2, 2))).id();
    let body = app.world_mut().spawn_empty().id();
    let remains = app
        .world_mut()
        .spawn((Remains(body), Pos(IVec2::new(3, 2)), Bleeds("blood".into())))
        .id();
    app.world_mut().write_message(Hit {
        source: attacker,
        target: body,
        damage: 1,
    });
    app.update();
    let bled = sent::<PlayEffect>(&app);
    assert_eq!(bled.len(), 1);
    assert_eq!(bled[0].entity, remains);
    assert_eq!(bled[0].strength, 1 + timing(&app).0.kill_strength);
}
