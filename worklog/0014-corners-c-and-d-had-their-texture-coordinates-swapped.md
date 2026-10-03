---
number: 14
title: Corners c and d had their texture coordinates swapped
date: 2026-10-03
area: render, format, bug
files: crates/hwtr-data/src/world.rs, crates/hwtr-viewer/src/main.rs, docs/formats/world.md
---

# 14. Corners c and d had their texture coordinates swapped

The user, flying the viewer, reported that textures on canyon walls and on
the "CHECKPOINT" banner were broken: seams across each quad and smeared
halves. The cause was the cell polygon's last texture word. [[12]] read
corner c's (u, v) from +18 and d's from +16; the packet build in
`world_cell_emit_gt4` says otherwise:

```
80031a34  sw  v1, 8(t6)        packet +48 = uv3 = low half of the word at +16
80031a3c  srl v1, v1, 16
80031a44  sw  v1, -4(t6)       packet +36 = uv2 = high half (+18)
```

The GPU's slots are a, b, d, c, so uv2 belongs to d and uv3 to c: c is at
+16 and d at +18, as the investigation's own table had it. The swap was mine,
made when turning that table into code, and it gave every quad one triangle
with two corners' texture coordinates exchanged. The world page and reader are
fixed; the reader's checks could not catch it, since any order parses.

Object quads were checked the same way in `mesh_draw_gt4` (0x80010344): each
vertex's fourth halfword is its own (u, v), its colour is at +32 + 8i, and
the GPU gets them in slots 0, 1, 3, 2 with their vertex. That path was right.

The viewer now starts on the race's first grid position, the SCP's first
start point (+24, 20.12 fixed point), which lands on DESERT1's checkered start
line under its gantry: the SCP positions are in world units.

**Still unknown:** The start orientation's quaternion layout (which component is w).
