---
number: 24
title: The track holds the car: collision, the pad, and racecheck
date: 2026-10-03
area: physics, input, test, tooling
files: crates/hwtr-game/src/collision/ground.rs, crates/hwtr-game/src/collision/walls.rs, crates/hwtr-game/src/collision/world.rs, crates/hwtr-game/src/body.rs, crates/hwtr-game/src/pad.rs, crates/hwtr-hle/src/bin/racecheck.rs, crates/hwtr-hle/src/port.rs, crates/hwtr/src/race.rs, crates/hwtr-render/src/mesh.rs, docs/engine/controls.md
---

# 24. The track holds the car: collision, the pad, and racecheck

Second milestone of [the plan to playable](../plans/playable.md), most of the
way: the port's car now stays on the track, its controls come through the
original's own controller code, and a new tool holds a whole race step of
the port to the original's.

**The collision stages.** `collision_update` (0x8004de6c) counts the step,
moves every object's points, finds contacts between cars (0x8004e938, not
yet ported), runs the stages of 0x8005148c, then gives each contact its
impulse. The stages, in order:

| stage | what | ported |
| --- | --- | --- |
| 0x800515e0 | points follow their zones through portals | yes |
| 0x80051bc0 | players' wheels on the ground | yes |
| 0x800536b4 | each car's nearest surface and floor | yes |
| 0x80054964 | points pushed back out of walls; contacts logged | yes |
| 0x800572f0 | the same for computer cars | no |
| 0x8005a4cc | players' cars across two zones (0x8005a548) | no |
| 0x8005c3a4 | effects of the zone a car's lead point entered | partly |
| (inline) | cars the walls flagged crash (0x8004619c) | no |

*Ground* gives each car two planes, `n · p + d`. A player's car takes them
from its last grounded wheel whose ground faces up. Other cars, and players
with none, take them from the zones their points are in: a road zone's
surface under the car's centre, between its two cross-sections, or a plane
zone's planes. A road zone with flag 0x2000, or a car whose zone holds
gravity (car flag 0x400), turns gravity toward the surface: the loops.

*Walls* treats a road zone as a tube of four sides between the edges of its
cross-sections: the floor through the right edge, the left wall, the roof,
the right wall. A plane zone uses its planes, but not its portals. A point
behind a side is pushed back along the side's normal, the body moving with
it. If the point is moving inward, the push is also logged as a contact (at
most 16 a step, 40 bytes each at 0x8012c6ec). A point more than 0x32000
deep flags the car instead (0x800), and touching plane kinds 3 and 4 flags
it too (0x2000, 0x1000). Those flags are the crashes in the last row.

*The impulse* (0x8006dc08) takes away half the point's inward speed,
through the body's mass and inertia:
`j = -0.5 vn / (1/m + n·((I⁻¹(r×n))×r))`. Friction opposes the sliding with
`mu j`, where `mu` is the surface table's percentage over 100 (0x800beac0),
four times that for a computer car.

**The pad.** "The input is inverted" was the first thing playing showed.
The port had guessed that action 0 meant steering right. Holding Left in
the original gives a negative steer, so action 0 is left and a positive
steer turns right. The rest of the mapping was guessed too, so the
controller read is now ported instead (`hwtr_game::pad`, documented in
[controls](../docs/engine/controls.md)):

- A digital pad's pedals are on or off. Its steering ramps while held, +40
  a frame to 75 and then 1.8 a millisecond.
- A DualShock in analog mode steers with the left stick and drives the
  pedals with the right. Each stick axis is split into halves and read
  through one of two 7-point curves; there, the d-pad, Cross and Square do
  nothing.

A differential test runs the original's 0x8001bec0 against the port over
3000 random frames. hwtr presents the gamepad as a DualShock, and its PS
button toggles analog mode, as the ANALOG button did. hwtr also now steps
as `race_frame` does: the pad is read inside each 25 ms step, steps run
until the race clock passes the real one, and a frame counts for at most
50 ms.

**racecheck.** Every ported function matched the original on its own, in
unit tests and in shadow checks during a race. Yet a native race from the
grid ended 5000 inches from where the original's did. The new tool
`racecheck STATE FRAMES` runs the original. Before each frame it takes a
native race from the original's RAM; after the frame's step, it runs the
port's step and compares the player's car. Since every step starts again
from the original, each difference is one step's worth of what the port
does not do.

Its first finding was a real bug. The friction divisor 0x64000 is
`100 << 12`, and the port had divided by 400. The impulse test fed the
port's friction value to both sides, so it could not see the error;
capturing the original's argument at 0x8006dc08 showed it. With that fixed,
323 of 408 steps on Desert 1 (accelerator held) match exactly. The other 85
differ only in fields of stages not yet ported: car +0x618, the respawn
zone (+0x7cc), a contact timer (+0x628), and the turbo count (+0x874; the
original gave a turbo for an air).

**Lessons kept.**
- No guesses: a name, sign or mapping is checked against the original (a
  test, `hwtr-hle --press ... --peek`, or the code) before it lands.
- A differential test must take every input from the original's path, not
  from the port's.
- The workspace forbids `unsafe`. The one use, viewing vertices as bytes,
  is now a packing into a reused staging buffer.

**Next.** The remaining stages of the race step:
- respawn (R1) and turbo (R2) in `car_update`;
- the landing and righting forces and the crash (0x80046ac0, 0x8004619c);
- the contact timers;
- computer cars' walls;
- car-to-car contacts.

Then computer cars driving, laps, and the HUD.

**Still unknown:** nothing
