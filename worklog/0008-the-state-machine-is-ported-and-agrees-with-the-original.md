---
number: 8
title: The state machine is ported and agrees with the original step for step
date: 2026-10-03
area: engine, test
files: crates/hwtr-game/src/fsm.rs, crates/hwtr-game/tests/fsm.rs, crates/hwtr-game/tests/common.rs
---

# 8. The state machine is ported and agrees with the original step for step

`hwtr-game` is the port itself, and its first module is the state machine
of [[6]]: `Fsm::new`, `post` and `tick` rewrite `fsm_init`, `fsm_post` and
`fsm_tick`, and `read_def` reads a machine's states out of `CCCPSX.EXE`, the
count taken from the state table running up to the machine record.

The test is the pattern every later port module follows. For both machines
and 20 seeds each, it runs the original functions in `hwtr-cpu` ([[7]]) with
every action function hooked to record its call instead of running, and the
port on the same machine definition, then applies the same random sequence of
up to 400 steps to both: a post of an event the current state handles (three
times in four) or of any event, or a tick. After every step it compares the
actions each tick called, in order, whether the machine finished, and the
original record's `current` (as a state index), `phase` and `event` against
the port's. It passes.

To be sure the test can fail, two deliberate bugs were tried and both were
caught within three steps: disabling the immediate switch a state with no
exit actions takes, and letting a second post overwrite a pending event.

Actions are generic in the port (`Def<A>`). The tables read from the disc
carry the original function addresses; as each action is ported, the port
maps its address to the Rust function.

**Still unknown:** nothing
