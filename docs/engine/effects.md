---
title: The race's effects: puffs, skid marks, sparks
status: partial
discs: US
covers: US CCCPSX.EXE:0x8002bdb4 effects_init, 0x8002c0dc pool_init, 0x8002c2b8 pool_alloc, 0x8002f354 particles_update, 0x8002f618 pool_draw, 0x8002b888 trails_emit, 0x80029230 trail_push, 0x800303cc skid_segment_spawn, 0x80030fc8 dust_puff_spawn, 0x8003119c spark_puff_spawn, 0x8002e9f8 collision_sparks, 0x8002c32c spark_spawn, 0x80049ecc car_pose, 0x80068540 view_setup, 0x800d25c0 0x800d25c8 0x800d25d0 0x800d25d8 pools, 0x8011ec2c trails, 0x800d0d98 fx_enable, 0x800d2578 fps, 0x800d0b68 frame_count, 0x8002e128 flash_draw
worklog: 45, 46, 47, 56, 61, 80
---

# The race's effects

Dust, smoke, skid marks, sparks and debris live in four rings of 56-byte
records. They are set up by `effects_init` (0x8002bdb4) when a race starts.
The world draw runs them once for each drawn view:

1. `particles_update` (0x8002f354)
2. `trails_emit` (0x8002b888)
3. `pool_draw` (0x8002f618) for each ring
4. the smoke columns (0x800307d0)

After the frame is drawn, `race_frame` calls 0x80068874. Its car pose
(0x80049ecc) reports each wheel's trail point for the next frame.

All of this runs per drawn frame, not per 25 ms step. The original runs
at one frame a vertical blank: `fps` at 0x800d2578 is
`FDIV(0x3e8000, 17 << 12) + 2048`, about 59.3. The random numbers the
spawners draw come from the game's single generator.

## Rings

| ring | header | records | capacity | side buffers | holds |
| --- | --- | --- | --- | --- | --- |
| puffs | 0x800d25c0 | 0x8011fb3c | 64 | +0x1c vel 0x80120c3c, +0x20 accel 0x8012093c | dust, grey smoke, ember sparks |
| skids | 0x800d25d0 | 0x80120f3c | 64 | +0x00 corners 0x80121d3c (48 bytes) | skid marks |
| sparks | 0x800d25d8 | 0x8012293c | 20 | +0x1c vel 0x80122d9c | collision sparks |
| debris | 0x800d25c8 | 0x80122e8c | 48 | +4, +8, +0xc, vel, accel | tumbling chunks |

- **Header:** u8 capacity, u8 oldest, u8 next, then the records pointer.
- **Record fields:**
  - +0x10 position (20.12)
  - +0x24 flags
  - +0x26 frames left
  - +0x2c r, g, b (u16 each)
  - +0x32 kind
  - +0x33 sprite growth
- **`pool_alloc`:** returns the record at `next` without clearing it. When
  the ring is full it pushes `oldest` forward, overwriting the oldest
  record.
- **`particles_update`, per ring from oldest to next:**
  - A live record loses a frame, unless the pause byte 0x800d261c is 1.
  - It then moves: position += velocity, velocity += acceleration.
  - Its sprite grows by 7 every other pair of frames (kinds 0 and 19), or
    by 4 (kind 1).
  - A dead record at the old end moves `oldest` on.
  - A dead record elsewhere is overwritten by the record before it, all 56
    bytes including the side-buffer pointers, and that earlier record is
    marked dead. Two records can then share one set of buffers, so a later
    allocation of the earlier slot writes into a live record's velocity.
    The port keeps this aliasing as a buffer index on each record.
- **Frame count:** 0x800d0b68 counts drawn frames that are not paused
  (0x80014d64).

## Trails and skid marks

Each frame, the car pose (0x80049ecc) looks at every car within 600 units
of a camera on every axis. A wrecked car (cvs +0x28 = 1) is skipped: the
trail point is pushed from inside the wheel pose (0x80020a14), which
leaves a wreck's wheels alone. For each of that car's wheels it computes a
side vector, `cross(heading, normal)`:

```
x = h.y*n.z - n.y*h.z,  y = n.x*h.z - h.x*n.z,  z = h.x*n.y - n.x*h.y
```

- **Skidding:** the wheel is skidding when it is slipping (+0x78) and on
  the ground (+0x3c), and the car was not just reset (+0x928, which the
  reset sets and every pose clears).
- **Trail point:** while skidding, the point is the contact (+0x50) moved
  by side x half width (+0x10, the six words before the diameters in
  handling block A). It moves out on wheels 0, 2 and 4, and in on the
  others.
