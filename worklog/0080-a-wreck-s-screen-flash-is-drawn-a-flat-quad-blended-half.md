---
number: 80
title: A wreck's screen flash is drawn: a flat quad blended half and half
date: 2026-10-04
area: render
files: crates/hwtr-render/src/renderer.rs, crates/hwtr-render/src/mesh.rs, crates/hwtr-game/src/effects.rs, crates/hwtr/src/race.rs, docs/engine/effects.md
---

# 80. A wreck's screen flash is drawn: a flat quad blended half and half

A wrecked player's screen flash is now drawn. Before this, the port kept its value but never drew it, because the blend mode was unknown (carried in 75).

**What 0x8002e128 draws.** It runs first in the ember update, each frame. For each flash slot that is set:

- **Primitive.** A 24-byte POLY_F4 (code 0x2a: flat, semi-transparent).
- **Size.** It covers the whole screen, 384 × 240. With two players, each view draws only its own player's flash, 240 / 2 − 1 high.
- **Colour.** A grey of the flash's value before this frame's dimming.
- **Order.** It goes in ordering-table slot 4, under the HUD's 1 and 2.
- **Fade.** The value then drops by 3; under 130 it is cleared and not drawn. A wreck sets it to 160, so the flash lasts about ten frames.

**The blend mode.** A flat semi-transparent polygon blends by whatever page the frame left set. I logged the HLE GPU's page each time a flat semi-transparent quad from the screen's corner drew, while replaying VOLCANO2's third racecheck script, which wrecks the player. Every frame of the flash, greys 160 down to 133, came out mode 0: half old, half new. The log was a one-off and is removed.

**Port:**

- **Renderer.** Untextured polygons (mode bit 30, `Vtx::FLAT`, an unused page bit), drawn in their colour as it is and blended whole when semi-transparent. A semi-transparent overlay layer, drawn after the world and before the HUD.
- **Effects.** `flash_drawn` holds this frame's grey. It is wrapped in `Drawn`, frame output that compares equal, so the effects tests that compare whole states still match.
- **App.** Draws a flash quad per shown flash.

**Checked:** a temporary forced wreck in a DESERT1 shot (not committed) shows the screen washed grey under the HUD at the wreck, and clear ten frames on. The effects tests pass.

**Still unknown:** nothing
