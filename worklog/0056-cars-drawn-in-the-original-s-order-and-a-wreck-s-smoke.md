---
number: 56
title: Cars drawn in the original's order, and a wreck's smoke
date: 2026-10-04
area: render
files: crates/hwtr-game/src/race.rs, crates/hwtr-game/src/effects.rs, crates/hwtr-hle/tests/effects.rs, docs/engine/effects.md
---

# 56. Cars drawn in the original's order, and a wreck's smoke

The cars are now drawn as the original's world draw draws them, and a
wreck smokes.

**Order and distance.**

- `Race::draw_cars` runs first in the frame's effects, before the
  particles, trails, rings and columns, as in 0x8001ef24.
- For each view it takes cars 0 to 5. Each shown car within `5400² +
  2²⁴` squared units of the eye is drawn, measured from the model's
  place at the last frame's end (`Race::model_poses`, set where
  car_set_pose runs).
- Before, the boost flame was drawn for every car after the columns. A
  flame draws a random number per quad, so the order and the distance
  test both matter to the game's sequence. The shadow moved into the same
  loop.

**Smoke (`Effects::wreck_smoke`).** With the full model (within 1350
units), a wrecked car puffs grey smoke from its wheels' mounts:

- on frames that are not a multiple of four, from every wheel, once its
  model moved 10 units since its last draw (measured only for a player's
  car, `Race::moved`)
- otherwise every seventh frame, from wheels 0 and 3

Each puff draws two random numbers.

**Not ported.** The lamp glows (0x80029fb0) also draw random numbers,
but only for a car whose cvs +0x28 is 0. Every car in the saved states
has 2 to 10 there, so they are left out and recorded as unknown.

**Checked against the original.** `a_wrecks_smoke_matches_the_original`
(tests/effects.rs) runs 3000 rounds of the original car draw, 0x80022064,
on a wrecked car. Each round varies the distance (near or far), how far
its model moved (measured by the original for a player's car), and the
frame count, with no flame and no shadow plane. The puff ring and the
seed match the port's, and more than 200 rounds puffed.

The draw limit (5400 returned by 0x800125f0) and the model thresholds
(1350² and 2700² read from a saved race) come from the code and the
state. The eye is taken as the camera's position in whole units. The
original inverts the view matrix, so the two could differ by a unit, which
matters only for a car right at the edge.

**Still unknown:** the lamp glows' condition (cvs +0x28 = 0); the lights 0x8002a81c/0x8002ad48; the eye from the inverted view matrix