- **`trail_push` (0x80029230):** called unless the ground is kind 10.
  Arguments are car, wheel, skidding, the ground's kind (+0x3d), the
  contact as the outer edge, and the point as the inner edge.

`trail_push` keeps, per car and wheel (0x8011ec30 + 384*car + 64*wheel):

- the last committed pair of edge points and the new pair
- a state: 0 none, 1 one pair, 2 a segment ready (0x8011f530)
- a build-up counter up to 80 (0x8011f554)
- the ground's kind (0x8011f578)

When a wheel is not skidding, its state goes to 0. When the last wheel of
a frame reports no skid, the skid origin is dropped. A push sets the
trail tick (0x8011ec2c) from 1 to 2. The world draw counts it back down to
1 each frame.

With the tick at 2, `trails_emit` goes through every ready wheel. A car
that is not shown skips its wheels 0 and 1. For each ready wheel it:

- makes a skid mark from the four edge points (0x800303cc)
- commits the new pair and sets the state to 1
- puffs dust 10 units above the inner edge (0x80030fc8)

Skid marks keep their corners relative to an origin (0x80127acc), set
from the first skid after it was dropped. A skid mark:

- lasts 0xffff frames, so it stays until its slot is reused
- gets flags 6 (0x806 on ground kinds 11 and 12, drawn additive instead
  of subtractive)
- gets its colour from 0x800be834 by 0x800be864[kind]

## Puffs

`dust_puff_spawn` (0x80030fc8) needs fx_enable bit 0 and `fps > 0x15fff`.
It always draws two `rand()` values before allocating. The new puff has:

- velocity `(r1&3, r2&3, r1&1) << 12` (or the vector it is given)
- acceleration `(0, 0, 10)`
- flags 0x1001, kind 0, 750 frames
- colour from 0x800be844 by 0x800be854[kind]

On ground of kind 0 the puff is grey smoke instead: kind 1, colour 80,
250 frames, flag 0x100.

The draw fades a puff's colour each frame, and kills it when any channel
reaches 0 or below. A local `fade` decides the amount:

- 21 below 22 fps
- otherwise 7, except kind 1, which fades 3 on frames where
  `frame_count % 3 == 0` and otherwise keeps the last value used in this
  call

A puff is a camera-facing quad from a template (kind 0 0x800be774, 1
0x800be794, 16 0x800be754, 19 0x800be7b4). Its growth is added to v0.x,
v2.y, v3.x and v3.y, so it grows away from one corner. The texture comes
from the effects sheet entry of the same number: kind 0 adds a quarter,
kind 1 subtracts.

## Sparks

In `collision_update`, after a contact's impulse, a player's car's first
contact of the step calls `collision_sparks` (0x8002e9f8) unless the
contact's kind is 1. If the body moves faster than 100 units/s, the spark
is set up as follows.

**Velocity:**

- the body's velocity less its part along the normal
- plus 10 times the normal's x and y, and 60 times its z
- divided by -15 (`FDIV(v, 0xffff1000)`)
- plus 1.0 up

**Position:**

- Find the face of the car's box that the normal lines up with: the
  component of the normal on a body axis is at least 3687 in size.
- On that face, the axis gets the half extent (negative if the component
  is positive). The other two axes get `((rand()%2000 << 12)/1000 - 4096)`
  times their half extent.
- Turn the result into world space: FX sums for the side and front faces,
  ApplyMatrixLV for top and bottom.
- If no face lines up, use the contact point.
- Then jitter x and y by `((rand()%10000 << 12)/1000 - 20480)`
  (0x8002c32c).

The spark flies straight for 90 frames and is drawn as a two-unit-wide
streak from its position to its position plus its velocity.

## Car visibility

The view set-up (0x80068540) hides a car in two cases:

- the view's camera rides inside that car
- the car is in its post-reset grace (+0x924) and
  `(grace * 0x51eb851f >> 37) & 1` is set, which blinks it every 100 ms

It does this through the cvs byte +0x1ef (iface_general+220).

## Wrecks, knocks and the boost flame

