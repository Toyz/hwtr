---
title: TIM images
status: solid
discs: US
covers: US PSXLEGAL.TIM, PSXRFA1.TIM, every *TIM member of CCCPSX.BIG (575 distinct files)
worklog: 9
---

# TIM images

Sony's standard texture file. Every image on the disc is a TIM: full-screen
backgrounds, menu art, HUD pieces, the 41 car skins.

## Layout

All little-endian.

```
u32        magic        0x00000010
u32        flags        bits 0-2 pixel mode: 0 4bpp, 1 8bpp, 2 15bpp, 3 24bpp
                        bit 3 a CLUT block follows
CLUT block (when flags bit 3)
  u32      length       bytes including this 12-byte header
  u16      x, y         VRAM position
  u16      w, h         colours per palette, number of palettes
  u16      colour[w*h]
image block
  u32      length       see below
  u16      x, y         VRAM position, x in 16-bit units
  u16      w, h         width in 16-bit units (pixels = 4w, 2w, w, 2w/3 by mode), height
  u16      data[w*h]
```

A 15-bit colour is `r | g << 5 | b << 10 | stp << 15`, 5 bits each. The GPU
treats 0x0000 as transparent when texturing; `0x8000` is opaque black.

## The image block length

The standard counts the 12-byte block header in `length`. On this disc 83
files do and 492 do not: the car skins (`<CAR>.TIM`, 33312 bytes each, 41 cars
in 12 archives) store 32768, the pixel bytes alone. A reader must size the
block from `w * h`, not from `length`.

## What the disc holds

Measured over all 575 distinct TIM files: every one parses.

- Car skins: 8bpp, 128 x 256, one 256-colour palette, image and palette at
  VRAM (0, 0) in the file (the game places them itself).
- Front-end screens such as `PSXMAIN1.TIM`: 8bpp, 640 x 240, one palette.
- `PSXLEGAL.TIM`: 8bpp, 640 x 240.

## Unknown

- Where the game uploads each TIM in VRAM; the positions in the files are all
  (0, 0) for the samples checked, so the loader decides.
