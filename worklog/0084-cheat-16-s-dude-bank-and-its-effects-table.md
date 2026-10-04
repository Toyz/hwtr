---
number: 84
title: Cheat 16's DUDE bank and its effects table
date: 2026-10-04
area: audio
files: crates/hwtr-game/src/snd.rs, crates/hwtr/src/spu.rs, crates/hwtr/src/race.rs, crates/hwtr/src/front.rs, crates/hwtr-hle/tests/effects_table.rs, docs/engine/race-sound.md
resolves: 83
---

# 84. Cheat 16's DUDE bank and its effects table

Cheat option 16 (0x80013ff8(16) in the race's sound load, 0x8001924c) changes two things:

1. **The bank.** `DUDE` is opened as VAB 0 instead of `MAINSFX2`. Every track's archive has it: one program of eight tones.
2. **The table.** It calls 0x8001a73c with 1 instead of 0. That function fills the effects table (0x8011aec0, 16 bytes per id) from the executable's records (0x800bd5f8: tone, program, note, id). With 1, every effect gets program 0, its tone if under 8 (else 3), and a note of that tone plus 60. The note is taken from the tone already stored, so a substituted 3 gives 63.

**In the port:**

- `hwtr_game::snd::effects_table(byte, dude)` builds the table both ways. It moved out of the app's `Effects::new`.
- The race loads `DUDEVH`/`DUDEVB` when `setup.options & 16`. The options come from the profile's cheats through the front end, so the cheat entered at the password screen reaches the race.

**Checked:** `effects_table_matches_the_original` (hle tests/effects_table.rs) calls 0x8001a73c(0) and (1) on a race state and compares all 61 entries, ids included. A guard makes sure the cheat changes more than 40 of them, so the test can't pass with two equal tables.

Entry 83's dude question is answered. Its reverb question is carried here.

**Still unknown:** the reverb's sound against a hardware recording, and the hardware's half-band filters around it
