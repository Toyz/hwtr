---
title: Haunted Highway 1
status: partial
discs: US
covers: US SCREENS.BIG::ENGLISHHWT line 15; US CCCPSX.EXE:0x800c5c0c, 0x800c5c4c
worklog: 4
---

# Haunted Highway 1

The first Haunted track is named in the text but has an empty slot in both
of the executable's track tables, and no archive.

## What exists

- `SCREENS.BIG::ENGLISHHWT` line 15: `Haunted Highway 1`, followed by
  `Haunted Highway 2` and `3`.
- `CCCPSX.EXE` has two parallel tables of four worlds by three tracks:

```
0x800c5bf4  archive names    desert1 desert2 desert3
                             glacial1 glacial2 glacial3
                             NULL     haunted2 haunted3      <- 0x800c5c0c
                             volcano1 volcano2 volcano3
0x800c5c34  display names    Dawn Encounter, Snake River Mine, Road to Rustwell
                             Cold Fusion, Command Center, Helicrash
                             NULL, R.M. Sludgeworks, R.M. Test Track   <- 0x800c5c4c
                             Exhaust Pipes, Serpent Sprint, Volcano Blowout!
```

## What is missing

- No `HAUNTED1BIG` in `CCCPSX.BIG`.
- Both table slots are zero, so even the name the code would show is gone.

## Reachable?

Not established. A loader indexing the table with world 2, track 0 would read
a null name pointer.

## Unknown

- Whether the front end skips the slot or can select it.
- What the `TCUP.BLD`, `SCCUP1.BLD` and `SCCUP2.BLD` members carried in the
  Haunted 2 and 3 archives are, and whether they relate to the missing track.
