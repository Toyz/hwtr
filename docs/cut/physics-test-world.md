---
title: The Physics Test world
status: partial
discs: US
covers: US SCREENS.BIG::ENGLISHHWT lines 3, 12-14; US CCCPSX.EXE:0x800d0b74 "PTest", 0x80011310 the world colour picker
worklog: 4
---

# The Physics Test world

A fifth world, "PTest", with three tracks named "Physics Test 1-3". The text
and one code path still know it; no track data exists.

## What exists

- `SCREENS.BIG::ENGLISHHWT` line 3 is `PTest`, between `Glacial` and
  `Haunted` in the world list, and lines 12-14 are `Physics Test 1`,
  `Physics Test 2`, `Physics Test 3`, in the same position in the track list.
- `CCCPSX.EXE` 0x80011310 compares a world name against a list of names with
  0x800a2d98 (a string compare) and passes three bytes to 0x80010e34. `PTest`
  (string at 0x800d0b74) gets `56, 88, 144`, the same as `Action`
  (0x800d0b6c). Glacial gets `240, 246, 255` and Desert `120, 59, 106`, so the
  three bytes are inferred to be a colour.

## What is missing

- No `PTEST*BIG` archive in `CCCPSX.BIG`.
- No slot in the track archive name table at `CCCPSX.EXE` 0x800c5bf4, which
  has three slots for each of Desert, Glacial, Haunted and Volcano only.

## Reachable?

Not established. The world list in the text puts it third, but the track
table the loader indexes has no row for it.

## Unknown

- Whether any menu code still counts five worlds.
- What the "Action" world name is (it shares PTest's colour).
