---
title: The best line (BLD)
status: guess
discs: US
covers: US <TRACK>.BLD in the 11 track archives, TCUP.BLD, SCCUP1.BLD, SCCUP2.BLD; US CCCPSX.EXE:0x8007b19c bestline_load
worklog: 12
---

# The best line (BLD)

Loaded during "LOADING BEST LINE.." by `bestline_load` (0x8007b19c): the
racing line the computer cars follow, inferred from the loader's progress
string. `"%s.bld"` when race-settings flag 0x100 is set (the cup files),
`"%s%d.bld"` otherwise.

## Layout

All little-endian; the header is 22 bytes and not aligned.

```
+0   u16      0xdf00
+2   u32      a time, 41400-74200 (inferred: a lap time in ms)
+6   u16      len1
+8   u16      len2
+10  u16[6]   all smaller than len1
+22  stream 1, len1 bytes
     stream 2, len2 bytes      22 + len1 + len2 = file size, on all 14
```

## Unknown

The streams' encoding.
