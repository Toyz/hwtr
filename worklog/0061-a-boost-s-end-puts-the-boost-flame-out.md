---
number: 61
title: A boost's end puts the boost flame out
date: 2026-10-04
area: engine
files: crates/hwtr-game/src/car/update.rs, crates/hwtr-game/src/car/mod.rs, crates/hwtr-game/src/race.rs, crates/hwtr-game/src/snapshot.rs, crates/hwtr-game/src/effects.rs, crates/hwtr-hle/tests/car.rs
---

# 61. A boost's end puts the boost flame out

A boost's end now puts the boost flame out, as the original does.

**The original.**

- **When it happens.** `cars_update` (0x8004064c) checks each car before
  its update. When the car is boosting and its speed plus 30 mph is under
  the boost's speed, it calls iface_general+0xd8 and clears the boost.
- **What the slot is.** Read from the running game, the slot is 0x8002aff4.
  The port already had that function as `Effects::flame_stop`: the flame
  out, the colour pulse stopped (grey unless wrecked), and the effects'
  wreck mark cleared.
- **Where else it runs.** The results' snapshot put-back (0x8004aef8) calls
  it for every car. A note in the port called that "the trail cleared";
  it is this flame stop.

**The port.**

- **Boost end.** `Car::run_timers` sets the car's new pending `flame_out`
  where it had a "not yet ported" note.
- **The race.** It calls `flame_stop` for that car, before a turbo that
  might relight the flame in the same step, as the original's order has
  it.
- **The results' replay.** It puts every car's flame out before charring
  or clearing its effects.

**Checked against the original.** `a_boost_ends_as_in_the_original`
(tests/car.rs) runs 900 rounds of the whole `cars_update`, with 0x8002aff4
hooked. Every car gets any speed, and a boost or none at any speed. The
cars whose flames the original puts out must be exactly those whose
`run_timers` sets `flame_out`. More than 300 boosts ended and more than
300 were kept.

**Still unknown:** nothing
