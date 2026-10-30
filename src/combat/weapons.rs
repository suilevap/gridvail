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
    delay: 0.0,
};
/// How long after a weapon's first cell is marked the next one is, in the
/// order it swings through them.
pub const STRIKE_MARK_STEP: f32 = 0.12;

/// Readies the weapon of each `UseWeapon`: aims it (`Telegraph`), then
/// strikes at once, or, with a wind-up, owes the wind-up's turns (`Delay`)
/// with the `Strike` scheduled for when they are paid.
pub fn ready_weapons(
    mut commands: Commands,
    mut uses: MessageReader<UseWeapon>,
    mut delays: MessageWriter<Delay>,
    mut strikes: MessageWriter<Strike>,
    weapons: Query<&Weapon, Without<Telegraph>>,
) {
    for use_weapon in uses.read() {
        let Ok(weapon) = weapons.get(use_weapon.user) else {
            continue;
        };
        let strike = Strike {
            wielder: use_weapon.user,
            dir: use_weapon.dir,
        };
        let mut user = commands.entity(use_weapon.user);
        user.insert(Telegraph {
            dir: use_weapon.dir,
        });
        if weapon.windup == 0 {
            strikes.write(strike);
            continue;
        }
        user.insert(Scheduled(strike));
        delays.write(Delay {
            entity: use_weapon.user,
            turns: weapon.windup,
        });
    }
}

/// A `Strike` hits every cell of the weapon's area, from where its wielder
/// stands now and the way it strikes: a hit on whatever there has health,
/// friend or foe. The weapon is no longer aimed.
pub fn strike(
    mut commands: Commands,
    grid: Res<MapGrid>,
    mut strikes: MessageReader<Strike>,
    mut hits: MessageWriter<Hit>,
    wielders: Query<(&Pos, &Weapon)>,
    health: Query<(), With<Health>>,
) {
    for &Strike { wielder, dir } in strikes.read() {
        let Ok((pos, weapon)) = wielders.get(wielder) else {
            continue;
        };
        for cell in weapon.area.cells(pos.0, dir) {
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

/// Marks every cell an aimed weapon will strike, planted in the order it
/// swings through them: from the side it swings from, nearer first.
pub fn mark_strikes(
    grid: Res<MapGrid>,
    marks: Option<ResMut<CellMarks>>,
    wielders: Query<(&Pos, &Weapon, &Telegraph), Without<DestroyRequested>>,
) {
    let Some(mut marks) = marks else {
        return;
    };
    for (pos, weapon, telegraph) in &wielders {
        let mut order: Vec<_> = (0..weapon.area.0.len()).collect();
        order.sort_by_key(|&i| (weapon.area.0[i].y, weapon.area.0[i].x));
        let cells: Vec<_> = weapon.area.cells(pos.0, telegraph.dir).collect();
        for (rank, &i) in order.iter().enumerate() {
            let mark = STRIKE_MARK.after(rank as f32 * STRIKE_MARK_STEP);
            marks.0.push((grid.safe_pos(cells[i]), mark));
        }
    }
}
