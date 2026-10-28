# Combat design

Design notes for the battle system: what it aims for, the control scheme,
and the cut taken for the first playable version (MVP). Nothing here is
implemented yet; enemies currently only bump the player (`EnemyAct::Attack`).

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

## Extensibility sketch

- An action in progress is one component checked every turn:
  `PendingAction { kind, target, since_turn, charge }` with `valid`,
  `preview` and `complete`. The turn system completes, carries over or drops
  it; the timer never fires it.
- Input layers (touch, keyboard, gamepad) emit the same intents: `Select`,
  `Act`, `AimUpdate`, `Release`, `Cancel`.
- Verbs produce effects (`Damage`, `Displace`, later `Launch`, `Impact`,
  `Ignite`, `Electrify`, `Noise`); reactions are rules from
  `(effect, target tags)` to follow-up effects, run from a queue within the
  turn. Chain reactions come from those rules, not special cases.
- A new enemy type is a behavior tree, tags, HP and a telegraph shape; a new
  object is tags and reactions; a new tool is an aim shape, a charge curve
  and effects.
