---
title: The car object
status: partial
discs: US
covers: US CCCPSX.EXE:0x80128fcc the car array, 0x800d263c the car count, 0x8004064c cars_update, 0x8003fbe4 car_update, 0x80040a90 update_wheels
worklog: 18
---

# The car object

Every car in a race is one 0x930-byte record in an array at 0x80128fcc; the
number in use is the word at 0x800d263c. `cars_update` (0x8004064c) walks the
array once per game frame with the frame time in seconds, 4.12
(`(ms << 12) / 1000`), and dispatches on the state byte at +0x891:

| +0x891 | update |
| --- | --- |
| 2 | `car_update` (0x8003fbe4): the full physics, see below |
| 1 | 0x80040494 |
| 0 | 0x80040644 |

In the races tested, only the player's car is at state 2. The full physics
runs about 0.65 times per vertical blank, because the game logic runs slower
than the display.

## Fields

Vectors are libgte `VECTOR`s (three words and a padding word), 4.12 fixed
point unless noted; the rotation is a libgte `MATRIX`'s 3x3 part (shorts,
4.12, rows as the GTE takes them). Names are from use and stay provisional
until every reader of a field is ported.

| offset | type | meaning | evidence |
| --- | --- | --- | --- |
| +0x10 | s32 | steering angle, radians 4.12; negative steers right | turns the wheel headings in `update_wheels` |
| +0xec | VECTOR | an axis of the body, read as "up" | `update_wheels` counts ground normals facing against it |
| +0x10c | VECTOR | position, world units | wheel world positions are offset from it |
| +0x12c | VECTOR | linear velocity | the contact-point velocity starts from it |
| +0x140 | short[3][3] | body rotation; column 0 sideways, column 1 forward | wheel headings come from column 1 |
| +0x1d0 | VECTOR | angular velocity | `ω × r` at each contact point |
| +0x218 | wheel[4] | the wheels, 0x88 bytes each | below |
| +0x548 | u8 | wheel count (4) | loop bound |
| +0x54b | u8 | wheels on the ground this step | counted by `update_wheels` |
| +0x54c | u8 | grounded wheels whose normal faces against +0xec by more than half (`-(n · up) >= 2049`) | counted by `update_wheels` |
| +0x770 | VECTOR | the point wheel mounts are measured from | subtracted from each mount before rotating |
| +0x891 | u8 | update state | the dispatch above |

## A wheel

| offset | type | meaning |
| --- | --- | --- |
| +0x00 | VECTOR | mount point, body space |
| +0x18 | u8 | flags; bit 1: the wheel steers |
| +0x1c | VECTOR | rolling direction, world space, unit length |
| +0x2c | VECTOR | mount point, world space |
| +0x3c | u8 | non-zero when touching the ground |
| +0x40 | VECTOR | ground normal under the wheel |
| +0x50 | VECTOR | contact point, world space |
| +0x60 | VECTOR | the body's velocity at the contact point |

## update_wheels (0x80040a90)

Run first in the physics step. With `c, s = cos(-steer), sin(-steer)`,
forward `f` and side `x` the rotation's columns 1 and 0:

- heading = `f` for a wheel that does not steer; `f·c - x·s` for steering
  wheels 0 and 1; `f·c + x·s` for steering wheels 2 and 3 (the rear pair
  turns against the front);
- on the ground, heading = normalise(heading - n (heading · n)), each
  component divided by the length with GCC's inline `(a << 12) / len`;
- world mount = `R (mount - car+0x770) + position` (`ApplyMatrixLV`);
- on the ground, contact velocity = `v + ω × (contact - position)`, and the
  counters at +0x54b and +0x54c.

Every multiply is GCC's `(a * b) >> 12` in 64 bits, kept to 32.

The original also copies four bytes of uninitialised stack into each
heading's padding word (+0x28): its stack copies of the vectors never have
their fourth word written. Nothing is known to read it.

## Unknown

- What +0xec and +0x770 are, beyond their use here.
- The rest of the record: 0x930 bytes, a few dozen fields named so far.
- What states 1 and 0 are for (other cars, finished cars?).
