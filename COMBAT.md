# Combat design

Design notes for the battle system: what it aims for, the control scheme,
and the cut taken for the first playable version (MVP). The MVP is in
`src/combat/` (see `ARCHITECTURE.md`); try it on `assets/maps/arena.txt`.

## Goals

- **Simple controls.** Four directions and wait, the same on keyboard,
  gamepad and touch. No on-screen buttons on touch; gestures only.
- **Stealth, then a short fight, then running away.** Most fights start from
  stealth on the player's terms, turn into a quick, deadly confrontation, and
  end with an escape.
- **A predator, but fragile.** The player is lethal when unseen and dies in a
  couple of hits when not.
- **Systemic and combo-based.** Enemies and the environment are tools: knock
  enemies into each other, into walls, into each other's strikes, later into
  hazards (electricity, acid, fire, rooms that rebuild).
- **Fast, not chess.** The step timer stays: idle turns still advance. The
  player must think fast and is allowed to make mistakes.
- **Extensible.** New tools, enemy types and objects are data plus small
  reaction rules.

## Controls: every move is read from where you face

There is no picking and nothing to cancel. Each move (a swipe, an arrow)
is one action, read relative to the way the player faces (the marker
beside the `@`): forward, left, right or back.

- **The weapon has an attack for each way.** An attack plays when one of
  its cells holds a target (anything with hit points); it takes the turn.
- **A move that meets nothing is a plain step**, and the player turns to
  face it. Being knocked, following through or stepping back does not turn
  the player; only walking and an attack that turns do.
- **Next to an enemy the player is engaged:** the moves toward it attack.
  Walking past one can sweep it.
- **A timeout never acts for the player.** The turn passes like a wait.

## The attacks: one for each way

