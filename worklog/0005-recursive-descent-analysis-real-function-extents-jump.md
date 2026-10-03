---
number: 5
title: Recursive-descent analysis: real function extents, jump tables, and calls through interface slots
date: 2026-10-03
area: tooling, decomp, test
files: crates/hwtr-psx/src/program.rs, crates/hwtr-psx/src/analysis.rs, crates/hwtr-psx/src/mips.rs, crates/hwtr-re/src/main.rs
---

# 5. Recursive-descent analysis: real function extents, jump tables, and calls through interface slots

The first function scan ([[3]]) only collected start addresses. `hwtr-psx`'s
`program` module now walks every function from its start along both arms of
each branch, so each function has a real extent, its calls, and its switch
tables, and `hwtr-re funcs` reports on the whole image:

```
1838 functions, code ends 0x800b5df8, 39 jump tables, 29 unowned code words
183 interface slots hold functions; 713 jalr sites load from a fixed slot,
642 resolved to one callee
```

## What went wrong on the way, and the fix for each

- **Data that decodes as code.** A linear scan for `jal` words found 1144
  "callers" of 0x800b5e84: a table of halfwords at 0x800b5e04 whose words
  read as `b` and `jal`. Certain code is now only what is reachable by direct
  calls from the entry; its highest address (0x800b5df8, right before the
  table) bounds every heuristic seed. Calls made by any walked function are
  then walked too, which found the last 3.8 KB of library code.
- **Switches.** GCC builds the table address two ways, `lui; addu; lw
  %lo(t)(b)` and `lui; addiu; addu; lw 0(b)`. A backward constant tracker
  (`lui`/`addiu`/`ori` through the last write to each register) handles both;
  the `sltiu` before gives the case count. All `jr` through a register now
  resolve: 39 tables.
- **Case labels taken for functions.** A data word pointing at a case label
  seeded a "function" there before its switch was found. A walk now passes
  through heuristic starts instead of stopping at them, and a final prune
  drops any heuristic start another function's walk reaches. No two functions
  overlap now.
- **`break` after a divide** ended up joining crt0 to the next function; it
  now ends a block (the divide checks always branch around it).

## Calls through interface slots

Most indirect calls load their target from a fixed bss word that init code
stored a function's address into ([[6]]). The analysis records every `sw`
of a function address to a constant address as a slot, then resolves each
`jalr` whose register was loaded from such a slot. The constant tracker's
window had to grow from 32 to 160 instructions: the table fillers are long
straight-line functions that set the base register once at the top.
`hwtr-re slots` lists them.

## New commands

`disasm --func`, `calls` (call tree), `xrefs` (calls, data references and
pointers to an address), `slots`, and `fsm` ([[6]]). A synthetic image with a
call and a two-case switch pins the walker in a unit test.

**Still unknown:** The 29 unowned words at 0x800a2b18, 0x800aa41c and 0x800b4974-0x800b4a00 (the last is position-independent code the card library copies elsewhere); the 142 jalr sites still unresolved.
