---
number: 63
title: Pad jolts from walls and other objects
date: 2026-10-04
area: input
files: crates/hwtr-game/src/collision/walls.rs, crates/hwtr-game/src/collision/pairs.rs, crates/hwtr-game/src/collision/world.rs, crates/hwtr-game/src/race.rs, crates/hwtr-game/src/car/mod.rs, crates/hwtr-game/src/pad.rs, crates/hwtr-hle/src/original/world.rs, crates/hwtr-hle/tests/collision.rs, docs/engine/controls.md
---

# 63. Pad jolts from walls and other objects

The pad now jolts when the car hits walls and other objects, as the
original's DualShock does. Before this only a wreck jolted it, and the
road's rumble was the only other feel.

**The original.**

- **The wall jolt (0x8005fd4c).** A player's car (not wrecked) jolts at
  its step's first contact with the track, right after the contact
  timers. The level is how fast it goes into the wall:
  `-(vel . normal) / 2304`, times 255 over 4096, at most 255. Then it
  waits 10 frames.
- **The pair jolt (0x8005fed0).** For each side of a pair whose car is a
  player's, after the pair's sound and before its crash check:
  - A prop the car knocks gives a set jolt (TUNING +0x30), and one that
    lifts it +0x31 (object flags 2 and 4).
  - A heavy knockable prop (weight over 10000) meeting a car that is not
    all-terrain jolts by speed instead, as anything else does. Speed is
    the relative velocity's length over 2304, capped at 4096, times 255
    over 4096. Then it waits 20 frames.
- **The waits.** They are per player (0x800d0e2c) and count down at the
  top of 0x8005fba8 each frame, before the road's feel.

**The port.** The collision now queues `jolts` and keeps `jolt_wait`. The
race sends each jolt to the motors of its port (`Motors::jolt`, the same
as the wreck's) and counts the waits down. `Tuning` gains `knock_jolt` and
`lift_jolt`. The "a player's rumble: not yet ported" and "The rumble is
not yet ported" notes are gone. The pad reader's "vibration timers not
yet ported" note was stale: those timers are `Motors::fade`, which the app
already runs at each read.

**Checked against the original** (tests/collision.rs):

- **Hooking the jolt.** Both tests hook the jolt, iface_controls+0x28 as
  the running game fills it.
- `wall_jolts_match_the_original`: 2000 rounds over the four DESERT1
  states. Each round gives any car velocity, any wall normal and any
  waits. The jolts and waits match. More than 200 jolted and more than
  200 did not.
- `pair_jolts_match_the_original`: 3200 rounds. The other object is any
  of the race's, made knockable, lifting, both or neither, at any weight.
  Every car has any speed, any all-terrain mark, any handling flag, and
  any tuning bytes and waits. More than 200 set jolts, 200 by speed and
  100 with none all match.

**Still unknown:** Whether a prop with a body of its own gives its velocity to the pair jolt (the port takes none).