An attack is data (the blade's in `assets/rules/actors.ron`): the cells it
strikes, drawn as art with digits for the order its swing goes through
them, its damage, how far it knocks each target (the way the swing travels
through its cell), where it leaves the player (stay, follow into the cell
the first knocked target left, or step the way of the move) and whether
the player turns to face the move. It hits every target in its cells in
order, or, for a weapon that `stop`s, only the first.

The blade (`@` faces up; digits are the swing's order):

```text
forward: lunge     left: sweep left    right: sweep right   back: back off
   1                  21                  12                   x
   @                  3@                  @3                   @
 knocked on;        what is ahead is    the same, to the     no hit: a step
 you take its       swept left, what    right                back, still
 cell               is beside swept                          facing it
                    back; you turn left
```

### Damage

- Enemies and the player have **2 HP** (the player's is a knob, so `1` can be
  tried as the fragile version).
- **Critical hit:** attacking an enemy that is unaware, or still in its `!`
  beat after spotting the player, does 2 damage and kills it.
- **In open battle** a hit does 1 damage and moves the enemy as above.
- **Slam:** if the cell the enemy would be moved to is blocked (a wall, a
  closed door, or another actor), it stays where it is and takes **+1
  damage**, and whatever blocked it takes **+1 damage** too (walls and doors
  have no HP). A slam into a wall therefore kills an alert enemy outright.
  If a lunge is blocked, the player stays put as well.

## Telegraphed enemy strikes

- An alert enemy next to the player winds up for one turn and marks its
  strike in red, then strikes on the next turn.
- **The strike is a direction from the enemy, not a fixed cell.** If the
  enemy is moved, its strike moves with it and hits whatever is in the new
  cell: the player, another enemy (friendly fire), or nothing.
- A hit does not interrupt the wind-up.

That makes each attack a different trade-off against an alert guard,
whose sweep covers the row in front of it:

```text
lunge (forward)     sweep (right)        back off
 . E .               . E .                . E .
 . @ .               . @ .                . @ .
 -> . E .            -> . . E             -> * * *
    * @ *               . @ *                . @ .
 you advance into    it lands on you      safe: you step
 its sweep           from beside          out of its row
```

`E` winds up toward `@`; `*` is where its sweep lands after the move. A
sweep can aim an enemy's strike at a second enemy standing beside the
player.

## MVP scope

The MVP answers one question: **under the real step timer, on a phone and on
a keyboard, do moves read from facing feel fast enough and give choices
that matter against telegraphed strikes?**

In:

1. Moves read from facing: an attack for each way, when its cells hold a
   target (the first cut picked a target with one input and attacked
   with the next: lunge, push or yank; contextual attacks replaced it).
2. Enemies with 2 HP: critical hits on unaware or `!` enemies, 1 damage and
   displacement in battle, slams for +1 to both sides.
3. Telegraphed strikes relative to the enemy, with friendly fire.
4. Player HP (default 2, knob), instant restart of the map on death (no
   intro).
5. An arena map (`assets/maps/arena.txt`, 2–3 enemies, corridors) and a
   step-timer knob (`turn=<seconds>`), so the pressure can be tuned on a
   phone from the preview link.
6. Scenario tests for each attack, plain walks, a moved telegraph,
   friendly fire, criticals and slams; the agent API exposes HP, the
   player's facing and the marked cells.

Out, roughly in the order they would follow:

- A preview of the attack a move will make, played as its anticipation,
  with a tap to cancel.
- Strikes meeting in a cell from opposite directions as a block or parry.
- Slowdown while a gesture is in progress; the one-stroke L gesture.
- Momentum: knocked bodies flying several cells, impacts passing force on,
  stagger.
- More verbs (kick, throw), dash and pounce (double swipe), feint.
- Hazards and chain reactions (electricity, acid, fire, crushing rooms).
- Objects and doors as targets (crates, lamps, kicking doors).
- Aiming and tools: long press to aim, long press on the player for a radial
  tool ring; aiming slows the world, telegraphs to enemies that see it and
  charges over turns. Projectiles cover a sector, resolved like FOV, not a
  line.
- Light-based detection, noise and alarms, bodies as evidence.
- More enemy types and telegraph shapes (lines, cones).

### As built

- **Timing.** An enemy winds up its weapon on its turn; for a weapon wound
  up one turn the strike lands at the start of the enemy's next turn,
  before anyone acts in it. The player gets exactly one action in between:
  step off, or knock the striker so its blow lands elsewhere.
- **Strike patterns decide the answer.** Each weapon strikes its own cells
  (drawn as art in `assets/rules/actors.ron`). Against a guard's sweep (the
  row in front) stepping aside does not help and, of the attacks, only
  backing off is safe: a lunge or a sweep leaves the player in the row the
  sweep covers once the guard is knocked. Against a spearman's spear (two cells
  ahead) stepping back does not help, but stepping aside does. Two kinds
  side by side ask for different answers at once. This answers the first
  playtest: a fight with one enemy that only struck the cell in front was
  no challenge, since any move dodged it.
- **Marks.** Whatever stands on a cell about to be struck is drawn red; an
  empty marked cell shows a red `×`. Lighting does not change these
  colours.
- **Holding a key.** A key held toward an enemy attacks it each turn, as a
  key held toward open floor walks.
- **Facing starts north**, so a swipe is read from somewhere on the first
  move.
- **Knobs.** `--turn SECONDS` (or `?turn=` on the web) sets how long a turn
  waits for the player; `--hp N` the player's hit points.
- **Found while building it.** Enemies see all around them, so one is
  rarely unaware when the player gets next to it. Critical hits mostly land
  on the `!` beat, or from darkness and around corners. Sight that depends
  on facing would make stealth openers a real choice.

### Open: fights feel hard to predict

From playtesting the preview: a fight does not yet feel intuitive or
predictable. To look into separately; possible causes, none decided yet:

- **The patterns.** A sweep or a spear may be hard to read from the glyph
  alone, and a pattern shows only once the enemy winds up.
- **Missing feedback on a move.** Nothing shows which attack a move will
  make: where the target and the player would end up, and which cells
  would then be struck.
- **Missing motion.** Knockbacks, swaps and strikes could animate, so the
  player sees what happened and why. A first step is in: each exchange is
  choreographed (below).

### Choreography

The simulation settles a blow in one frame; the animation plays it in
order (`src/choreography/`):

