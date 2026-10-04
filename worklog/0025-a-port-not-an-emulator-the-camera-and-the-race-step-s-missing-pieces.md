---
number: 25
title: A port, not an emulator; the camera; the race step's missing pieces
date: 2026-10-04
area: architecture, physics, camera, input, tooling
files: crates/hwtr-game/src/camera.rs, crates/hwtr-game/src/collision/camera.rs, crates/hwtr-game/src/car/righting.rs, crates/hwtr-game/src/car/wreck.rs, crates/hwtr-game/src/car/impact.rs, crates/hwtr-game/src/car/stunt.rs, crates/hwtr-game/src/rand.rs, crates/hwtr-hle/src/original/, crates/hwtr-hle/tests/, crates/hwtr/src/race.rs
---

# 25. A port, not an emulator; the camera; the race step's missing pieces

**No memory model in the port.** hwtr-game used to carry `ram.rs`, a
`layout` codec beside every type, and fields shaped by the PS1 records:
padding words, `unknown_25`, bytes used as flags. That is an emulator's
view, not a port's. Now:
- all of it lives in `hwtr_hle::original`: the `InMemory` trait for the
  game's types, and functions for the collision world, the setup, the
  tuning and the cameras. hwtr-hle runs the original, so it is the one
  place that knows where the original keeps anything;
- the differential tests moved there too;
- the codecs write only fields and bits the port models, leaving the
  rest of each byte and block as it was;
- the types are Rust's own. Flags are `bool`, the armed air-control axes
  are `Armed { along, across }`, the respawn zone is an `Option`, and the
  pads and unused unknowns are gone.

Every rename came from the code:
- handling +0x04 is the steering lock;
- handling +0x78 bit 0, and car +0x865 (set by a power-up), make a car
  all-terrain;
- car +0x5e3 is `finished`: the lap function sets it once the laps reach
  setup +0x19, the lap count;
- car +0x25 and +0x27 are the reset (R1, which puts the car back on the
  track) and the turbo (R2).

**The race step's missing pieces.** racecheck (worklog 24) listed what the
port's step did not yet do. Ported since, each checked against the
original:
- the righting forces (0x80046ac0): couples that turn a car off its side,
  end or roof after the tuning's time. On its roof the car decides at
  random, with PsyQ's `rand` now ported as `Rand`, whether to be turned
  back or wrecked;
- the wreck (0x8004619c), the hard-impact test (0x8007db14), the contact
  timers, and the crashes the walls flag;
- the stunt watch and awards (0x8003cb74, 0x80080148). Air time and
  flips, rolls and spins are named and paid in points and turbos; the
  name, drawn at random, comes out for the HUD.

The tests caught two of my mistakes: a wrong TUNING offset (0x80136a51
is +0x39), and a friction divisor of 400 where the code has 100.

**The pad.** A gamepad now acts as a DualShock in its power-on digital
mode: Cross accelerates, the d-pad steers with the original's ramp.
`--analog` gives the analog layout, which moves the throttle to the
right stick.

**The camera.** Playing showed the placeholder camera ruining the feel:
rigid, 480 in behind. The game's camera (0x800369e4) is quite different.
- It sits 180 in behind and aims 24 in above the car (other views
  220/260 in, plus a bumper view; Circle cycles them).
- It follows the car's travel, or its heading when slow.
- It springs toward a target (0x80039d54): a tenth of the gap a step,
  with speed and acceleration held.
- It rises over the car when the car has turned to face it.
- It is kept 48 in inside the track's floor, walls and roof (0x8005d094,
  now `Collision::keep_inside`).
- Before the race it flies the SCP's table F, now named: keyframes of an
  eye and a target, 200 ms apart.

A differential test runs 1800 randomised steps in three states; every one
matches.

**retro_rt.** The disc, window, loop, pad and GPU plumbing moved onto the
shared runtime, checked pixel for pixel against the previous build. The
rest (rrt-emu for the interpreter, the CLI args, the RNGs) waits until
the game is playable.

**Next.** car_update's turbo (R2) and reset (R1), the respawn point, the
opening sweep, then computer cars, laps and the countdown, the HUD, the
menus and sound.
