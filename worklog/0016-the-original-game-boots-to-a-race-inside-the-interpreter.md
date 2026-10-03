---
number: 16
title: The original game boots to a race inside the interpreter
date: 2026-10-03
area: tooling, engine, test, render
files: crates/hwtr-hle/src/lib.rs, crates/hwtr-hle/src/hw.rs, crates/hwtr-hle/src/gpu.rs, crates/hwtr-hle/src/main.rs, crates/hwtr-cpu/src/machine.rs, crates/hwtr-cpu/src/bus.rs, docs/engine/libraries.md
---

# 16. The original game boots to a race inside the interpreter

`hwtr-hle` runs the unmodified `CCCPSX.EXE` in `hwtr-cpu` from its entry point,
headless, and it plays: through the "LOADING..." title, the memory card check
(answering "VALID MEMORY CARD NOT FOUND", as a console with no card does), the
main menu, and with scripted presses (down, X, Start, X) into a race on Dawn
Encounter (DESERT1), the grid of six cars at 368 x 240. It runs at about 110
frames a second. Its frames are drawn by its own software GPU from the
commands the game sends, so this is the original's picture, not a
reconstruction; it agrees with `hwtr-viewer`'s ([[12]], [[15]]).

The point is ground truth: any game state on any frame, from the original
code, to check ported functions against with realistic inputs.

## What the host supplies

- **Hardware, at the register level** (`hw.rs`): GP0/GP1, DMA2 (linked lists,
  blocks), DMA6 (ordering-table clear), DMA4 and the SPU's registers and RAM,
  root counters, interrupt registers. A software rasterizer (`gpu.rs`) draws
  into VRAM: 4/8/15-bit texture pages, modulation over 128, the four
  semi-transparency modes, drawing area and offset.
- **libcd at its API** (CdInit, CdSearchFile, CdControl/B, CdRead, CdReadSync),
  served from the disc image, because the hardware path is interrupt-driven.
- **The kernel** (in `hwtr-cpu`): InitHeap, malloc and free as a first-fit
  heap, events, Enter/ExitCriticalSection, the GPU BIOS calls, C0/B0 tables.
- **libmcrd**: no card in either slot.
- **The pad**: libpad's port-1 buffer written every frame (digital pad).
- **printf**: logged under `hwtr_hle::printf` instead of printed; the game
  narrates its loading ("Opening file decals.bmf").

## What it took

Each stop was a fact about the game or its libraries, now on
[the libraries page](../docs/engine/libraries.md):

1. The game is linked for **8 MB** (`_ramsize` 0x800000). A heap that never
   reused memory handed out addresses past 2 MB, which alias low RAM; a CD
   read of 69 sectors overwrote the code. A first-fit heap fixed it.
2. A **timed screen** waited on a clock that only the **vertical blank
   callback** advances; the callback is registered through VSyncCallback.
3. The **buffer flip spins** until that callback runs, so the vertical blank
   became a real interrupt: every 560,000 instructions the handler runs
   between two instructions, on its own stack, returning to libetc's
   dispatcher address (0x8009fff8), masked inside critical sections.
4. The game waits for the GPU to be **busy** right after DrawPrim; the GPU
   reports busy for two status reads after drawing.
5. **libgpu's draw-complete callback** fires from 0x800a20d0 when DMA2 is idle;
   with DMA finishing instantly it recursed 221 levels and ran the handler
   stack into low memory. DMA2 now stays busy for 4,000 instructions and its
   completion arrives as an interrupt that runs the queue processor, as
   libgpu's own handler does.
6. libmcrd keeps its own busy flag, so every command starter is answered,
   not only the two the first check used.

Debugging tools added on the way: write watchpoints, a stack guard, the CPU
state at the faulting instruction even inside nested calls, the hottest code
and the `fsm_main` state at the end of a run.

## A correction

[[1]] and the first docs named the developer Stainless Games. The title screen
the game draws says "Portions (c) 1999 Stormfront Studios"; the README,
`cairns.toml` and the docs now say Stormfront.

**Still unknown:** Sound (the SPU and libsnd run against a bare register model); the memory card (always absent); how close the software GPU is to the hardware (no dithering, no mask bits, edge rules unverified); timings (instructions per frame and DMA duration are estimates).
