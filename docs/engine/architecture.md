---
title: How the program is put together
status: partial
discs: US
covers: US CCCPSX.EXE:0x8009fc10 _start, 0x80010a5c main, 0x8005fa00 game_init, 0x8005fb3c, 0x8002881c, 0x8001bd88, 0x80019870, 0x8001e3b4, interface tables 0x8011df28, 0x8012fcdc, 0x8012fd30, 0x8012fe54
worklog: 3, 6
---

# How the program is put together

`CCCPSX.EXE` is C compiled with the PsyQ toolchain, linked with the PsyQ
libraries (1997). Its game code is organised as modules that publish their
functions in **interface tables**: arrays of function pointers in bss, filled
at start-up and called through with `jalr`. The game's flow is two
[table-driven state machines](state-machine.md).

## Memory map

```
0x80010000  game code
~0x8009fc10 PsyQ crt0 and libraries
0x800b5df8  end of code; read-only data and initialised data follow
0x800c2918  fsm_main's state records, its table at 0x800c5668, machine 0x800c5bdc
0x800d0b48  $gp
0x800d23cc  bss start (cleared by _start)
0x8011df28  interface tables (bss)
0x80143b44  bss end; heap from here (InitHeap)
0x801ffff0  initial stack
```

## Start-up

```
_start   0x8009fc10   clear bss, set $sp and $gp, InitHeap, jal main
main     0x80010a5c
  0x8009fcb8          library start-up (calls through a pointer table)
  0x80012bac          system init
  0x80012c74
  0x80012ca0
  0x8001dab0(0)
  0x80012e04("screens")
  game_init_fsm 0x80061d78
      game_init 0x8005fa00  fill the interface tables; on success fsm_init(&fsm_main)
  0x80015648(0)       loads and shows a TIM (inferred: the loading screen)
  while (game_tick()) ;      fsm_tick(&fsm_main) until it finishes
  game_shutdown 0x80061db8
  0x80012d08          "Program Terminated"
```

## The interface tables

`game_init` first points every slot of a table at a default stub with
0x8005fb3c (address, byte count), so an unfilled slot is safe to call, then
calls the function that fills it. Each filler returns nonzero on success; on a
failure `game_init` calls 0x80019a90, 0x8001be94 and 0x80028b08 for the tables
already filled (inferred to undo them) and returns 0.

| table | bytes | slots | filled by | what the slots point at (by their strings) |
| --- | ---: | ---: | --- | --- |
| 0x8011df28 | 252 | 63 | 0x8002881c | race loading (`LOADING ...`), memory card (`BASLUS-00964HTWHEELS`), TIM loading, world colour |
| 0x8012fcdc | 84 | 21 | 0x8001bd88 | *unknown*; 0x8001b170 switches on a field selector and reads 98-byte records at 0x8011b2b8 indexed by its first argument |
| 0x8012fd30 | 292 | 73 | 0x8001e3b4 | general: power-up loading (`%s.pup`), `fsm_second`'s tick, file names (`%s%d`) |
| 0x8012fe54 | 176 | 44 | 0x80019870 | sound: engine banks (`Electrc`), `mainsfx2`, `dialog%d`, static sound slots |

`hwtr-re slots` lists every slot with the functions stored into it and the
number of `jalr` sites that call through it. 183 slots hold functions; 713 of
the 784 `jalr` sites load their target from a fixed slot, and 642 of those
resolve to the one function ever stored there.

## Unknown

- Each module's name and each slot's meaning.
- The 0x8012fcdc module's purpose; the indexed records suggest one per
  car or player, a guess.
- Which slots are rewritten after init. Only 0x8012fd30 has two functions
  stored into it by name (0x80010e28 and the stub, the latter from the fill
  loop).
