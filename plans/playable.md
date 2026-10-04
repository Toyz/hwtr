# To playable

Written 2026-10-04. The goal: `hwtr` plays the game, front end to finish
line, on ported code only. Each milestone ends with something new running in
`hwtr` and a worklog entry.

Gameplay code is ported exactly and checked against the original (unit
tests from save states, the shadow checks under `lockstep`). Presentation
(renderer, HUD drawing, menu drawing, audio output) is rewritten natively for
the PC from the disc's assets, following the original's behaviour.

## M1. A car built from the disc

`cars_load` (0x8003beec) for each car: the CWH handling block into the car
(`car_read_cwh` 0x80021208, 0x8004528c), the rigid body from mass and size
(0x8006bdd8), the grid pose (0x8006abe4 quaternion to matrix, the SCP start
points), the wheels from the model. Native: `Car::load`.

## M2. Collision with the track

The SCP as a native type (zones, cross-sections, planes, the rest). The
collision objects: points from the body (0x8004e47c), zone tracking
(0x800515e0), wheels on the ground (0x80051bc0), the other stages (0x800536b4,
0x80054964, 0x800572f0, 0x8005a4cc), contacts and impulses
(`collision_update` 0x8004de6c, 0x8006dc08).

## M3. Drivable

The pad to the car's controls (0x80034940 and the controls tables), the race
step at 40 Hz (`race_frame` 0x80033ed8: cars, collision, clock), the car drawn
from its body, the chase camera. One car on the track in `hwtr`.

## M4. A race

Opponents (the AI and best line, 0x80078ed8 / 0x8007265c, state-1 cars
0x80040494), the countdown, checkpoints and laps, positions, the HUD from the
overlay sheets, the finish and results.

## M5. The game

The front end from `SCREENS.SCR`: title, menus, car and track selection,
options. Sound: the CD-DA music from the disc, the VAB sound effects mixed on
the PC. Saves to a file.
