---
number: 60
title: Wreck sounds and camera shake; pause and race end silence the cars
date: 2026-10-04
area: audio
files: crates/hwtr-game/src/engines.rs, crates/hwtr-game/src/car/wreck.rs, crates/hwtr-game/src/car/mod.rs, crates/hwtr-game/src/race.rs, crates/hwtr/src/race.rs, crates/hwtr-hle/tests/hits.rs, docs/engine/race-sound.md
---

# 60. Wreck sounds and camera shake; pause and race end silence the cars

Wrecks now sound and shake as in the original, and the pause and the
race's end silence every car.

**A wreck (0x8004619c's tail).**

- **A player's car.**
  - Its camera shakes for 250 ms (0x8003bebc writes camera +0x54). Before
    this the shake was ported but never set. The shake draws random
    numbers, so the port's sequence drifted after every player wreck.
  - Its pad jolts and effect 29 plays, as before.
- **Every car, not only a player's.**
  - Its engine is let go (0x80016004).
  - Its wreck level is set full (0x80015b44).
  - Effect 29 is keyed where it is (0x80016078), on its own wreck voice
    (car sound +0x48).
- **`Engines::wrecked`.** The new call ports the three every-car steps.
- **The mixer.** It now keeps the wreck voice at the wreck level, right
  after the engine's bend. A contact that changes the scrape releases it
  (0x800161f4).
- **`crashed`.** The car's new pending mark tells the race to run all this
  for any wreck. `jolted` stays the player's.

**Silence (0x80036634, 0x800364cc).**

- **What was wrong.** The pause only keyed the engines off, and the race's
  end did nothing in the app. A looping tyre or scrape voice could go on
  through the pause, and the engines through the results.
- **The fix.** `Engines::silence` lets go each car's engine, wreck, tyres,
  scrape, crash and impact, in the original's order. The app runs it on
  Pause, before effect 14 as the original does (0x800346a8), and on
  Finish.

**Importance, made exact.**

- **Keys.** Keys through 0x8001a824 record an importance: the engines 1
  (0x80015ebc), the tyres and the scrape 0.
- **Plain let-go (0x8001a8dc).** It clears the importance to 0.
- **Releases (0x800161f4, 0x80016c0c, 0x80016404).** They give the voice
  back at 255.
- **The port.** `stop`, `key_off` and the key-ons now do the same, and
  `key_off` only lets go when the engine voice plays (the original checks
  first).

**Checked against the original** (tests/hits.rs):

- The shared record check now compares the wreck level and voice and the
  importance table. Every existing test runs with a random wreck voice
  and level.
- `a_wreck_sounds_as_in_the_original`: 2000 rounds call 0x80016004,
  0x80015b44 and 0x80016078 and compare the keys, let-gos and records.
- `the_pause_silences_as_in_the_original`: 1000 rounds of 0x80036634,
  with the world sounds and the music left out, compare every let-go and
  record.
- The contact test caught the scrape key's importance, now fixed.
- The whole workspace is green.

**Still unknown:** The world's sounds' part of the silence (0x80017208 per world sound); the music pause (0x80014a0c).
