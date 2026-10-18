//! Concrete balance and display presets. Changing an archetype changes its
//! bundle, never an AI/damage/animation role switch.
use crate::model::*;
use bevy::prelude::*;

pub(super) fn player_combat() -> impl Bundle {
    (
        Health::new(12),
        Team(0),
        BumpAttack { damage: 2 },
        Brace {
            reduction: 1,
            charges: 0,
        },
        AttackPoints::default(),
        AttackExecuted::default(),
    )
}

fn attack_body(
    health: i32,
    damage: i32,
    windup: u8,
    recovery: u8,
    display: AttackPresentation,
) -> impl Bundle {
    (
        (
            Health::new(health),
            Team(1),
            AttackPower(damage),
            AttackTiming { windup, recovery },
        ),
        (
            AttackCycle::default(),
            AttackPoints::default(),
            AttackExecuted::default(),
        ),
        (
            InterruptOnHit { recovery: 1 },
            Vulnerability {
                bonus: 1,
                window: AttackWindow::Recovering,
            },
        ),
        (
            display,
            CellOverlay::default(),
            Unlit,
            DeathGlyph(' '),
            TrackPlayer,
        ),
    )
}

pub(super) fn skirmisher() -> impl Bundle {
    (
        attack_body(
            3,
            1,
            0,
            1,
            AttackPresentation {
                name: "Foe",
                glyph: 'e',
                attack: "STRIKE",
                idle_hint: "approaching",
                ready_hint: "dodge ! or interrupt",
                avoid_hint: "sidestep",
                opening_hint: "counter",
            },
        ),
        LineAttack { reach: 1 },
        AimAtAttack,
    )
}

pub(super) fn spear() -> impl Bundle {
    (
        attack_body(
            3,
            2,
            0,
            2,
            AttackPresentation {
                name: "Spear",
                glyph: 'S',
                attack: "LUNGE",
                idle_hint: "bait its lane",
                ready_hint: "LUNGE | sidestep",
                avoid_hint: "sidestep",
                opening_hint: "counter",
            },
        ),
        LineAttack { reach: 2 },
        Lunge { cells: 1 },
        AimAtAttack,
    )
}

pub(super) fn shield() -> impl Bundle {
    (
        attack_body(
            4,
            1,
            0,
            2,
            AttackPresentation {
                name: "Shield",
                glyph: 'G',
                attack: "BASH",
                idle_hint: "front guarded | bait bash",
                ready_hint: "BASH | sidestep",
                avoid_hint: "sidestep",
                opening_hint: "any side",
            },
        ),
        LineAttack { reach: 1 },
        DirectionalGuard {
            window: AttackWindow::OutsideRecovery,
        },
        AttackAnimation::Bump { seconds: 0.3 },
    )
}

pub(super) fn brute() -> impl Bundle {
    (
        attack_body(
            5,
            3,
            1,
            3,
            AttackPresentation {
                name: "Brute",
                glyph: 'B',
                attack: "SLAM",
                idle_hint: "bait slam, then retreat",
                ready_hint: "SLAM NEXT | get out",
                avoid_hint: "out",
                opening_hint: "counter",
            },
        ),
        NeighbourAttack,
        Armour {
            reduction: 1,
            window: AttackWindow::Preparing,
        },
        InterruptResistance {
            window: AttackWindow::Preparing,
        },
        AttackAnimation::Hop {
            seconds: 0.35,
            height: 0.6,
        },
    )
}

/// The existing goal/behavior-tree controller can use the same bump combat.
pub(super) fn hunter_combat() -> impl Bundle {
    (
        Health::new(3),
        Team(1),
        BumpAttack { damage: 1 },
        DeathGlyph(' '),
    )
}
