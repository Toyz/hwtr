---
number: 6
title: Two table-driven state machines run the game, and modules publish themselves in interface tables
date: 2026-10-03
area: engine, decomp, tooling
files: docs/engine/state-machine.md, docs/engine/architecture.md, symbols/cccpsx.txt
---

# 6. Two table-driven state machines run the game, and modules publish themselves in interface tables

`main` (0x80010a5c) is short: a chain of init calls, then `while
(game_tick()) ;`. `game_tick` (0x80061ddc) is `fsm_tick(&fsm_main) == 0`.
The whole game flow is data.

## The machine

`fsm_tick` (0x8006bb18) reads a 24-byte machine record and 20-byte state
records: three lists of `void (*)(void)` (enter, update, exit), a list of
`{i16 event, i16 target}` transitions, and four `i8` counts. One call runs one
phase. `fsm_post` (0x8006baac, 155 callers) accepts an event only if none is
pending and the current state has a transition for it; otherwise it is
dropped. The details, including the one-tick shortcut a state with no exit
functions takes, are in [the state machine page](../docs/engine/state-machine.md).

Two machines, found from the two callers of `fsm_init`:

- `fsm_main` at 0x800c5bdc: 349 states, initial 1, final 0. State 1 runs
  twelve enter functions (the boot). Its state table at 0x800c5668 holds
  exactly 349 pointers and ends at the machine record, which is how the count
  was confirmed rather than guessed.
- `fsm_second` at 0x800c6640: 37 states, initial 0, final 3, its table at
  0x800c65ac also ending at the record. Its tick (0x8009cb74) is called only
  through an interface slot.

`hwtr-re fsm` dumps a machine and names, for each transition, the functions
that post its event (calls to `fsm_post` with constant `a0` and `a1`): 622
transitions in `fsm_main` and 68 in `fsm_second`.

## Interface tables

`game_init` (0x8005fa00, reached from `main` through 0x80061d78) points every
slot of four bss tables at a stub that returns 0, then calls each table's
filler:

```
0x8011df28  63 slots  race loading, memory card, TIM loading
0x8012fcdc  21 slots  unknown; per-index 98-byte records at 0x8012b2b8
0x8012fd30  73 slots  general: power-ups, fsm_second's tick
0x8012fe54  44 slots  sound: engine banks, effects, dialog
```

The rest of the game calls through these, which is why 784 `jalr` sites
outnumber what one would expect from C function pointers alone. Resolving
them ([[5]]) is what makes the call graph usable.

The race loader that prints `LOADING WORLDS..` and the other progress strings
(0x80033304) is slot 0x8011df30.

**Still unknown:** What each of fsm_main's 349 states is; what fsm_second drives; the names and purposes of the four interface tables' modules, iface_b's especially.
