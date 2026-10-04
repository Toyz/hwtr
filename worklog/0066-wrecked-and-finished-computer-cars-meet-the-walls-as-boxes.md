---
number: 66
title: Wrecked and finished computer cars meet the walls as boxes (0x800572f0)
date: 2026-10-04
area: physics
files: crates/hwtr-game/src/collision/walls.rs, crates/hwtr-game/src/collision/world.rs, crates/hwtr-hle/tests/collision.rs, docs/engine/track-walls.md
---

# 66. Wrecked and finished computer cars meet the walls as boxes (0x800572f0)

Ported the collision stage that keeps wrecked or finished computer cars inside the track (0x800572f0). Before this, such a car met the walls only through its single centre point, so it could sink halfway into the road or a wall.

How it works:

- Each awake computer car that is wrecked (+0x62c) or has finished (+0x5e3) is treated as a box. The half axes are the body's rotation columns times the object's half width, its half length and the car origin's height (+0x778).
- Only the zone of its point is tried. Plane zones come first, then road zones.
- Each side meets the box corner deepest toward it. Each axis is negated when it points along the normal, and the negation persists into the next side. That matters for ties (an axis square to a side), and the port keeps it.
- Planes: no radius prefilter. Kinds 3 and 4 set 0x2000 and 0x1000 on the car.
- Road sides: every normal is computed. The distance is taken at the centre, less `sum |a_i . n|`.
- Going through sets 0x800 instead of putting the body to sleep.

Port changes:

- `Collision::computer_walls` in collision/walls.rs runs after `walls` in `stages`.
- `Shape { Point, Ball, Box }` replaces the ball option in `road_sides`.
- `press` now takes the flags to mark when a body goes through. The walls stage passes them only for player cars, as before.

Verified with `computer_walls_match_the_original` in hle tests/collision.rs: 80 steps over 4 states, with computer cars wrecked or finished at random and half-turned about each axis. About 1180 boxes were tested, 306 of them in road zones, with 495 contacts and 691 go-throughs. Every contact and car matches.

New doc: docs/engine/track-walls.md (both wall stages).

Also removed a stale "not yet ported" trace for the knocked props' debris, which was already ported.

**Still unknown:** Why the origin's height serves as the box's third half axis
