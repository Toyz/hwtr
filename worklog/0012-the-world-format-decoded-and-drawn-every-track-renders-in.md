---
number: 12
title: The world format, decoded and drawn: every track renders in hwtr-viewer, and up is +z
date: 2026-10-03
area: format, render, tooling, content
files: crates/hwtr-data/src/world.rs, crates/hwtr-data/tests/disc.rs, crates/hwtr-viewer/src/main.rs, crates/hwtr-viewer/src/render.rs, crates/hwtr-viewer/src/scene.rs, docs/formats/world.md, docs/formats/glm.md, docs/formats/scp.md, docs/formats/bld.md, symbols/cccpsx.txt
---

# 12. The world format, decoded and drawn: every track renders in hwtr-viewer, and up is +z

A background investigation of the world loader and renderer (one research
agent, report-only) produced a reading of WLD, DLW, WLB, GLM, GLB, SCP and BLD.
Its WLD/DLW/WLB and GLM/GLB layouts are now `hwtr-data::world`, and the
reader's own checks hold on every file: all 33 world models (11 each) parse
with every offset and index in range, every vertex pad 0xdead, the cells'
polygon counts summing to the header's, and the grid bounds rule holding; all
33 texture files are exactly two VRAM blocks. SCP and BLD stay `guess` pages
until a reader checks them.

The headline facts: a track is a grid of 1024-unit cells, each with up to
256 vertices and 256 colours and its own polygons (20 bytes, 4-bit textured,
colours baked per vertex, no lighting); objects are 76-byte-quad meshes in a
child/sibling tree; `.DLW` is the same track exported mirrored for mirror
races (chosen by bit 6 of the race flags); `.WLB` is the sky; all world
textures come from the track's GLM (five 4-bit pages and 80 CLUTs) and GLB
(the sky's 8-bit page), not from TIM files. BLD is the best line, the AI's
racing line, which the loader string names.

## hwtr-viewer

`hwtr-viewer TRACK` builds a 1024 x 512 VRAM from the GLM, GLB and SFX.GLM
blocks and draws the track with a shader that does the PlayStation's texture
lookup: texture page and CLUT from each polygon, 4-, 8- or 15-bit texels out
of VRAM, colour 0x0000 transparent, texel times vertex colour over 128. It
flies with W A S D or the DualSense's sticks, and `--shot` renders offscreen
to PNG, which is how these pictures were checked from here. DESERT1,
GLACIAL2, VOLCANO1 and HAUNTED2 all draw with correct textures: Volcano
Island shows its blue and red-yellow striped track, the checkered start line,
palm trees, the volcano and a boat.

## Which way is up: a mistake, caught

The first picture, drawn with z as up and no culling, looked to me like the
track seen from below, so I negated z. That was wrong twice over: negating one
axis is a reflection (it mirrors the track), and the user, looking at the
viewer, said the map was upside down. The question was settled from the
game's code rather than by eye. `world_cell_emit_gt4` (0x800317e0) draws a
polygon when NCLIP on corners a, b, d is negative, or on c, b, d positive;
with the GTE's camera space (x right, y down, z forward, right-handed) and a
rotation-only view, that means front faces have their right-hand normal
toward the viewer. On DESERT1, VOLCANO1, GLACIAL2 and HAUNTED3, about four of
every five near-horizontal faces have that normal pointing +z (DESERT1: 2248
against 533). So up is +z, no conversion is needed, and the viewer now culls
back faces the way the game does (double-sided faces emitted both ways), which
would leave holes if the orientation were wrong. It shows every track upright.

**Still unknown:** The world's minor tables (rec40, rec24b, anim and dyn fields); SCP's B, D, E, F; the BLD streams; whether near polygons are subdivided; the sky's camera.
