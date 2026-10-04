---
number: 78
title: Two write-only questions: the fastest lap's car and the flying table's in-use byte
date: 2026-10-04
area: decomp
files: docs/engine/results.md, docs/engine/flying-wheels.md
resolves: 67, 71
---

# 78. Two write-only questions: the fastest lap's car and the flying table's in-use byte

Two small questions, answered from the executable alone.

**Who reads the fastest lap's car (0x800d0e69, from 67).** Nobody. There are two stores and no load:

- 0x80061e04, the HUD's race setup, writes 0xff (none).
- 0x80064ce4, the standings, writes the fastest car.

Nothing else touches it:

- **Loads:** no gp-relative load and no address built from `lui 0x800d`.
- **Neighbours:** the only base pointer nearby is 0x800d0e60, a two-entry per-player array.
- **Pointers in data:** a scan of the executable finds none to 0x800d0e60–0x800d0e69.

So it is a write-only byte. The port keeps nothing for it.

**The flying table's in-use byte (from 71).** Only 0 or 1 ever lands in a record's +0:

- the table's set-up at race load (0x8007c7a0) and clearing (0x8007c824) write 0
- the throw (0x8007c9b0) writes 1
- taking a car's wheels away (0x8007da78) writes 0
- a snapshot put back (0x8007ec08) copies a byte from a snapshot, and only in-use records are saved

The snapshot code's branch for kind 1 never meets another kind.

Docs: results.md (standings, layout) and flying-wheels.md (the in-use byte).

**Still unknown:** nothing
