---
title: VAB sound banks
status: partial
discs: US
covers: US every *VH and *VB member of CCCPSX.BIG (499 banks, 1829 samples); US CCCPSX.EXE:0x8001a0c4 the bank loader
worklog: 9
---

# VAB sound banks

Sony's standard SPU sound bank, as two files: the header `.VH` (programs,
tones, sample sizes) and the body `.VB` (the samples, PS-ADPCM). Every sound
effect, engine sound and voice line in the game is a VAB.

## Layout

All little-endian.

```
VH
  char[4]   magic          "pBAV"
  u32       version
  u32       id
  u32       size           VH + VB
  u16       reserved
  u16       programs       programs in use
  u16       tones          tones over all programs
  u16       vags           samples in the VB
  u8        master volume
  u8        master pan
  u8        attr1, attr2
  u32       reserved
  program[128]             16 bytes; a program with 0 tones is unused
    u8      tones
    u8      volume, priority, mode, pan
    u8      reserved
    u16     attr
    u8[8]   reserved
  tone[16 * programs]      32 bytes, 16 slots per used program in program order
    u8      priority, mode, volume, pan
    u8      center         note at which the sample plays at its recorded rate
    u8      shift          fine tune of center, 1/128 semitone
    u8      min, max       note range
    u8      vibrato width, time, portamento width, time
    u8      pitch bend min, max
    u8[2]   reserved
    u16     adsr1, adsr2   SPU envelope registers
    u16     program
    u16     vag            1-based sample number
    u16[4]  reserved
  u16       vag_size[256]  entry 0 unused; entry n = sample n's bytes / 8
VB
  the samples back to back, in vag order
```

## PS-ADPCM

16-byte blocks of 28 samples. Byte 0: low nibble shift (0-12; 13-15 behave as
9), high nibble filter (0-4). Byte 1: flags, bit 0 end, bit 1 repeat, bit 2
loop start. Bytes 2-15: 4-bit samples, low nibble first. Each sample is
`(nibble << 12 >> shift) + (s1 * f0 + s2 * f1 + 32) >> 6`, clamped to 16 bits,
with filters `(0,0) (60,0) (115,-52) (98,-55) (122,-60)`.

## Measured

All 499 banks parse; in every one the tone count matches the programs' and
the `vag_size` entries sum exactly to the VB's length. 1829 samples.

## Unknown

- The recording rate of each sample: a VAB does not store it; it follows from
  the note played against `center`. `hwtr-re vab` writes WAVs at 22050 Hz.
- How the game picks programs and notes (the sound module, interface table
  0x8012fe54).
