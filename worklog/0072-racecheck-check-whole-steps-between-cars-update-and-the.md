---
number: 72
title: racecheck: check whole steps between cars_update and the collision's return
date: 2026-10-04
area: test
files: crates/hwtr-hle/src/bin/racecheck.rs
---

# 72. racecheck: check whole steps between cars_update and the collision's return

racecheck reported differences with steering and turbo scripts: 5, 12 and 32 steps on the drive, speed and air states. That held at the earlier commit 2b4ca54 too, so it was not a regression. All of them turned out to be the harness, not the port.

1. **Steps cut by the vertical blank.** `Hle::frame` stops at the vertical blank wherever the main loop is. A race step can therefore straddle two frames: at steps 85, 86 and 100 the step counter moved but the collision stages ran in the next frame. racecheck had compared a half-done original step with the port's whole one, and the original's wheel contacts and ground looked one step behind. racecheck now only checks a frame in which the cars' update (0x8004064c) began and the collision update (0x8004de6c) returned.
2. **The step's tail.** The previous step's places, wrong-way watch, camera and clock tick could run at the start of the next frame. The start state is now memory as `cars_update` begins, and the end state is memory as the collision update returns, both captured by hooks, instead of the frame's edges. This fixed `contact_time` off by one clock tick.
3. **The wrong-way watch.** 0x8005c9b4 runs inside `cars_update`, after the places. racecheck now runs the port's `laps::watch_way` after the car update.
4. **Port events in the diff.** The `Pending` event queues the port keeps for the race (sounds, lines and others) printed in the debug diff and misaligned it. They are cleared before diffing.
5. **Resets.** A reset step is counted, not compared: racecheck does not model the reset path.

On a difference, racecheck now also reports:

- the car as the collision began against the port's car after its update
- the whole world before the stages
- each stage (zones, wheels, ground) run from the original's world against the original's result

One gotcha found on the way: `Machine::check` keeps one check per address, so a second hook on the same function replaces the first.

Result, with every whole step checked:

- Script `0:x:600,100:right:40,250:left:60`: 412 + 606 + 612 + 612 steps, all the same.
- Script `0:x:900,50:left:120,300:square:60,400:right:200`: 411 + 608 + 612 + 612, all the same.
- Script `0:x:900,0:r1:900,...`: everything the same except 17 resets, which are not checked.

The port's race step matches the original on every step checked, including today's computer-car walls stage.

**Still unknown:** The reset path is not checked by racecheck
