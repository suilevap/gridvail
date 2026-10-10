use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

use super::*;
use crate::lighting::GRAY;
use crate::model::*;
use crate::simulation::test_app;

fn visibility(cells: &[(IVec2, Vis)]) -> VisibilityMap {
    let mut data = vec![Vis::empty(); 64];
    for (pos, value) in cells {
        data[(pos.y * 8 + pos.x) as usize] = *value;
    }
    VisibilityMap { revision: 0, data }
}

#[test]
fn static_decor_renders_when_known_but_unseen() {
    let mut app = test_app::headless();
    app.insert_resource(DynamicLight::sized(64))
        .insert_resource(RenderBuffers::new(8, 8));
    app.world_mut().spawn((
        Player(0),
        Pos(IVec2::ZERO),
        Speed::default(),
        visibility(&[
            (IVec2::new(0, 0), Vis::VISIBLE | Vis::KNOWN),
            (IVec2::new(1, 0), Vis::KNOWN),
            (IVec2::new(2, 0), Vis::KNOWN),
        ]),
    ));
    app.world_mut()
        .spawn((Pos(IVec2::new(1, 0)), Glyph::new('#', 1, GRAY)));
    app.world_mut().spawn((
        Pos(IVec2::new(2, 0)),
        Speed::default(),
        Glyph::new('e', 1, 12),
    ));
    app.world_mut().run_system_once(compose_frame).unwrap();
    let frame = &app.world().resource::<RenderBuffers>().current;
    assert_eq!(
        (frame[0].ch, frame[1].ch, frame[2].ch, frame[3].ch),
        ('.', '#', '.', '?')
    );
}

#[test]
fn static_light_rebuilds_only_on_change() {
    let mut app = test_app::headless();
    app.insert_resource(StaticLight::new())
        .insert_resource(DynamicLight::sized(64));
    let lamp = app
        .world_mut()
        .spawn((
            Pos(IVec2::new(4, 4)),
            LightSource {
                radius: 2,
                kind: LightKind::Fire,
                value: 100,
            },
            FovResult {
                revision: 0,
                obstacle_revision: 0,
                portal_revision: 0,
                pos: IVec2::new(4, 4),
                radius: 2,
                data: vec![1.0; 64],
                samples: vec![crate::foundation::fov::ViewSample {
                    delta: IVec2::ZERO,
                    world: IVec2::new(4, 4),
                    transform: crate::foundation::portal::CellTransform::IDENTITY,
                    value: 1.0,
                }],
            },
        ))
        .id();
    app.world_mut()
        .run_system_once(render_light_layers)
        .unwrap();
    assert_eq!(app.world().resource::<DynamicLight>().data[36].value, 100);
    app.world_mut().get_mut::<FovResult>(lamp).unwrap().revision = 7;
    app.world_mut()
        .run_system_once(render_light_layers)
        .unwrap();
    assert_eq!(
        app.world().resource::<StaticLight>().last_seen_revisions,
        vec![(lamp, 7)]
    );
}

#[test]
fn every_glyph_entity_is_an_object_above_the_ground() {
    let mut app = test_app::headless();
    app.insert_resource(DynamicLight::sized(64))
        .insert_resource(RenderBuffers::new(8, 8));
    let seen = Vis::VISIBLE | Vis::KNOWN;
    let player = app
        .world_mut()
        .spawn((
            Player(0),
            Pos(IVec2::ZERO),
            Speed::default(),
            Glyph::new('@', 1, 15),
            visibility(&[
                (IVec2::new(0, 0), seen),
                (IVec2::new(1, 0), seen),
                (IVec2::new(2, 0), seen),
            ]),
        ))
        .id();
    // A marker hidden behind a wall is not drawn; the wall is.
    app.world_mut()
        .spawn((Pos(IVec2::new(1, 0)), Glyph::new('i', 0, 15)));
    let wall = app
        .world_mut()
        .spawn((Pos(IVec2::new(1, 0)), Glyph::new('#', 1, GRAY)))
        .id();
    app.world_mut().run_system_once(compose_frame).unwrap();
    let buffers = app.world().resource::<RenderBuffers>();
    let flat: Vec<char> = buffers.current[..3].iter().map(|cell| cell.ch).collect();
    let ground: Vec<char> = buffers.ground[..3].iter().map(|cell| cell.ch).collect();
    assert_eq!(flat, ['@', '#', '.']);
    assert_eq!(
        ground,
        ['\0', '\0', '.'],
        "ground stays blank under objects"
    );
    let mut objects: Vec<Entity> = buffers.objects.iter().map(|o| o.entity).collect();
    objects.sort();
    let mut expected = vec![player, wall];
    expected.sort();
    assert_eq!(objects, expected);
    for object in &buffers.objects {
        let index = buffers.idx(object.pos).unwrap();
        assert_eq!(object.cell.color, buffers.current[index].color);
        assert_eq!(object.position, object.pos.as_vec2());
    }
}

