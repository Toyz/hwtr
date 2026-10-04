---
number: 77
title: The sound chip takes ADPCM from rrt-kit; no emulator in the game binary
date: 2026-10-04
area: build
files: crates/hwtr/src/spu.rs, crates/hwtr/Cargo.toml, Cargo.lock
---

# 77. The sound chip takes ADPCM from rrt-kit; no emulator in the game binary

retro_rt moved PS-ADPCM and the SPU's Gaussian interpolation from rrt-emu into rrt-kit (`rrt_kit::adpcm::spu`, unchanged, with its tests), and added a CD-XA decoder (`rrt_kit::adpcm::xa`). I updated rrt from c693b31 to 7325d77.

**The sound chip off the emulator crate.** The app's sound chip (crates/hwtr/src/spu.rs) decoded its samples with `rrt::emu::spu`. That was the only thing linking rrt-emu, the originals' CPUs and HLE, into the game binary, against the rule that the hwtr app is the port alone. It now uses `rrt::kit::adpcm::spu`, the same functions moved. The app drops rrt's `emu` feature: `cargo tree -p hwtr` no longer lists rrt-emu. hwtr-hle never used it.

**XA.** Nothing to use it for. The disc has no XA: no file is Form 2 (docs/disc/layout.md), the music is CD-DA and the movies are EA's WVE. If a later disc or revision turns out to carry XA, the decoder is there.

Checked: the workspace tests pass, and a DESERT1 race runs with the sound chip on (`HWTR_SHOT_SOUND=1`).

**Still unknown:** nothing
