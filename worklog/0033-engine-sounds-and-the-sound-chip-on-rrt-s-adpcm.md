---
number: 33
title: Engine sounds, and the sound chip on rrt's ADPCM
date: 2026-10-03
area: audio
files: crates/hwtr-game/src/engines.rs, crates/hwtr-game/src/snd.rs, crates/hwtr/src/race.rs, crates/hwtr/src/spu.rs, crates/hwtr-hle/tests/engines.rs
---

# 33. Engine sounds, and the sound chip on rrt's ADPCM

Each car's engine now plays. This covers the whole engine path of the race's sound interface, `iface_sound`: 44 slots filled at 0x80019870. Sound isn't a kit: no state machine drives it. The race calls into it as a service, so the port is a plain module, `hwtr_game::engines`, that returns `Change`s (key on, key off, bend, volume) for the app to carry out on the voices.

**The banks.** 0x8001924c loads one bank per car:
- The car's name is looked up in the 42-entry table at 0x800bd160 (28 bytes each: name, engine kind at +0x10, redline at +0x14, idle at +0x18).
- That gives one of the 22 kinds at 0x800bd9c8 (22 bytes each: bank name, key note +0x10, bend at idle +0x12, bend at redline +0x14).
- A car whose slot is greater than the player count gets kind + 11, the smaller "O" bank. The test is `players < slot`, so with one player, slot 1 keeps its full bank (an off-by-one in the original, kept).
- The banks come from the track's BIG (`ELECTRCVH`, `GENER8OVB` and so on). Each keeps its own buffer, as the menu banks do.

**Keying.**
- 0x80015ebc keys the engine on voice `slot`: program and tone of effect 0, the kind's key note (64), fine 64, silent.
- A player's car also keys its overrun on voice `cars + slot`: program 1, note 57.
- It stores the voice it keyed last at +0x4a, and the mixer checks that voice's status.
- It runs at race_audio_start (0x800350d4), at every car reset (0x80041384, which also plays effect 13 for a player), and on leaving the pause (0x80036758).
- 0x80016004 lets the voices go on a wreck, for any car (0x8004619c). So does entering the pause (0x80036634).

**Each frame.** 0x800354ac runs once per display frame, after the steps:
- The revs (car +0x550, or 0 when wrecked) are clamped to 1000..30000 and slewed by at most `fx(0x2710000, 409)` = 4090000 a frame, about 1000 rpm.
- The throttle is the pad's eased pedal level (0x8011b2b8 +0x16) × 4096 / 255.

**The one-player mixer** (0x80017928):
- The volume comes from 0x80018280. The distance (0x80018e04) is measured in twelfths and runs out at `fx(0xbb8000, 1/12)`. The side (0x80018590) is the offset dotted with forward × up of camera 0's matrix (columns 1 and 2), over 3000, clamped to ±4096.
- That packs the left volume into the high byte and the right into the low byte, each ×2/5.
- Then 0x8001a36c bends the voice by the revs between idle and redline, from the kind's idle bend to its redline bend. For a player's car it splits each side by the throttle: the engine voice gets the on-throttle share and the overrun the rest, each slewed 0x14000 a frame (0x8001a318). Computer cars get half of each side on the engine voice.
- Last, the mixer bends the +0x4a voice to the key note plus a Doppler factor (0x80018760) applied to the previous frame's bend. For a player that voice is the overrun on program 1, so libsnd's owner check (program 0) refuses the bend. For a computer car it is the engine, so with one player the computer cars' engines sit at 64 + bend × Doppler, not on their revs. That reads like a bug in the original, but the whole-mixer test confirms the original does it, so the port keeps it.

**The two-player mixer** (0x80017268) gives every car a third of the effects volume and does no Doppler.

**libsnd's pitch bend** (0x800a63d8 → 0x800ac6a8 → 0x800ae64c) is `snd::bend_pitch`:
- step = bend − 64.
- Above 64: x = step × the tone's bend_up (byte 13), note + x/63, fine (x % 63) × 2.
- Below 64: x = step × bend_down (byte 12), q = floor-ish(x/64), note + q − 1, fine (x − 64q) × 2 + 127.
- Then the same pitch table as the key-on.

The bend only takes effect if the voice's owner record (0x801433c0 + 56v) has the same program, so the app keeps an owner for each voice. `SsUtSetVVol` (0x800a6c64) writes `l × 129`, `r × 129` straight to the voice's volume registers.

**Verified** against the original in `hwtr-hle/tests/engines.rs`, on the desert1-race state:
- distance, side and volume, 3000 random rounds;
- Doppler and slew, 3000 rounds;
- 0x8001a36c's levels, bend, voice volumes and pitches, 2000 rounds, against the engine banks;
- the whole one-player mixer, 300 rounds with every car and the camera moved, comparing every engine voice's shadow volume and pitch and each car's bend. HLE doesn't play the SPU, so every voice reads as stopped and the original would skip them all; the test hooks 0x80019c18 to report the engine voices as playing.

The app test `the_engines_sound` runs desert1 headless for 40 s: sound from the grid, and the player's pitch climbing from 594 to about 2000 once driven, with dips at the gear changes. The sound is written to `target/test-tmp/engines.wav`.

**The sound chip on rrt.** The ADPCM decode and the interpolation now come from `rrt::emu::spu` (`decode_frame`, `interpolate`). The interpolation is the chip's 4-point Gaussian, replacing my linear one, so each voice keeps the previous block's last three samples. The voices, ADSR and mixer stay ours, as rrt intends. `rrt-emu` has no dependencies, so taking it at play time costs nothing.

**A bug fixed** (the user hit it in a debug build: "attempt to shift left with overflow"). The decay and release shifts were taken as the field × 4. That is the "rate" encoding (shift << 2 | step) mixed up with the shift. Decay is bits 4-7 of ADSR1 and release bits 0-4 of ADSR2, used as is (psx-spx). In release builds the shift had wrapped silently, so decays and releases ran far too slow.

**Still unknown:** The other per-car voices (0x8011a840 +0x48, +0x4c to +0x52: skids, scrapes, the boost), the twelve placed effects at 0x8011aab0, the track's ambient bank, the announcer's DIALOG banks and the crash banks are not yet ported. 0x80015d04 (slot 0x34, run for car 0 when bit 0x40 of its flags is set) changes the master volume and isn't ported.
