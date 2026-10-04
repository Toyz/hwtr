---
title: Track collision (SCP)
status: guess
discs: US
covers: US <TRACK>.SCP in the 11 track archives; US CCCPSX.EXE:0x8004c084 collision_scp_load, 0x8004c334 collision_load, 0x800515e0, 0x8005cd38, 0x8005a548, 0x8005c3a4 zone_effects, 0x80049a8c boost_pad, 0x80048fa4 launcher, 0x8006a200 trigger_fire, 0x8007f670 anim_start, 0x8007f628 anim_set_triggered, 0x8007f17c anim_step
worklog: 12, 15, 53, 57
---

# Track collision (SCP)

`"%s%d.scp"`, loaded during "LOADING COLLIS..". Not yet read by a tool here;
this page records the investigation's reading so it can be checked.

## Layout

All little-endian.

```
+0    u32[6]   counts nA nB nC nD nE nF
+24   6 x { s32 x, y, z, pad }   start positions, 20.12 (x negated when mirrored)
+120  6 x s32[4]                 start orientations, quaternions of length 4096
+216  6 x u32                    zero; pointers at run time
+240  A[nA] 20 bytes, B 24, C 12, D 20, E 12, F 32   ending at the end of the file

A, a sector, 20 bytes
  +0 u16 flags   +2 u16 group   +4 u8 angle (inferred)   +5 s8[3] origin, units of 256
  +8 u16 lap distance, tenths   +10 u8 number of planes   +11 u8 number of fences
  +12 u16 first plane in C   +14 u16 first fence in D
C, a plane, 12 bytes
  s16 nx, ny, nz (4.12)   s16 d   u16   u8   u8 (nonzero: skipped)
```

The start points are in world units once shifted down 12 bits: DESERT1's
first is (78, -18524, -216), on the checkered line. The orientations read as
(x, y, z, w): DESERT1's (-2, -180, 4083, 268) / 4096 turns a car about z by
about 172 degrees, so its model's +y (forward) faces down the track; drawn
that way every car on DESERT1's grid faces the first corner.

A sector is inferred to be a convex volume bounded by its planes. B, E
and F are unexplained here (F is the flyby; see the port's `scp.rs`).

D, a fence, 20 bytes (0x8005a548)
  s16 x, y, z centre   s16 along (unit, 4.12)   s16 normal (unit, 4.12)   s16 half length

A fence is a straight barrier inside its sectors: a sign's post, a rail. A
player's car spanning more than one sector is tested against the fences of
each (a separating-axis test, 0x8005bcec, then the fence cut to the car's
box); overlapping, the car is moved out along its box's axis of least
overlap, and while it moves into the normal the two cut ends become
contacts, surface 1.

## Special sectors (flag 0x4000)

When a driving car (car +0x891 = 2) enters a sector whose flags have
0x4000, `zone_effects` (0x8005c3a4) does one of two things.

- **Boost pad (0x4000 with 0x800), 0x80049a8c.** Nothing happens if the
  car is within 5 mph of 130 mph, or slower than 2. Otherwise the car's
  momentum is scaled by 130 mph over its speed. A car not already
  boosting starts a boost toward 130 mph: the turbo's sound (effect 1)
  for a player, and the boost flame. The Glacial tracks have 60 to 102
  pads and VOLCANO3 has 282.
- **Launcher (0x4000 alone), 0x80048fa4.** The sector's group (+2) is a
  speed in mph (0 means 130). Its byte +4 is a heading: `+4 * 360 / 256`
  degrees about the vertical, which gives the way `(cos, sin, 0)`.
  Nothing happens if the car is slower than 2 mph or moving against that
  way. Otherwise:
  - With every wheel down and the car's forward axis along the way, the
    car is turned level onto it.
  - Its momentum becomes the way times the speed times its mass.
  - A player hears effect 45.
  - The car is boosted at that speed.
  - The desert tracks have 3 to 6 launchers, HAUNTED3 5, VOLCANO1 1 and
    VOLCANO2 2.

## Triggers (table E, flag 2)

Table E holds the triggers, 12 bytes each:

```
+0 u16 flags   +2 u16 animation A   +4 u16 animation B   +6 u16 key
+8 u8 sound A  +9 u8 sound B        +10 u8 A's second byte   +11 u8 B's
```

**At load.** For each sector with flag 2, collision_scp_load
(0x8004c084) looks up the trigger at the sector's parameter. It marks
animation A as triggered when the trigger has flag 2 (0x8007f628), then
animation B too when it also has flag 4.

**Running.** A triggered animation stands still until started. Each
frame, the animation step (0x8007f17c) advances it only by the time it
has left to run (+0x18), and the pose that follows keeps every moving
animation's time within its round. When a triggered animation comes to
rest, its looped sound is stopped (0x8006a424). While it runs, the sound
follows it (0x8006a4cc).

**Firing.** A car entering the sector fires the trigger (0x8006a200).
Nothing happens when the trigger has flag 1, or lacks flag 2.

1. Animation A is started toward the key (0x8007f670).
   - An animation still running is left alone.
   - Otherwise it is given `min(key, keys - 1) * period / (keys - 1)`
     milliseconds to run, and the trigger's number is noted on it.
2. If animation A started:
   - with flags 0x28, its world sound (0x80036270) plays at the
     animation's place: sound A and A's second byte
   - with flag 0x10, the same sound plays looped
   - otherwise a player's car hears effect 27
3. With flag 4, animation B is started the same way. If it starts, its
   world sound plays under flags 0x28 or 0x10, but there is no effect 27.

DESERT3 has 2 trigger sectors, GLACIAL2 6 and GLACIAL3 1. The port plays
the looped sounds once.

## Unknown

Most of it: B, D, E, F, several A fields, and the code that walks them.