1. the attacker swings as its weapon or move says in the roster
   (`swing`): a thrust straight at the target (a spear reaches further),
   a sweep in an arc across the front, round the way its cells go, or a
   hook that reaches out and pulls back; a move with no swing (backing
   off) makes no gesture; every
   swing winds up (eases into a pulled-back or far-side pose), strikes
   fast into the contact, follows through and eases back to rest; a
   lunge's step or a step back waits until just after contact;
2. at contact the target is flung (fast, slowing to a stop) or shakes
   away from the blow,
   and what a knocked body runs into shakes too;
3. a killed target dies after it has been moved, as a blow that does not
   kill: its remains finish the slide, then sink.

Anticipation lasts as long as what it anticipates:

- **An enemy's wind-up is its telegraph.** When it aims, it eases into
  its wound-up pose (drawn back to the side a sweep starts from, back for
  a thrust) and holds it until it strikes; the strike then plays only the
  fast release, so its blow lands sooner. The pose is relative to the
  enemy, like its strike, so knocking it moves both.
- **The marked cells are planted in the order the weapon swings through
  them** (the digits of its art), each growing `·`, `+`, `×`. An aim
  dropped without a strike eases the pose back.

Effects give it weight (`src/effects/`): a swing leaves a streak where
the weapon passed (a line for a thrust, an arc for a sweep, drawn in the
order it swings), a blow flashes where it lands and splashes blood the
way it drove the target (more on a kill), and the drops land and lie a
while before they fade. Effects are data: each is a file in
`assets/effects` (`swoosh.ron`, `blood.ron`), and the roster names them
(a weapon's or move's `streak`, an actor's `bleeds`), so a new look (a
crate's splinters, a robot's sparks) is a new file and a roster line.

Still to come, each its own step once needed: hit points shown on
enemies, decals that last, camera shake or hit-stop, sound.

To watch it, `--slomo 4` (`?slomo=4`) runs the whole game four times
slower, and `--hp` / `--enemy-hp` give everyone more hit points.

## How it is put together

Small modules that share only data and messages (see `ARCHITECTURE.md`):

- **Facing is an intent.** A `Turn` message turns an actor; the player
  `FacesItsWalks`, so only its walks and its attacks turn it.
- **Moves are data.** `Maneuvers` hold one move per `Relation` of a move
  to the facing (forward, left, right, back): its `Area` (cells in swing
  order), whether it stops at the first target, damage, knock, `Stance`
  and turn. `choose_attacks` turns a move that meets a target into an
  `Attack`; `perform_attacks` answers it. A new fighting style is a new
  `moves` block in the roster.
- **Weapons own their strikes.** A `Weapon` (an `Area`, damage, wind-up)
  on any actor answers `UseWeapon`: it aims (`Telegraph`), owes its
  wind-up in turns (`Delay`) with the `Strike` scheduled for when that is
  paid, then strikes its area. The AI only decides to attack. New patterns
  are new art in the roster.
- **Time is general.** An actor in `Debt` sits out its turns until paid,
  and any message can wait on that (`Scheduled`). A wind-up is one user; a
  stun or a stagger is a `Delay` with nothing scheduled.
- **Knockback rides the simulation's movement.** A `Knock` gives the
  target a speed and marks it `Knocked`; the same movement passes that move
  walkers move it, and a knocked body that is blocked is a `Collision`. A
  lunge's step after the target and a step back are a `Follow`. Pushing
  enemies on into others, pushing objects by walking into them, and knocks
  longer than a cell are postponed.
- **Damage is its own thing** (`damage/`): `Damage` comes off `Health`,
  whatever sent it. Hits, critical hits and collisions are separate small systems that each send their share; a hazard, fire or a fall can send
  `Damage` too.
- **Who is who is data.** Actor kinds, by map glyph, name their
  behaviour, hit points and weapon or moves (`assets/rules/actors.ron`),
  read by the one generic RON reader: a new data file needs only a type.
- **Input is intent.** Keys, swipes and the agent API all write the same
  `MoveCommand`; nothing reads devices.
- Still to come: actions in progress (aiming, with a charge), built on the
  same pieces; and reactions to impacts and damage
  (fire, electricity, falling) as more small systems on those messages.
