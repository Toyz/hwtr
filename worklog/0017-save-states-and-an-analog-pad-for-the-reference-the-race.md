---
number: 17
title: Save states and an analog pad for the reference; the race drives
date: 2026-10-03
area: tooling, test, input, race
files: crates/hwtr-cpu/src/state.rs, crates/hwtr-cpu/src/machine.rs, crates/hwtr-hle/src/lib.rs, crates/hwtr-hle/src/hw.rs, crates/hwtr-hle/src/gpu.rs, crates/hwtr-hle/src/main.rs, crates/hwtr/src/main.rs
---

# 17. Save states and an analog pad for the reference; the race drives

Two additions to the reference of [[16]] that the race port needs.

**Analog pad.** With a gamepad connected, `hwtr --original` (and
`hwtr-hle --analog`) presents a DualShock in analog mode: libpad's buffer
carries type 0x73 and the four stick bytes after the buttons, right stick
first. The game takes it ([[11]]: 0x73 goes to the analog reader) and still
reaches the race.

**Save states.** `Hle::save`/`load` (`hwtr-hle --save FILE`, `--load FILE`)
write the whole running game: the CPU with every GTE register and the pending
delayed load, RAM and scratchpad, the heap's blocks, the interrupt timing
(instruction clock, time to the next vertical blank, the handler, pending
device interrupts, the critical-section mask), and the hardware model (VRAM
and the GPU's command state, DMA registers, SPU registers and RAM). A state
is 3.7 MB.

They are exact. The harness is deterministic (two runs of the same 1500
frames save identical bytes), and running to frame 2500 in one go gives the
same bytes as loading the frame-2400 state and running 100 frames. One false
alarm on the way: zsh does not split an unquoted `$VAR` into words, so a run
given its options through a variable got none of them and sat on the memory
card screen, which looked like nondeterminism until checked.

`work/states/desert1-race.bin` is the grid of Dawn Encounter at the start of
the race (frame 2400 of the scripted run). Loaded and driven with X held, the
race runs: the player's Splittin' Image II pulls away on the first lap, and the
HUD shows speed 73, 00:03.50, LAP 1/4, position 5/6 and the turbo meter.

**Still unknown:** Sound is still silent; what the analog stick does in a race has not been checked against the digital pad.
