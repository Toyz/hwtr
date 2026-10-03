---
title: The FCity world
status: guess
discs: US
covers: US CCCPSX.EXE:0x800d0b7c "FCity", 0x80011310 the world colour picker
worklog: 4
---

# The FCity world

A world name, `FCity`, that only one piece of code knows.

## What exists

- `CCCPSX.EXE` 0x800d0b7c holds `"FCity"`, between `"PTest"` and `"Desert"`.
- The world colour picker at 0x80011310 tests for it and, on a match, passes
  `0, 0, 0` to 0x80010e34. Haunted (whose compare result is ignored) and any
  name not in the list, Volcano among them, end at the same `0, 0, 0`.

## What is missing

- No text in `ENGLISH.HWT`, no archive in `CCCPSX.BIG`, no slot in the track
  table at 0x800c5bf4.

## Reachable?

No path to it is known.

## Unknown

- What the name stands for. "Future City" or similar is a guess from the
  abbreviation alone.
