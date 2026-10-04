---
number: 89
title: The unlock pickups checked, and a page for the power-ups
date: 2026-10-04
area: race
files: crates/hwtr-game/src/powerup.rs, crates/hwtr-hle/tests/powerup.rs, docs/engine/power-ups.md
resolves: 88
---

# 89. The unlock pickups checked, and a page for the power-ups

The power-ups have been ported since the front-end batch (entries 28 to 44), but the docs never had a page for them. They also had no check on the cars the unlock pickups note.

**The unlock record.** UNCAR1 and UNCAR2 carry flags 1 << 24 and 1 << 25. In 0x80065520, a player's car (flag 1) in slot 0 or 1 copies the track's first or second car (0x800d0ea6, 0x800d0ea8) into the record. The record is laid out by car, then by player: 0x800d0eaa and 0x800d0eac hold the first car for players 1 and 2, 0x800d0eae and 0x800d0eb0 the second. The port keeps it by player (`PowerUps::unlocked[player][car]`), and the app's `Race::result` packs it per player.

**Checked:** `the_unlock_pickups_note_cars_as_the_original_does` (hle tests/powerup.rs) takes each pickup with the car in slot 0, 1 and 2. Each car is made a player's under full physics, since only such a car takes a power-up. The test compares the record. My first version left slots 1 and 2 as computer cars, so it noted only slot 0. Both sides agreed there, but player two's slot was never exercised. Forcing full physics fixed that, and the test now requires four notes. To call it, `PowerUps::apply` is now public.

**docs/engine/power-ups.md** is new. It covers the `.PUP` layout (factors, flags, kick), taking a pickup, "Random" (never the unlocks), the frame's expiry, the unlock record and the tests.

**Still unknown:** taking a pickup and the pickups' return have no hle test; the power-up callback flag 2 (0x8012fe00) is unused; what cheats 1 and 64 do (nothing in this build reads them); the car model's scale is drawn by the app, with no hle check; the standings, win and lose screens against an hle shot; whether the original saves the card after a cup; what damp_spin's 8 and 25 stand for; the car preview's NCCT lighting (0x80032ae8), the main menu's stat bars (0x8011dfb4), the menu's engine sound and who sets 0x800d10b0; the TOC entry past the last track; the card screens against hle and the card-seen flag (0x800d0f28); the pickup touch taken after the collision step; no hle check of a knock; the draw-mode byte 0x800d246c; trackside camera flags past bit 1 and the front end's setup against hle; the billboard units; the volume record's +8 position; the dialog bank's tones; cvs +0xc0; a differential check of the load-time marking