#[test]
fn objects_behind_a_portal_are_drawn_where_they_are_seen() {
    use crate::foundation::fov::ViewSample;
    use crate::foundation::portal::CellTransform;

    let mut app = test_app::headless();
    app.insert_resource(DynamicLight::sized(64))
        .insert_resource(RenderBuffers::new(8, 8));
    // The player at (1, 1) sees (2, 1) directly, and (3, 1) and (4, 1) through
    // a portal that shows map cells 4 rows down.
    let through = CellTransform::translation(IVec2::new(0, 4));
    let sample = |delta: IVec2, transform: CellTransform| ViewSample {
        delta,
        world: transform.apply(IVec2::new(1, 1) + delta),
        transform,
        value: 1.0,
    };
    let mut view = PlayerView::with_radius(4);
    view.pos = IVec2::new(1, 1);
    view.samples = vec![
        sample(IVec2::ZERO, CellTransform::IDENTITY),
        sample(IVec2::new(1, 0), CellTransform::IDENTITY),
        sample(IVec2::new(2, 0), through),
        sample(IVec2::new(3, 0), through),
    ];
    let seen = Vis::VISIBLE | Vis::KNOWN;
    app.world_mut().spawn((
        Player(0),
        Pos(IVec2::new(1, 1)),
        Speed::default(),
        view,
        visibility(&[
            (IVec2::new(1, 1), seen),
            (IVec2::new(2, 1), seen),
            (IVec2::new(3, 5), seen),
            (IVec2::new(4, 5), seen),
        ]),
    ));
    // An enemy at map (4, 5), moving in from (4, 4).
    let enemy = app
        .world_mut()
        .spawn((
            Pos(IVec2::new(4, 5)),
            Speed::default(),
            Glyph::new('e', 1, 12),
            AnimatedPos {
                position: Vec2::new(4.0, 4.5),
                lift: 0.0,
            },
        ))
        .id();
    app.world_mut().run_system_once(compose_frame).unwrap();
    let buffers = app.world().resource::<RenderBuffers>();

    let frame = |p: IVec2| buffers.current[buffers.idx(p).unwrap()].ch;
    assert_eq!(frame(IVec2::new(4, 1)), 'e', "drawn where it is seen");
    assert_eq!(
        frame(IVec2::new(3, 1)),
        '.',
        "seen floor through the portal"
    );
    assert_ne!(frame(IVec2::new(4, 5)), 'e', "not at its own map cell");
    let drawn = buffers
        .objects
        .iter()
        .find(|object| object.entity == enemy)
        .expect("enemy drawn");
    assert_eq!((drawn.instance, drawn.pos), (0, IVec2::new(4, 1)));
    assert_eq!(drawn.position, Vec2::new(4.0, 0.5), "its motion maps too");
    assert_eq!(
        buffers.seen[buffers.idx(IVec2::new(4, 1)).unwrap()],
        Some(SeenCell {
            world: IVec2::new(4, 5),
            transform: through,
        })
    );
}

