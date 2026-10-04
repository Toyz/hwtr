---
number: 23
title: Cars built from the disc: the handling, the body, the grid
date: 2026-10-03
area: physics, format, test
files: crates/hwtr-game/src/car/handling.rs, crates/hwtr-game/src/car/load.rs, crates/hwtr-game/src/body.rs, crates/hwtr-game/src/race.rs, crates/hwtr-hle/src/bin/loadcheck.rs, plans/playable.md
---

# 23. Cars built from the disc: the handling, the body, the grid

First milestone of [the plan to playable](../plans/playable.md): the port
builds a race's player car from the disc's data, and it matches the
original's `cars_load` (0x8003beec) field for field.

**Where a car comes from.** `cars_load` clears the six car records, then per
car:

1. reads the grid point and quaternion from the SCP (0x8005ccc8);
2. copies the CWH's block A (308 bytes) to car +0x63c and block B (64 bytes)
   over the engine from its redline (+0x568) (`car_read_cwh`);
3. fits springs and wheels (0x8004528c);
4. builds the rigid body (0x8006bdd8, inertia 0x80071940);
5. turns the quaternion into the rotation (0x8006abe4, stored transposed);
6. places the car;
7. marks players' cars state 2 and computer cars state 1 (with AI setup);
8. scales a skill value by the difficulty;
9. makes the collision object.

The code reads the handling straight out of the car, so block A is now a
`Handling` type embedded in `Car`, converted to and from its exact bytes.
Mass, brake, drag, grips and damping live there, with an `Axle` each for
front and rear. Each axle has six contiguous words (stiffness, grip, travel,
damping in and out, ride height), and its two downforce words are
interleaved with the other axle's.

**Findings on the way.**

- The race setup the front end fills in is at 0x80138c94. It holds the
  track ("Desert", number 1: hence `Desert1.scp`), the car count, one 13-byte
  record per car (name, driver, car id, player, grid place) and the
  difficulty. The player drove "spltimg" (Split Image) from grid place 3.
- The drive layout is four CWH bytes: front driven, front steers, rear
  driven, rear steers. Split Image is all-wheel drive with front steering.
- What [[19]] called the drag point (car +0xfc) is the rigid body's centre
  of mass offset (body +0xcc), set as the body is made. `aero` applies drag
  there, and the start position subtracts it.
- The inertia is a box's, `m(a² + b²)/12` per axis, kept 8 bits up in 64
  bits. The inverse is `12·2³²/(m(a² + b²))`, both through `__divdi3`.
- The mount points' padding words carry data (two halfwords, 0x20/0x20 or
  0x10/0x10), so they are modelled.
- The start height uses the higher of the two ride heights. The first port
  took the lower, since `if !(a < b) skip; a = b` reads as a minimum until
  you look twice. loadcheck showed it as a 1-inch difference in z.

**Checking it.** The function runs during loading, across many frames, so a
shadow check that skips calls with interrupts in them never saw it.
`Machine::check_through_interrupts` keeps those. `loadcheck` boots the
original from the disc, drives the menus with the scripted input, and when
`cars_load` returns it builds each player's car the port's way: the setup
from RAM, the CWH from the disc, the grid from the SCP. It then compares
the car field by field. Two things are excluded:

- flag 0x20, which the zone code sets while making the collision object
  (M2);
- the centre vector's padding word, which the original fills from
  uninitialised stack.

**Still unknown:** computer cars' start placement on the AI's line (0x8007c6fc) and their AI setup (0x80079e58); the collision object (0x8004c478) and the zone code that sets car flag 0x20; the meanings of several handling words (+0x04, +0x6c, wheel +0x10)
