---
number: 1
title: The disc and what is on it
date: 2026-10-03
area: disc, content, tooling
files: crates/hwtr-disc/src/lib.rs, crates/hwtr-disc/src/cue.rs, crates/hwtr-disc/src/iso.rs, docs/disc/layout.md
---

# 1. The disc and what is on it

The archive in `original/` is a Redump-style dump of Hot Wheels Turbo Racing,
NTSC-U, `SLUS-00964` (Electronic Arts, 1999; developed by Stainless Games): a
CUE sheet, one 2352-byte-sector BIN for the Mode 2 data track, and thirteen
BINs of Red Book audio. `readme.html` is a download site's redirect page and is
not part of the disc.

Nothing on this machine reads a PlayStation disc, so `hwtr-disc` was written
for it: the CUE sheet, raw sectors with their CD-XA subheaders, and ISO 9660
with the XA system use field. `hwtr-re disc info|ls|extract|cat|audio` drives
it.

## What the data track holds

Twenty-one root directory entries and no subdirectories:

- `SYSTEM.CNF` boots `cdrom:\SLUS_009.64;1` with `TCB = 4`, `EVENT = 10`,
  `STACK = 801FFF00`.
- `SLUS_009.64` (124928 bytes) is a boot loader, loaded at 0x80100000, high
  enough to load the game under itself. Its strings name the two movies, the
  two TIMs and `\CCCPSX.EXE;1`.
- `CCCPSX.EXE` (798720 bytes, image at 0x80010000, entry 0x8009fc10) is the
  game.
- `CCCPSX.BIG` (78272596 bytes) is every asset; see [[2]].
- `EA_LOGO.WVE` and `ONLINE.WVE` are EA's movie format. They are Form 1 data.
- `PSXLEGAL.TIM`, `PSXRFA1.TIM` are full-screen TIMs (154144 bytes each).
- Thirteen `.DA` entries carry the CD-DA attribute (XA 0x4555).

## The `.DA` entries are the music tracks

Each `.DA` entry's LBA is exactly the absolute LBA of an audio track's INDEX 01
(the data track's 43786 sectors, plus each earlier audio BIN, plus the 150
sector pregap), and its size is that track's playable sectors times 2048. Track
2 is `AVENUEX.DA` at LBA 43936 through track 14, `FUEL.DA` at 277816, all
thirteen matching. `disc extract` skips them; `disc audio` writes the tracks
as WAV.

## No XA, no STR

Over all 43786 data-track sectors, 43632 have the Form 1 data subheader
`00 00 08 00` and the other 154 are empty Form 2 sectors: LBA 12-15 in the
system area and the 150-sector postgap. No file uses Form 2, so there is no XA
ADPCM audio and no STR video on the disc. The port needs CD-DA playback, VAB
sample playback and an EA WVE decoder, and nothing for XA.

The extracted files go to `work/fs/`; the reference is
[the disc page](../docs/disc/layout.md).

**Still unknown:** How SLUS_009.64 chains to CCCPSX.EXE; whether the game itself reads PSXLEGAL.TIM or PSXRFA1.TIM from the file system.
