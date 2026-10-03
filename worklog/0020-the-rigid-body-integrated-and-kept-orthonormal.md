---
number: 20
title: The rigid body integrated and kept orthonormal
date: 2026-10-03
area: physics, test
files: crates/hwtr-game/src/body.rs, crates/hwtr-game/src/math.rs, crates/hwtr-game/src/car.rs, crates/hwtr-game/tests/body.rs, crates/hwtr-game/tests/math.rs, crates/hwtr-hle/src/port.rs, docs/engine/rigid-body.md
---

# 20. The rigid body integrated and kept orthonormal

The motion half of the car's step is ported. `integrate` (0x8006c504) turns
the force and torque sums from [[19]] into motion, `orthonormalize`
(0x80025be4) repairs the rotation afterwards, and `wheel_spin` (0x80044fc4)
sets how fast each wheel is drawn turning. In the running game, 9151 calls
of the ported functions matched over 2400 frames of scripted driving from
the race start (55 skipped because an interrupt landed inside). The layout
and the steps are on the [rigid body](../docs/engine/rigid-body.md) page.

**The car carries a rigid body at +0x30.** `car_update` calls
`0x8006c504(car + 0x30, dt)`, and the body's fields line up with the car's
known ones: position at body +0xdc is car +0x10c, velocity, rotation and the
two sums likewise. The port defines those car fields through the body
(`POS = BODY + body::POS`). The integrator has four callers, so it is a
general rigid body, not just a car part.

**The car's "up" vector was gravity.** [[18]] named car +0xec "up" from its
use in `update_wheels`. The integrator multiplies body +0xbc (car +0xec) by
`strength × mass` and adds it to the force. Its value is (0, 0, -1) and the
strength 386 in/s², so it is gravity's direction. The +0x54c count is then
of grounded wheels on a floor (normal within 60° of straight up). Both names
are corrected.

**The 64-bit pieces.** Angular momentum, torque and both inverse inertias
are 64-bit. Two 730-instruction helpers take their arguments by value on
the stack: an 88-byte block (a 72-byte `s64[3][3]` plus 16 bytes) and a
32-byte `MATRIX`, with the output pointer after them. They multiply
`A · M` (0x80026884) and `M · A` (0x800273ec), so `R I⁻¹ Rᵀ` is two calls.
Speed and spin rate both go through the 64-bit square root that
`vec_length` uses, so the port's `Tables::length` serves both.

**A wrong save state.** The "desert1-drive" state saved at frame 330 has the
player nearly stopped, with the body asleep: the start countdown ends
after frame 300. Holding X does accelerate (about 830 in/s by frame 700). A
third state, desert1-speed (frame 700, in a turn), is now in the tests. The
shadow runs were not affected; they drive for hundreds of frames past the
countdown.

**Mutation testing** again separated real gaps from equivalent mutants. The
clamps compared with `>` against `>=` cannot be told apart, because at
exactly the limit the scale is `div_fx(x, x) = 4096`. A 16-bit clamp in
place of truncation in `orthonormalize` is equivalent too, since
normalised components never reach 16 bits. Everything else, from the
shift counts and the skew matrix's signs to the transpose, is caught.

**Still unknown:** what 0x8006b754 and 0x8007c894 integrate; where mass, inertia and gravity are set up; car +0x770; the airborne control 0x8003d71c and the rest of car_update
