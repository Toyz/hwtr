---
number: 55
title: Car shadows
date: 2026-10-04
area: render
files: crates/hwtr-game/src/car/draw.rs, crates/hwtr-game/src/math.rs, crates/hwtr-game/src/race.rs, crates/hwtr-render/src/mesh.rs, crates/hwtr-render/src/scene.rs, crates/hwtr-hle/tests/car.rs, docs/engine/car-wheels.md
---

# 55. Car shadows

Cars now cast shadows on the ground under them, as the original draws
them.

**Geometry (0x80029478), `car::draw::shadow_quads`.**

- Shadows are cast only on ground the car is near (its nearest surface)
  that faces more than 60 degrees up.
- The box comes from the rear right wheel's mount and tyre, half the
  car's length and height, the handling's origin, and a per-car reach
  table (0x800be00c, 42 cars).
- It is turned and placed with the car, each corner is dropped straight
  down onto the plane, and the box is split at its midpoints into four
  quads.

**Drawing (0x80028b34, 0x80021b88).**

- Each quad takes a quarter of the slot's `<car>.shd` 64x64 4-bit
  texture, colour 96, drawn subtracted.
- The texture goes at the original's place: (384 + 16 * slot, 256), with
  its CLUT at (384, 482 + slot). `Tables` now has the reach table, the
  places and each slot's first column.
- The race emits the shadows as effect quads for shown cars. The scene
  loads each car's SHD.

**Checked against the original.** `shadows_match_the_original`
(tests/car.rs) runs 1800 rounds against 0x80029478. Each round sets the
car's model root (position and a turn about any axis) and its ground
plane, then compares every corner of the four prims the original writes.
More than 1000 cast a shadow, and more than 100 were refused on steep
ground, where the original writes nothing.

**On screen.** A DESERT1 frame with and without shadows differs only in
the band under the car's rear, darkened.

**Still unknown:** the model scale cheats (options 2 and 4); cvs +0x130 (an engine share) and the light colours (+0x1e9, +0x1ea, +0x1f2) from 0x8002bad0 and 0x8002bb0c
