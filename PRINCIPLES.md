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

### 7. Each mechanism is the general case

Before building a feature's mechanism, ask what it is a case of. Knockback
is a case of bodies moving and colliding; a wind-up is a case of an action
costing time; a weapon's reach is a case of an area of cells; a door
opening is a case of a reaction to a bump. Build (or extend) the general
mechanism and make the feature its first user, rather than a mechanism
shaped to the one feature.

Two checks keep this honest:

- **Real users.** Generalize when there are two users, or one plus one
  already planned (pushing crates as well as knocking enemies). Until then,
  keep the concrete case, but name and shape it as the general one so it
  can grow without a rewrite.
- **One of each.** Two systems that do the same job under different rules
  (two movement resolvers, two ways of counting turns) are one mechanism
  waiting to happen.

The mechanisms the game should converge on are listed under
[General mechanisms](#general-mechanisms).

### 8. Data, not constants

Stats, patterns and tunables live in `assets/rules/` (the actor roster:
kinds, hit points, weapons drawn as art, move sets), are read by
`content/` into plain specs, and are turned into components by `app`. Code
keeps a default only where data would add nothing. Data files share one
format read by one generic reader, so a new file or field is a new type,
not a new parser (see [Data](#data-one-format-and-prefabs)). Tunables a system reads
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
- What is it a case of? Is there a general mechanism below it should use
  or extend?
- Can an existing message carry it (`ContextAction`, `Damage`, `Displace`,
  `Hit`)? Answer that instead of adding a path.
- Which stage does each system belong to?
- Which numbers belong in `assets/rules/`?
- What does the player see, and which marks or presentation system shows
  it?
- Can each system be tested alone?

## General mechanisms

What today's systems are special cases of, and the general mechanism each
should become. These are proposals, not yet built; each is its own change,
and each lists what it would absorb, so the code ends with one of each.

### Physics: bodies that move, push and collide

**Today, two movement resolvers.** Walking already has the parts of
physics: `Speed` (a velocity), `Friction`, `Collider`, and a resolver that
records collisions. Knockback has a second one (`displace`), with its own
rules for blocking, and its own collision report (`Impact`). Neither lets
the player push a crate. Walking also moves a whole `Speed` in one jump, so
a fast body would pass through what lies between.

**General.** One grid physics for every body:

- a body has a velocity and friction, and moves one cell per sub-step
  until its velocity is spent, so speed is distance and nothing is skipped;
- anything changes a body's motion by an `Impulse` (a velocity added):
  walking is an impulse of one, a knockback one cell, a throw several;
- `Mass` (or weight classes) says what moves what: a light body is pushed
  by a walker that bumps it, a heavy one is not;
- each sub-step resolves every body together (chains and explicit
  exchanges such as a yank allowed; head-on walkers still blocked, as the
  original game requires), and every block is a `Collision` message with
  the speed it happened at;
- reactions read collisions: impact damage (faster hurts more), momentum
  passed on to what was hit (billiards, a crate shoved into an enemy),
  doors that open when bumped, the animation's bump.

**Absorbs.** `movement` and the resolution systems, `displace`,
`CollisionBuffer`, `Impact`. **First users.** Maneuvers' knockback,
pushing crates, throwing. **Also enables** previews (below): planning a
sub-step is a pure function of the grid and the bodies.

### Time: actions cost turns, and effects can wait

**Today.** `Tokens` give one action per turn, and recharging assigns, so
nothing can cost more than a turn. `Channeling` spends whole turns and then
says it is done; only weapons use it, so it reads as part of them. Other
ways of counting turns are each their own: the AI's alert beat (`Pause`),
`THINK_FRAMES`, and soon status effects.

**General.** An action economy with debt, plus scheduled effects:

- every action has a cost in turns (walking 1, a heavy swing 2); the cost
  is paid from `Tokens`, which may go negative, and a recharge pays the
  debt off first, so an actor in debt sits out turns without anything
  checking it;
- anything can add time off by adding debt: a stun, a stagger after an
  impact, a slow; haste or slowness is a different recharge;
- a scheduled effect is a message held until a time comes (an actor's own
  debt is paid, or a number of world turns pass): the wind-up is "cost the
  wind-up, then send the strike"; a fuse, a status that wears off, a
  respawn are the same.

**Absorbs.** `Channeling`, `ChannelDone`, the AI's `Pause` turns.
**First users.** Weapon wind-ups (cost plus scheduled strike), staggering a
knocked enemy.

### Data: one format, and prefabs

**Today.** Each rules file has its own format and its own parser
(`content/actors.rs`, `statures.rs`, the tile rules), and the app converts
each spec to components by hand (`ActorKind::equip`). A new file or a new
field means new parser code and new glue.

**General.** One serde format for every new data file (RON: readable,
comments, multi-line strings for ASCII art; already compiled into the web
build through `bevy_asset`, so it adds nothing to the download), read by
one generic loader into a type that derives `Deserialize`.
A new file is a new type; a new field is a new field. ASCII art stays art,
as strings, read by one shared "art to cells" helper (weapon patterns,
later areas and room layouts).

The step after that is **prefabs**: a kind of actor or object lists its
components by name with their values, read through Bevy's reflection, so a
new component is usable from data once it is registered, with no glue in
`app`. Reflection was trimmed from the web build to keep it small (#56),
so register only the gameplay components and check the download size.

Maps stay in their own text format (their point is the picture), and the
tile rules stay in the original game's formats (they are reference data
for the parity tests).

**Absorbs.** The actor roster parser, the statures parser, `ActorKind`'s
hand-written conversion, the spawn constants in `app/map.rs`.

### Interactions: something happens to a thing, and the thing reacts

**Today.** Every interaction is its own system with its own trigger:
doors open on a bump by a key holder (`open_doors`), items are picked up on
entering a cell, plates switch rooms on entering, exits load the next map
on entering, items drop on destruction, maneuvers answer a `ContextAction`.
The README already calls plates "a stand-in trigger until a trigger/action
system".

**General.** A small set of messages for what happens to a thing
(`Bumped`, `Entered`, `Acted` (the context action), `Damaged`, `Died`),
and reaction components on the thing that say what it does then
(`OpensWithKey`, `PickedUpOnEnter`, `SwitchesRooms`, `LeadsTo`,
`DropsOnDeath`), each answered by one small system. With prefabs, reactions
come from data, so a lever, a trap or a breakable crate is a new
combination, not new code.

**Absorbs.** `open_doors`, `pick_up_items`, `press_plates`, `take_exits`,
`drop_items`; `ContextAction` becomes one of the messages. **First users.**
Doors and plates (backfill), then hazards.

### Areas: sets of cells around a point

**Today.** Weapon `StrikeShape`s, field-of-view and light results, the
transition's `Oval`, room rectangles and cell marks each describe "these
cells" their own way.

**General.** An `Area`: a shape (listed cells, a radius, a sector, a
rectangle) placed at an origin with a facing, optionally limited to what
can be seen from the origin (the FOV code already answers that), giving
the cells it covers. Strikes, explosions, noise, aimed throws (the
design's sector projectiles) and marks all ask an area for their cells.

**Absorbs.** `StrikeShape`; the cell-set parts of the transition and
rooms. **First users.** Weapon strikes, then aiming.

### Conditions: states with a duration

**Today.** `Exposed` (worked out from awareness every frame), `Telegraph`
and `Channeling` (until the strike), the alert beat; soon stunned,
staggered, burning.

**General.** Conditions as small components with a duration on the time
mechanism above, applied by whatever causes them and removed when they
wear off; systems read the condition, never its cause.

**Absorbs.** How `Exposed` and the alert beat are kept. **First users.**
Staggered after an impact, exposed for a turn after a stagger.

### Previews: what an action would do, before doing it

**Today.** Marks show a wound-up strike and a pick, but nothing shows what
a second direction would do, which is likely part of why fights feel hard
to predict (`COMBAT.md`).

**General.** Resolvers split into a pure plan and an apply step (physics
sub-steps, damage). A preview runs the plan for a candidate action without
applying it and marks the result: where bodies would end up, what would be
hit. Any action that goes through the general mechanisms can then be
previewed with no code of its own.

**First users.** The three maneuvers on a pick.

### Not yet

`Health` stays as it is until a second pool exists (stamina, ammunition,
light fuel); then a general pool with modifiers. Behaviour trees stay in
code.

### Suggested order

1. **Data**: small, and every later mechanism takes its numbers from it.
2. **Time**: small; replaces `Channeling` and makes "time off" possible.
3. **Physics**: the largest; replaces both movement resolvers and makes
   pushing objects possible; its pure planning makes previews possible.
4. **Previews**: on top of physics, for the predictability issue.
5. **Interactions**: with the doors, plates and hazards backfill.
6. **Areas** and **conditions**: when aiming and status effects arrive.

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
   Make them a message (`Bumped`), like `Impact`; part of
   [physics](#physics-bodies-that-move-push-and-collide). (Principle 3.)
3. **Doors do several jobs in one system** (`simulation/items.rs`,
   `open_doors`). It spends the key, opens the door, frees the grid cell
   and changes the glyph. Split it into a decider (a bump into a door by a
   key holder), a resolver (the door opens and frees its cell) and
   presentation (the glyph follows the door's state). Opening by
   `ContextAction` then comes for free; part of
   [interactions](#interactions-something-happens-to-a-thing-and-the-thing-reacts).
   (Principles 1, 5, 9.)
4. **The AI draws** (`ai::show_mood`). It writes glyph characters and
   colours from the enemy's act. Have the AI expose its mood, and let
   presentation show it, as marks do. (Principle 9.)
5. **Hard-coded spawn stats** (`app/map.rs`). Sight radii, lamp, acid and
   electricity light values, glyphs and colours are constants in spawn
   functions. Move them into data, as actor kinds already are, ideally as
   [prefabs](#data-one-format-and-prefabs). (Principle 8.)
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
