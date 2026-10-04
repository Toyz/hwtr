---
number: 79
title: Triangle turns the HUD off and on; the turbo has no sound of its own
date: 2026-10-04
area: input
files: crates/hwtr-game/src/hud.rs, crates/hwtr-game/src/car/controls.rs, crates/hwtr-game/src/pad.rs, crates/hwtr-game/src/race.rs, crates/hwtr-hle/tests/hud.rs, docs/engine/controls.md
---

# 79. Triangle turns the HUD off and on; the turbo has no sound of its own

Two questions carried in 75: what action 12 is, and what the turbo's own sound is.

**Action 12 is the HUD on/off button**, Triangle by default. I scanned all 17 calls to the level getter 0x8001b170. Only 0x80034940 asks for action 12, at the end of each player's race pad read, and passes its level to 0x8006545c:

- **No change.** The level is kept per player at 0x800d0e60; if it hasn't changed, nothing happens.
- **A press.** The player's show mask (0x800d0e58) goes to 0 if it shows everything the HUD shows when on (0x800d0e5c, set with it at the race's setup). Otherwise it goes back to that full mask.
- **Any change, press or release.** Every line of the player's stunt announcement is marked done, both halves (0x800beae4 + 32 a line, +0xd), cutting short one under way.

The original's controls screen labels the button "HUD On/Off". The port had no such button, so Triangle did nothing in a race.

The port now has it:

- `Controls::hud` is action 12's level.
- `Hud::hud_button` does 0x8006545c.
- `Race::step` calls it for each player after applying the controls, as 0x80034940 does.

`the_hud_button_matches_the_original` (hle tests/hud.rs) runs random presses and releases on both players through 0x8006545c and the port. It compares the masks and every half's done flag after each one: 20 rounds of 60, 487 toggles. In a DESERT1 shot the HUD is gone after one Triangle press.

**The turbo has no sound of its own.** A turbo (0x80049440) makes one indirect call, 0x8012fe04, which is 0x8002af60: the boost flame, and it calls nothing. car_update's turbo branch sounds only when there is no turbo left: effect 44 through `iface_sound`, which the port already plays. Entry 33 had counted a "boost" among the car's voices. The voices at 0x8011a840 +0x4c to +0x52 are the tyres, the scrape, the crash and the impact.

**Still unknown:** nothing
