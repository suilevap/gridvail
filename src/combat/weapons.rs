use bevy::prelude::*;

use crate::lighting::RED;
use crate::model::*;

/// How a cell about to be struck is marked.
pub const STRIKE_MARK: CellMark = CellMark {
    ground: RenderCell {
        ch: '×',
        color: RED,
        depth: 0,
    },
    tint: RED,
};

/// Readies the weapon of each `UseWeapon`: aimed at once (`Telegraph`), and
/// wound up for its turns (`Channeling`) before it strikes; one with no
/// wind-up strikes at once.
pub fn ready_weapons(
    mut commands: Commands,
    mut uses: MessageReader<UseWeapon>,
    mut done: MessageWriter<ChannelDone>,
    weapons: Query<&Weapon, Without<Telegraph>>,
) {
    for use_weapon in uses.read() {
        let Ok(weapon) = weapons.get(use_weapon.user) else {
            continue;
        };
        let mut user = commands.entity(use_weapon.user);
        user.insert(Telegraph {
            dir: use_weapon.dir,
        });
        if weapon.windup > 0 {
            user.insert(Channeling {
                turns: weapon.windup,
            });
        } else {
            done.write(ChannelDone(use_weapon.user));
        }
    }
}

/// When a wielder is done winding up, its weapon strikes every cell of its
/// shape, counted from where the wielder stands now: a hit on whatever
/// there has health, friend or foe.
pub fn strike(
    mut commands: Commands,
    grid: Res<MapGrid>,
    mut done: MessageReader<ChannelDone>,
    mut hits: MessageWriter<Hit>,
    wielders: Query<(&Pos, &Weapon, &Telegraph)>,
    health: Query<(), With<Health>>,
) {
    for &ChannelDone(wielder) in done.read() {
        let Ok((pos, weapon, telegraph)) = wielders.get(wielder) else {
            continue;
        };
        for cell in weapon.shape.cells(pos.0, telegraph.dir) {
            let Some(target) = grid.get(grid.safe_pos(cell)) else {
                continue;
            };
            if target != wielder && health.contains(target) {
                hits.write(Hit {
                    source: wielder,
                    target,
                    damage: weapon.damage,
                });
            }
        }
        commands.entity(wielder).remove::<Telegraph>();
    }
}

/// Marks every cell a wound-up weapon will strike.
pub fn mark_strikes(
    grid: Res<MapGrid>,
    marks: Option<ResMut<CellMarks>>,
    wielders: Query<(&Pos, &Weapon, &Telegraph), Without<DestroyRequested>>,
) {
    let Some(mut marks) = marks else {
        return;
    };
    for (pos, weapon, telegraph) in &wielders {
        for cell in weapon.shape.cells(pos.0, telegraph.dir) {
            marks.0.push((grid.safe_pos(cell), STRIKE_MARK));
        }
    }
}
