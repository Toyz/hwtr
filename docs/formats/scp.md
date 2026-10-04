---
title: Track collision (SCP)
status: guess
discs: US
covers: US <TRACK>.SCP in the 11 track archives; US CCCPSX.EXE:0x8004c084 collision_scp_load, 0x8004c334 collision_load, 0x800515e0, 0x8005cd38, 0x8005a548
worklog: 12, 15
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

## Unknown

Most of it: B, D, E, F, several A fields, and the code that walks them.