#[test]
fn objects_behind_a_turning_portal_are_drawn_turned() {
    use crate::foundation::fov::ViewSample;
    use crate::foundation::portal::CellTransform;

    let mut app = test_app::headless();
    app.insert_resource(DynamicLight::sized(64))
        .insert_resource(RenderBuffers::new(8, 8));
    // The player at (1, 1) looks east through a portal that turns the view
    // a quarter turn: frame cells (3, 1) and (4, 1) show map cells (5, 5)
    // and (5, 4), so east in the frame is north on the map.
    let through = CellTransform {
        quarters: 1,
        offset: IVec2::new(4, 8),
    };
    assert_eq!(through.apply(IVec2::new(3, 1)), IVec2::new(5, 5));
    assert_eq!(through.apply(IVec2::new(4, 1)), IVec2::new(5, 4));
    let sample = |delta: IVec2, transform: CellTransform| ViewSample {
        delta,
        world: transform.apply(IVec2::new(1, 1) + delta),
        transform,
        value: 1.0,
    };
    let mut view = PlayerView::with_radius(4);
    view.pos = IVec2::new(1, 1);
    view.samples = vec![
        sample(IVec2::ZERO, CellTransform::IDENTITY),
        sample(IVec2::new(1, 0), CellTransform::IDENTITY),
        sample(IVec2::new(2, 0), through),
        sample(IVec2::new(3, 0), through),
    ];
    app.world_mut().spawn((
        Player(0),
        Pos(IVec2::new(1, 1)),
        Speed::default(),
        view,
        visibility(&[(IVec2::new(1, 1), Vis::VISIBLE | Vis::KNOWN)]),
    ));
    // An enemy halfway through a step north, from (5, 5) to (5, 4).
    let enemy = app
        .world_mut()
        .spawn((
            Pos(IVec2::new(5, 4)),
            Speed::default(),
            Glyph::new('e', 1, 12),
            AnimatedPos {
                position: Vec2::new(5.0, 4.5),
                lift: 0.0,
            },
        ))
        .id();
    app.world_mut().run_system_once(compose_frame).unwrap();
    let buffers = app.world().resource::<RenderBuffers>();
    let drawn = buffers
        .objects
        .iter()
        .find(|object| object.entity == enemy)
        .expect("enemy drawn");
    assert_eq!(drawn.pos, IVec2::new(4, 1));
    // Its step north on the map is a step east in the frame...
    assert!(drawn.position.distance(Vec2::new(3.5, 1.0)) < 1e-5);
    // ...and its glyph is drawn turned back the other way, three quarter
    // turns, so shapes line up with the frame.
    assert_eq!(drawn.quarters, 3);
}

#[test]
fn an_object_seen_twice_is_drawn_twice() {
    use crate::foundation::fov::ViewSample;
    use crate::foundation::portal::CellTransform;

    let mut app = test_app::headless();
    app.insert_resource(DynamicLight::sized(64))
        .insert_resource(RenderBuffers::new(8, 8));
    // Frame cells (2, 1) and (5, 1) both show map cell (2, 1).
    let mut view = PlayerView::with_radius(4);
    view.pos = IVec2::new(1, 1);
    let back = CellTransform::translation(IVec2::new(-3, 0));
    view.samples = vec![
        ViewSample {
            delta: IVec2::new(1, 0),
            world: IVec2::new(2, 1),
            transform: CellTransform::IDENTITY,
            value: 1.0,
        },
        ViewSample {
            delta: IVec2::new(4, 0),
            world: IVec2::new(2, 1),
            transform: back,
            value: 1.0,
        },
    ];
    app.world_mut().spawn((
        Player(0),
        Pos(IVec2::new(1, 1)),
        view,
        visibility(&[(IVec2::new(2, 1), Vis::VISIBLE | Vis::KNOWN)]),
    ));
    app.world_mut()
        .spawn((Pos(IVec2::new(2, 1)), Glyph::new('k', 0, 14)));
    app.world_mut().run_system_once(compose_frame).unwrap();
    let buffers = app.world().resource::<RenderBuffers>();
    let mut drawn: Vec<(u16, IVec2)> = buffers
        .objects
        .iter()
        .map(|object| (object.instance, object.pos))
        .collect();
    drawn.sort_by_key(|(_, pos)| pos.x);
    assert_eq!(drawn.len(), 2);
    assert_eq!(drawn[0].1, IVec2::new(2, 1));
    assert_eq!(drawn[1].1, IVec2::new(5, 1));
    assert_ne!(drawn[0].0, drawn[1].0, "each place is its own instance");
}

#[test]
fn reveal_shows_what_the_player_cannot_see() {
    let mut app = test_app::headless();
    app.insert_resource(DynamicLight::sized(64))
        .insert_resource(RenderBuffers::new(8, 8))
        .insert_resource(RevealAll);
    let mut view = PlayerView::with_radius(4);
    view.pos = IVec2::new(1, 1);
    app.world_mut()
        .spawn((Player(0), Pos(IVec2::new(1, 1)), view, visibility(&[])));
    app.world_mut().spawn((
        Pos(IVec2::new(6, 6)),
        Speed::default(),
        Glyph::new('e', 1, 12),
    ));
    app.world_mut().run_system_once(compose_frame).unwrap();
    let buffers = app.world().resource::<RenderBuffers>();
    assert_eq!(
        buffers.current[buffers.idx(IVec2::new(6, 6)).unwrap()].ch,
        'e'
    );
    assert_eq!(
        buffers.current[buffers.idx(IVec2::new(5, 6)).unwrap()].ch,
        '.'
    );
}

