---
number: 9
title: TIM and VAB: every image and every sound bank on the disc decodes
date: 2026-10-03
area: format, audio, render, tooling
files: crates/hwtr-data/src/tim.rs, crates/hwtr-data/src/vab.rs, crates/hwtr-data/src/png.rs, docs/formats/tim.md, docs/formats/vab.md
---

# 9. TIM and VAB: every image and every sound bank on the disc decodes

Both are Sony's standard formats, and `hwtr-data` now reads them:
`hwtr-re tim DIR` converts every TIM under a directory to PNG (with a small
built-in PNG writer, so the tools stay free of dependencies) and
`hwtr-re vab DIR` checks every bank.

## The car skins break the TIM standard

The first run converted 83 TIMs and failed 492: every car skin. Their image
block's length field is 32768, which is the pixel data alone, where the
standard counts the 12-byte block header too (32780). The CLUT block in the
same files counts its header correctly (524 = 12 + 512). Whatever tool built
the skins wrote one field differently. The reader now sizes each block from
its `w * h` and accepts either length; all 575 distinct TIMs decode. The Deora
skin renders as a recognisable flame-painted body with its Hot Wheels logo,
and `PSXMAIN1.TIM` as the main menu background, 640 x 240.

## VAB

All 499 banks (both halves present for each) parse, each bank's tone count
matches its programs, and the per-sample sizes in the header sum exactly to
the VB's length: 1829 samples. Decoded voice and effect samples look like
sound rather than noise (zero-crossing rates 0.16-0.33, peaks near full
scale). A VAB does not store sample rates, so the WAVs are written at 22050 Hz
until the sound code says otherwise.

The track archives' engine banks hold two programs and 32 tones; the menu's
per-car banks one program and 16.

**Still unknown:** Where the game puts each TIM in VRAM; the sample rate of each VAB sample, which depends on the note the sound code plays.
