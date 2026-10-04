---
number: 86
title: The scale cheats in the car's draws
date: 2026-10-04
area: render
files: crates/hwtr-game/src/car/draw.rs, crates/hwtr-game/src/lights.rs, crates/hwtr-game/src/race.rs, crates/hwtr-render/src/mesh.rs, crates/hwtr-render/src/scene.rs, crates/hwtr/src/race.rs, crates/hwtr/src/main.rs, crates/hwtr-hle/tests/car.rs, crates/hwtr-hle/tests/lights.rs, docs/engine/cheats.md, docs/engine/car-lights.md, docs/engine/car-wheels.md
resolves: 59, 82, 85
---

# 86. The scale cheats in the car's draws

Cheats 2, 4 and 32 change how the car is drawn, not only its body. car_bind_state (0x80021888) sets the model's scale (cvs +0x14): 8192 under 2 or 32, 1365 (a third) under 4, else 4096. These draws use it:

| draw | cheat | change |
| --- | --- | --- |
| car_draw (0x80022064) | 2 or 4 | the body node's rotation (model +0x4), each column times the scale; wheels on the car go through it |
| car_draw | 32 | each wheel node's own rotation (model +0x8 nodes, 72 bytes each); a wheel off the car (cvs +0x1e8) skips the body |
| shadow (0x80029728) | 2 or 4 | the car's matrix, so the box shrinks or grows |
| fxp_parse (0x80022cd0) | 2 or 4 | each glow's place, in place at load |
| glows (0x80029fb0) | always | size times the scale |
| beams (0x8002a81c) | always | push times the scale; off under 2 and 4, so under 32 they reach twice as far |

**In the port:**

- `car::draw::{model_scale, body_scaled, wheels_scaled, scale_columns}` hold this logic.
- The shadow takes the scaled matrix.
- `glows` and `beams` take the scale. Their old fixed 4096 was this value.
- `Lamps::scale` scales the glow places, from `Race::set_lamps`.
- The app's `car_triangles` takes a body scale. Each wheel is flagged as on the car or flying.
- `hwtr --track NAME --cheats BITS` starts a race under cheats directly. Shots at DESERT1 show third-size cars under 4 and doubled wheels under 32.

**Checked against the original:**

- `shadows_match_the_original` under cheats 0, 2, 4, 32 and 6.
- `lamps_draw_as_the_original` with the model's scale at one, two or a third. This is where the beams' scale turned up: the first run failed on them.
- `fxp_lamps_scale_as_the_original` runs fxp_parse under five cheat bytes.

The car model itself is drawn by the app's renderer, so its scale has no hle check.

This closes entry 59: no code sets option 0x80, because its slot's code 77777777 is taken by slot 0 first. It also closes the scale-cheat items of 82 and 85. Entry 82's reverb item was done in 83. Their other questions are carried here.

**Still unknown:** what cheats 1 and 64 do (nothing in this build reads them); cheat 8's flat textures, not ported; the car model's scale is drawn by the app, with no hle check; the analog stick against the digital pad in a race; what damp_spin's 8 and 25 stand for; the car preview's NCCT lighting (0x80032ae8), the main menu's stat bars (0x8011dfb4), the menu's engine sound and who sets 0x800d10b0; the TOC entry past the last track; the card screens against hle and the card-seen flag (0x800d0f28); the pickup touch taken after the collision step; no hle check of a knock; the draw-mode byte 0x800d246c; trackside camera flags past bit 1 and the front end's setup against hle; the billboard units; the volume record's +8 position; the dialog bank's tones; cvs +0xc0; a differential check of the load-time marking
