---
number: 27
title: Computer cars drive their route; cars meet
date: 2026-10-03
area: race, physics, decomp, test
files: crates/hwtr-game/src/ai.rs, crates/hwtr-game/src/collision/pairs.rs, crates/hwtr-game/src/collision/create.rs, crates/hwtr-game/src/body.rs, crates/hwtr-hle/src/original/ai.rs, crates/hwtr-hle/tests/ai.rs
---

# 27. Computer cars drive their route; cars meet

The other five cars race now, and cars collide with each other.

## The computer cars' driver (fakeai, 0x8007265c)

A computer car does not run the car physics. Its driver (a 592-byte
record at 0x801315f4) moves it kinematically and writes the result into the
body.

**The route.** The BLD's first stream, never explained until now, is the
route: a program of keypoints (absolute, or relative steps), jumps,
weighted branches between alternative ways, and jump times (0x8007b388). The
header's six words after the stream lengths are each grid place's start in
it, and computer cars start there, not on the SCP grid (0x8007c6fc). At a
branch the car picks by weights: its handling's flags word (+0x78) says
which routes suit it, and bits 29 and 30 favour the side it has swerved to.
The branch counters carry on from car to car within a step, as the original
keeps them in registers.

**The step**, about twenty passes over the six drivers:
- speed: pace, rubber band, knocks, launch;
- the route: advance, choose at branches, base raised by the ride height;
- the swerve toward a random target, held inside the route's widths;
- drift from knocks;
- the orientation: rebuilt from the averaged ground normal and travel,
  turned by spin, re-orthonormalised, leant into the route's turn;
- stunts over jumps, chosen by the car's skill and air control (a botched
  one wrecks it on landing);
- the steering and wheels, for drawing;
- the rubber band toward the player every few tenths of a second.

Knocks come back from the collision code as momentum on the body, which
0x8007937c turns into a shove and a change of pace and swerve (0x80078ef8).

Along the way:
- The ride height is the larger of the two axles' (I first read the branch
  as the smaller).
- The let-go of a finished car writes three 32-bit words over the body's
  64-bit angular momentum: an original bug, kept.
- The AI has its own random generator, an LCG seeded from PsyQ's `rand`.

A differential test runs 9000 steps over three saved races with random
knocks. Drivers, cars and both random seeds are exact. 2100 stunt frames,
7887 airborne frames and 1542 knocks are covered, and mutations of the
branch, stunt and swerve code are caught.

## Cars meet (0x8004e938, 0x8005001c, 0x8006f20c, 0x8007e000)

- Cars are paired by a broad check (Chebyshev distance under 1.5 times the
  summed radii), then a 15-axis box test (0x8004ed4c). The axis a pair was
  last apart along is kept and becomes the contact's normal when they touch.
- The contact is the other box's corner nearest the face, or the midpoint
  for an edge pair. Both are pushed out by the overlap.
- After the track contacts, each pair is checked for a crash wreck (relative
  speed past 120 mph, adjusted by skill), then given an impulse between the
  two bodies, the bounce by kind from TUNING +42 to +45.
- The fences' box test writes the same overlap scratch.
- Taking an impulse off the second body must subtract r×P: adding r×(−P)
  rounds differently by one unit.

Computer cars also get the right collision object (0x8004ccc4): one point
at the centre, the box their own size.

The pair test drives cars into one another: 16398 pairs and 58 crash
wrecks, exact. The exceptions: a wreck's crumple and flying wheels, whose
random draws and debris objects the port doesn't make yet.

The original has 58 collision objects on Desert 1: the six cars, the
world's props (ids 32 to 48) and the power-ups (49 to 57). The props and
power-ups come next.

**Still unknown:** the computer cars' walls (0x800572f0); the world's props and power-ups as collision objects (0x8004cedc, 0x80067f98) and a knocked prop (0x8006b958); a wreck's crumple (0x8002e574) and flying wheels (0x8007c9b0), whose random draws the port lacks; the unfinished computer cars' estimated times in the standings (0x80061824); the pair's sounds and rumble (0x80035c7c, 0x8005fed0)
