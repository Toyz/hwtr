---
number: 70
title: The stunt results stop the race's clock (0x800d0e54)
date: 2026-10-04
area: ui
files: crates/hwtr-game/src/hud.rs, crates/hwtr-game/src/race.rs, crates/hwtr-hle/tests/laps.rs, docs/engine/results.md
---

# 70. The stunt results stop the race's clock (0x800d0e54)

Ported the last piece of the stunt points' results table (0x80064294): stopping the clock.

0x800d0e54 is the time the HUD's "time left" counts down from. At the race start (0x80061ed8) it is set from the setup's limit (+0x20), only under flag 4, the race against the clock. Each time the stunt points' table is drawn, it takes player one's time so far: the race clock less car 0's lap start (+0x5a8). If that is less than 0x800d0e54 (unsigned), it becomes the new value. The time left therefore reads zero from the moment the results appear.

In the port this is `Hud::stop_clock`, called from the stunt branch of the results drawing on `Hud::limit`, which is the port's 0x800d0e54. Without a time limit, `limit` is None and nothing changes; in the original it would lower a value nothing shows.

Verified with `the_stunt_results_stop_the_clock_as_in_the_original` (hle tests/laps.rs). It calls 0x80064294 150 times with random clock, lap start and limit, and compares 0x800d0e54 with `Hud::limit`. The limit was lowered in 34 of the cases, and every case matches.

Docs: results.md, under the standings.

**Still unknown:** None for this change
