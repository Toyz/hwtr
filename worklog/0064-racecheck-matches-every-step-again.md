---
number: 64
title: racecheck matches every step again
date: 2026-10-04
area: tooling
files: crates/hwtr-hle/src/bin/racecheck.rs, crates/hwtr-game/src/camera.rs
---

# 64. racecheck matches every step again

`racecheck` reported 590 of 616 steps different on DESERT1. None of it was
the port; the tool had fallen behind.

**What was wrong.**

1. **The wheels' roll (+0x80).** It now turns at the frame's end, in the
   pose pass (0x80049ecc, ported in 0054). The step-only check left it
   unturned. The check now takes the original's.
2. **Pairs were skipped.** The check never ran the pair search or the pair
   impulses, so every step with another car touching differed. It now
   runs the whole collision step, as `Collision::update` does.
3. **The other cars were stale.** They were taken from the frame's start,
   but the original's computer cars have moved by the time its collision
   step runs. A check hook on collision_update (0x8004de6c) now snapshots
   them there.

**Result.** With `--press` Cross and stick sweeps, every step matches:

- `desert1-race`: 616 of 616 steps
- `desert1-drive`: 1020 of 1020
- `desert1-speed`: 1020 of 1020
- `desert1-air`: 1020 of 1020

That covers the player's car update and the whole collision step against
the original, including contacts with the computer cars.

Also: the camera's `ViewMode::Other` comment called modes 2 to 4 unported.
They are the trackside cameras (ported in the attract work), and the
comment now says so.

**Still unknown:** A wreck in racecheck's runs (none of these crashed).
