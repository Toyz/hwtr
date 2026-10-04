---
number: 51
title: The commentator: wreck, stunt and ten-turbo lines, with their random draws
date: 2026-10-04
area: audio
files: crates/hwtr-game/src/race.rs, crates/hwtr-game/src/car/mod.rs, crates/hwtr-game/src/car/update.rs, crates/hwtr-game/src/car/stunt.rs, crates/hwtr-game/src/engines.rs, crates/hwtr/src/race.rs, crates/hwtr-hle/tests/hits.rs, docs/engine/race-sound.md
---

# 51. The commentator: wreck, stunt and ten-turbo lines, with their random draws

The commentator speaks: after a wreck, after scored stunts, and at the
tenth turbo.

**Requests (0x80036484).** Cars ask for lines through `Car::say`, and
the race passes them to `Commentary::ask` in call order after each
step:

- line 0: the wreck line, half a second into a player's wreck
- lines 1 and 2: after a scored stunt
- line 3: ten turbos

Line 3 always replaces what is waiting. The others are dropped while one
waits or within 1000 ms of the last.

**Speaking (0x800354ac).** Once a frame after the steps, unless the
race's sound is shut:

- A line that has waited 301 ms is spoken.
- Line 3 is effect 57.
- The others go through `dialog_play` (0x80019754), which draws
  `rand()%2` (and `rand()%6` past line 2) from the game's generator.

The port missed these draws before, so its random sequence went apart
from the original's whenever the commentator spoke. They are now drawn
in the new `Race::sound_frame`, called between the steps and
`Race::frame`.

**Playback.** The app loads the dialog bank the race drew (DIALOGn, VAB
3). `Engines::dialog` keys it on an importance-0 effect voice at three
eighths of the effects volume, program 0, the tone, note tone+60.

**Verified.** `the_commentator_asks_and_speaks_as_in_the_original` in
crates/hwtr-hle/tests/hits.rs runs 3000 rounds against the original. It
compares:

- the queue state after 0x80036484, from random states and lines
- the seed after 0x80019754 for lines 0 to 5
- its key-on and key-off arguments against `Engines::dialog`

The per-frame wrapper, the 301 ms wait, follows 0x800354ac directly and
has no differential test.

**Still unknown:** what the dialog bank's tones say; VAB 2
