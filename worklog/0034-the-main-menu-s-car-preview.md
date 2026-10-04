---
number: 34
title: The main menu's car preview
date: 2026-10-03
area: ui
files: crates/hwtr-game/src/front/mod.rs, crates/hwtr-game/src/front/main_menu.rs, crates/hwtr/src/pieces.rs, crates/hwtr/src/front.rs
---

# 34. The main menu's car preview

The main menu now draws each player's car in their box, spinning. The user noticed it missing. In the original, too, the box is empty until the CARS line is chosen: 0x8008bb90 asks for the model after 1.5 s on that line, and an `hwtr-hle` run from the menu-main state confirms it.

**Motion** (0x80084bcc, its state half; `CarPreview` in the front end):
- Each player has a size and its goal (0x800d1090, 0x800d1098), an angle and its goal (0x800d10a0, 0x800d10a8), a turning flag (0x800d10b0), and a place and its goal (0x80136c10, 0x80136c30).
- Each frame they ease toward their goals by 0x80083370's factor: a hundredth per millisecond since the last frame, a tenth after a gap over 100 ms, 0.15 the first time. The size snaps to 0 below a third when shrinking away.
- Unless turning, the angle spins a turn every five seconds, by the same per-player clock as the help line's (0x800834f4), and the wheels roll at four times the angle (0x80020fac: each wheel node RotX(−4 × angle)).

**Setters**, all ported:
- 0x80084adc / 0x80084b54 put the preview at, or aim it at, front-end coordinates, through world_x/y/z.
- The sizes: 0x80084960 (1.5), 0x8008499c (1.0, rest angle as the goal), 0x80084a14 (1.75), 0x80084aa0 (2.0), 0x80084928 (0).
- 0x80084828 sets the rest angle `fx(71, 0x82000)`, 2.25 rad.

**The main menu's use of them:**
- Entering (0x8008bf18) puts both previews at (500, 500) and aims them at (435, 450) and (775, 450).
- Moving onto CARS (0x8008d35c / 0x8008d470) grows them to 1.5 and aims them at (445, 480) / (685, 480). Moving off sets 1.0 and aims them back.
- When a model finishes loading (0x8008cccc), the preview is aimed by the current line.
- The enter and update actions 0x8008c1f0, 0x8008c238, 0x8008c3f4, 0x8008c42c and 0x8008c4dc were stubs and now do what the original does.

**Drawing.** The original's chain (`pieces::car_triangles`), all in the GTE's fixed point:
1. The camera, 866 units back.
2. The size, as a diagonal.
3. The place, sized too.
4. The tilt: RotMatrixX of `fx(-5 << 16, 71)` ≈ −1.39 rad.
5. The spin: RotMatrixZ (Z is the car's up).
6. The shift: ApplyMatrixLV of (−a1, −a2, 0), from the car's handling block A at +0x80 and +0x84 in CWHS.BMF (0x8008c544 copies them to 0x80138eb4 through 0x80024068). In the disasm the vector is at sp+0xec: x = −a1, y = −a2, z = 0; the decompiler had shown it the other way round.
7. set_object_matrix applies the front end's view.
8. car_draw (0x800286d4) doubles the translation (the half-scale cars) and projects with H 554.

The faces are culled by NCLIP, sorted by average depth plus their bias, and drawn over the pieces at a flat 0x80. The models and skins come from SCREENS.BIG (`<CAR>.BMF` part 0, `<CAR>.TIM`), named by the table at 0x800c5ce8. The skins go to VRAM x < 640, the PlayStation's frame buffers, which the port's renderer doesn't use.

**Verified** against the original by measuring the teal car's extent in the player box over a full turn: 80 `hwtr-hle` frames from menu-main with Down, and 41 port shots. Widest 0.291 of the screen in both; narrowest 0.116 against 0.128; centre x 0.395..0.446 against 0.401..0.427; height 0.100 against 0.112 (the port's flat lighting colours a few more pixels teal).

**Still unknown:** The preview's lighting: car_draw lights the car (NCCT, via 0x80032ae8) where the port uses the race's flat 0x80. The stat bars under the car (0x8011dfb4 with 191, 61, from CWHS block A +0x130), the car's engine sound the menu plays (SCREENS.BIG <CAR>VH), and the previews on the car select and two-player screens (0x8008e310, 0x80091cf0, 0x8009a89c) are not ported. 0x800d10b0 (turn to an angle rather than spin) is never set by the main menu; its setter isn't found yet.

**Later the same day: no loading wait.** The user found the preview far too slow to appear: over five seconds, between the 1.5 s ask timer, only asking on the CARS line, and the ease in. The original's delays exist to spare CD reads, so they're gone (see the PC-native resources rule):
- All 41 models and skins are parsed when the front end loads.
- A swap copies the skin into the player's VRAM place.
- The main menu asks for the previews on entry.
- 0x8008bb90 asks at once instead of after 1.5 s.

The motion and the drawing are as described above. `hwtr-hle` from menu-main also shows the original with a static car on the RACE! line, which this entry's first account doesn't explain. The menu-main state was probably saved after the CARS line had loaded a model, and what draws it there isn't traced yet.
