---
number: 2
title: CCCPSX.BIG is twelve nested archives with a word-and-byte checksum
date: 2026-10-03
area: format, content, tooling
files: crates/hwtr-data/src/big.rs, docs/formats/big.md, docs/content/inventory.md
---

# 2. CCCPSX.BIG is twelve nested archives with a word-and-byte checksum

`CCCPSX.BIG` is a `u32` count and a table of 24-byte entries (`char[12]` name,
`u32` offset, `u32` size, `u32` sum), and its twelve members are archives of
the same format: `SCREENS.BIG` for the front end and one per track. Offsets are
relative to the archive that holds the table. Names are 8.3 with the dot
dropped (`ELECTRICVH`), so the extension boundary is not stored.

## The checksum

The fourth field matched the 32-bit wrapping sum of the member's little-endian
words on the first members tried, and CRC-32 of data or name did not. Summing
all 3409 members (12 top level, 3397 nested) left 10 mismatches, every one a
member whose size is not a multiple of 4. Zero-padding the last word was wrong;
the differences were exactly the trailing bytes summed as bytes (`ENGLISH.HWT`
ends `20 0d 0a`: 0x20 + 0x0d + 0x0a = 55, the shortfall). With trailing bytes
added one at a time all 3409 match. 13 members have unaligned sizes; three of
them happen to agree under both rules, which is why only 10 failed.

## What the archives hold

Each track archive is self-contained: 41 cars (`CAR`, `TIM`, `BMF`, `SHD`
each), engine VAB banks by engine type, crash and voice banks, the HUD sheets
(`ACTN*.OVL`, byte-identical in all 11), the 13 power-ups (`PUP`), and the
track's own `GLM`, `GLB`, `WLD`, `WLB`, `DLW`, `SCP`, `BLD`. The cars make up
most of each 5 MB track archive. The `.OVL` members are not code: they start
with a count and small records and contain no MIPS. So all code is in
`CCCPSX.EXE`, which simplifies the decomp to one image.

`SCREENS.BIG` repeats data: 391 entries, 354 names; 31 names twice and 3 three
times, every repeat with the same size and sum.

`ENGLISH.HWT` names five worlds, `Desert`, `Glacial`, `PTest`, `Haunted`,
`Volcano`, with three tracks each, including "Physics Test 1-3" and "Haunted
Highway 1". The disc has archives for only eleven tracks: no Physics Test and
no `HAUNTED1BIG`.

`hwtr-re big ls` lists members with checksums verified; `big extract` writes
them to `work/big/`. Reference: [the BIG archive](../docs/formats/big.md) and
[the inventory](../docs/content/inventory.md).

**Still unknown:** How the game looks a name up and whether it ever checks the sum; the formats of every member type except TIM, VAB and the text files.
