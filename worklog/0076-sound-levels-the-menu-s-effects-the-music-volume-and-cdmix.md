---
number: 76
title: Sound levels: the menu's effects, the music volume and CdMix
date: 2026-10-04
area: audio
files: crates/hwtr-game/src/cd.rs, crates/hwtr-game/src/front/boot.rs, crates/hwtr/src/cd.rs, crates/hwtr-hle/tests/sound.rs, docs/engine/sound-levels.md
resolves: 32
---

# 76. Sound levels: the menu's effects, the music volume and CdMix

I went after three open questions about sound levels. Two turned out to be how the original works; the third was a real gap in the port.

**Menu sounds at full volume (from 37, carried in 75).** The menu's sound player is 0x80015908, behind `iface_game+0x1c`. It scales only by the level it is given (4096ths, made 127ths) before `SsUtKeyOn`. I checked all 44 call sites: 43 pass a fixed 4096. The remaining one is the Sound FX slider's release (0x80092f50), which plays at the new level. Moving the slider (0x80092fd0, 0x8009311c) sets nothing global for effects. So the original's menu sounds ignore the slider too, and the port already matched it.

**0x800a5c58 is CdMix (32).** Entry 32 took it for libsnd's master volume. It calls 0x800abc08, which writes the CD drive's attenuation registers (index 2, then 3, then apply 0x20). That is libcd's `CdMix`: the music level for the left into the left and the right into the right, nothing across.

**When the music volume applies (from 36, carried in 75).** 0x80014b6c has six callers:

- the defaults (0x8008a184)
- a successful card read (0x80088828; a failed read sets nothing)
- the options screen's two sliders
- the pause menu's music line

The port did only the options screen and the pause menu, and set the CD level when a race started. So after loading a save with quieter music, the menus played at the default level. Both boot paths now ask for it.

**The arithmetic.** The port computed `m × 127 / 255`. The original rounds down twice, `((m << 12) / 255 × 127) >> 12`, which comes out one lower for settings 249, 251 and 253. `hwtr_game::cd::music_level` now does it the original's way. `the_music_level_matches_the_original` (hle tests/sound.rs) checks all 256 settings against the level 0x80014b6c keeps at 0x800d2488.

New reference page: docs/engine/sound-levels.md.

**Still unknown:** the Gaussian interpolation table (from 32); the pause menu's path to the music level against hle
