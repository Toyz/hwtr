---
title: Track textures (GLM, GLB)
status: solid
discs: US
covers: US <TRACK>.GLM, <TRACK>.GLB, SFX.GLM in the 11 track archives; SCREENS.GLM; US CCCPSX.EXE:0x80024604 glm_load, 0x80012864, 0x80028184
worklog: 12
---

# Track textures (GLM, GLB)

Raw VRAM uploads: each file is two rectangles of halfwords that
`glm_load` (0x80024604) passes to LoadImage, palettes first.

## Layout

All little-endian.

```
block, twice
  u16  x, y, w, h       VRAM rectangle, x and w in halfwords
  u16  data[w * h]
```

| file | block 1 (CLUTs) | block 2 (texels) |
| --- | --- | --- |
| `<TRACK>.GLM` | (384, 384) 256 x 80: 80 16-colour CLUTs, one per row | (384, 0) 320 x 256: five 4-bit texture pages, 6-10 |
| `<TRACK>.GLB` | (384, 474) 256 x 8: eight 256-colour CLUTs | (896, 256) 64 x 256: one 8-bit page (tpage 0x9e), the sky |
| `SFX.GLM` | (384, 488) 256 x 24 | (480, 256) 160 x 160 |

In the GLM's CLUT rows, entries 16-255 are filler (0x8000).

## Measured

All 33 (11 GLM, 11 GLB, 11 SFX.GLM) are exactly two blocks.

## SCREENS.GLM

A different format, in the front end archive: repeated `{u32 len; TIM}`,
seven 8-bit 128 x 64 TIMs (`len` is 12 more than the TIM). `0x80028184`
ignores the TIMs' own positions and uploads image i at a table at 0x800bddbc
with its CLUT at (640, 450 + i).

## Unknown

Nothing about the layout.
