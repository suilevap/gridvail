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

## Controls: one turn, one composed move

Within a turn the player composes a move from a short sequence of inputs.
The turn resolves when the move is complete.

- **Swipe or arrow into an enemy selects it.** Selecting is free: it is an
  extra input dimension, not an action, and spends no turn.
- **The second direction is the action**, read relative to the selected
  enemy (see below).
- **A timeout never acts for the player.** If the turn ends mid-composition,
  the turn passes like a wait and whatever is in progress carries over to the
  next turn, as long as it is still valid (the selected enemy is still
  adjacent and alive). Anything in progress that stops being valid is dropped
  with a visible fade.
- **Cancel is always available and free.** On touch, any tap cancels a
  selection (with nothing selected a tap is a wait). On keyboard, a
  dedicated key (not Esc, which opens settings); on a gamepad, B / Circle.

## The attack: three outcomes by direction

Every completed attack hits. What the second direction changes is where the
player and the enemy end up.

| Second direction | Name | Player ends | Enemy ends |
|---|---|---|---|
| Into the enemy (same direction again) | lunge | the enemy's cell | one cell further on |
| Sideways (either side) | push | where they were | one cell to that side |
| Back (away from the enemy) | yank | the enemy's cell | the player's old cell (swap) |

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

That makes each outcome a different trade-off against an alert enemy:

```text
lunge (into)        push (sideways)      yank (back)
 . E .               . E .                . E .
 . @ .               . @ .                . @ .
 -> . E .            -> . . E             -> . * .
    . @*.               . @ *                . E .
                                             . @ .
 you advance, but    safe; its strike     safe; its strike
 you stand in its    lands beside you     lands behind it
 strike
```

`E` winds up toward `@`; `*` is where its strike lands after the move. A push
can aim an enemy's strike at a second enemy standing beside the player; a
yank turns it away.

## MVP scope

The MVP answers one question: **under the real step timer, on a phone and on
a keyboard, does select → direction feel fast enough and give choices that
matter against telegraphed strikes?**

In:

1. Dual input: select (free, carries over, any tap cancels), then lunge,
   push or yank.
2. Enemies with 2 HP: critical hits on unaware or `!` enemies, 1 damage and
   displacement in battle, slams for +1 to both sides.
3. Telegraphed strikes relative to the enemy, with friendly fire.
4. Player HP (default 2, knob), instant restart of the map on death (no
   intro).
5. An arena map (`assets/maps/arena.txt`, 2–3 enemies, corridors) and a
   step-timer knob (`turn=<seconds>`), so the pressure can be tuned on a
   phone from the preview link.
6. Scenario tests for each outcome, carry-over and cancel, a moved telegraph,
   friendly fire, criticals and slams; the agent API exposes HP, the
   selection and the marked cells.

Out, roughly in the order they would follow:

- Outcome preview arrows on the selected enemy.
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
  (drawn as art in `assets/rules/actors.txt`). Against a guard's sweep (the
  row in front) stepping aside does not help and, of the maneuvers, only a
  yank is safe: a lunge or a push leaves the player in the row the sweep
  covers once the guard is knocked. Against a spearman's spear (two cells
  ahead) stepping back does not help, but stepping aside does. Two kinds
  side by side ask for different answers at once. This answers the first
  playtest: a fight with one enemy that only struck the cell in front was
  no challenge, since any move dodged it.
- **Marks.** Whatever stands on a cell about to be struck is drawn red, a
  picked target yellow; an empty marked cell shows a red `×`. Lighting does
  not change these colours.
- **Holding a key.** Walking into an enemy with a held key picks it; the
  same key held on does nothing until it is let go, so a lunge always
  takes a fresh press.
- **Knobs.** `--turn SECONDS` (or `?turn=` on the web) sets how long a turn
  waits for the player; `--hp N` the player's hit points.
- **Found while building it.** Enemies see all around them, so one is
  rarely unaware when the player gets next to it. Critical hits mostly land
  on the `!` beat, or from darkness and around corners. Sight that depends
  on facing would make stealth openers a real choice.

## How it is put together

Small modules that share only data and messages (see `ARCHITECTURE.md`):

- **Picking is its own thing** (`selection/`): a `Selector` picks
  `Selectable` things and turns its next move into a `ContextAction`. What
  that does is up to whoever answers it: maneuvers today; a door, a lever
  or a tool tomorrow, without touching picking.
- **Moves are data.** `Maneuvers` hold one move per `Relation` of the
  second direction to the first (a name, the hit's damage, whether it
  knocks the target and the actor follows). A new fighting style is a new
  `@moves` block in the roster.
- **Weapons own their strikes.** A `Weapon` (pattern, damage, wind-up) on
  any actor answers `UseWeapon`: it aims (`Telegraph`) and winds up
  (`Channeling`), then strikes its cells. The AI only decides to attack.
  New patterns are new `@weapon` art.
- **Multi-turn actions are general.** `Channeling` spends an actor's turns
  before anyone decides, so nothing else needs to check whether an actor
  is busy.
- **Knockback is general.** `Displace` moves things together (swaps,
  chains); a blocked forced move is an `Impact`.
- **Damage is its own thing** (`damage/`): `Damage` comes off `Health`,
  whatever sent it. Hits, critical hits and impacts are separate small
  systems that each send their share; a hazard, fire or a fall can send
  `Damage` too.
- **Who is who is data.** Actor kinds, by map glyph, name their
  behaviour, hit points and weapon or moves (`assets/rules/actors.txt`).
- **Input is intent.** Keys, swipes and the agent API all write the same
  `MoveCommand` (and `CancelIntent`); nothing reads devices.
- Still to come: actions in progress beyond a pick (aiming, with a
  charge), built on the same pieces; and reactions to impacts and damage
  (fire, electricity, falling) as more small systems on those messages.
