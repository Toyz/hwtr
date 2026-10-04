---
title: The world model (WLD, DLW, WLB)
status: partial
discs: US
covers: US <TRACK>.WLD, <TRACK>.DLW, <TRACK>.WLB in the 11 track archives; US CCCPSX.EXE:0x800246d4 world_model_load, 0x8001e744 world_load, 0x8001e2c8 race_load_world, 0x8001ef24 world_cull_and_draw_view, 0x800317e0 world_cell_emit_gt4, 0x8001e87c world_object_draw, 0x80010344 mesh_draw_gt4, 0x800242a4 clut_anim_update
worklog: 12, 62
---

# The world model (WLD, DLW, WLB)

One format for three files per track: `.WLD` the track, `.DLW` the same track
exported mirrored (left and right swapped), `.WLB` the sky. The track is a
grid of 1024-unit cells, each with its own vertices, colours and polygons;
objects (buildings, arches, signs) are separate meshes placed by matrix.
Textures come from the track's [GLM and GLB](glm.md), never from TIMs.

Mirrored races load `.DLW` instead of `.WLD`: `race_load_world` (0x8001e2c8)
passes bit 6 of the race settings flags to `world_load` (0x8001e744).

## Layout

All little-endian. Every offset field is from the start of the file;
`world_model_load` (0x800246d4) adds the load address to each.

```
header, 104 bytes
  +0   u32  grid_w            cells along x (0 in WLB)
  +4   u32  grid_h            cells along y
  +8   s32  origin_x          cell x = (x - origin_x) >> 10
  +12  s32  origin_y
  +16  s32  max_x             (grid_w - 1) * 1024 < max_x - origin_x <= grid_w * 1024
  +20  s32  max_y
  +24  u32  total_polys       sum of the cells' polygon counts
  +28  off  poly_base         start of all cell polygons
  +32  u32  n_listed          always equal to n_objects
  +36  u32  n_objects
  +40  off  objects           52-byte records
  +44  off  listed            u32 offsets of objects; the loader sets bit 31 (visible) of each one's flags
  +48  u32  n_dyn             84-byte records (collision volumes, inferred)
  +52  off  dyn
  +56  u32  n_cameras         40-byte records: trackside cameras
  +60  off  cameras
  +64  u32  n_anim            24-byte records: object animations
  +68  off  anim
  +72  u32  n_pickups         32-byte records
  +76  off  pickups
  +80  u32  n_clutanim        44-byte records
  +84  off  clutanim
  +88  u32  n_sounds          24-byte records: the track's sounds
  +92  off  sounds            s32 x, y, z (20.12), u32 flags (1: follows the
                              animation whose +20 word names it), u32 sound,
                              u32 volume (0-255); see ../engine/world-sound.md
  +96  u32  flags             bit 0: +100 is the background colour (set in every WLB, clear in every WLD)
  +100 u32  background        0x00BBGGRR

cell[grid_w * grid_h], 24 bytes, row-major (cy * grid_w + cx), from +104
  +0   u32  n_polys
  +4   off  polys             20-byte polygons
  +8   u32  n_verts           at most 256
  +12  off  verts             8 bytes: s16 x, y, z, u16 0xdead
  +16  u32  n_colours         at most 256
  +20  off  colours           4 bytes: u8 r, g, b, 0

cell polygon, 20 bytes
  +0   u8[4]  vertex index a, b, c, d (perimeter order; a triangle has c == d)
  +4   u8[4]  colour index a, b, c, d
  +8   u8 u, v   of a      +10 u16 clut
  +12  u8 u, v   of b      +14 u8 tpage (low byte)   +15 u8 flags
  +16  u8 u, v   of c      +18 u8 u, v of d

object, 52 bytes
  +0   s16[3][3] rotation, 4.12
  +18  u16       0xcdcd
  +20  s32[3]    position in world units (20.12 once loaded: shifted left 12)
  +32  off/0     child, drawn with this object's matrix
  +36  off/0     sibling, drawn with the parent's matrix
  +40  u32       mesh quads
  +44  off       mesh, 76-byte quads
  +48  u32       flags: bit 31 visible, 0xe0 billboard, 0x100 semi-transparent

mesh quad, 76 bytes
  +0   4 x { s16 x, y, z; u8 u, v }
  +32  4 x { u8 r, g, b, x; u32 0xcdcdcdcd }
  +64  u32 flat colour (unread by the renderer)
  +68  u16 clut   +70 u16 tpage
  +72  u32 bit 0 double-sided, bits 16-31 depth bias (x4 ordering-table entries)

trackside camera, 40 bytes (0x80021390 reads one; the attract race cuts to them)
  +0   u32    flags (bit 1: never chosen)   +4 u32 field of view, 4.12 radians
  +8   s16[3][3] rotation, 4.12, then a pad   +28 s32[3] position, 20.12

pickup, 32 bytes
  +0   s32[3] position, 20.12   +12 u32   +16 off/0 object   +20 char[12] name ("Handling", "Gyro", ...)

clut animation, 44 bytes
  +0 u8 first, +1 u8 last       range of CLUT entries that cycle
  +2 s16 speed                   frames per step, sign is direction
  +4 u16 x, +6 u16 y             the CLUT in VRAM
  +8 u32                         replaced at load by a frame counter
  +12 u16[16] colours
```

## Axes and units

x and y are the ground plane and the grid's axes, z is height, **up is +z**,
and the space is right-handed. The evidence is the game's own culling: a face
is drawn when NCLIP on corners a, b, d is negative (or on c, b, d positive),
which with a rotation-only view matrix means its right-hand normal points at
the viewer; on four tracks, 4 of every 5 near-horizontal faces have that
normal on +z (DESERT1 2248 against 533). Drawn that way with back faces
culled, every track is upright: trees stand, roads face the sky. Vertex
coordinates are world units; matrix translations are 20.12 at run time.

## Drawing

- The GPU receives a quad's corners as a, b, d, c (POLY_GT4, code 0x3C), so
  the perimeter order a, b, c, d is drawn as triangles (a, b, d), (b, d, c).
  Object quads likewise as 0, 1, 3, 2.
- Colour is per vertex, baked: no lighting, no fog.
- Polygon flags: bit 0 draws a back-facing polygon; bits 3-7 are a depth
  bias of `(flags >> 3) * 4` ordering-table entries.
- Cells are chosen around the view (`world_cull_and_draw_view`, 0x8001ef24)
  into five distance buckets; near polygons are redrawn subdivided
  (0x800328dc, inferred).
- The sky (`.WLB`, objects only) is drawn at the far end of the ordering
  table with its own camera, inferred to be centred on the viewer; its quads
  use the 8-bit page of the GLB.

## Measured

All 33 files (11 each of WLD, DLW, WLB) parse with every index and offset in
range, every vertex pad 0xdead, every cell polygon count summing to
`total_polys`, and the bounds rule above holding. DESERT1.WLD: 28 x 31 cells,
9071 polygons, 42 objects, 9 pickups.

DLW against WLD (from the investigation): every x becomes `1 - x` and
polygon winding reverses; polygon counts differ by 0-14 per track because the
mirrored export re-split some polygons.

## Unknown

- Trackside camera flags other than bit 1, anim field +16 (+20 names the track sound that follows it), dyn +72..+80, object flags 0x2,
  0x4, 0x8, and pickup +12.
- Whether 0x800328dc subdivides near polygons, and the stray `gpf` in
  0x800317e0.
- The sky's exact camera.
