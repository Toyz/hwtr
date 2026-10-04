---
number: 87
title: Cheat 8 paints each car one colour
date: 2026-10-04
area: render
files: crates/hwtr-game/src/car/draw.rs, crates/hwtr/src/race.rs, crates/hwtr-hle/tests/tim_upload.rs, docs/engine/cheats.md
resolves: 86
---

# 87. Cheat 8 paints each car one colour

Cheat 8 (code 34563456) makes the cars flat colours, not the track. I had it down in cheats.md as "flat textures" after one glance at the check in `tim_upload` (0x80012a10). Following it through corrected that.

**When the fill happens.** `tim_upload` fills the image with its first byte before LoadImage when all three hold:

- its fill is on (0x800d244c)
- cheat 8 is set
- the caller's last argument (keep) is 0

**Which uploads it hits.**

- **The fill flag.** 0x800127b4 turns it on at boot. It drops only around SFX.GLM (0x80012864) and one sprite upload (0x8001dbd8).
- **The keep argument.** Of the six callers, tim_load_upload, 0x80014420 and screens_load always pass 1.
- **The two that fill.** car_load_race's direct upload (0x80021c74, CLUT at (384, 464 + slot)) and 0x80023aa8 (CLUT at (640, 448 + slot)) pass 0. Both are a car's skin.
- **The track.** Its textures go up through glm_load's own LoadImage and never meet the check.

**The fill's sizes.** It counts bytes from the image's halfwords. 4-bit fills the whole image with the first texel's nibble twice, and 8-bit fills it all with the first byte. 16-bit gets only its first half, but no skin is 16-bit.

**In the port:** `car::draw::flat_skin` is the fill. Under cheat 8 the app re-places each car's flattened skin after the scene loads. `hwtr --track desert1 --cheats 8` shows each car in one solid colour.

**Checked:** `the_flat_cars_fill_as_the_original` (hle tests/tim_upload.rs) runs `tim_upload` on the Deora's skin through the whole hle machine (the upload polls the GPU). It covers depths 0, 1 and 2, with the cheat, the fill flag and keep each switched off once, and compares the image bytes left behind. The plain CPU test machine stalls in two waits: the draw-table count at 0x800117e0, whose clock doesn't run inside a call, and libgpu's status poll.

This closes entry 86's flat-texture item. The rest is carried here.

**Still unknown:** what cheats 1 and 64 do (nothing in this build reads them); the car model's scale is drawn by the app, with no hle check; the analog stick against the digital pad in a race; what damp_spin's 8 and 25 stand for; the car preview's NCCT lighting (0x80032ae8), the main menu's stat bars (0x8011dfb4), the menu's engine sound and who sets 0x800d10b0; the TOC entry past the last track; the card screens against hle and the card-seen flag (0x800d0f28); the pickup touch taken after the collision step; no hle check of a knock; the draw-mode byte 0x800d246c; trackside camera flags past bit 1 and the front end's setup against hle; the billboard units; the volume record's +8 position; the dialog bank's tones; cvs +0xc0; a differential check of the load-time marking
