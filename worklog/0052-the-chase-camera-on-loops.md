---
number: 52
title: The chase camera on loops
date: 2026-10-04
area: render
files: crates/hwtr-game/src/collision/camera.rs, crates/hwtr-game/src/camera.rs, crates/hwtr-hle/tests/camera.rs, docs/engine/loop-camera.md
---

# 52. The chase camera on loops

The chase camera now follows the car round loops, as the original does.

**`Collision::loop_axes` (0x8005e984).** In a loop zone (flags 0x9000)
it returns two vectors:

- **Way round:** the two side portals' normals, blended by how far the
  lead point is from each.
- **Up:** the solid planes of this zone and the zone beyond its upright
  portal, each weighted by one less its share of the summed distances.

**Chase (0x800369e4) on a loop.**

- Forward is the car's velocity projected on the way round, or the way
  itself when the projection is short. It also replaces the travel
  vector, so a snapped camera moves along it.
- Up is the loop's when the car is near a surface, else world up.
- Right is forward x up.
- A car flagged on a loop whose lead point is not in a loop zone is
  treated as off the loop.
- The banked and level axes moved into `Camera::chase_axes`.

**Verified.** `the_loop_camera_matches_the_original` in
crates/hwtr-hle/tests/camera.rs. DESERT1, the only track with saved
states, has no loops, so the test marks every DESERT1 zone that has two
side portals and an upright one as a loop in RAM. It then puts player
one's lead point at random spots around those zones, 400 rounds per
state, and compares:

- `loop_axes` against 0x8005e984, both vectors
- the whole chase step (0x800369e4) against the port with the car
  flagged on a loop, a random velocity, near a surface or not, and
  sometimes snapped

All 1200 matched, and `camera_steps_match_the_original` still passes.

**Still unknown:** a loop zone with no upright portal (the original reads address 0)
