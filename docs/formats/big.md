---
title: The BIG archive
status: solid
discs: US
covers: US CCCPSX.BIG and the 12 BIG archives nested in it; US CCCPSX.EXE:0x800cd574 "\CCCPSX.BIG;1"
worklog: 2
---

# The BIG archive

`CCCPSX.BIG` holds all of the game's data. It is an archive of twelve
archives in the same format: one for the front end (`SCREENS.BIG`) and one per
track. Each track archive is self-contained: it repeats every car, every sound
bank and every HUD sheet a race needs, so a race loads from one place.

## Layout

All little-endian. Offsets are from the start of the archive that holds the
table, so a nested archive's offsets are relative to its own first byte.

```
u32            count
entry[count]   24 bytes each, immediately after count
  char[12]     name      8.3 name with the dot removed, NUL padded: "SCREENSBIG"
  u32          offset    from the start of this archive
  u32          size      bytes
  u32          sum       checksum of the member's bytes, below
member data    at the offsets, in table order, not aligned
```

The first member starts right after the table (`4 + 24 * count`).

## Fields

`name` - The 8.3 name with its dot dropped and upper-cased, so the split
between name and extension is not stored: `ELECTRICVH` is `ELECTRIC.VH`,
`SCREENSBIG` is `SCREENS.BIG`. The game builds names with format strings such
as `"%s.vh"`, `"%s.car"` and `"%s%d.scp"` (`CCCPSX.EXE` 0x800cd854,
0x800cdbb8, 0x800ce108), so it is assumed to drop the dot from those before the
lookup; the lookup itself has not been traced.

`sum` - The wrapping 32-bit sum of the member's whole little-endian words, plus
each of its 1-3 trailing bytes added as an unsigned byte:

```
sum = 0
for each full u32 word w:   sum = sum + w        (mod 2^32)
for each trailing byte b:   sum = sum + b        (mod 2^32)
```

It holds for all 3409 members at every level (12 top-level, 3397 nested).
The 13 members whose size is not a multiple of 4 are what tell the trailing
rule apart from zero-padding a final word.

## The top level

```
offset      size      name
0x00000124  22112732  SCREENSBIG     front end: 354 members
0x01516b04   5146956  DESERT1BIG     273
0x019ff454   5166988  DESERT2BIG     273
0x01eecbe4   4989980  DESERT3BIG     273
0x023af004   5109132  GLACIAL1BIG    273
0x0288e594   5091852  GLACIAL2BIG    273
0x02d697a4   4929036  GLACIAL3BIG    273
0x0321cdb4   5213148  VOLCANO1BIG    273
0x03715994   5018124  VOLCANO2BIG    273
0x03bdeba4   5216700  VOLCANO3BIG    273
0x040d8564   5190476  HAUNTED2BIG    275
0x045cb8b4   5087124  HAUNTED3BIG    274
```

No archive nests deeper than two levels. There is no `HAUNTED1BIG`.

## Worked example

The first bytes of `CCCPSX.BIG`, and of `SCREENS.BIG` at 0x124:

```
00000000: 0c00 0000                                  count = 12
00000004: 5343 5245 454e 5342 4947 0000              "SCREENSBIG"
00000010: 2401 0000                                  offset 0x124
00000014: dc69 5101                                  size   22112732
00000018: 82a2 f715                                  sum    0x15f7a282
...
00000124: 8701 0000                                  count = 391 (SCREENS.BIG)
00000128: 454c 4543 5452 4943 5648 0000              "ELECTRICVH"
00000134: ac24 0000                                  offset 0x24ac, from 0x124
00000138: 200c 0000                                  size   3104
0000013c: 23a7 9bab                                  sum    0xab9ba723
```

`SCREENS.BIG` lists 391 entries but 354 distinct names: 31 names appear twice
and 3 three times, and each repeat has the same size and sum as the first. The
data is stored again, not shared.

## Notes

- The member types, by the extension in the name, are listed in the
  [content inventory](../content/inventory.md).
- `ACTNFNT.OVL`, `ACTNOVL1-3.OVL` and `SCRNFNT.OVL` are not code. They begin
  with a count and small tables, and the four `ACTN*` files are byte-identical
  in all 11 track archives.

## Unknown

- How the game looks a name up (linear scan, case folding, first or last match
  for the repeated names in `SCREENS.BIG`).
- Whether the game ever checks `sum`.
