---
number: 35
title: The intro movies (WVE) and the boot sequence
date: 2026-10-03
area: video
files: crates/hwtr-data/src/wve.rs, crates/hwtr/src/intro.rs, crates/hwtr/src/spu.rs, crates/hwtr/src/main.rs
---

# 35. The intro movies (WVE) and the boot sequence

The port now starts as the disc does. The boot executable `SLUS_009.64` (0x80101a40) plays `EA_LOGO.WVE`, then `ONLINE.WVE` (the attract movie), then shows `PSXLEGAL.TIM` and `PSXRFA1.TIM`, then runs `CCCPSX.EXE`. The port used to open on PSXLEGAL until a button was pressed.

**The container.** WVE is big-endian chunks, each a tag and a size that includes the header:
- `VLC0` holds 224 u16.
- `au00` / `au01` hold sound: u32 BE first sample, u16 8, u16 2, then frames.
- `MDEC` holds a picture: u16 BE width 320, height 224, u32 BE number, then a frame with the PlayStation bitstream header (halfword count, 0x3800, quantiser scale, version 2).

**Not the standard bitstream.** Wrapped in a minimal STR, ffmpeg's psxstr/mdec fails on every picture after the first ("ac-tex damaged"). The boot executable's own decoder (0x801090ac, with the tables built at 0x80108cfc from 0x80108c8c) shows why:
- The code shapes are fixed in the executable: 96 short codes at 0x8011b348 and 128 long ones at 0x8011b4c8, each a length byte and the code left-aligned in a halfword. They are MPEG-1's codes, sign bits included.
- Each movie's VLC0 says what each code stands for: an MDEC word, run in the top 6 bits and a signed 10-bit level. Sony's default values are at 0x8011b6c8, and EA_LOGO's differ, so the meanings are permuted per movie.
- Decoding, per block:
  - A 10-bit DC; a DC of 0x1ff ends the frame, and 64 end words follow.
  - Then codes: if the 13-bit prefix is below 32, eight zero bits are skipped and the 9-bit long table is used; otherwise the short one.
  - Word 0x7c1f escapes to the next 16 raw bits; 0xfe00 ends the block.
- Bits are halfwords little-endian, high bit first.
- Macroblocks run top to bottom, then left to right. Each holds Cr, Cb and four Y blocks.
- After the decode it's the MDEC (psx-spx): DC × 2; AC (level × q[k] × qscale + 4) / 8 with MPEG-1's intra matrix in zigzag order; clamp; IDCT; full-range YCbCr; signed output clamped and raised by 128.

`hwtr_data::wve` ports all of this. The port reads the code shapes from SLUS_009.64 on the disc. Every picture of both movies decodes all 1680 blocks and stops within 40 bits of its data's end, and the pictures look right: the EA logo and its letters, the desert race.

**The sound** is PS-ADPCM without the flags byte: 15-byte frames of a shift/filter byte and 14 bytes of nibbles. In each chunk the first half of the frames is the left channel and the second half the right. The data decided between the layouts:
- frame-by-frame interleave: left/right correlation 0.17, and about 100 clipped samples;
- halves: correlation 0.78, no clipping.

The rate is 22050 Hz: the chunks' sample counter steps 5880 per 210 frame pairs, and the sound then lasts as long as the pictures at 15 a second (EA_LOGO 5.0 s / 75 pictures; ONLINE 40.1 s / 601).

**The sequence** (`intro.rs`):
- The movies play their sound as a stream beside the SPU's voices (`Spu::play_stream`, interpolated linearly to 44.1 kHz), and pick the picture by the stream's clock, as 0x8010546c waits on its sound clock.
- EA_LOGO can't be skipped. ONLINE stops on any button once four pictures are up (0x80100ecc tests both pads for anything held).
- The legal screens follow 0x801013f0: PSXLEGAL fades in by 8 levels a frame to 128 (16 frames), holds 5000 ms (no skip), and fades out. PSXRFA1 fades in and stays up while the game loads; with nothing to load, the front end follows at once.

With no window (`--shot`), the intro is skipped unless `--intro` is given, so shot scripts keep their timings.

**Still unknown:** The MDEC's own IDCT and rounding (the port uses a float IDCT in the standard scaling, as ffmpeg's mdec decoder does). The boot executable's exact pacing: 0x80103f60 aims picture k at k x 0x271000 / 2997 of a clock (0x8011db00 +0x7b24) whose rate isn't traced; 15 a second follows from the data (5880 samples to 4 pictures at 22050 Hz). The 22050 Hz rate itself is inferred (it makes the sound and pictures the same length), not read from the SPU setup. Whether the legal screen's PSXRFA1 hold has a minimum on the console (it stays up while CCCPSX.EXE loads).
