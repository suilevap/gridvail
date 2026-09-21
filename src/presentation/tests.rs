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
                pos: IVec2::new(4, 4),
                radius: 2,
                data: vec![1.0; 64],
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
