---
number: 15
title: Cars on the start grid: half scale, quaternions x y z w
date: 2026-10-03
area: render, format, race
files: crates/hwtr-viewer/src/scene.rs, crates/hwtr-viewer/src/main.rs, docs/formats/car.md, docs/formats/scp.md
---

# 15. Cars on the start grid: half scale, quaternions x y z w

`hwtr-viewer` now puts six cars on the track's start grid (`--cars` picks
them; Deora, Twin Mill, Rocket, Bisector, Snake and HW500 by default), each
skin uploaded where a race puts car slot n (the table at 0x800bdd30, palettes
at (384, 464 + n)) and drawn with the 8-bit page that gives.

Two facts came out of placing them:

- **Cars are half scale in the world.** The SCP's six start points are 210
  units apart across and 284 along the track; the Deora model is 146 x 358.
  At full scale the grid would overlap; at half scale every car sits in its
  slot, which also matches the half-scale wheel records and FXP points of
  [[13]]. Where the game applies the half (matrix scale or a shift) is not
  traced.
- **The start orientations are quaternions (x, y, z, w), 4096 = 1.** Read
  with w last, DESERT1's (-2, -180, 4083, 268) is a turn of about 172 degrees
  about z; with w first it would stand the car on its end. Turned that way the
  cars' noses (model +y) point down the track toward the first arch.

The render from behind the grid reads "CHECK POINT" correctly on the
gantry, which also confirms the world is not mirrored ([[12]], [[14]]).

**Still unknown:** Where the half scale is applied in the car's world matrix; the lit colours (the viewer draws every car at base colour 0x80).
