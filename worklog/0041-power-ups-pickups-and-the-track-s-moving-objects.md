---
number: 41
title: Power-ups, pickups and the track's moving objects
date: 2026-10-04
area: race
files: crates/hwtr-game/src/powerup.rs, crates/hwtr-game/src/world_anim.rs, crates/hwtr-game/src/race.rs, crates/hwtr-game/src/collision/create.rs, crates/hwtr-game/src/collision/pairs.rs, crates/hwtr-game/src/car/load.rs, crates/hwtr-game/src/hud.rs, crates/hwtr-data/src/world.rs, crates/hwtr-render/src/mesh.rs, crates/hwtr-render/src/scene.rs, crates/hwtr/src/race.rs, crates/hwtr-hle/tests/powerup.rs, crates/hwtr-hle/tests/world_anim.rs
---

# 41. Power-ups, pickups and the track's moving objects

**Power-ups** (`powerup.rs`).

Each track's pickups (9 on DESERT1, from the WLD's 32-byte records) name a
power-up file (`<name>.PUP`, 180 bytes) or "Random". The layout of a
`.PUP`:
- name, label, seconds, flags;
- 24 factors for the handling and engine;
- a kick, the steering lock's factor, layout additions, and "never runs
  out".

Each pickup is a kind-5 collision box at its place, half its size each way,
with one point. A car whose zones meet it takes it (0x80067f98):
- it loses what it had (0x80067d50) and takes the pickup's power-up;
  "Random" picks any loaded but uncar1/uncar2;
- the HUD shows the power-up's icon (0x80062534: glyph icon+2 of font 1 at
  (314, 178));
- a player hears sound 24;
- the pickup goes, and comes back 10 s later (0x80068130) unless its name
  gives no power-up number (the unlock pickups).

A power-up runs out after its seconds (0x800679b0). Taking one (0x80065520)
on a car under full physics does the following, and losing it (0x80065ecc)
undoes it with `div_fx`:
- kicks the car (momentum += rot × kick × mass);
- multiplies the handling's fields, the engine's redline, idle and torque,
  and the steering lock;
- sets flags: strong brakes, all-terrain (4x4), steel (and skill × 2.5),
  rubber, gyro;
- Turbo adds a turbo (and shows it on the meter, 0x80065398);
- unlock flags note the track's first or second car (tables 0x800c5cd0 and
  0x800c5cdc by world and number) for player one or two;
- the springs and wheels are fitted again (0x8004528c, the wheels' state
  kept), and the body takes the new mass.

The race result now carries those unlocks (0x80067708), so a race can
unlock cars. A reset ends the car's power-ups.

In the cars' knocks:
- a steel car's object hits without moving, and a rubber car's throws the
  other off (object flags 32 and 64 around 0x8006f20c, cancelling when both
  are alike);
- a steel car wrecks any car it hits, a rubber car any computer car;
- a steel or rubber car is not wrecked by speed (0x8007e000).

hle test `powerup.rs` takes and loses each of the nine shipped power-ups on
a player's car in both the original and the port, comparing the whole car
after each. Exact.

**Moving objects** (`world_anim.rs`, 0x8007eff4 and 0x8007f17c). The WLD's
24-byte animation records (object, period ms, keys, two trigger words) and
their 28-byte keys (position and quaternion) are now parsed. Each frame
that the race is not standing still, every animation advances by the race
clock plus the time before the start. An object's pose:
- the segment `time × (keys − 1) / period`;
- the blend fraction by `div_fx`;
- position by lerp (0x8006b1a8) and orientation by slerp (0x8006ad78);
- the rotation is the quaternion's matrix transposed (0x80021508).

hle test `world_anim.rs` poses all 13 of DESERT1's animations at 8 times
each against the original's 0x8007f2a4. Exact. The pickups are animated
this way (5 keys over 3 s), and so are the UFO and the like.

**Drawing.** The app's track mesh now leaves out the pickups' and moving
objects' trees (their siblings still drawn) and draws them each frame at
their poses, a pickup only while it is out.

**Still unknown:** The pickup touch is taken after the collision step rather than inside it (0x80067f98 runs from 0x8004e938). Triggered animations (0x800d0ffc +0x14, set by special zones) never start: no special zone is ported. Boing, car01 and car02's files use an older layout, but no track places those pickups. The props (0x8006b754) and wreck debris (0x8007c894) are still not ported.
