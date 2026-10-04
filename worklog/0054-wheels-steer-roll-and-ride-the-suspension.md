---
number: 54
title: Wheels steer, roll and ride the suspension
date: 2026-10-04
area: render
files: crates/hwtr-game/src/car/draw.rs, crates/hwtr-game/src/car/mod.rs, crates/hwtr-game/src/effects.rs, crates/hwtr-game/src/race.rs, crates/hwtr-render/src/mesh.rs, crates/hwtr/src/race.rs, crates/hwtr-hle/src/original/car.rs, crates/hwtr-hle/src/original/effects.rs, crates/hwtr-hle/tests/car.rs, crates/hwtr-hle/tests/effects.rs, docs/engine/car-wheels.md, docs/engine/effects.md
---

# 54. Wheels steer, roll and ride the suspension

The cars' wheels now steer, roll and ride their suspension on screen.
Before, every wheel sat at its model offset.

**The pass (0x80049ecc).** Each car near a camera, every frame, as in
`Race::effects_frame`.

- `Car::turn_wheels`: each wheel's new angle (+0x80) grows by its spin
  rate times the frame's seconds unless the car sleeps, and wraps at
  ±100π.
- `Car::wheel_poses` (0x80020a14), skipped for a wrecked car, gives each
  wheel's node rotation and its lift:
  - rotation: the identity steered about z by the steering input
    (radians, reversed from the third wheel on), then turned about the
    axle by the angle, both through libgte's RotMatrixZ/X and the GTE
    product
  - lift: compression less the axle's ride height
- The race keeps the poses (`Race::wheel_poses`). The renderer draws
  each wheel child through its rotation, at twice its record's mount
  raised by twice its lift.

**Two findings.**

- A wrecked car pushes no trail points: the push sits inside 0x80020a14,
  behind its wrecked check. `Effects::car_pose` now skips wrecked cars
  and returns which cars were near a camera.
- cvs +0x18, which the boost flame adds to its tip quads and which was
  taken as 0, is twice the last posed wheel's lift. It is now
  `Effects::lift`, read by the codec and used by `draw_flame`.

Full steering lock turns a wheel about 57 degrees. An HLE frame of the
original at full lock shows its front wheel just as far round.

**Verified.**

- `wheel_poses_match_the_original` (tests/car.rs): about 2600 random
  steering, angle and lift values against 0x80020a14. It checks the
  node's rotation and place, cvs +0x18, and that a wrecked car's node is
  left alone.
- `the_car_pose_pass_matches_the_original` (tests/effects.rs): 600
  rounds of the whole pass against 0x80049ecc, with random spins,
  angles, compressions, skids, steering, sleep, reset marks and wrecks.
  It checks the trails, every wheel's angle, the reset marks and each
  posed wheel's rotation.

**Still unknown:** the rest of the car's draw prep: +0xbc (an engine share), +0xc0 (the nearest surface, likely the shadow), +0xc4 (car flags)