- **A wreck (0x80029e10 mode 0, from every caller of 0x8004619c):** stops
  the boost flame and marks the car wrecked (cvs +0x28). Then
  `wreck_spawn` (0x8002e574) works from the car's centre, which is the
  model's place plus its handling origin, turned:
  - five smoke columns that follow the car (0x8003063c), each drawing two
    random numbers
  - a blackened model (root colour 0x181818)
  - a player's screen flash, from 160 down by 3 a frame until under 130
    (0x8002e128): drawn first each frame in the grey it was, as a flat
    semi-transparent quad (POLY_F4, code 0x2a) over the whole 384 x 240
    screen (with two players each view its own, 240 / 2 - 1 high), at
    ordering-table slot 4, under the HUD. Its blend is the page the frame
    leaves set: mode 0, half and half (read from the hle GPU's page as the
    quad drew, every frame of a wreck's flash)
  - twenty embers (0x8002d800), seven random numbers each, then shedding
    a spark puff every frame (0x8002d70c)
  - up to 48 of the model's root faces as chunks: each gets 15 random
    numbers (0x8002c5dc) and flies with an eighth of the car's velocity
    and gravity -2 a frame squared
- **Smoke columns (0x800307d0):** start three frames apart and run for
  20 frames, stepping every second frame through sheet frames 5..14. The
  quad grows 4 units a step from 188. They shed grey smoke in their last
  two steps.
- **Chunks:** all draw through one quad, so they share its colour. Each
  chunk drawn takes 3 off it, and a chunk dies when the shared red is
  exactly 0 at its turn.
- **A knock (0x8006b958 in the pair loop) always throws the prop's debris
  (0x8002e27c):**
  - flag 2: smoke columns 100 units above its foot
  - flag 1: ten dust puffs around its foot
  - unless flag 0x20: its object's quads as chunks lasting 180 frames
- **A turbo lights the boost flame (0x8002af60)** on a player's car that
  is shown. It burns for 97 frames, drawn by `car_draw` (0x8002b05c):
  - on each side, a body quad and a tip quad behind the car in car space
  - placed from the handling's size, rear mounts, tyre width and diameter
    and the exhaust table 0x800be088
  - each quad's length takes a random factor, `rand()%1375`, every frame
  - colour 112 less the frames shown, sheet entry 15 (additive)
  - the quads drawn from the tyre diameter raised by cvs +0x18: twice the
    lift of the car's last posed wheel (see [the car's wheels](car-wheels.md))
  - meanwhile the car's root colour pulses from 128 to 248 and back, a
    step every 20 ms of the system clock (0x8002bc04)
  - it goes out early (0x8002aff4, through iface_general+0xd8) when
    `cars_update` finds the car's boost over: the car slowed to more than
    30 mph below the boost's speed. Putting it out stops the pulse (grey
    again unless wrecked) and clears the effects' wreck mark. The results'
    snapshots put every car's flame out the same way.
- **A reset (0x80029f04):** clears every puff and spark, idles the
  columns and restores a wrecked car's colour.

The wreck's and the knock's random numbers are drawn where the original
draws them: in `Car::wreck` and in the knock, as `WreckDraws` and
`PropDraws`. The effects use them after the step.

## The cars drawn, and a wreck's smoke

For each view, the world draw (0x8001ef24) draws the cars before
anything else on this page.

- **Which cars.** Slots 0 to 5 in order. A car is drawn if it is shown
  and the squared distance from the view's eye to its model is under
  `5400² + 2²⁴`: about 6778 units, with 5400 returned by 0x800125f0. The
  model is placed where car_set_pose left it at the last frame's end.
- **The car draw (0x80022064).** For a player's car (cvs +0x1ee) it
  first measures how far the model moved since its last draw (cvs +0x1c,
  from cvs +0x0). Then it draws:
  - the shadow
  - the exhaust glows
  - the headlights, and the body's and headlights' fades
  - the boost flame
  - the tail lights' palette
  - the wheels
  - a wreck's smoke

  It ends by keeping the model's place in cvs +0x0. The glows draw a
  random number for each glow with strength (above half revs) unless
  paused; [the car's lights](car-lights.md) has them and the rest.
- **The model.** The distance picks the model: the full model (with its
  wheels) to 1350 units, then the medium model to 2700, then the low.
- **A wreck's smoke.** With the full model, a wrecked car (cvs +0x28 =
  1) not in the frozen results (draw mode 3) puffs grey smoke (0x80030fc8
  at frame 10, ground kind 0) from each wheel's mount, last wheel first:
  - on every frame that is not a multiple of four, when the model moved
    10 units or more
  - otherwise on every seventh frame, from wheels 0 and 3 only

  Each puff draws two random numbers, so a wreck changes the game's
  sequence every few frames.

## Unknown

- The camera-space units of the billboard translation: the draw adds the
  record's acceleration x and y to it.
- Who sets fx_enable (0x800d0d98); it is 0x1ff in every race seen.
