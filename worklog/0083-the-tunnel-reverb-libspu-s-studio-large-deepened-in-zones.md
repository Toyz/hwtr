---
number: 83
title: The tunnel reverb: libspu's studio large, deepened in zones with flag 0x80
date: 2026-10-04
area: audio
files: crates/hwtr-game/src/engines.rs, crates/hwtr-game/src/snd.rs, crates/hwtr/src/spu.rs, crates/hwtr/src/race.rs, crates/hwtr-hle/tests/reverb.rs, docs/engine/sound-levels.md
---

# 83. The tunnel reverb: libspu's studio large, deepened in zones with flag 0x80

0x80015d04, carried since entry 33 as "the master volume for car 0's flag 0x40", turned out to be the race's reverb: a tunnel echo the port did not have.

**What it is:**

- **The call.** 0x80015d04 is `iface_sound+0x34`. The race's sound frame (0x800354ac) calls it for each car, but only in a race with one human driver (0x800d2622, counted as the sound loads).
- **The level.** The fifth output of 0x80045a84 (the car's sound input): `(car.flags << 6) & 0x1000`, so 4096 while the car's flag 0x40 is set. That flag comes from zone flag 0x80.
- **The target.** For car 0, unless the sound is paused (0x800d24d3), `level × 100 / 4096`.
- **The step.** The depth (0x800d0c40) moves toward the target by at most 4 a frame (0x80015cc0).
- **The setter.** 0x800a66d4 is not `SsSetMVol`. It fills a libspu reverb attribute with mask 6, depth left and right, scaled `depth × 0x7fff / 127`: `SsUtSetReverbDepth`.
- **The voices.** 0x800a6b94 feeds the first 20 voices to the reverb.
- **The set-up.** The race's sound load (0x8001924c) calls `SsUtSetReverbType(4)`, then `SsUtReverbOn`. The registers it leaves in the hle's SPU are psx-spx's "studio large" preset, with mBASE at 0xf204.
- **The pause.** 0x80019f7c turns a sounding reverb off: no depth, no voices, `SsUtReverbOff`. Going on turns it back on.

**Port:**

- **Game side.** `engines::Reverb` holds the depth, sounding and paused, with `step` (0x80015d04) and `pause` (0x80019f7c). `Change::ReverbDepth` and `Change::Reverb` carry it to the app. `snd::STUDIO_LARGE`, `STUDIO_LARGE_BASE` and `reverb_depth` hold the preset and the depth scaling.
- **App.** Turns the reverb on as the race's sound loads, steps it each frame with one player, pauses it with the race, and drops it with the race's sound.
- **Sound chip.** It had no reverb at all. It now runs the SPU's reverb as psx-spx describes the hardware: every other sample, from the voices fed to it, through reflections, four combs and two all-pass filters, into a ring the size of the work area, out at the depth.

**Checked (hle tests/reverb.rs):**

- The 32 registers, mBASE and the reverb enable bit match what the original's race leaves.
- 400 steps in and out of a zone match 0x80015d04's depth and the output register it writes.
- The pause both ways matches 0x80019f7c.

A unit test checks that an impulse rings and dies away. In shot runs with sound, the reverb comes in on DESERT1–2, GLACIAL1–3, HAUNTED3 and VOLCANO2 within 40 seconds of driving.

The reverb's arithmetic follows psx-spx. The hle models the SPU's registers, not its sound, so it is not checked against hardware; the hardware's half-band filters on the reverb's input and output are left out.

While reading 0x8001924c I also found cheat bit 16, which loads a bank named "dude"; it is not ported yet.

**Still unknown:** the reverb's sound against a hardware recording, and the hardware's half-band filters around it; cheat bit 16's 'dude' bank (0x8001924c), not ported
