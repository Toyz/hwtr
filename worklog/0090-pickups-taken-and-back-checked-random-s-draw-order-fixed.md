---
number: 90
title: Pickups taken and back, checked; Random's draw order fixed
date: 2026-10-05
area: race
files: crates/hwtr-game/src/powerup.rs, crates/hwtr/src/race.rs, crates/hwtr-hle/src/original/powerup.rs, crates/hwtr-hle/src/original/mod.rs, crates/hwtr-hle/tests/powerup.rs, docs/engine/power-ups.md
resolves: 89
---

# 90. Pickups taken and back, checked; Random's draw order fixed

Taking a pickup and the pickups coming back had no check against the original. racecheck skips any step where a power-up is taken. Building the check turned up one real bug.

**The bug: "Random" drew from the wrong order.** 0x800671a8 loads each power-up the track's pickups name into a list at 0x800d2698. The list's add (0x80061c00) links each new one at the head, so the original holds them newest first. A "Random" pickup draws the k-th of that list (0x80067ac4). The app pushed them oldest first, so with the same seed a Random pickup gave a different power-up from the original's on any track with two or more power-ups. The load is now `hwtr_game::powerup::load_defs`, newest first, and the app uses it.

**The codec.** hwtr-hle's `original::powerup` reads the original's state into a `PowerUps`:

- the power-up list
- the pickups (24 bytes each: name, number, power-up number, when taken, out, collision object)
- each car's held list from 0x8012ff04 (nodes of item, previous, next and count; entries of 12 bytes)
- the clock 0x800d0e34
- the unlock record

**Tests (hle tests/powerup.rs):**

- `power_ups_load_in_the_originals_order` runs on all 11 tracks. It fails on the old order.
- `pickups_are_taken_and_come_back_as_in_the_original` runs 150 rounds per track. In each, a random car takes a random pickup, put out or not, through 0x80067f98. Then the clock moves up to 6 s on and 0x80068130 steps. It compares every car, the pickups, the held lists, the unlock record and the seed. Over the run that is 1443 taken, 897 run out, 965 back and 140 Random draws. The port's own `turbo_given` event has nothing in memory, so it is cleared before comparing, as the apply test already did.

The take's other helpers, 0x80067c3c and 0x80067c98, never matter after the drop-all (0x80067d50) has emptied the list. The test confirms it.

power-ups.md now covers the load order, the records and the tests.

**Still unknown:** racecheck still skips steps with a power-up taken (the race applies the pickup after the collision step); the power-up callback flag 2 (0x8012fe00) is unused; what cheats 1 and 64 do (nothing in this build reads them); the car model's scale is drawn by the app, with no hle check; the standings, win and lose screens against an hle shot; whether the original saves the card after a cup; what damp_spin's 8 and 25 stand for; the car preview's NCCT lighting (0x80032ae8), the main menu's stat bars (0x8011dfb4), the menu's engine sound and who sets 0x800d10b0; the TOC entry past the last track; the card screens against hle and the card-seen flag (0x800d0f28); no hle check of a knock; the draw-mode byte 0x800d246c; trackside camera flags past bit 1 and the front end's setup against hle; the billboard units; the volume record's +8 position; the dialog bank's tones; cvs +0xc0; a differential check of the load-time marking
