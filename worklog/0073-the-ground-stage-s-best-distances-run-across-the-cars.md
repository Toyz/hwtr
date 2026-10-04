---
number: 73
title: The ground stage's best distances run across the cars; racecheck on all 11 tracks
date: 2026-10-04
area: physics
files: crates/hwtr-game/src/collision/ground.rs, crates/hwtr-hle/tests/collision.rs, crates/hwtr-hle/src/bin/racecheck.rs, docs/engine/track-walls.md
---

# 73. The ground stage's best distances run across the cars; racecheck on all 11 tracks

I took racecheck past DESERT1: it now has race states for all 11 tracks. Running it there found one real port bug in the ground stage and three more harness gaps.

**New states** (work/states/<track>-race.bin, each a race just after the start):

- **Unlocked tracks.** From a boot run, the menus are driven to the main menu's TRACK row. Right is pressed once per track (120 frames apart, held 4 frames), then up and X. One up is always lost while a preview loads, so the X opens the garage. A second X returns to the menu on CARS, and up + X starts the race. A fresh profile has six tracks: Desert 1-3 and Glacial 1-3.
- **Locked tracks.** For Volcano 1-3 and Haunted 2-3, the player's profile (pointer at 0x800d2764) gets track bits 0xfff poked at +0x1c before the presses.

**The port bug: ground (0x800536b4).** The original keeps its best nearest and floor distances (s5 and a stack slot) from 0, once, across all the cars, not per car. A car's first candidate takes its place regardless, so this mostly doesn't show. But a player's car whose wheels gave it a nearest surface and no floor (steep wheels, as on Snake River Mine's banks) measures its zones' candidates against the previous car's best. The port reset the best per car. It now carries it across cars. On DESERT2's first script this took 89 differing steps to 0.

`ground_matches_the_original` now covers it:

- nine states instead of four
- two steps in three, the player's wheels set on a steep plane with gravity up
- the car searched before it raised

It fails on the old code and passes on the new.

**Harness gaps in racecheck:**

1. **Grace over.** When a reset's grace runs out, `cars_update` puts the car back in the collision (object flag 1 cleared). The world taken at collision entry (item 3) already has it.
2. **Power-ups.** A power-up taken in the pair search is applied by the race. A step where the original's car changed `power_up` is now counted, not checked, like a reset.
3. **The world at collision entry.** A computer car's update can take its object out of the collision or reset it to another zone before the collision runs. The port's collision step now starts from the original's whole collision world as 0x8004de6c begins, not the world at the start of the step.
4. **Flying wheels.** As in `Collision::update`, racecheck now runs `add_flying` first.
5. **Events.** The port-side `jolted` flag is cleared before diffing, with the other event queues.

A trigger fired inside the original's step can create kind-0 objects. The port's race creates them after the step, so object numbers differ. That only renumbers objects, and racecheck reports the counts when they differ.

**Result.** All 11 tracks × 4 input scripts (steering, reset spam, square, triangle with long turns): 26,908 steps, 26,867 the same, 0 different. 41 steps with a reset or a power-up are not checked.

**Still unknown:** racecheck does not check resets or power-ups taken
