---
number: 21
title: Ports on native types; the original's layout only in codecs
date: 2026-10-03
area: design, test
files: crates/hwtr-game/src/car/mod.rs, crates/hwtr-game/src/car/layout.rs, crates/hwtr-game/src/body.rs, crates/hwtr-game/src/ram.rs, crates/hwtr-hle/src/port.rs
---

# 21. Ports on native types; the original's layout only in codecs

**Decision.** Ported code is idiomatic Rust on native types. The original's
memory layout appears only in `layout` modules, whose `read`/`write` codecs
convert between RAM and those types for the tests and the shadow checks.

**Before.** The first ports ([[18]]-[[20]]) worked on the original's RAM
through offsets (`ram.i32(car + MASS)`). That made a ported function a
drop-in for the original and easy to diff, but it kept the PS1 layout inside
the port's logic, read like C, and would have to be unpicked later anyway.

**Now.** `Car` (with `Wheel`, `Axle`, `Engine`), `Tuning` and `Body` are
plain structs. The physics is methods on them (`place_wheels`, `drivetrain`,
`aero`, `physics`, `spin_wheels`, `Body::integrate`), and per-axle values
(grip, damping, downforce) are an `Axle` each rather than paired fields.
Checking is unchanged in strength. A test or shadow check reads the struct
out of a copy of RAM, runs the method, writes it back, and compares all of
RAM with what the original left. All tests pass, and the shadow run still
matches 9151 of 9151 calls over 2400 frames. `hwtr-hle`'s port table pairs
each original address with an adapter that does that conversion.

**Rules this sets for later ports:**
- Byte flags whose values are not known to be only 0 and 1 stay as the
  stored byte, with methods for their meaning (`Wheel::on_ground`), so the
  codec round-trips them exactly.
- Fields not yet understood are named by offset (`unknown_6b4`) rather than
  guessed.
- Fixed-point arithmetic stays exact (`fx`, wrapping operations). Idiomatic
  means the structure: types, methods, iterators, `Option`. It does not
  mean floating point.
- Bytes the original fills from uninitialised stack (the heading padding)
  are not modelled. The checks skip them.

**Still unknown:** nothing
