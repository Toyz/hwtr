---
number: 68
title: loadcheck holds the computer cars and their drivers to the original
date: 2026-10-04
area: test
files: crates/hwtr-hle/src/bin/loadcheck.rs, crates/hwtr-game/src/race.rs, crates/hwtr-game/src/ai.rs, crates/hwtr-game/src/car/load.rs
---

# 68. loadcheck holds the computer cars and their drivers to the original

loadcheck now checks the computer cars as well as the players'. Each computer car is built the port's way and compared with the original when `cars_load` (0x8003beec) returns:

- `Car::load` places it on the grid.
- `race::computer_start` (0x8007c6fc) moves it to its route's start.
- `Ai::add` (0x80079e58) gives it a driver, built from the AI read out of RAM as `cars_load` entered.

loadcheck also compares each computer driver with the original's.

The run found two port gaps and two harness gaps.

Port gaps:

- The computer cars' start placement was already in `Race::new` but had never been checked. It matches. It now lives in `race::computer_start`, and the stale "not yet ported" note on `Car::load` is gone.
- The driver's respawn line is part of the car record (+0x7d0), zeroed at load. Its choice word therefore reads 0, branch 0, not -1, until the first `car_update` keeps a real one. `Ai::add` now starts it that way.

Harness gaps:

- `model_faces` is filled by the app from the model's root faces, so loadcheck now does the same.
- The collision's course has to be set before `add_car`, as `Race::new` does, or a car whose zone is ahead of the line gets lap distance 0 instead of 69400.

Result with the default script (DESERT1, one player and five computer cars): all six cars and all five drivers are the same as the original's.

`LOADCHECK_DUMP=DIR` writes a differing driver's full debug output on both sides, for diffing.

**Still unknown:** Only DESERT1 with the default script is checked
