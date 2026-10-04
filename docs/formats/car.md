---
title: Cars (BMF, CAR, SHD, CWH, FXP, DECALS)
status: partial
discs: US
covers: US every <CAR>.BMF, <CAR>.CAR, <CAR>.SHD, <CAR>.TIM, DECALS.BMF, CWHS.BMF; US CCCPSX.EXE:0x80021b88 car_load_race, 0x800240d4 bmf_split, 0x80022b5c car_install_lod, 0x80023cb8 model_fixup, 0x80022064 car_draw, 0x80032d1c model_emit_gt4, 0x80021208 car_read_cwh, 0x80022cd0 fxp_parse, 0x8002388c decal_unpack, 0x80029478 car_draw_shadow
worklog: 13, 58
---

# Cars (BMF, CAR, SHD, CWH, FXP, DECALS)

Each of the 41 cars is four files, identical in all 12 archives: `.BMF`
(three detail levels of model, handling, effect points), `.CAR` (a copy of the
full model for the front end), `.TIM` (the skin, see [TIM](tim.md)) and
`.SHD` (the shadow, a 4bpp 64 x 64 TIM with 16 colours). The `.car`, `.ca0`,
`.ca1`, `.cwh` and `.fxp` names in the code are parts of the BMF, used only in
debug prints.

## BMF container

All little-endian.

```
u32      n             5 for a car, 41 for DECALS.BMF and CWHS.BMF
u32      4 * n
u32      size[n]       a multiple of 4
parts                  back to back, ending at the end of the file
```

Car parts: 0 full model, 1 low model, 2 medium model, 3 CWH, 4 FXP. The game
maps detail level 0 to part 0, 1 to part 2, 2 to part 1. `.CAR` is byte for
byte part 0.

## Model

Offsets are from the model's start.

```
header, 20 bytes
  +0   u32  n_sub        wheels: 4 (6 for TOWJAM), 0 in the low and medium models
  +4   u32  0x14         the root node
  +8   u32  0x5c         n_sub child nodes, 72 bytes each
  +12  u32  wheels       n_sub records of 32 bytes
  +16  u32  0            at run time, the car's state

node, 72 bytes
  +0x00 s16[3][3] rotation, 4.12 (identity on every car)
  +0x12 u16       0xcdcd
  +0x14 s32[3]    translation, model units (20.12 once loaded)
  +0x20 off       first child (root only)
  +0x24 off/0     next sibling
  +0x28 u16 nv    +0x2a u16 nn
  +0x2c off       vertices, 8 bytes: s16 x, y, z, pad
  +0x30 off       normals, 8 bytes: s16 x, y, z (4096 long), pad
  +0x34 u32 nf    +0x38 off faces, 20 bytes each
  +0x3c u32       0xcdcdcdcd
  +0x40 u16 clut, +0x42 u16 tpage   written at load
  +0x44 u32       base colour 0x808080

face, 20 bytes; corners A B C D around the edge, triangles repeat C as D
  +0   u8[4]  vertex index A B C D
  +4   u8[4]  normal index A B C D
  +8   u8 u, v of D     +10 of A     +12 of C     +14 of B
  +16  u16    flags (2 or 3; the renderer does not read them)
  +18  u16    depth bias (40, 48, 50, 52)

wheel record, 32 bytes (inferred)
  s32 x, y, z; the same again; s32 radius; s32 half width   (half model scale)
```

Axes, inferred from the data: x lateral, y forward (rear axle at y = 0), z up
(wheel z equals its radius), the same up as [the world](world.md).

## Scale and placement

Cars are drawn at half their model's scale in world units (inferred: the
start grid's six points are 210 units apart across and 284 along, while the
Deora model is 146 wide and 358 long; at half scale the grid fits, and the
wheel records, effect points and car state are all at half scale). In a race
the skin of car slot n goes to VRAM at the table 0x800bdd30 ((960, 256),
(704, 0), (768, 0), (832, 0), (896, 0), (640, 256)) as an 8-bit page, its
palette to (384, 464 + n).

## Drawing

`car_draw` (0x80022064) picks the detail level by distance. Each face:
NCLIP on A, B, C (drawn when negative), AVSZ4 for depth plus the face's bias,
then a packet with corners A, B, D, C. Lit cars (NCCT over the normals with
the node colour) are POLY_GT4 (0x3C); unlit ones POLY_FT4 (0x2C).

## CWH, 376 bytes

```
+0     u32  0x0b9757a5
+4     308 bytes handling block A, copied into the car state
+312   64 bytes  handling block B
```

`CWHS.BMF` holds 41 CWH blocks in car-id order for the front end.

## FXP

```
+0  u8 a (0-10), u8 b (1, 2, 4), u8 c, u8 0
A[a]  40 bytes      exhaust glows: s32[3]+pad place, s32[3]+pad stretch, 2 words
B[b]  48 bytes      headlights: s32[3]+pad place, s32[3]+pad direction, 16 bytes
C[b]  44 bytes      s32[3]+pad, s32[3]+pad, 3 words; only c are read, and never drawn
```

fxp_parse (0x80022cd0) points the car's view state at the records (cvs
+0x30, +0x134, +0x168) and scales the places by the model's scale under
two cheats. [The car's lights](../engine/car-lights.md) draws the glows and
headlights.

The part is exactly `4 + 40a + 92b` bytes on every car.

## DECALS.BMF

41 parts in car-id order, each a 192 x 64 15-bit image packed as halfwords:
0xffff then a count of zero pixels, anything else a pixel. Part k is the car
named k in the table at 0x800c5ce8 (0x80023540 finds it by name with
`car_name_to_id`); unlike CWHS.BMF there is no KYLE/JETHREAT swap (part 10
is Kyle Petty's stock car, part 11 the Jet Threat, seen rendered).

The front end unpacks a car's picture into one of two VRAM slots (0x800d0d24:
(640, 256) and (640, 320)) when the car is chosen, and 0x80023660 draws it as
a modulated textured quad, colour 255, its top left at the given screen
pixel. The menus draw it while the car's model is not in (flag 0x800d2798):
the main menu at (191, 61) and (403, 61), the cup screen at (191, 61) on
every line but CAR (where the model is loaded), the garage at (95, 85) or
(95, 120) and (350, 120), the unlock announcer at (215, 77).

## Measured

`hwtr-data` reads all 492 car BMFs: five parts each; every model parses with
all indices in range; the full model has the wheels and more faces than the
medium, which has more than the low; the CWH magic holds; the FXP size rule
holds; every `.CAR` equals part 0. DECALS.BMF's 41 parts each unpack to
exactly 12288 pixels; CWHS.BMF's 41 parts are all CWH blocks.

## Notes

- **A data bug:** in CWHS.BMF, index 10 holds JETHREAT's handling and index
  11 KYLE's, while the car-name table (0x800c5ce8) has KYLE at 10 and
  JETHREAT at 11. The front end therefore shows those two cars with each
  other's handling (from the investigation; the port should keep it unless
  told otherwise).
- Six cars (DRAGSTER, JETHREAT, LAKESTER, RDROCKET, SNAKE, SPDBLSTR) have
  FXP `c > b`, so `fxp_parse` reads C records past the end of the BMF.

## Unknown

- The meaning of CWH's fields, the FXP's C records, face flags.
- How the half-scale car state relates to model scale.
