---
number: 81
title: Which effects a race has: fx_enable from the views and the scale cheats
date: 2026-10-04
area: render
files: crates/hwtr-game/src/effects.rs, crates/hwtr-game/src/race.rs, crates/hwtr-hle/tests/effects.rs, docs/engine/effects.md
---

# 81. Which effects a race has: fx_enable from the views and the scale cheats

**Who sets fx_enable (0x800d0d98), carried in 75.** The race load sets it, at the end of 0x80028b34 (called through 0x80013f50 with the view count, 0x800d0bc5):

- **Its value.** `(views < 2) | 0x1fe`.
- **Cheat options.** If cheat option 2 or 4 is on (0x800d2468, copied from the race setup's +0x1c as the renderer starts), bits 0x02, 0x20, 0x80 and 0x100 are cleared.

**What reads each bit.** I went through every load of the word:

| bit | effect | read by |
| --- | --- | --- |
| 0x01 | puffs | 0x80030fc8, 0x8003119c |
| 0x02 | skid marks | 0x8002b888 |
| 0x04 | sparks | 0x8002c32c |
| 0x10 | lamp glows | 0x80029fb0 |
| 0x20 | headlight beams | 0x8002a81c |
| 0x40 | wreck effects | 0x80029e10 |
| 0x80 | the boost flame | 0x8002b05c (the whole block, its flicker draws included) |
| 0x100 | (nothing reads it) | |

The port had every effect on in every race, as `const ENABLED = 0x1ff`. That was right with one view and no cheats. With two views the original has no puffs, and the scale cheats turn off skid marks, headlight beams and the boost flame.

**Port:**

- `Effects::enabled` is the word.
- `Effects::set_enabled(views, options)` is 0x80028b34's rule, and `Race::new` sets it from its cameras and the setup's options.
- Puffs and skid marks test it.
- The race skips the beams and the flame without their bits, the flame's random draws included.

`the_effects_on_match_the_original` (hle tests/effects.rs) runs 0x80028b34 with views 0 to 3 under ten cheat bytes and compares the word.

**Still unknown:** nothing
