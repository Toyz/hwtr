---
number: 45
title: Dust, skid marks and sparks; semi-transparent drawing
date: 2026-10-04
area: render
files: crates/hwtr-game/src/effects.rs, crates/hwtr-game/src/race.rs, crates/hwtr-game/src/collision/walls.rs, crates/hwtr-game/src/car/handling.rs, crates/hwtr-game/src/car/load.rs, crates/hwtr-render/src/renderer.rs, crates/hwtr/src/race.rs, crates/hwtr-hle/src/original/effects.rs, crates/hwtr-hle/tests/effects.rs, docs/engine/effects.md
---

# 45. Dust, skid marks and sparks; semi-transparent drawing

The race had no dust, no skid marks and no sparks. The effects system sits
at 0x8002b05c..0x80031334: four rings of 56-byte records, which a pass in
the world draw ages, moves, fades and draws once a frame. A subagent wrote
up the whole range as pseudo-C (scratchpad particles.md), and the port was
written from that and checked against the original function by function.

What is ported:

- **The rings** (0x8002c2b8 alloc, 0x8002f354 update). The update
  compacts dead records by copying the previous slot over them, side-buffer
  pointers and all, and marks the previous one dead. Two records can then
  share one velocity buffer. The port models each record's buffer as an
  index that the copy carries along.
- **Dust and grey smoke** (0x80030fc8): two raw `rand()` calls per puff.
- **Skid marks** (0x800303cc) and the **trail points** each wheel pushes
  (0x80029230), which `trails_emit` (0x8002b888) turns into marks and dust.
- **The draw** (0x8002f618): fades the puffs (7 a frame; grey smoke 3
  every third frame) and kills them at nothing, and lays out the quads.
- **Contact sparks** (0x8002e9f8 with 0x8002c32c's jitter), thrown in the
  collision's contact loop after the impulse, on a player's car's first
  contact of the step.

The trail points come from the car pose (0x80049ecc), which `race_frame`
runs after the frame is drawn. For each car near a camera, each skidding
wheel's contact is moved by the tyre's half width across it. That width is
wheel +0x10, which the port had been skipping: it is the six words before
the diameters in handling block A. A reset car leaves no marks for one
frame (+0x928). The same pass showed the car-visibility rule (0x80068540):
a car is hidden from the camera riding in it, and blinks every 100 ms of
its reset grace. The app now draws that.

The test fills the rings with records in any state and calls each original
function: the update, a puff, a run of trail pushes then the emit, a
spark, and the puffs' draw. Rings, trails and seed match in all 250
rounds. Two fixes came out of it: `frame_count` is counted outside the
update (0x80014d64), and a skid-kind record in the wrong ring must not
panic.

The renderer had no semi-transparency. It now has it: bit 31 of a
vertex's mode marks a semi-transparent polygon. Its texels with the top
bit set go to one of four blended passes (half/half, add, subtract,
quarter-add, chosen by the page's mode bits). Its other texels draw as
opaque. The blended passes depth-test without writing, with a small bias
so marks on the road don't fight it. Next to an hle shot of the same
slide, the port shows the same whitish dust haze.

**Still unknown:** The debris chunks, wreck embers, smoke columns, screen flash, boost flame and car_draw's exhaust puffs are still to port; the billboard's camera-space translation units; who sets fx_enable (0x800d0d98).
