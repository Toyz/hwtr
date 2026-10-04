---
number: 47
title: The boost flame and the car's colour pulse
date: 2026-10-04
area: render
files: crates/hwtr-game/src/effects.rs, crates/hwtr-game/src/math.rs, crates/hwtr-game/src/car/update.rs, crates/hwtr-game/src/car/mod.rs, crates/hwtr-game/src/race.rs, crates/hwtr/src/race.rs, crates/hwtr-hle/src/original/effects.rs, crates/hwtr-hle/src/original/car.rs, crates/hwtr-hle/tests/effects.rs, docs/engine/effects.md
---

# 47. The boost flame and the car's colour pulse

R2 now lights the boost flame. 0x8002af60 starts it on a turbo, for a
player's car that is shown. For 97 frames `car_draw` draws it
(0x8002b05c): on each side a body quad and a tip quad behind the car, in
car space. The quads are placed from:

- the handling's width
- the rear wheels' mounts (+0xa0 + 16k), tyre half widths and diameters
- the exhaust table at 0x800be088 (x, y, z by car number)

Each quad's length takes `rand()%1375` every unpaused frame. The flame
fades from 112 by the frames it has shown. While it burns, the car's
root colour pulses grey 128..248 and back, a step every 20 ms of the
system clock (0x8002bc04).

The flame's two template quads (0x8011eb04, 0x8011eb50) are set up by
0x80028b34 from vertex tables at 0x800bdf7c and 0x800bdfbc and sheet
entry 15 (clut 0x7dd8, page 0x0037, additive). Their live contents in a
race state gave the corners and texels the draw rewrites.

The car's colour became a value instead of a "charred" flag: grey, black
after a wreck, or pulsing. Beside it is the effects' own wreck mark (cvs
+0x28), which the flame's stop (0x8002aff4) clears as the original does.
A wreck stops the flame first. The effects test lights player one's
flame through the original's 0x8002af60 and draws it for 100 frames
against 0x8002b05c: count, pulse, colour and seed match, and so does the
stop at 97. The wreck round now goes through the original's whole
0x80029e10.

In the app, the shot after a turbo shows the flames streaming from under
the car's tail, as in the hle shot of the original.

**Still unknown:** cvs +0x18 (added to the flame body's z) taken as 0; the turbo's sound.
