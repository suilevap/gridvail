use bevy::prelude::*;

use crate::model::*;

/// Every hit does its damage.
pub fn hits_deal_damage(mut hits: MessageReader<Hit>, mut damage: MessageWriter<Damage>) {
    for hit in hits.read() {
        damage.write(Damage {
            target: hit.target,
            amount: hit.damage,
        });
    }
}

/// A hit on an `Exposed` target does `CriticalHit::bonus` more.
pub fn critical_hits(
    critical: Res<CriticalHit>,
    mut hits: MessageReader<Hit>,
    mut damage: MessageWriter<Damage>,
    exposed: Query<&Exposed>,
) {
    for hit in hits.read() {
        if exposed.get(hit.target).is_ok_and(|exposed| exposed.0) {
            damage.write(Damage {
                target: hit.target,
                amount: critical.bonus,
            });
        }
    }
}

/// A body knocked into something, and what it ran into, both take
/// `ImpactDamage` for each cell of speed it had left: the faster, the
/// harder.
pub fn collisions_deal_damage(
    impact_damage: Res<ImpactDamage>,
    mut collisions: MessageReader<Collision>,
    mut damage: MessageWriter<Damage>,
) {
    for collision in collisions.read() {
        for target in [collision.mover, collision.obstacle] {
            damage.write(Damage {
                target,
                amount: impact_damage.0 * collision.speed,
            });
        }
    }
}
