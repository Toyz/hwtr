---
number: 4
title: The track tables, and the worlds and track the disc does not ship
date: 2026-10-03
area: decomp, content, race
files: docs/cut/physics-test-world.md, docs/cut/fcity-world.md, docs/cut/haunted-highway-1.md, symbols/cccpsx.txt
---

# 4. The track tables, and the worlds and track the disc does not ship

`ENGLISH.HWT` names Physics Test 1-3 and Haunted Highway 1, which have no
archives ([[2]]). The executable explains the second and adds a third
missing world.

## Two parallel 4 x 3 tables

At 0x800c5bf4 is an array of twelve `char*`, the archive name stems, by world
then track; at 0x800c5c34 a parallel array of display names. Between them,
0x800c5c24 holds the four world names. In both twelve-slot tables the Haunted
track 1 slot (0x800c5c0c and 0x800c5c4c) is zero:

```
desert1   Dawn Encounter      glacial1  Cold Fusion
desert2   Snake River Mine    glacial2  Command Center
desert3   Road to Rustwell    glacial3  Helicrash
NULL      NULL                volcano1  Exhaust Pipes
haunted2  R.M. Sludgeworks    volcano2  Serpent Sprint
haunted3  R.M. Test Track     volcano3  Volcano Blowout!
```

So Haunted Highway 1 was removed from the code's tables as well as the disc,
and only the text file still has its name. The tables have no row at all for
the Physics Test world.

## A world colour picker that knows more worlds

0x80011310 compares its argument with a string compare at 0x800a2d98 against
`Action`, `PTest`, `FCity`, `Desert`, `Glacial`, `Haunted` (0x800d0b6c-0x800d0b94)
and calls 0x80010e34 with three small values: `56, 88, 144` for Action and
PTest, `120, 59, 106` for Desert, `240, 246, 255` for Glacial, and `0, 0, 0`
for FCity, Haunted (whose compare result is not tested) and anything else.
Glacial's near-white suggests a colour; it is inferred, not traced.

`FCity` appears nowhere else on the disc. Each cut item now has its own page
under `docs/cut/`, a section added for exactly this.

**Still unknown:** Whether any menu can reach the Haunted 1 slot or a fifth world; what 'Action' and 'FCity' are; what TCUP.BLD, SCCUP1.BLD and SCCUP2.BLD hold.
