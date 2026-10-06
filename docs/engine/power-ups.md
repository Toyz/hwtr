---
title: Power-ups (the track's pickups)
status: partial
discs: US
covers: US CCCPSX.EXE:0x80067418 pickups_load, 0x800671a8, 0x80067b64 pickup_power_up, 0x80067ac4 pick, 0x80067f98 pickup_take, 0x80067d50 power_ups_drop, 0x800679b0 power_ups_step, 0x80068130, 0x80065520 power_up_apply, 0x80065ecc power_up_remove, 0x80067708, 0x80067dfc power_up_icon, 0x800672a0, 0x800d0e9c pickups, 0x800d0ea0 pickup_count, 0x800d2698 power_up_list, 0x80061c00 list_add, 0x80061cac list_remove, 0x80067c3c, 0x80067c98, 0x8006122c race_time, 0x800d0e34, 0x8012ff04 held, 0x800d0ea6 unlockable, 0x800d0eaa unlocked
worklog: 89, 90
---

# Power-ups

Each pickup on a track names a power-up (`<name>.PUP`, 180 bytes, in the
track's archive) or "Random". A car that drives through a pickup that is
out (its collision box, kind 5) takes it. The pickup comes back ten
seconds later (0x80068130) if it gives a power-up by name.

## A power-up (`.PUP`)

| at | what |
| --- | --- |
| +0x00 | its file name, 12 bytes |
| +0x0c | what the HUD calls it, 32 bytes |
| +0x2c | seconds it lasts |
| +0x30 | a kind, kept with the car's power-up |
| +0x34 | flags (below) |
| +0x38 | 24 factors, 4.12 |
| +0x98 | a kick at taking it, in the car's axes, times its mass |
| +0xa8 | the steering lock's factor |
| +0xac | four bytes added to the drive and steer layout |
| +0xb0 | it never runs out |

**The factors**, in order:

- the handling: its mass, its centre of gravity along and up, brake grip
  and bias, drag, downforce and its scale front and rear
- the air power's yaw, pitch and roll
- each axle's stiffness, grip and damping in and out, front then rear
- the engine's redline, idle and peak torque

**The flags:**

| flag | what |
| --- | --- |
| 1 | strong brakes |
| 2 | a callback no power-up uses |
| 4 | a turbo |
| 8 | all-terrain (4x4) |
| 16 | steel (in the cars' knocks; skill times 2.5) |
| 32 | rubber |
| 64 | gyro |
| 1 << 24 | the track's first car unlocked |
| 1 << 25 | the track's second car unlocked |

## Loading (0x800671a8)

For each pickup, in the track's order, the power-up it names is loaded
once ("Random" loads none) into a list (0x800d2698). The list links each
new one at its head (0x80061c00), so it holds them newest first. That
order matters: "Random" draws the k-th of the list.

**The records.** A pickup is 24 bytes:

| at | what |
| --- | --- |
| +0x00 | its name |
| +0x04 | its number on the track |
| +0x08 | its power-up's number (0x800672a0) |
| +0x0c | when it was last taken, race ms (0x800d0e34) |
| +0x10 | out |
| +0x14 | its collision object |

Each car's held power-ups are a list from 0x8012ff04, newest first, each
12 bytes: the power-up, since when, its kind, whether it never runs out.

## Taking one (0x80067f98)

1. **The power-up.** A named pickup gives its own. "Random" gives any
   loaded power-up but UNCAR1 and UNCAR2, drawn by 0x80067ac4 until it is
   neither (0x80067b64).
2. **The old ones go.** The car loses every power-up it had (0x80067d50,
   each undone).
3. **It is applied** (0x80065520) unless it never runs out. Applying one:
   - kicks the car
   - multiplies its factors in, and sets its flags
   - fits the springs and wheels again (0x8004528c)
4. **The HUD.** It shows the power-up's icon (0x80067dfc), except a
   turbo's. A player hears sound 24.

Each frame (0x800679b0) a power-up past its seconds is undone (0x80065ecc:
the factors divided back out, the flags cleared, the springs and wheels
fitted again).

## The unlock pickups

UNCAR1 and UNCAR2 carry flags 1 << 24 and 1 << 25. Taken by a player's car
(car flag 1) in slot 0 or 1, under full physics, they note the track's
first or second car (0x800d0ea6, 0x800d0ea8) for that player:

| | player 1 | player 2 |
| --- | --- | --- |
| first car | 0x800d0eaa | 0x800d0eac |
| second car | 0x800d0eae | 0x800d0eb0 |

The race hands the noted cars to the front end with its result. Its
results then add any the player did not have, and announce them.

## In the port

`hwtr_game::powerup`:

- `PowerUp::parse`
- `PowerUps` (`take`, `drop_all`, `step`, `apply`, `unlocked`)
- `apply` and `remove`

The app's `Race::result` packs `unlocked` into the result's words, each
player's cars 0 to 31 then 32 on. `load_defs` is 0x800671a8's order; the
app had kept the power-ups oldest first, which made a "Random" pickup draw
a different power-up from the original's for the same seed.

Tests (hle tests/powerup.rs):

- `power_ups_change_a_car_as_the_original_does` applies and removes nine
  shipped power-ups (HANDLING, GYRO, BRAKES, STICKY, TURBO, UNCAR1, STEEL,
  RUBBER, 4X4) on a player's car through 0x80065520 and 0x80065ecc. It
  compares the whole car after each.
- `the_unlock_pickups_note_cars_as_the_original_does` takes UNCAR1 and
  UNCAR2 with the cars in slots 0 to 2 made players' and compares the
  notes.
- `power_ups_load_in_the_originals_order` runs `load_defs` over each of
  the 11 tracks' pickups and compares it with the original's list.
- `pickups_are_taken_and_come_back_as_in_the_original` runs 150 rounds on
  each track. In each, a random car drives through a random pickup, put
  out or not (0x80067f98), then the clock moves on up to 6 s and a frame's
  step runs (0x80068130). It compares every car, the pickups, each car's
  held list, the cars noted and the random seed after each. Over the run
  there are about 1400 pickups taken, 900 power-ups run out, 960 pickups
  back and 140 "Random" draws.

  The test leaves the port's turbo event (`turbo_given`) out, as it has
  nothing in the original's memory.

## Unknown

- racecheck still counts a step where a power-up was taken and does not
  check it: the race applies a pickup after the collision step, where the
  original takes it inside.
- The callback flag 2 (iface slot 0x8012fe00), which no shipped power-up
  sets.
