---
title: The PsyQ libraries as linked
status: partial
discs: US
covers: US CCCPSX.EXE:0x8009fc10 _start, 0x8009fdb8 VSyncCallback, 0x800a38cc VSync, 0x800a05a0 DrawSync, 0x800a20d0 the libgpu queue, 0x800a4d18 CdInit, 0x800a4e18 CdSearchFile, 0x800a589c CdControl, 0x800a5b0c CdControlB, 0x800a4b18 CdRead, 0x800a4c18 CdReadSync, 0x800a5e00 CdPosToInt, 0x800a5cfc CdIntToPos, 0x800a8778, 0x800a9184, 0x800a8524 libmcrd, 0x800a77c4 PadInitDirect, 0x800a2d28 printf, 0x800c8690 _ramsize
worklog: 16
---

# The PsyQ libraries as linked

What the game expects of the console, through the Sony libraries linked into
`CCCPSX.EXE` (1997 versions, by their `$Id` strings: `sys.c 1.135`,
`intr.c 1.76`, `bios.c 1.86`). These are the behaviours a host has to supply
to run the original code, and the ones the port must reproduce where the game
depends on their timing.

## Memory

`_ramsize` (0x800c8690) is 0x800000 and the stack size (0x800c868c) 0x8000:
the game is linked for 8 MB. crt0 puts the stack at 0x807ffff8, which on a 2 MB
console mirrors to 0x801ffff8, and gives `InitHeap` 0x80143b48 up to 0x807f7ff8.
It fits in 2 MB only because the BIOS allocator reuses freed blocks; addresses
past 0x80200000 alias low RAM, including the code.

## Library entry points

| address | function | notes |
| --- | --- | --- |
| 0x8009fdb8 | VSyncCallback(func) | through libetc's callback table at `*0x800c775c`, entry +20 with id 4 |
| 0x800a38cc | VSync(mode) | mode < 0: the counter at 0x800c86b4; 1: horizontal count; else wait |
| 0x800a05a0 | DrawSync(mode) | 1: returns 1 while DMA2 or the GPU is busy, else the queue length |
| 0x800a20d0 | libgpu queue processor | runs queued commands while DMA2 is idle; when the queue is empty and DMA2 idle, calls the draw-complete callback (`*0x800c77d8`) |
| 0x800a4d18 | CdInit | |
| 0x800a4e18 | CdSearchFile(CdlFILE *, name) | "CdSearchFile: searching %s..." |
| 0x800a589c | CdControl(com, param, result) | |
| 0x800a5b0c | CdControlB(com, param, result) | |
| 0x800a4b18 | CdRead(sectors, buf, mode) | |
| 0x800a4c18 | CdReadSync(mode, result) | polls with VSync(-1) timeouts, retries through 0x800a4928 |
| 0x800a5e00 | CdPosToInt | BCD to sector, less 150 |
| 0x800a5cfc | CdIntToPos | |
| 0x800a8778 | libmcrd MemCardAccept (inferred) | prints "event multipul open" when busy |
| 0x800a9184 | libmcrd MemCardSync (inferred) | -1 nothing pending, 0 running, 1 done |
| 0x800a8524, 0x800a8f1c, 0x800a92a0, 0x800a89dc, 0x800a8c7c, 0x800a94a8 | other libmcrd commands | by their guard messages |
| 0x800a77c4 | PadInitDirect(buf1, buf2) | 34-byte buffers at 0x8011b388 and 0x8011b3aa |
| 0x800a2d28 | printf | the game prints its file opens ("Opening file %s") and more |
| 0x800a45c8 | toupper | |
| 0x800a5e80 | memcpy | |

## Timing the game depends on

- **The vertical blank callback drives the clock.** System init registers
  0x8001224c with `VSyncCallback`. It adds 17 to the millisecond clock at
  `$gp + 0x18c4` (read by 0x80010e28, 55 callers), counts frames, and
  finishes a requested buffer flip. Timed screens loop until that clock
  advances.
- **The flip waits by spinning.** 0x800122f4 bumps a pending-flip count and,
  when two are pending, spins until the vertical blank handler takes one. The
  handler must arrive as an interrupt during that spin.
- **The draw-complete callback runs in interrupt context.** libgpu calls it
  from 0x800a20d0 only when DMA2 is idle and the queue empty. The game's
  callback starts the next drawing, so if a host's DMA finishes inside the
  call that started it, the callback recurses without bound (221 levels
  deep before the stack ran out, in the first attempt).
- **The game waits for the GPU to have started.** 0x80013a64 loops on
  `DrawSync(1) == 0` right after DrawPrim; on hardware the GPU is busy at that
  point.
- **Critical sections.** The libraries bracket their work with
  `syscall` Enter/ExitCriticalSection (a0 = 1, 2); no interrupt may arrive
  between.
- **A stray argument.** The vertical blank handler calls 0x80011d28 with one
  argument, and the callbacks return to libetc's dispatcher at 0x8009fff8;
  code that reads past its arguments sees those values.

## Unknown

- libetc's callback table entries other than +20, and where libgpu registers
  its DMA handler.
- libspu and libsnd: not yet mapped; the harness lets their polling run
  against a register model.
- The exact libmcrd function names.
