---
number: 13
title: The car formats: three detail levels, handling and effect points in one BMF
date: 2026-10-03
area: format, render, content, test
files: crates/hwtr-data/src/car.rs, crates/hwtr-data/tests/disc.rs, docs/formats/car.md
---

# 13. The car formats: three detail levels, handling and effect points in one BMF

The second background investigation covered the car loaders and model
renderer. `hwtr-data::car` now reads what it found, and these hold on every
file (`cars_parse` in the disc test):

- Every car BMF (492: 41 cars in 12 archives) is a five-part container whose
  parts end exactly at the end of the file. Part 0 is the full model, 1 the
  lowest detail, 2 the medium; the game maps detail level 1 to part 2.
- All three models parse with every vertex and normal index in range; the
  full model alone has the wheel nodes, and has more faces than the medium,
  which has more than the low (Deora: 278, 93, 9).
- Part 3 is a 376-byte CWH handling block with magic 0x0b9757a5; part 4 an
  FXP block of exactly `4 + 40a + 92b` bytes.
- Every `.CAR` file is byte for byte its BMF's part 0. The `.car`, `.ca0`,
  `.ca1`, `.cwh` and `.fxp` names in the code's prints are these parts.
- DECALS.BMF's 41 parts each unpack (0xffff then a zero-run count) to
  exactly 192 x 64 pixels; CWHS.BMF's 41 parts are all CWH blocks.

Faces are quads with corners A, B, C, D (triangles repeat C), their UVs
stored in the order D, A, C, B, and reach the GPU as A, B, D, C. `.SHD` is a
plain 4bpp TIM, 64 x 64.

Two data bugs from the investigation, recorded on the car page for the port
to keep or fix deliberately: CWHS.BMF has JETHREAT's and KYLE's handling
swapped relative to the car-name table, so the front end shows each with the
other's numbers; and six cars' FXP blocks claim more C records than they
hold, so the game reads past the end of their BMF.

Cars are not drawn in the viewer yet.

**Still unknown:** The meaning of the CWH handling fields and the FXP records; the face flags; whether DECALS.BMF shares the CWHS order bug.
