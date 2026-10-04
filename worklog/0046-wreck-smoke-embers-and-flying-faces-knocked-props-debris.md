---
number: 46
title: Wreck smoke, embers and flying faces; knocked props' debris
date: 2026-10-04
area: render
files: crates/hwtr-game/src/effects.rs, crates/hwtr-game/src/math.rs, crates/hwtr-game/src/car/wreck.rs, crates/hwtr-game/src/car/mod.rs, crates/hwtr-game/src/collision/pairs.rs, crates/hwtr-game/src/race.rs, crates/hwtr-render/src/mesh.rs, crates/hwtr/src/race.rs, crates/hwtr-hle/src/original/effects.rs, crates/hwtr-hle/tests/effects.rs, crates/hwtr-hle/tests/car.rs
---

# 46. Wreck smoke, embers and flying faces; knocked props' debris

The rest of the effects that wrecks and knocks make, from the same
write-up (scratchpad particles.md, sections 3, 5 and 7). Each piece was
checked against the original function in the effects test.

**A car's wreck.** 0x8004619c calls 0x80029e10 with mode 0 from every
caller, which runs `wreck_spawn` (0x8002e574). From the car's centre (the
model's place plus its handling origin, turned) that gives:

- five smoke columns following the car (0x8003063c): ten frames from the
  effects sheet, two frames a step, the last two steps shedding grey
  smoke (0x800307d0)
- a blackened model
- a player's screen flash, from 160 down by 3 a frame
- twenty embers (0x8002d800): thrown at random, their velocity turned a
  little each frame, each leaving a spark puff every frame (0x8002d70c)
- each of up to 48 root faces of the model thrown off as a chunk
  (0x8002c5dc)

All chunks draw through one quad, so they share its colour. Every chunk
drawn takes 3 off that colour, and a chunk dies only when the shared red
reaches exactly 0. That fade is ported as it is.

**Random numbers.** The wreck draws 150 numbers, plus 15 a chunk, inside
0x8004619c, after the throw. The port takes them in `Car::wreck` itself
(`WreckDraws`) and applies them after the step, so the game's sequence
holds. The car wreck test now checks the seed as well as the car: it
matches in 600 rounds for computer cars. A player's car still diverges,
because the damage model's flying wheels (0x8007c9b0) draw first.

**Knocked props.** 0x8006b958 runs inside the pair loop (from 0x8007e000)
and always throws the prop's debris (0x8002e27c):

- smoke columns 100 units above its foot (volume flag 2)
- a ring of ten dust puffs (flag 1)
- unless flag 0x20, its object's quads as chunks lasting 180 frames

The knock takes those numbers in the same way (`PropDraws`).

**Reset and snapshots.** A reset clears every puff and spark, idles the
columns and restores the car's colour (0x80029f04). A snapshot put back
blackens a wrecked car (mode 2) or clears the others.

**Math.** New helpers:

- libgte's RotMatrixZ/Y/X on the identity
- the GTE matrix product, moved from the AI
- the effects' table square root
- matrix_to_quaternion (0x8006a5bc), decoded from the asm: the trace
  path, and the largest-diagonal path with the {1, 2, 0} successor table
  at 0x800bee60
- quat_from_axis_angle (0x8006aabc)

**Checks.** The test wrecks car 0 from its own model in the original's
memory, with 189 faces giving 48 chunks. Over 12 frames it compares:

- the wreck itself
- the update
- the chunks' draw (the shared fade)
- the columns' draw with its dust

Knocks are compared with every flag mix over the track's volumes.
Neither showed up on a scripted drive, so neither has been seen on
screen yet.

**Still unknown:** Not yet seen on screen (no wreck or knock in a scripted drive). The wreck flash is kept but not drawn (its POLY_F4's blend mode unknown). The volume record's +8 position for props that follow an object. The damage model (0x8007c9b0) still to port.
