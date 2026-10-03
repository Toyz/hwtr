---
title: The table-driven state machines
status: solid
discs: US
covers: US CCCPSX.EXE:0x8006ba60 fsm_init, 0x8006baac fsm_post, 0x8006bb18 fsm_tick, 0x800c5bdc fsm_main, 0x800c6640 fsm_second
worklog: 6
---

# The table-driven state machines

The game's flow, front end and race alike, is data: state records with lists
of functions to run on entering, on every tick, and on leaving, and a list of
event transitions. Two machines exist. `main` ticks the first until it ends;
the second is ticked through an interface slot (0x8012fe20 -> 0x8009cb74).

| machine | address | states | state table | initial | final | ticked by |
| --- | --- | ---: | --- | ---: | ---: | --- |
| `fsm_main` | 0x800c5bdc | 349 | 0x800c5668 | 1 | 0 | 0x80061ddc, from `main`'s loop |
| `fsm_second` | 0x800c6640 | 37 | 0x800c65ac | 0 | 3 | 0x8009cb74 |

Both state tables end exactly where their machine record begins (349 x 4 bytes
from 0x800c5668 is 0x800c5bdc; 37 x 4 from 0x800c65ac is 0x800c6640). The
state records themselves are laid out contiguously before each table
(0x800c2918-0x800c5667 and 0x800c6100-0x800c65ab).

## Layout

All little-endian.

```
Machine, 24 bytes
  +0x00  State**  states        table of State*, indexed by state number
  +0x04  i16      initial       state number fsm_init starts in
  +0x06  i16      final         state number that ends the machine
  +0x08  State*   current
  +0x0c  State*   next          set by a transition, entered after exit runs
  +0x10  State*   final_state   states[final], cached by fsm_init
  +0x14  i16      phase         0 enter, 1 update, 2 exit, 3 done
  +0x16  i16      event         pending event, -1 for none

State, 20 bytes
  +0x00  fn*[]    enter         void (*)(void), run once when entered
  +0x04  fn*[]    update        run once per tick while current
  +0x08  fn*[]    exit          run once when left
  +0x0c  Trans[]  transitions
  +0x10  i8       n_enter
  +0x11  i8       n_update
  +0x12  i8       n_exit
  +0x13  i8       n_transitions

Trans, 4 bytes
  +0x00  i16      event
  +0x02  i16      target        state number
```

## Functions

`fsm_init(m)` 0x8006ba60: `current = states[initial]`, `final_state =
states[final]`, `event = -1`, `phase = 0`, `next = NULL`. Returns 0.

`fsm_post(m, event)` 0x8006baac: stores `event` only if no event is pending
**and** the current state has a transition on it. Otherwise the event is
dropped. Returns 0. So the first accepted event in a tick wins, and an event
the current state does not handle is not queued for later.

`fsm_tick(m)` 0x8006bb18 runs exactly one phase per call:

```
phase 3 (done):   return 2
phase 0 (enter):  call current.enter[0..n_enter]; phase = 1
phase 1 (update): call current.update[0..n_update]
                  if current == final_state: phase = 2
                  else if event != -1:
                      for each transition t of current (re-reading current):
                          if t.event == event:
                              next = states[t.target]; phase = 2; event = -1
                              if current.n_exit == 0:
                                  if current == final_state: phase = 3
                                  else: current = next
                                        phase = next.n_enter > 0 ? 0 : 1
phase 2 (exit):   call current.exit[0..n_exit]
                  if current == final_state: phase = 3
                  else: current = next; phase = next.n_enter > 0 ? 0 : 1
return phase == 3 ? 2 : 0
```

`game_tick` 0x80061ddc returns `fsm_tick(&fsm_main) == 0`, and `main` loops
while it is nonzero, so the program ends when `fsm_main` finishes its final
state's exit.

## Notes

- A state with no enter functions skips straight to update, and one with no
  exit functions switches inside the update phase, so a transition can take
  one tick or three depending on the lists. A port must keep this timing.
- The transition loop re-reads `current` and `event` each iteration; after an
  immediate switch `event` is already -1, so no second transition can match.
- `fsm_main`'s final state 0 has no functions and transitions only to
  itself; entering it ends the game.
- `hwtr-re fsm FILE ADDR` dumps a machine with each transition's posters
  (calls to `fsm_post` whose `a0` and `a1` are constants): 622 of
  `fsm_main`'s and 68 of `fsm_second`'s transitions resolve that way.

## Unknown

- What each state is. `fsm_main` state 1 runs twelve init functions and is
  the boot state; the rest are unnamed.
- What `fsm_second` drives; its tick is reached only through an interface
  slot.
