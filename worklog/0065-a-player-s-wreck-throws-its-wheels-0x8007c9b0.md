---
number: 65
title: A player's wreck throws its wheels (0x8007c9b0)
date: 2026-10-04
area: physics
files: crates/hwtr-game/src/flying.rs, crates/hwtr-game/src/collision/walls.rs, crates/hwtr-game/src/collision/create.rs, docs/engine/flying-wheels.md
---

# 65. A player's wreck throws its wheels (0x8007c9b0)

A player's wreck now throws the car's wheels, as 0x8007c9b0 does. Each wheel becomes a body of its own (mass 2122) and a collision object of kind 6. It bounces off the walls and the other objects until the car is put back on the road.

What was ported:

- `hwtr_game::flying`: the 8-record table (0x801323f4, 680 bytes each), `throw`/`throw_all` (0x8007c9b0) and `step` (0x8007c894: integrate 25 ms, then orthonormalize).
- `Car::wreck_throwing`: throws for player cars (flags & 1) after the speed is halved. The AI's wreck path still calls `wreck()`, which throws nothing.
- Collision:
  - `add_flying` (0x8004d798 for each new wheel) and `remove_flying` (0x8007da78 / 0x8004da1c).
  - `body_of`/`body_of_mut`, so pairs and walls treat a wheel's body like a car's.
  - The ball sides: each side meets the centre less the normal times the reach.
  - Bounce 1.3 (0xd000/0xa000) in 0x8006dc08 for kind 6.
- Fix: the object's +0x7c holds the contact's normal, not its point. It is renamed `contact_normal` everywhere, including the hle codec.
- App: a flying wheel is drawn as the car's own wheel node, posed from the body taken into the car's frame.

Verified:

- `wheels_fly_off_as_in_the_original`: every thrown record matches.
- `flying_wheels_step_as_in_the_original`: the bodies match after the step.
- `flying_wheels_collide_as_in_the_original`: 1440 collision steps with wheels in contact, all equal.
- The whole workspace is green.
- A shot run with a forced player wreck (a local hack, not committed) shows the car on its side without wheels and a wheel flung up the canyon wall.

The doc is docs/engine/flying-wheels.md.

**Still unknown:** The flying wheel snapshots (0x8007e6e0, 0x8007ec08) are not ported; the rest of the 680-byte record
