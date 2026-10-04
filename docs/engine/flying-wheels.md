---
title: Flying wheels (a player's wreck throws its wheels)
status: solid
discs: US
covers: US CCCPSX.EXE:0x8007c9b0 wheels_throw, 0x801323f4 flying_wheels, 0x8007c894 flying_step, 0x8004d798 object_add, 0x8004da1c object_remove, 0x8007da78 flying_remove, 0x8006dc08 contact_impulse
worklog: 65, 71
---

# Flying wheels

When a player's car wrecks (flag 1 on the car), its wheels come off. Each
becomes a rigid body of its own and a collision object of kind 6, and
bounces down the track until the car is put back on the road. Computer
cars keep their wheels.

## The table (0x801323f4)

Eight records of 680 bytes. A wreck with no free record left throws no
more wheels.

| offset | type | what |
|---|---|---|
| +0 | u8 | in use |
| +1 | u8 | the car it came from |
| +2 | u8 | which wheel |
| +4 | ... | its collision object |
| +0x20 | s32[3] | its box: half the wheel's width, half its diameter twice |
| +0x60 | s32 | its reach: half its diameter |
| +152 | body | its rigid body (see [rigid-body](rigid-body.md)) |

## Throwing (0x8007c9b0)

Called from the wreck, after the car's speed is halved. For each wheel in
order, into the first free record:

1. **The body.** It has mass 2122. Its size for the inertia is
   `[width / 2, diameter / 2, diameter / 2]`.
2. **Where.** Its rotation is the car's. Its place is the wheel's mount
   less the car's origin, turned by the car, plus the car's place.
3. **The push.** `r` runs from the car's place to the wheel in the world.
   The push is `r` made unit times `rand(20)` mph. Its height is replaced
   by `rand(30)` mph. The two draws come in that order.
4. **Its speed.** The car's speed, plus the car's spin crossed with `r`,
   plus the push. The momentum is that times the mass.
5. **Its spin.** The car's first rotation column times the wheel's spin
   rate. The angular momentum is the world inertia (`R I Rᵀ`) times that.

## In the world

At the start of the next collision step, each wheel thrown and not yet in
the world gets its object (0x8004d798):

- kind 6, with one point at its centre
- the zone of its car's point for that wheel (a computer car's only point)
- added to that zone and to the lists of everything, the moving and the
  wall-hitting

From then on the object meets the walls and the other objects as the
cars do, with these differences:

- **A ball.** Each side meets the point nearest it: the centre less the
  side's normal times the reach. The road's sides always compute the
  normal and take the reach off the distance.
- **Bouncier.** A contact gives back 1.3 of the inward speed
  (`0xd000 / 0xa000`), where other objects give back half. It uses the
  plain contact friction, and only pushes its body back.
- **Its points' thresholds** are a computer car's.

The object's +0x7c holds the last contact's normal (not its point). This
is the same for every kind.

## Each step (0x8007c894)

After the collision, each flying wheel's body is stepped 25 ms (`dt` 102
in 4.12), and its rotation squared up (0x80025be4).

## Drawn

The car's own wheel model is drawn where the flying wheel is (0x8007c8fc).
The port gives the car's pose for that wheel as the body's rotation and
place taken into the car's frame (`Rᵀ·R_wheel`, `Rᵀ·(p - P)·2` less the
root).

## In the results' snapshots

Each snapshot keeps the wheels flying: car, wheel, rotation and place
(0x8007e6e0). Putting one back fills the table from its first slot
(0x8007ec08). See [the results](results.md).

## Taken away (0x8007da78)

When the car is put back on the road, each of its flying wheels' objects
leaves its lists and its points' zones (0x8004da1c), and the record is
freed.

## In the port

`hwtr_game::flying` holds the table and throws and steps the wheels.
`Collision::flying` keeps them, and `add_flying` and `remove_flying` link
them into the world. Tests in `crates/hwtr-hle`:

- `car.rs` `wheels_fly_off_as_in_the_original` compares every record a
  wreck throws.
- `car.rs` `flying_wheels_step_as_in_the_original` compares the bodies
  after 0x8007c894.
- `collision.rs` `flying_wheels_collide_as_in_the_original` runs 1440
  collision steps with wheels flying, all matching.

## Unknown

- The rest of the 680-byte record.
