---
number: 29
title: Vibration: the road's feel and wrecks through the pad
date: 2026-10-03
area: input
files: crates/hwtr-game/src/pad.rs, crates/hwtr-game/src/race.rs, crates/hwtr-game/src/car/wreck.rs, crates/hwtr/src/race.rs, crates/hwtr/src/main.rs, crates/hwtr-hle/tests/pad.rs, crates/hwtr-hle/tests/car.rs
---

# 29. Vibration: the road's feel and wrecks through the pad

The game drives a DualShock's motors, and the port does too now, through
rrt's `Input::rumble`. The controls screen's first line is "Vibration", on
by default: it is the last word of a player's button mapping (0x8001d578).

## What runs the motors

Each port keeps two actuator bytes (0x8011b2b8 + 98 a port, +0x18 the small
motor, +0x19 the large one's power). `hwtr_game::pad::Motors` ports the three
routines that set them.

- **The road** (0x8001b7b4). Each race frame while racing (race phase 1),
  every player's car tells its pad how the ground feels (0x80045ff4,
  `Car::road_feel`):
  - roughness: the average over all its wheels of the roughness of the
    surface under each one touching the ground (the table at 0x800bea7c;
    smooth road is 0, the rough kinds 32 to 255);
  - speed: `255 * (speed / 2304) / 4096`, capped at 255.

  On rough ground faster than a crawl (speed 11 or more), the large motor
  runs at `max(roughness, 96) * max(speed, 64) / 256`.
- **A wreck** (0x8001b934, from 0x8004619c). A player's wreck jolts its
  pad at `TUNING.PRM` byte 0x32: the large motor at 2.5 times that (nothing
  below 3, as 40 below 40), and from 110 the small motor too, for half a
  second.
- **The wind-down** (0x8001ccc4), at each pad read. The large motor loses
  one step per 8 ms since the last read (at most 125 a read); the small motor
  stops after 500 ms. Both stop when the game's state (0x800d246c) is not
  racing.

The decompiler got two of these wrong, so the port follows the disassembly
in both places:
- it dropped the `srl 3` that turns milliseconds into steps;
- it misread a delay slot in the jolt's clamp, which raises small levels to
  40 rather than capping big ones.

## Tests

- `hwtr-hle/tests/pad.rs`: 6000 random calls of the three routines (libpad's
  state and actuator calls stubbed to a DualShock), with the motor bytes
  compared after each.
- `hwtr-hle/tests/car.rs`: `road_feel` against 0x80045ff4 on 3000 random
  ground-and-speed setups.

## In the program

The race winds the motors down at each read (before each 25 ms step) and the
program sends them to the pad each frame; leaving the race stops them. Only
player one's pad is driven, as the program has one pad.

**Still unknown:** nothing
