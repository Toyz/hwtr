---
title: The chase camera on loops
status: solid
discs: US
covers: US CCCPSX.EXE:0x8005e984 loop_axes, 0x800369e4 camera_step
worklog: 52
---

# The chase camera on loops

A zone whose flags have both 0x8000 and 0x1000 set (0x9000) is part of a
loop. A car whose lead point is in such a zone gets car flag 16 (see the
zone effects in `collision/create.rs`). With flag 16 set, the chase view
(`camera_step` 0x800369e4, mode 1) first asks `loop_axes` (0x8005e984)
for the loop's way round and up at the car.

## loop_axes (0x8005e984)

It works from the car's collision object (car +0x908): its lead point
(object +0x54, the first world point) and that point's zone (object
+0x58). It answers false unless the zone's flags hold 0x9000. Otherwise:

1. **Portals.** It goes through the zone's portals (plane kind 0).
   - A portal whose normal is less than half up (|nz| < 2048) is a side:
     the first gives normal A and distance a, the second normal -B and
     distance b.
   - A portal more upright names the zone beyond it. The last such
     portal wins.
2. **The way round.** The share `s = b / (a + b)` gives
   `A * s + (-B) * (1 - s)`, normalised. It leans from one side portal to
   the other as the point moves through the zone.
3. **Up.** The zone's solid planes (kind not 0), and those of the zone
   beyond, are summed. Each normal is weighted by
   `1 - distance / total`, where total is the sum of all their distances
   from the point, so the nearest surface counts most. Distances in the
   zone beyond are measured from its own origin. The sum is normalised.

The answers are written to the two vectors the caller passes, and it
answers true. With no upright portal the original reads a zone record
at address 0. The port leaves that zone out of the up sum; no loop is
known to lack one.

## The chase on a loop

If `loop_axes` answers false, the car is taken as off the loop for this
step: the camera function clears flag 16 in its own copy of the car's
flags. If it answers true:

- **Forward.** The car's velocity is projected on the way round. When
  the projection's components add up to less than 1.0 in size, the way
  round itself is used. The vector takes the place of the car's travel,
  so a snapped camera moves with it.
- **Up.** The loop's up, when the car is near a surface (car +0x8f0, the
  nearest surface found); world up otherwise.
- **Right and up.** Right is `forward x up`, normalised. Up is then
  `right x forward`.

From there the chase goes on as off a loop. The camera's own frame is
still built against world up while flag 16 is set.
