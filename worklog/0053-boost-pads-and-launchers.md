---
number: 53
title: Boost pads and launchers
date: 2026-10-04
area: physics
files: crates/hwtr-game/src/car/update.rs, crates/hwtr-game/src/collision/create.rs, crates/hwtr-game/src/collision/world.rs, crates/hwtr-game/src/race.rs, crates/hwtr-hle/tests/laps.rs, docs/formats/scp.md
---

# 53. Boost pads and launchers

Boost pads and launchers now work: the special sectors (flag 0x4000)
that `zone_effects` had only logged.

**Boost pad (0x4800), 0x80049a8c, `Car::boost_pad`.**

- Faster than 2 mph and more than 5 short of 130, the car's momentum is
  scaled to 130 mph.
- A car not already boosting starts a boost: the turbo sound for a
  player, and the flame.
- Counts: 95 pads on GLACIAL1, 60 on GLACIAL2, 102 on GLACIAL3 and 282
  on VOLCANO3.

**Launcher (0x4000 alone), 0x80048fa4, `Car::launch`.**

- The sector's group is a speed in mph (0 for 130) and its byte +4 a
  heading in 256ths of a turn.
- A car moving its way is thrown along it at that speed.
- With every wheel down and the car facing along it, the car is also
  turned level onto it.
- A player hears effect 45, and the car is boosted.

`zone_effects` and `move_to_zone` now take the tables, for the
launcher's sin and cos. The trigger sectors (flag 2, 0x8006a200, world
animations with sounds) are still only logged; the earlier code had
called them power-ups.

**Verified.** `boost_pads_and_launchers_match_the_original` in
crates/hwtr-hle/tests/laps.rs runs 800 rounds per state, half on
DESERT1's own launchers and half on plain sectors marked 0x4800 in RAM.
The car drives at random speeds and headings, sometimes already
boosting, with all or some wheels down. The whole car after
`zone_effects` is compared against 0x8005c3a4. More than 200 launches
and more than 200 boosts took effect.

A VOLCANO3 shot run fires a pad's turbo sound.

**Still unknown:** the trigger sectors (flag 2, 0x8006a200)
