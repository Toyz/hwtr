---
number: 59
title: Race option 0x80 keeps the headlights up
date: 2026-10-04
area: race
files: crates/hwtr-game/src/collision/create.rs, crates/hwtr-game/src/collision/world.rs, crates/hwtr-game/src/race.rs, crates/hwtr-hle/src/original/world.rs, crates/hwtr-hle/tests/laps.rs
---

# 59. Race option 0x80 keeps the headlights up

The zone a car enters (0x8005c3a4) sets the car's headlight bit (car +0x8
bit 2) from the zone's flag 0x8. It also sets it on every zone when the
race's options word has 0x80. That word (0x800d2678) is what
collision_load copies from the race setup's +0x1c: the players' cheats,
as the front end puts them there.

The port had a note that this was missing and set the bit from the zone
alone. `Collision::options` now holds the word, the race sets it from
`RaceSetup::options`, and `zone_effects` reads it. With the lights ported
(0058), the option keeps a player's headlights up for the whole race.

**Checked against the original.** `zone_effects_match_the_original`
(tests/laps.rs) now gives each of its 1800 rounds an options word of 0,
0x80, 0x7f or 0xff, in both the port and memory. The cars still match.
The world codec reads the word from 0x800d2678.

**Still unknown:** Which cheat code sets option 0x80.
