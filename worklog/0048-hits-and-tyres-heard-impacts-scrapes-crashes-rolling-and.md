---
number: 48
title: Hits and tyres heard: impacts, scrapes, crashes, rolling and skidding
date: 2026-10-04
area: audio
files: crates/hwtr-game/src/engines.rs, crates/hwtr-game/src/collision/world.rs, crates/hwtr-game/src/collision/walls.rs, crates/hwtr-game/src/collision/pairs.rs, crates/hwtr-game/src/race.rs, crates/hwtr/src/race.rs, crates/hwtr-hle/src/original/world.rs, crates/hwtr-hle/tests/hits.rs, docs/engine/race-sound.md
---

# 48. Hits and tyres heard: impacts, scrapes, crashes, rolling and skidding

Cars now sound their hits and their tyres, as the original does.

**Contacts with the track (0x80035a88).**

- At a car's first contact of a collision step, the collision records a
  `Hit::Track`.
- Its volume is the car's mph (20 to 100) over 100.
- The engines' sound keeps each car's scrape effect and its 200 ms
  timer:
  - With the timer at 0, a contact strikes the surface's impact
    (0x80016280) and keys its scrape (0x80016820).
  - With the timer running, only a change of scrape keys anything.
- 0x800be8a4 gives the effect pair for each surface.

**Crashes (0x80035c7c and 0x80016a18).**

- These are the first thing the pair loop does, before the wreck checks.
- Over 10, 20 and 50 mph of relative speed they give kinds 2, 3 and 4.
- The tone comes from the game's generator (`rand()%4`, `%2+4`, `%2+6`)
  at the hit, so the race's random sequence stays as the original's.
- They play from the crashes bank on an importance-1 voice.

**The crashes bank.**

- `race_load`'s sound set-up (0x8001924c) draws the crashes bank
  (`rand()%4` until it is not 1, then +1) and the dialog bank
  (`rand()%12+1`).
- This happens before `fakeai_load`, so `Race::new` now draws both first.

**Tyres (0x800354ac and 0x80045a84).**

- The input reports three things:
  - the highest-ranked ground under the wheels that are down
    (0x800bea6c)
  - the share of wheels down, times speed over 100 mph
  - the share skidding
- A player's tyres are keyed on their own voice (22 or 23) from
  0x800be888: the roll effect, or the skid effect while skidding is at
  least the roll.
- Effects 16 and 23 are muffled to a fifth and three fifths.

**Mixer and voices.**

- The one-player mixer now sets the volume of the tyre, scrape, crash
  and impact voices.
- A car with only those voices playing is no longer skipped.
- Voice allocation (0x80019abc) moved into `Engines` so the hits and the
  app's effects share it.
- `Change::KeyOn` now names its bank: a car's engine, the effects or the
  crashes.
- The race's sound is shut at the race's end (0x800364cc), so no hit is
  heard or draws after that. The codec reads the flag at 0x800d2623.

**Verified.** crates/hwtr-hle/tests/hits.rs runs against the original
with the sound chip stood in for:

- 3000 contacts: keys, key-offs, scrape, timer, levels and voices
- 3000 pairs: random seed, keys and the crash voice
- 2000 tyre keys
- 2000 tyre inputs
- 500 mixer passes over the hits' voices

A shot run of DESERT1 shows wall scrapes and prop crashes firing. Shots
have no audio, so playback is untested beyond libsnd's key-on arguments.

**Still unknown:** voice +0x48 (0x80016078); the world's own sounds (0x8011aab0); the two-player mixer for the hits' voices; VAB 2; whether props give the pair sound a velocity
