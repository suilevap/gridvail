# Design principles

How gameplay code is put together, so features stay small, independent and
easy to extend. `ARCHITECTURE.md` says where code lives; this says how to
shape it. New code follows these principles. Existing code that does not
yet is listed under [Backfill](#backfill), to be brought in line in
changes of its own, not alongside features.

The combat code (`selection/`, `combat/`, `damage/`,
`simulation/channel.rs`, `simulation/displace.rs`) follows them and is the
reference for the examples below.

## Principles

### 1. One system, one job

A system does one thing you can say in a sentence without "and". Taking
damage and removing the dead are two systems (`take_damage`,
`destroy_dead`); so are base damage, critical hits and impact damage
(`hits_deal_damage`, `critical_hits`, `impacts_deal_damage`), each adding
its share. When a system grows an `if` for a second concern, that concern
is a second system.

### 2. Entities are compositions of small components

What an entity does comes from the components it has, not from what kind
of thing it is. A system asks "has `Health`", "is `Movable`", "has a
`Weapon`", never "is an enemy". A crate that can be pushed is `Movable`; a
lamp that can be picked is `Selectable`. Neither needs combat code to
change.

Components are small and hold data only: a few fields, no behaviour. A
property that comes and goes often (being off guard) is a component
written in place (`Exposed(bool)`), so entities do not move between
archetypes every turn. One that comes and goes rarely (a wound-up strike)
is a component inserted and removed (`Telegraph`, `Channeling`).

### 3. Systems meet in messages and components, not in each other

A system does not call, name or know the systems around it. Producers
write a message or a component; whoever cares reads it.

- The AI decides to attack and writes `UseWeapon`; the weapon decides how
  (aim, wind-up, strike).
- Picking turns the second input into a `ContextAction`; maneuvers answer
  it, and a door or a lever could answer it too, without picking knowing.
- Anything that hurts sends `Damage`; `damage/` does not know combat.

Modules at the same level do not import each other: `selection`, `combat`
and `damage` share only `model` types. Check with
`grep -rn "use crate::combat" src/selection src/damage` and the like.

### 4. Extend by adding, not by editing

A new feature is a new component, a new message answer or a new system,
not another arm in an existing `match`. A new weapon is a `Weapon`; a new
fighting style is a `Maneuvers` set; a hazard is a system that sends
`Damage`. If adding a feature means editing a system that already works,
look for the message or component that system should have exposed.

### 5. Intent, effect, resolution

Changes to the world pass through three kinds of system:

1. **Deciders** (input, AI) write intents: `MoveCommand`, `UseWeapon`,
   `ContextAction`.
2. **Effect** systems turn intents into requests: `Hit`, `Displace`,
   `Damage`.
3. **Resolvers** apply requests to shared state, and are the only systems
   that change it: `displace` and movement resolution move things on the
   grid, `take_damage` changes `Health`.

Deciders and effect systems never write the grid, positions or health
directly. That keeps the rules of the world (swaps, blocking, who goes
first) in one place each.

### 6. Order by stage, not by neighbour

Systems join a stage set (`ActionStage::Ongoing`, `Decide`, `Effects`,
`Movement`, `Hits`, `Damage`, plus `PlayerCommands` and `MarkCells`), and
the stages are ordered once, in one place. `.after(some_system)` is for
order inside one module only. A new system finds its stage; it does not
look for a system to sit after.

### 7. Build the general mechanism, not the special case

When a problem will come back, solve it once, generally, where it belongs,
instead of with a check in the consumer.

- Multi-turn actions: `Channeling` spends an actor's turns before anyone
  decides, so nothing has to ask "is it winding up?". Aiming with a tool
  later is the same mechanism.
- Knockback: `Displace` moves any set of things together and reports what
  got blocked (`Impact`), for maneuvers today and explosions tomorrow.
- Feedback: `CellMarks` shows any cell to the player; strikes and picks
  are two producers among many to come.

### 8. Data, not constants

Stats, patterns and tunables live in `assets/rules/` (the actor roster:
kinds, hit points, weapons drawn as art, move sets), are parsed by
`content/` into plain specs, and are turned into components by `app`. Code
keeps a default only where data would add nothing. Tunables a system reads
are resources (`CriticalHit`, `ImpactDamage`), so the game and tests can
change them without touching the system.

### 9. Gameplay does not draw

Gameplay systems never set colours or glyphs for the player to see. They
expose state (a `Telegraph`, a `Selected`) and a presentation system turns
it into marks (`CellMarks`), which the frame composer draws. That keeps
what happens apart from how it is shown, so feedback can change freely.

### 10. Input is intent, from any device

Keys, touch and the agent API all write the same commands (`MoveCommand`,
`CancelIntent`), and gameplay never reads a device. Each device says what
only it knows, such as whether a direction is a fresh press or held on
(`MoveCommand::repeat`).

### 11. Steady turns allocate nothing

Buffers are kept and reused (`Local<DisplaceBatch>`, `CellMarks`),
messages carry what happens, and components are inserted or removed only on
rare transitions. `tests/allocations.rs` guards steady turns.

### 12. Each piece is tested alone

A module is tested with `MinimalPlugins`, its own plugin and messages
written by hand (`src/combat/tests.rs`, `src/selection/tests.rs`,
`src/simulation/displace.rs`). Behaviour across modules is tested through
`GamePlugin` on small ASCII maps (`tests/combat.rs`). A system that cannot
be tested without half the game is doing too much.

## Checklist for a new feature

- What does it read and write? Name the components and messages first.
- Can an existing message carry it (`ContextAction`, `Damage`, `Displace`,
  `Hit`)? Answer that instead of adding a path.
- Which stage does each system belong to?
- Which numbers belong in `assets/rules/`?
- What does the player see, and which marks or presentation system shows
  it?
- Can each system be tested alone?

## Backfill

Existing code that predates these principles, roughly in order of how much
it would help. Each item is its own change.

1. **One long simulation chain** (`simulation/plugin.rs`). About twenty
   systems are ordered by naming each other in one `.chain()`. Split them
   into stages like `ActionStage`: movement resolution, reactions to moves
   (doors, items, plates, rooms), lifecycle (drops, destruction),
   derivation (tiles, direction glyphs). (Principle 6.)
2. **Collisions as a shared buffer** (`CollisionBuffer`). Bumps are a
   resource vector read by doors, animation, the HUD and the agent API.
   Make them a message (`Bumped`), like `Impact`. (Principle 3.)
3. **Doors do several jobs in one system** (`simulation/items.rs`,
   `open_doors`). It spends the key, opens the door, frees the grid cell
   and changes the glyph. Split it into a decider (a bump into a door by a
   key holder), a resolver (the door opens and frees its cell) and
   presentation (the glyph follows the door's state). Opening by
   `ContextAction` then comes for free. (Principles 1, 5, 9.)
4. **The AI draws** (`ai::show_mood`). It writes glyph characters and
   colours from the enemy's act. Have the AI expose its mood, and let
   presentation show it, as marks do. (Principle 9.)
5. **Hard-coded spawn stats** (`app/map.rs`). Sight radii, lamp, acid and
   electricity light values, glyphs and colours are constants in spawn
   functions. Move them into the roster or a second rules file, as actor
   kinds already are. (Principle 8.)
6. **Hazards that do nothing.** `ElectroField` and `AcidPool` are markers
   with light only. As small systems sending `Damage` (and later
   reactions to `Impact`), they would be the first hazards, with no change
   to combat. (Principles 3, 4.)
7. **Input gated by knowing other features** (`player_input`). It checks
   the settings menu and cutscenes itself. Make "input blocked" one
   resource or run condition that those features set. (Principles 3, 10.)
8. **The HUD knows every feature** (`rendering/text.rs`, `update_hud`). One
   system reads players, keys, enemies, collisions, the camera and combat.
   Let each feature contribute its own line or segment. (Principles 1, 3.)
9. **The agent API state is one function** (`agent_api::get_state`). Split
   it per section (player, enemies, strikes, visual), each its own
   serializer. (Principle 1.)
10. **The enemy blackboard is one large component** (`EnemyMind`). Sight,
    keys, orders and strolls are filled by one `perceive` system.
    Perception per sense, each filling its own part, would let new senses
    (hearing, smell) be added alone. (Principles 1, 2.)
