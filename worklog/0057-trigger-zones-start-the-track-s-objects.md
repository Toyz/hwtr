---
number: 57
title: Trigger zones start the track's objects
date: 2026-10-04
area: engine
files: crates/hwtr-game/src/collision/scp.rs, crates/hwtr-game/src/world_anim.rs, crates/hwtr-game/src/collision/create.rs, crates/hwtr-game/src/collision/world.rs, crates/hwtr-game/src/race.rs, crates/hwtr/src/race.rs, crates/hwtr-hle/tests/world_anim.rs, crates/hwtr-hle/tests/laps.rs, docs/formats/scp.md
---

# 57. Trigger zones start the track's objects

Trigger zones now work: the track objects they name stand still until a
car drives in, then move on by one segment.

**What was wrong.** The port ran every object animation continuously.
In the original, an animation named by a trigger (SCP table E, now
`Scp::triggers`) only runs when the trigger fires, for a set share of its
round, then rests.

**What changed.**

- `Race::set_anims` marks the triggered animations from the trigger
  sectors at load, as collision_scp_load does (0x8007f628).
- `WorldAnims::step` follows 0x8007f17c:
  - a continuous animation moves by the frame's time
  - a triggered one moves only by what it has left to run
  - every moved animation's time is kept within its round, as the pose
    update does
  - it returns which triggers' animations still run and which stopped,
    for the looped sounds
- `zone_effects` records the trigger (`Collision::triggers_hit`); it now
  takes `&mut self`.
- The race fires it after the step (`WorldAnims::fire`, 0x8006a200):
  - It starts animation A toward the trigger's key (0x8007f670), and B
    with flag 4.
  - Each started animation sounds its world sound under flags 0x28 or
    0x10, as a `RaceEvent::Knock` at the animation's place.
  - Without those flags a player's car hears effect 27, but only for
    animation A.

**Checked against the original.** `triggers_start_animations_as_the_original`
(tests/world_anim.rs) runs 400 rounds. Each builds a trigger of any flags
in RAM naming two of DESERT1's animations, which are marked triggered,
one perhaps still running. It runs the original 0x8006a200 and then six
steps of 0x8007f17c. It compares the sounds asked for (world sound and
effect 27, from hooks) and every animation's time, time left and owner
after each step.

**Gaps.**

- DESERT1 has no trigger sectors, so the load-time marking is ported
  straight from the code without a differential test.
- Looped world sounds (flag 0x10) play once; nothing stops them when
  their animation rests.

Shot runs of DESERT3, GLACIAL2 and GLACIAL3 complete without errors.

**Still unknown:** the looped trigger sounds (0x10) and their stop (0x8006a424, 0x8006a4cc); a differential check of the load-time marking
