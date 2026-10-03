---
title: The rigid body and its integrator
status: partial
discs: US
covers: US CCCPSX.EXE:0x8006c504 integrate, 0x80025be4 orthonormalize, 0x80026884 and 0x800273ec the 64x16-bit matrix products, 0x80026650 vec_length
worklog: 20
---

# The rigid body and its integrator

Cars carry a rigid body at +0x30; the car's position, velocity, rotation,
angular velocity and its force and torque sums are the body's fields (see
[the car object](car-object.md)). `integrate` (0x8006c504) steps it. Its
callers are `car_update` for cars under full physics, 0x80040494 for cars at
state 1, and 0x8006b754 and 0x8007c894, not yet identified. After each step
the caller runs `orthonormalize` (0x80025be4) on the rotation.

## Layout (from the body's start)

| offset | type | meaning |
| --- | --- | --- |
| +0x58 | s64[3][3] | inverse inertia, body axes |
| +0xb0 | s32 | mass |
| +0xb4 | s32 | inverse mass |
| +0xb8 | s32 | gravity's strength, 386 (in/s²) |
| +0xbc | VECTOR | gravity's direction, (0, 0, -1) on the tracks seen |
| +0xdc | VECTOR | position |
| +0xec | VECTOR | momentum |
| +0xfc | VECTOR | velocity |
| +0x10c | s32 | speed |
| +0x110 | MATRIX | rotation, body to world (the translation part is not used here) |
| +0x130 | s64[3] | angular momentum |
| +0x148 | s64[3][3] | inverse inertia, world axes |
| +0x1a0 | VECTOR | angular velocity, radians a second |
| +0x1b0 | s32 | its length |
| +0x1b4 | s32[3] | force summed over the step |
| +0x1c8 | s64[3] | torque summed over the step |
| +0x1e0 | u8 | asleep: not integrated (the car clears it when a pedal is pressed) |

## One step of `dt` seconds

1. Asleep: nothing.
2. Gravity `direction × (strength × mass)` joins the force sum.
3. Momentum `+= F dt`; velocity `= momentum × inverse mass`; speed is its
   length (the 64-bit square root shared with `vec_length`). Above 0x900000
   (2304 in/s, about 131 mph) momentum and velocity are scaled down to it.
4. Position `+= v dt`; the force sum is cleared.
5. Angular momentum `+= (torque × dt) >> 12` in 64 bits.
6. World inverse inertia `= (R · I⁻¹) · Rᵀ`, through 0x800273ec (`M · A`) and
   0x80026884 (`A · M`): each term a 64-bit entry times a 4.12 short, shifted
   down 12.
7. Angular velocity `= (I⁻¹_world · L)`, each product of two 64-bit values
   shifted down 20, summed, then down 8. Above 4π (`fx(0x4000, 0x3244)`)
   both it and the angular momentum are scaled down to it.
8. Rotation `+= ([ω dt]ₓ · R) >> 8`, entry by entry in 16 bits, where
   `[ω dt]ₓ` is the skew matrix of `(ω << 8) dt >> 12` in 64 bits.
9. The torque sum is cleared.

The rotation then drifts off orthonormal; `orthonormalize` fixes it by
classical Gram-Schmidt on the columns, each normalised with the inline
`(a << 12) / length` divide and kept to 16 bits.

## Unknown

- What 0x8006b754 and 0x8007c894 integrate.
- Where mass, inertia and gravity are set up (the car loader?).
