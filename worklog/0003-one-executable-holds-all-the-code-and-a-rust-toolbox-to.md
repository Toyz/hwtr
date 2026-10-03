---
number: 3
title: One executable holds all the code, and a Rust toolbox to read it
date: 2026-10-03
area: decomp, tooling, design, build
files: crates/hwtr-psx/src/mips.rs, crates/hwtr-psx/src/exe.rs, crates/hwtr-psx/src/analysis.rs, crates/hwtr-re/src/main.rs, crates/hwtr-re/src/docs.rs, symbols/cccpsx.txt
---

# 3. One executable holds all the code, and a Rust toolbox to read it

`CCCPSX.EXE` is the whole game: a PS-X EXE whose 796672-byte image loads at
0x80010000 and enters at 0x8009fc10. The BIG archive's `.OVL` members are
sprite sheets, not code ([[2]]), and no string names a code overlay, so there
is one image to reverse.

## The decision: the RE tools are Rust

Earlier projects kept their reverse engineering tools in Python under `tools/`
and the port in Rust. This one writes the tools in Rust too, in the same
workspace as the port, so a reader of a format exists once and the port uses
the same code the investigation proved. Python stays available for throwaway
probes (the BIG checksum in [[2]] was found with one), but anything kept is
Rust. The crates so far:

```
hwtr-disc   CUE sheets, raw Mode 2 sectors, ISO 9660 with CD-XA     (port uses it)
hwtr-data   BIG archives; later every asset format                    (port uses it)
hwtr-psx    PS-X EXE, R3000A + GTE disassembler, static analysis
hwtr-re     the command line: disc, big, exe, disasm, funcs, strings, docs
```

`hwtr-re docs index|check` replaces piney_apples' `tools/docs.py`.

## The disassembler

MIPS I with COP0 and the GTE: every R3000A opcode, the GTE commands by
function field with their `sf`, `lm` and MVMVA fields, `lwc2`/`swc2` and the
GTE data and control register names. It prints the usual pseudo-ops (`nop`,
`move`, `li`, `b`, `beqz`, `negu`) and annotates `lui`/`addiu` and
`lui`/load-store pairs with the string or name they form. Unit tests pin
encodings, including RTPS (0x4a180001) and RTPT (0x4a280030).

## crt0 and the memory map

The entry point is PsyQ's crt0, unchanged in shape:

```
0x8009fc10  clear 0x800d23cc .. 0x80143b44   (bss, 464760 bytes)
            sp  = (word at 0x800c8690) - 8 | 0x80000000
            gp  = 0x800d0b48
            InitHeap(0x80143b44 + 4, ...)   (BIOS A0:39)
            jal 0x80010a5c                   main
```

The executable carries `gp0 = 0` in its header, so `$gp` is only known from
this code. Game globals are read `gp`-relative (`lw v0, 6364(gp)` in
0x800113f8), which the reference scan does not yet resolve.

## Function discovery

`hwtr-re funcs` finds 1540 starts: 834 `jal` targets, 668 frames opened right
after a `jr ra`, 36 BIOS stubs (`li t2, 0xA0|0xB0|0xC0; jr t2; li t1, N`, 31
named), the entry, and one data pointer. The PsyQ libraries sit from about
0x8009fc10 up, by their `$Id` strings (`sys.c 1.135 1997/09/02`, `intr.c
1.76`, `bios.c 1.86`); the game below.

Names live in `symbols/cccpsx.txt`, read by `disasm`, `funcs` and `strings`,
each with a confidence note.

**Still unknown:** Which of the ~1540 found starts are PsyQ library functions; the prologue heuristic's false positives; the jalr and jump-table targets the scan does not see.
