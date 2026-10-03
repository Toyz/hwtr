---
number: 18
title: A decompiler for reading, and ports checked inside the running game
date: 2026-10-03
area: tooling, test, physics
files: crates/hwtr-psx/src/decomp.rs, crates/hwtr-game/src/car.rs, crates/hwtr-game/src/ram.rs, crates/hwtr-game/src/math.rs, crates/hwtr-cpu/src/machine.rs, crates/hwtr-hle/src/port.rs, crates/hwtr-hle/src/bin/lockstep.rs, crates/hwtr-game/tests/car.rs
---

# 18. A decompiler for reading, and ports checked inside the running game

The car update tree under `cars_update` is about 150 KB of MIPS. Reading it
from the disassembly alone was too slow, so `hwtr-re pseudo FILE ADDR|NAME`
now prints a function as C-like pseudo-code, and the race port has a way to
check each function against the original as the game plays.

## The decompiler

Each basic block is executed symbolically: registers hold expression trees,
and stores, calls and branches become statements. What made it readable:

- **Liveness.** A backward pass over the blocks decides what to write out: a
  value appears only where it is read later. Delay slots run in execution
  order (a branch reads its operands before its slot, a call reads its
  arguments after), and hi/lo are pseudo-registers, so a `mult` in one block
  and an `mflo` in the next connect.
- **Parallel writes.** Writing several registers at a block end is one
  parallel assignment; it is ordered so each register is read before it is
  overwritten, with a temporary for a cycle, and the branch condition is
  rewritten in terms of the written registers. The first version printed
  `s1 = s1 + 1; if (((s1 + 1) & 255) < 3)`, which is wrong.
- **Materializing.** A pending value that a store, a call or a register
  overwrite would change is written out first, but only if it is still live.
  A dead one is dropped. Getting that wrong produced `a0->0x14 = ...` where
  the original stored through `local_10[i]->4`.
- **Idioms folded.** The stack frame (sp-relative slots read as `local_NN`,
  prologue saves and epilogue restores vanish), `$gp` globals, GCC's
  fixed-point multiply `mflo >> 12 | mfhi << 20` as `fx(a * b >> 12)`, and its
  divide-by-zero and overflow traps (`break 0x1c00`, `break 0x1800` and the
  branches around them, dropped).
- **Arity.** A call shows as many arguments as the callee reads, found by
  running the same liveness pass on the callee (memoised, with recursion
  read as four). `car_get_model(a0, s1 & 255)`, not `(a0, s1 & 255, a2, a3)`.
  Call results are named after the callee (`r_cos`, `r_vec_length`).

`0x800a9f78` turns out to be libgcc's `__divdi3` over `__udivmoddi4`
(`0x800b23e4`).

## The first car function

`0x80040a90`, now `update_wheels`, places the wheels each physics step. The
car record is 0x930 bytes, array at 0x80128fcc, count at 0x800d263c;
`cars_update` dispatches on the state byte at +0x891 (2: `car_update`, 1:
`0x80040494`, 0: `0x80040644`). Wheels are 0x88 bytes from +0x218, the count
at +0x548. For each wheel: the heading is the body's forward axis (rotation
column 1), turned by the steering angle at +0x10 for wheels with flag bit 1
(front pair one way, rear the other); on the ground it is laid into the
ground plane (heading minus its projection on the normal) and renormalised
with GCC's inline `(a << 12) / b`; the world mount point is
`R (mount - car+0x770) + pos`; and for a grounded wheel the contact-point
velocity is `v + ω × r`. It counts grounded wheels at +0x54b, and at +0x54c
those whose ground normal faces against the body's +0xec axis
(`-(n · up) >= 2049`).

One quirk: each heading's VECTOR padding word gets four bytes of
uninitialised stack (the stack copies' fourth words are never written). The
port leaves the word alone, and both checks below skip it.

## Checking a port

Two checks, both from save states:

- **Unit.** `tests/car.rs` loads the machine part of a state (no disc
  needed), runs the original on each car with `Machine::call`, the port on a
  copy of the same RAM, and compares all of RAM except the stack. It does
  this on the saved cars, then 63 more rounds with steering, wheel flags,
  contact and normals scrambled, including normals at the 2049 threshold.
  The first version missed a mutation of that threshold; the boundary cases
  were added for that reason.
- **In the game.** `Machine::check` adds shadow checks: when a watched
  function is entered, the port runs on a copy of RAM; when the original
  returns (same `ra`, same `sp`), the two must agree. Calls that an
  interrupt lands inside are skipped. `lockstep STATE FRAMES` runs the game
  this way: 600 frames of scripted driving from `desert1-drive` gave 385 calls
  matched and 5 skipped.

The first idea was to replace the original with the port and compare RAM
against an unpatched run, frame by frame. It fails at frame 27, in a GPU
packet buffer, not in the cars. A hook runs in zero instructions, and the
reference times its interrupts in instructions, so the vertical blank lands
somewhere else and the draw buffer is caught at a different stage. That mode
stays as `--replace` for later, when the port owns the frame.

The call rate says something about the game: update_wheels runs about 0.65
times per vertical blank, for the player's car only. The game logic runs
slower than the display, and the other cars sit at state 1.

**Still unknown:** what car +0xec and +0x770 are (named UP and ORIGIN from their use only); why only the player's car runs this physics (the others take 0x80040494 at state 1); the instruction cost of a hooked function, which --replace mode needs before patched and unpatched runs can agree on timing
