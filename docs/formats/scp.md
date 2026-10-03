---
title: Track collision (SCP)
status: guess
discs: US
covers: US <TRACK>.SCP in the 11 track archives; US CCCPSX.EXE:0x8004c084 collision_scp_load, 0x8004c334 collision_load, 0x800515e0, 0x8005cd38
worklog: 12
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
  +8 u16   +10 u8 number of planes   +11 u8   +12 u16 first plane in C
C, a plane, 12 bytes
  s16 nx, ny, nz (4.12)   s16 d   u16   u8   u8 (nonzero: skipped)
```

A sector is inferred to be a convex volume bounded by its planes. B, D, E
and F are unexplained.

## Unknown

Most of it: B, D, E, F, several A fields, and the code that walks them.