#[test]
fn a_child_goes_with_its_parent_even_into_a_portal_wall() {
    use crate::foundation::fov::ViewSample;
    use crate::foundation::portal::CellTransform;

    let mut app = test_app::headless();
    app.insert_resource(DynamicLight::sized(64))
        .insert_resource(RenderBuffers::new(8, 8));
    // The player at (2, 1) faces a portal wall at (3, 1), which shows map
    // cell (5, 5) beyond it. Their marker is bound one cell ahead, in the
    // portal wall's own cell.
    let through = CellTransform::translation(IVec2::new(2, 4));
    let mut view = PlayerView::with_radius(4);
    view.pos = IVec2::new(2, 1);
    view.samples = vec![
        ViewSample {
            delta: IVec2::ZERO,
            world: IVec2::new(2, 1),
            transform: CellTransform::IDENTITY,
            value: 1.0,
        },
        ViewSample {
            delta: IVec2::X,
            world: IVec2::new(5, 5),
            transform: through,
            value: 1.0,
        },
    ];
    let seen = Vis::VISIBLE | Vis::KNOWN;
    let player = app
        .world_mut()
        .spawn((
            Player(0),
            Pos(IVec2::new(2, 1)),
            Speed::default(),
            Glyph::new('@', 1, 15),
            view,
            visibility(&[(IVec2::new(2, 1), seen), (IVec2::new(5, 5), seen)]),
        ))
        .id();
    let marker = app
        .world_mut()
        .spawn((
            Pos(IVec2::new(3, 1)),
            Glyph::new('►', 0, 15),
            BoundTo {
                parent: player,
                offset: IVec2::X,
                offset_dir: IVec2::X,
            },
        ))
        .id();
    app.world_mut().run_system_once(compose_frame).unwrap();
    let buffers = app.world().resource::<RenderBuffers>();
    let drawn = buffers
        .objects
        .iter()
        .find(|object| object.entity == marker)
        .expect("the marker is drawn with its player");
    assert_eq!((drawn.pos, drawn.instance), (IVec2::new(3, 1), 0));
}

#[test]
fn the_frame_window_keeps_each_position_in_its_slot_as_it_scrolls() {
    let mut buffers = RenderBuffers::following(IVec2::new(6, 4), 64);
    buffers.centre_on(IVec2::new(10, 10));
    assert_eq!(buffers.origin, IVec2::new(7, 8));
    for index in 0..buffers.current.len() {
        assert_eq!(buffers.idx(buffers.pos_of(index)), Some(index));
    }
    assert_eq!(buffers.idx(IVec2::new(6, 10)), None, "left of the window");
    let slot = buffers.idx(IVec2::new(9, 9)).unwrap();
    // A step right scrolls the window; positions still in it keep slots.
    buffers.centre_on(IVec2::new(11, 10));
    assert_eq!(buffers.idx(IVec2::new(9, 9)), Some(slot));
    assert_eq!(buffers.idx(IVec2::new(7, 9)), None, "scrolled out");
    for index in 0..buffers.current.len() {
        assert_eq!(buffers.idx(buffers.pos_of(index)), Some(index));
    }
}

#[test]
fn what_is_seen_past_the_map_edge_is_drawn() {
    use crate::foundation::fov::ViewSample;
    use crate::foundation::portal::CellTransform;

    let mut app = test_app::headless();
    app.insert_resource(DynamicLight::sized(64))
        .insert_resource(RenderBuffers::following(IVec2::new(9, 9), 64));
    // At (1, 1), looking west through a portal: frame cell (-2, 1), off the
    // 8x8 map, shows map cell (5, 5).
    let through = CellTransform::translation(IVec2::new(7, 4));
    let mut view = PlayerView::with_radius(4);
    view.pos = IVec2::new(1, 1);
    view.samples = vec![ViewSample {
        delta: IVec2::new(-3, 0),
        world: IVec2::new(5, 5),
        transform: through,
        value: 1.0,
    }];
    app.world_mut()
        .spawn((Player(0), Pos(IVec2::new(1, 1)), view, visibility(&[])));
    app.world_mut().spawn((
        Pos(IVec2::new(5, 5)),
        Speed::default(),
        Glyph::new('e', 1, 12),
    ));
    app.world_mut().run_system_once(compose_frame).unwrap();
    let buffers = app.world().resource::<RenderBuffers>();
    assert_eq!(buffers.origin, IVec2::new(-3, -3), "centred on the player");
    let off_map = buffers.idx(IVec2::new(-2, 1)).expect("in the window");
    assert_eq!(buffers.current[off_map].ch, 'e');
    let drawn = buffers.objects.iter().find(|o| o.cell.ch == 'e').unwrap();
    assert_eq!(drawn.pos, IVec2::new(-2, 1));
}
