---
number: 42
title: The track's collision volumes: obstacles, bumps and knocked fences
date: 2026-10-04
area: race
files: crates/hwtr-data/src/world.rs, crates/hwtr-game/src/collision/create.rs, crates/hwtr-game/src/collision/pairs.rs, crates/hwtr-game/src/collision/object.rs, crates/hwtr-game/src/race.rs, crates/hwtr-render/src/scene.rs, crates/hwtr/src/race.rs
---

# 42. The track's collision volumes: obstacles, bumps and knocked fences

The port's race had no world collision objects: cars drove through signs,
fences and rocks. The WLD's 84-byte volume records (the table at +48/+52)
are now parsed:
- flags; the object drawn for it (0xffffffff for none);
- centre (20.12), size and rotation;
- weight;
- two words, of which the first byte is the knock's sound.

Each record becomes a collision object (0x8006b2a8 through 0x8004cedc and
0x8004d798):
- kind 2 if it follows its object (flag 8), else 0; no track has a body of
  its own (kind 1, flag 16);
- object flags: knockable (1 → 2), lifts (2 → 4), wrecks (64 → 8);
- a box of half its size, one point at its centre, in that point's zone;
- the following ones are among the moving objects, and each step take
  their objects' animated pose (0x8006b754).

DESERT1 has 17 volumes: fences (flag 5, sound 12), bumps (flag 6, weight
100, sound 11), and plain boxes.

In the crash check (0x8007e000), a knockable volume lighter than 10000, or
hit by an all-terrain car, is knocked over (0x8006b958). Its object leaves
the collision (flag 1) and stops being drawn (0x80020824 clears its visible
bit), and with flag 4 its sound plays. A heavy one stands like a wall. A
bump lifts a car of low skill that is not steel (momentum z += weight ×
264/100 × mass) and is knocked, and the speed check still applies. The app
draws knockable objects apart so a knocked one can go.

In a shot, the player's car on DESERT1 now glances off the sign it used to
drive into and carries on down the track.

**Still unknown:** A knocked object's flight (0x8002e27c through the particle pool) is not drawn: the object just goes. The knock's sound goes through 0x80036270, a positional player whose table this does not use; the port plays the record's first byte as an effect, unheard so far. No hle check of a knock yet.
