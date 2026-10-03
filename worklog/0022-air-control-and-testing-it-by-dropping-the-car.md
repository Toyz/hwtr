---
number: 22
title: Air control, and testing it by dropping the car
date: 2026-10-03
area: physics, test, input
files: crates/hwtr-game/src/car/mod.rs, crates/hwtr-game/src/body.rs, crates/hwtr-game/tests/car.rs, crates/hwtr-game/tests/body.rs, crates/hwtr-hle/src/main.rs, crates/hwtr-hle/src/port.rs, crates/hwtr-cpu/src/machine.rs, docs/engine/car-object.md
---

# 22. Air control, and testing it by dropping the car

`car_update` runs 0x8003d71c when no wheel touches the ground. It is air
control: the stick turns the car in mid-air. It is ported, with its helpers
0x80071bc0 (hold a body axis to a direction) and 0x8003d5cc (damp the spin).
The rules are on the [car object](../docs/engine/car-object.md) page.

**Reading it.** Most of its 2354 instructions are six inlined copies of the
force-and-torque accumulation from [[19]], one pair per case. Filtering
those out of the pseudo-code left the structure: three cases (roll, yaw,
pitch), each a couple of opposite forces either side of the drag point.
The case selection compares absolute stick values across blocks with
reused registers, so the disassembly settled the conditions: strictly
greater than both others, and ties do nothing.

**Arming.** A stick axis only acts after it has been centred (within ±0.2)
while the car does over 15 mph, so a stick held through take-off does not
flip the car. This also named two bytes that [[19]] left as unknowns.
`physics` places an airborne wheel's downforce differently when car +0x86a
and +0x86b are both set. Those are "air control acting" and "an axis
armed".

**Getting it to run in the game.** In 2400 frames of scripted driving the
car left the ground for 56 steps but never with an armed stick, so `align`
and `damp_spin` saw no calls. Instead of finding a jump, `hwtr-hle` got
`--poke ADDR=WORD` to edit a state after loading. Lifting the moving car
300 inches made the game reset it (physics stopped running); 100 inches
gave a short drop. From that state with the stick swinging, `lockstep`
matched 27 air-control calls, 23 aligns and 21 spin dampings, among the
rest. `lockstep` now reports matches per function, so "it matched" can no
longer hide "it never ran".

**Mutation testing.** It caught a wrong boundary in the test itself. The
speed cases aimed at 15 mph used 66 in/s, but 15 mph is `fx(15, 17.6)` =
264 in/s, so the `<` against `<=` mutant survived until the test used the
game's own rounding. The align threshold at exactly ½ needed a crafted
case (an identity rotation, a direction ½ along the axis). The roll-or-yaw
tie is an equivalent mutant: one of the two is always zero, as the
handbrake picks which the stick drives.

This is the first port written directly on native types ([[21]]):
`Car::air_control`, `Body::align`, `Body::damp_spin`, with `AirPower` and
`AxisLock` added to `Car` and the codec.

**Still unknown:** what the constants in damp_spin (8 and 25) stand for; recovery (0x80041384) and the player's effects (0x8003cb74) remain in car_update
