---
number: 19
title: The car's physics step, ported: tyres, suspension, engine, aerodynamics
date: 2026-10-03
area: physics, decomp, test
files: crates/hwtr-game/src/car.rs, crates/hwtr-game/tests/car.rs, crates/hwtr-hle/src/port.rs, crates/hwtr-psx/src/decomp.rs, docs/engine/car-object.md
---

# 19. The car's physics step, ported: tyres, suspension, engine, aerodynamics

`car_physics` (0x800427a8, 2567 instructions) and the three functions it
calls are ported and agree with the original. That covers the unit tests,
and 2349 calls in 900 frames of scripted driving under `lockstep`
(accelerating, braking, steering, handbrake). The details are on the
reference page ([car object](../docs/engine/car-object.md)). This entry
covers how they were worked out and the traps on the way.

## Reading the units

The constants made sense only once read as imperial: `0x949000 / 1000` and a
second `/1000` is 0.002376, sea-level air in slug/ft³. Dividing by 12
(`div_fx(4096, 0xc000)` = 341) turns inches into feet, `0xb0000 / 0xa000` =
17.6 is inches per second per mph, and `0x182000` = 386 is g in in/s². So the
world is in inches. That also explains the 1024-unit world cells of [[13]]:
about 85 feet.

Which wheels are which came from the saved race, not the code. Flag bit 0 is
set on the two wheels at y = -1.3 in and clear on the two at y = +102 in, so
bit 0 is the rear axle, and `aero`'s two results (`local_50`, `local_54`)
are front and rear downforce. With that, the rest named itself: brake bias
is the front share, `+0x688` the rear grip, `+0x24` a handbrake that frees
only rear wheels sideways.

## What the tests caught, and what they did not

The first version of each test passed, and mutation testing showed why that
was not enough. Mutating the port and checking that the test fails found
blind spots where random inputs never reach a boundary:

- `drivetrain`'s torque-curve index 16 and the `redline < rpm` test only
  matter at `rpm == redline` exactly. The game reaches that whenever no
  driven wheel grips at full throttle (`rpm = max(rpm, redline × 1.0)`), so
  every fourth round now does that.
- `car_physics`'s all-wheels-airborne path ((1/4)^4 per random round) and its
  slip test at `limit == len`. The latter needs a car at rest, spring-less
  and frictionless. Zeroing the contact velocities did nothing at first,
  because `update_wheels` recomputes them from the car's velocity and spin.
  Zeroing those worked.
- One mutant is equivalent (`speed < 0` vs `<= 0` for damping: zero either
  way) and is left.

The inputs also have to stay within what the game produces. GCC's inline
divides trap on zero (`break 0x1c00`), and the original faulted in the test
on a zero gear ratio (cars at state 1 have no engine set up) and on a zero
mean ground normal. The tests now keep to cars at state 2 and to upward
normals.

## Two decompiler bugs, found by porting

- A store of a register that had been written out by name was taken for a
  prologue spill and hidden (`sw s4, 88(sp)` in `aero`). Spills are now
  recognised only in the entry block, and only while the register is
  unchanged.
- Writing out a register to protect a pending value could leave a value
  already built from it (the next assignment, a store's operands, call
  arguments) reading the new value. `drivetrain`'s 64-bit multiply printed
  one partial product twice. Copies of a value being written are now marked
  and come to read the register. Values in flight are tracked through the
  write-outs, and one still reading the old value gets a temporary.
  `hwtr-re pseudo FILE --all` decompiles all 1838 functions as a check.

In both cases the disassembly settled it. The pseudo-code is a reading aid
and the tests are the authority.

## The checks themselves

A callee may save its register arguments into the 16 bytes above its entry
`sp` (the caller's frame), and `aero` does. The comparisons now skip only
the stack below the entry `sp` plus those 16 bytes, so a result a function
leaves in its caller's frame (`aero`'s two downforces) is compared.

**Still unknown:** what car +0xec, +0x770, +0x6b4, +0x865, +0x86a/+0x86b and surface 6 mean; where the tuning bytes at 0x80136a18 come from; the rest of car_update (integration, collisions)
