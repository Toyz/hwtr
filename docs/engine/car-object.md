---
title: The car object and its physics step
status: partial
discs: US
covers: US CCCPSX.EXE:0x80128fcc the car array, 0x800d263c the car count, 0x8004064c cars_update, 0x8003fbe4 car_update, 0x800427a8 car_physics, 0x80040a90 update_wheels, 0x80041af0 aero, 0x80060138 drivetrain, 0x80136a18 the tuning bytes
worklog: 18, 19
---

# The car object and its physics step

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

## Units

The world is in **inches** and seconds. Speeds are inches per second; the
physics divides lengths and speeds by 12 where it wants feet (drag), uses
g = 386 in/s² (`0x182000`) for weight, and converts mph with 17.6 in/s each.
Air density is 0.002376 slug/ft³. Everything is 4.12 fixed point, multiplied
as GCC's `(a * b) >> 12` in 64 bits kept to 32. Force and torque sums that
can overflow are 64-bit.

## Fields

Vectors are libgte `VECTOR`s (three words and a padding word). The rotation
is a libgte `MATRIX`'s 3x3 part (shorts, rows as the GTE takes them). Names
come from use and stay provisional until every reader of a field is ported.

| offset | type | meaning |
| --- | --- | --- |
| +0x04 | s32 | flags; bits 7, 8, 9 pick a set of tuning bytes (below) |
| +0x10 | s32 | steering angle, radians; negative steers right |
| +0x14 | s32 | accelerator, 0 to 1 |
| +0x18 | s32 | brake, 0 to 1; the throttle in reverse |
| +0x24 | u8 | handbrake: the rear wheels lose their sideways grip |
| +0xec | VECTOR | an axis of the body, read as "up" |
| +0xfc | VECTOR | where drag acts, from the position, world axes |
| +0x10c | VECTOR | position |
| +0x12c | VECTOR | velocity |
| +0x13c | s32 | speed (the velocity's length) |
| +0x140 | short[3][3] | rotation; columns: sideways, forward, up |
| +0x1d0 | VECTOR | angular velocity |
| +0x1e4 | s32[3] | force summed over the step |
| +0x1f8 | s64[3] | torque summed over the step |
| +0x218 | wheel[4] | the wheels, 0x88 bytes each |
| +0x548 | u8 | wheel count |
| +0x549, +0x54a | u8 | wheels on the front axle, on the rear |
| +0x54b | u8 | wheels on the ground this step |
| +0x54c | u8 | grounded wheels whose normal faces against +0xec (`-(n · up) >= 2049`) |
| +0x550 | engine | the engine and gearbox (below) |
| +0x644 | s32 | mass |
| +0x648, +0x64c | s32 | centre of gravity: fraction of half the length forward, of half the height up |
| +0x650 | s32 | braking force per unit weight |
| +0x654 | s32 | brake bias: the front axle's share |
| +0x658 | s32 | drag coefficient |
| +0x65c, +0x660 | s32 | downforce coefficients, front and rear |
| +0x664, +0x668 | s32 | downforce factors, front and rear |
| +0x670 | s32 | front tyre grip |
| +0x678, +0x67c | s32 | front damping in compression, in rebound |
| +0x688 | s32 | rear tyre grip (and the base of the front's when a tuning set applies) |
| +0x690, +0x694 | s32 | rear damping in compression, in rebound |
| +0x6b4 | s32 | bit 0 cancels the surface-6 drag |
| +0x770 | VECTOR | the point wheel mounts are measured from |
| +0x780, +0x784, +0x788 | s32 | width, length, height |
| +0x865 | u8 | non-zero cancels the surface-6 drag |
| +0x86a, +0x86b | u8 | both set: an airborne wheel's downforce acts along the body from the position |
| +0x891 | u8 | update state |

The player's car in Dawn Encounter, for scale: mass 5.18, wheels 31.7 and
35.6 inches across, springs at about 650 and 330 (front, rear) at rest, front
grip 7.3, rear 9.8, brake bias 0.21, 78.5 x 190 x 48 inches.

### A wheel

| offset | type | meaning |
| --- | --- | --- |
| +0x00 | VECTOR | mount point, body space |
| +0x14 | s32 | diameter |
| +0x18 | u8 | flags: 1 rear axle, 2 steers, 4 driven |
| +0x1c | VECTOR | rolling direction, world space, unit length |
| +0x2c | VECTOR | mount point, world space |
| +0x3c | u8 | non-zero when touching the ground |
| +0x3d | u8 | the surface under it; 6 drags the car |
| +0x40 | VECTOR | ground normal |
| +0x50 | VECTOR | contact point |
| +0x60 | VECTOR | the body's velocity at the contact point |
| +0x70 | s32 | tyre friction, scaling the grip |
| +0x74 | s32 | spring force along the normal |
| +0x78 | u8 | set when the tyre's force passed its grip |

The player's car has wheels 0 and 1 in front (flags 6: steer, driven) and 2
and 3 behind (flags 5: rear, driven): all-wheel drive.

### The engine (+0x550)

| offset | type | meaning |
| --- | --- | --- |
| +0x00 | s32 | rpm |
| +0x04 | s32 | rpm over the ratio: the driven wheels' speed |
| +0x08 | u8 | rolling backwards |
| +0x09 | u8 | gear, from 0 |
| +0x0c | s32 | overall ratio in use, negative in reverse |
| +0x10 | s64 | drive force |
| +0x18, +0x1c | s32 | redline, idle |
| +0x20 | s32 | forward gears |
| +0x24 | s32 | final drive |
| +0x28 | s32[6] | gear ratios |
| +0x40 | s32 | reverse ratio |
| +0x44 | s32 | peak torque, foot-pounds |
| +0x48 | u8[17] | torque curve, 255 at the peak, idle to redline |

### The tuning bytes (0x80136a18)

Read by `aero` and `car_physics`: +10 a speed in mph added to the car's for
downforce while on the ground; for the set chosen by car flag bit 7, 8 or 9,
+33/+36/+39 the front downforce and +34/+37/+40 the rear, in percent, and
+35/+38/+41 tenths added to the front grip; +51 the surface-6 drag, in percent.
Where they come from (TUNING.PRM, the difficulty?) is not yet known.

## The physics step: car_physics (0x800427a8)

1. Count the driven wheels on the ground.
2. **update_wheels (0x80040a90).** Each wheel's heading is the forward axis,
   turned by the steering angle for steering wheels (front pair one way, rear
   the other), and on the ground laid into the ground plane and renormalised.
   Then its world mount point is `R (mount - car+0x770) + position`, and for a
   grounded wheel the contact velocity `v + ω × r`. It counts +0x54b and
   +0x54c.
3. **drivetrain (0x80060138).** The road speed under the driven wheels (the
   mean contact point's velocity, in the mean ground plane, less its sideways
   part) turns them at `speed / (π d)` revolutions a second. Times 60 and the
   overall ratio, that is the rpm, in the lowest forward gear under the
   redline (reverse when rolling backwards). If no driven wheel grips, the
   engine revs to at least `redline × throttle`. Above the redline the rpm is
   held there with no drive. Otherwise the torque curve (17 points, idle to
   redline) times the peak torque, ×12 to inch-pounds, through the ratio and
   over the wheel radius, times the throttle, is the drive force. In the air
   the engine revs with the throttle and gives no drive. The rpm never falls
   below idle.
4. **aero (0x80041af0).** Above speed 1: drag `0.5 ρ v² Cd w h` (feet) against
   the velocity, at +0xfc. Front and rear downforce come from the same dynamic
   pressure, with the tuning speed bonus on the ground, each scaled by its
   coefficient and factor and, with a tuning set, by its percentage. Each
   axle's downforce is shared between its wheels.
5. **Each wheel on the ground** adds, at its contact point measured from the
   centre of gravity:
   - sideways friction, `-8 m / wheels` times the contact velocity across the
     heading (all of it, for a rear wheel under the handbrake);
   - braking along the heading, `-m g μ × pedal × share / 2`, with the share
     the bias in front and `1 - bias` behind (halved again with six wheels);
     positive when rolling backwards, where the accelerator brakes;
   - the drive force, `(drive >> 8)` shared between the driven wheels on the
     ground;
   - damping, `-k` times the contact velocity along the normal, with
     compression and rebound constants, in rebound no larger than the spring;
   - the spring along the normal.

   The part across the normal is cut to the grip, `(load - downforce) × grip
   × friction`, and the wheel marked slipping when it was. On surface 6 a drag
   of `-m v × percent × 0.4`, across the normal, is added.
6. **Each wheel in the air** adds its axle's downforce share: down the body's
   up column at its world mount while any wheel is on the ground, otherwise
   along -(+0xec), at a point set by +0x86a/+0x86b.

Every force goes into +0x1e4 and its torque about the position, `r × F` with
each product taken in 64 bits from both factors shifted up 8 and the result
down 20, into +0x1f8.

The original also copies four bytes of uninitialised stack into each heading's
padding word (+0x28), because its stack copies of the vectors never have their
fourth word written. Nothing is known to read it.

## Unknown

- What +0xec and +0x770 are, beyond their use here; what surface 6 is.
- What +0x6b4, +0x865 and +0x86a/+0x86b mean.
- Where the tuning bytes come from.
- What states 1 and 0 are for (other cars, finished cars?).
- The rest of `car_update`: integrating the sums into motion, collisions.
