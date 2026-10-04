---
number: 49
title: Unsticking: a stuck car turns where it stands
date: 2026-10-04
area: physics
files: crates/hwtr-game/src/car/update.rs, crates/hwtr-hle/tests/car.rs, docs/engine/car-object.md
---

# 49. Unsticking: a stuck car turns where it stands

A car stuck against something now turns itself free, as the original
does.

`car_update` already counted the stuck timer: accelerator and steering
past half, all wheels down, under 5 mph, a contact in the last 100 ms.
Past 500 ms it now calls the port of 0x8004b478.

0x8004b478 adds a pure couple to the step's sums:

- **Side force:** the car's x axis times
  `steer * (width² + length²)`.
- **Where it acts:** at the centre plus the y axis times the body's mass,
  and negated at the centre minus that.
- **Effect:** the torque goes through the same 64-bit products as
  `Body::apply_force`, so the port uses that directly. The force sums to
  nothing and the car yaws the way it is steered.

**Verified:** `unsticking_matches_the_original` in
crates/hwtr-hle/tests/car.rs. It runs 512 rounds on each of three saved
races, randomising steering, size, mass, force sum and place, and
compares the whole car's memory against the original after the call.

Also corrected a stale comment: contacts between cars (0x8004e938) are
ported.

**Still unknown:** nothing
