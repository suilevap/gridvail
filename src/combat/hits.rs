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

/// Driven into something, both the mover and what it hit take
/// `ImpactDamage`.
pub fn impacts_deal_damage(
    impact_damage: Res<ImpactDamage>,
    mut impacts: MessageReader<Impact>,
    mut damage: MessageWriter<Damage>,
) {
    for impact in impacts.read() {
        for target in [impact.mover, impact.obstacle] {
            damage.write(Damage {
                target,
                amount: impact_damage.0,
            });
        }
    }
}
