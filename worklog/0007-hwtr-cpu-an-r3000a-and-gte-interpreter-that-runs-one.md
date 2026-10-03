---
number: 7
title: hwtr-cpu: an R3000A and GTE interpreter that runs one original function at a time
date: 2026-10-03
area: tooling, test, render
files: crates/hwtr-cpu/src/cpu.rs, crates/hwtr-cpu/src/gte.rs, crates/hwtr-cpu/src/bus.rs, crates/hwtr-cpu/src/machine.rs
---

# 7. hwtr-cpu: an R3000A and GTE interpreter that runs one original function at a time

The port is to be held to the original function by function, the method that
worked for piney_apples: run the original on chosen inputs, run the Rust
rewrite on the same inputs, compare. `hwtr-cpu` is the interpreter that runs
the original.

## What it is

- **CPU.** MIPS I as the R3000A runs it. The load delay slot is real: the
  instruction after a load reads the old value, and a write by that
  instruction to the same register beats the load landing. Branch delay
  slots, `lwl`/`lwr`/`swl`/`swr` with forwarding from a pending load,
  `div` by zero and `0x80000000 / -1` with the hardware's results, `add`
  and `addi` overflow. Exceptions do not vector into a BIOS; they stop the
  step as a `Fault` (syscall, break, overflow, address, bus, reserved).
- **GTE.** All 22 commands and both register files, following psx-spx and
  Mednafen's implementation (which passes the hardware test suites): MAC1-3
  checked against 44 bits and wrapped after every addition, MAC0 against 32,
  the per-component saturation flags, the UNR reciprocal for the divide, the
  RTPS IR3 flag quirk, MVMVA's lost far-colour product and its mx = 3 garbage
  matrix, H reading back sign-extended, IRGB/ORGB, LZCS/LZCR.
- **Bus.** 2 MB of RAM mirrored through 8 MB, the scratchpad, and I/O ports
  that are recorded (address, width, value) rather than emulated.
- **Machine.** `Machine::with_exe` loads `CCCPSX.EXE` and sets `$gp` from its
  crt0. `call(func, args)` puts the first four arguments in a0-a3 and the rest
  on the stack, plants a return address, and runs until the function returns.
  `hook` and `stub` replace a function with a host closure; calls into the
  BIOS tables at 0xa0/0xb0/0xc0 go to a small shim (memcpy, memset, strcmp,
  strcpy, strlen, printf, malloc as a bump allocator, free, FlushCache).

## Checked by

Unit tests for the load delay (both directions), a branch delay slot, an
unaligned word through `lwr`/`lwl`, RTPS projecting a point to a hand-worked
screen position (H = 256, SZ3 = 512 gives a reciprocal of exactly 0x8000),
the divide overflow flag, NCLIP, AVSZ3 and the register read-back rules. The
real check is [[8]]: the original state machine run in it agrees with the
port step for step.

**Still unknown:** The GTE has only been checked against hand-worked cases, not against hardware traces; the BIOS shim covers a handful of A0 functions, so any test that reaches others stops with Fault::Bios until they are added.
