---
number: 58
title: Car lights: exhaust glows, headlights, tail lights, tunnel darkening
date: 2026-10-04
area: render
files: crates/hwtr-game/src/lights.rs, crates/hwtr-game/src/race.rs, crates/hwtr-game/src/effects.rs, crates/hwtr-data/src/car.rs, crates/hwtr-render/src/renderer.rs, crates/hwtr/src/race.rs, crates/hwtr-hle/src/original/effects.rs, crates/hwtr-hle/tests/lights.rs, docs/engine/car-lights.md
---

# 58. Car lights: exhaust glows, headlights, tail lights, tunnel darkening

The cars' lights are in, as the original runs them:

- glows behind the exhausts at high revs
- headlight beams, which come up in dark and lit zones
- the body darkening in tunnels
- tail lights that brighten under braking

Docs: docs/engine/car-lights.md.

**A wrong note, corrected.** The effects doc said the lamp glows
(0x80029fb0) never run, because cvs +0x28 was "2 to 10" on every car. That
reading was cvs +0x2c, the glow count. +0x28 is 0 on every live car, so
the glows do run, and they draw a random number per glow per draw once a
car is above half revs. The port's random sequence would have drifted from
the original's at speed. It no longer does.

**The frame's end (0x80049ecc).**

- **Glow strength (+0xbc).** Every car gets one from its revs:
  `2 * (rpm - idle) / (redline - idle) - 1`, at least 0, and none for a
  wreck.
- **Light targets (+0xc4, 0x8002bb0c).** A player's car gets them from its
  zone bits and brake:
  - a dark zone sends the body to 48 and the headlights to 112
  - zone flag 0x8 brings the headlights up and shows them
  - none of these sends the body back to 128 and the headlights out
  - bit 4 is the brake lights
- **`hwtr_game::lights`.** `glow_level` and `Lights::aim` port these two,
  and the race runs them after the car pose pass.

**The car draw (0x80022064).**

- **Glows (0x80029fb0).** A triangular prism per FXP glow, flickering by a
  random draw, its back stretched along the FXP vector by the strength.
  Car 36 has an eight-times hexagon. They are drawn additive on sheet 4.
- **Beams (0x8002a81c).** Three quads per headlight, their far corners
  pushed 170 units along the lamp's direction, drawn additive on sheet 17
  at the headlights' level.
- **Fades (0x8002ad48).** The body's colour and the headlights' level, by
  5 and 4 a draw.
  - **Which colour.** The colour that fades is the drawn model's. Each
    detail level has its own root colour, which charring and the boost
    pulse (full model only) never showed. `Effects::root_colour` is now per
    level, the race keeps the level each car was drawn at, and the app
    uses that level's colour.
- **Tail lights (0x80021f60).** The palette's first seven colours go to
  (384, 464 + slot): as the skin has them while braking, else halved. The
  renderer gained `load_vram` for a one-row upload, and the app loads the
  palette when it changes.
- **FXP.** `hwtr_data::car::fxp` reads the glow (40-byte) and headlight
  (48-byte) records.

**Checked against the original** (crates/hwtr-hle/tests/lights.rs):

- The FXP records parse to what fxp_parse pointed the view state at.
- The off palettes equal 0x8011d470.
- 900 rounds of 0x80049ecc with any revs, zone bits, brake, wrecks and
  light bytes match every car's glow strength and lights.
- 4500 rounds of the car draw match:
  - each glow's and beam's matrix place and corners, hooked at the matrix
    set (0x80015128) and the quad-list draw (0x80010678)
  - the lights after
  - the drawn model's colour
  - the palette uploaded
  - the seed

  The rounds cover any lights, strength, wreck mark, pause, frozen mode
  and model.
- More than 1000 rounds drew glows, 300 drew beams, and 500 faded.

**On screen.** A DESERT1 shot at full throttle shows the player's car's
glows as bright additive flames at its pipes, and its tail lights dim
while not braking.

**Still unknown:** What the FXP's C records (cvs +0x168) light; why only car 36 has a hexagonal glow.
