---
number: 67
title: Estimate the unfinished computer cars' times for the standings (0x80061824)
date: 2026-10-04
area: race
files: crates/hwtr-game/src/laps.rs, crates/hwtr-game/src/race.rs, crates/hwtr-hle/tests/laps.rs, docs/engine/results.md
---

# 67. Estimate the unfinished computer cars' times for the standings (0x80061824)

When a race ends, the results now estimate times for computer cars still short of the line, as 0x80033aa0 does through 0x80061824. They also order the field the way the original does.

Before this, every unfinished computer car showed no time and scored nothing.

The estimate (`Laps::estimate`) works like this:

- **This lap.** Take the driver's race left to run (AI record +0x1a8, `Driver::progress`). Make it positive and multiply by 1000, then take away the later laps at the lap length. Hold that at 0, using an unsigned compare. Divide by the car's pace (handling +0x6c, `ai_pace`) and add it to the race clock: that is this lap's end.
- **Each later lap.** The lap length over the pace, plus `random(lap length / 20)` from the race's rand.
- **Then** the best lap is updated, and the car is finished with all its laps done.

The ordering is a selection sort with swaps, not a stable sort:

- For each place, it picks the fastest time from that place on.
- A car with no time never displaces one.
- Points 10/8/7/6/5/4 go to the first six with a time.

The port's `Race::standings` now calls the free function `race::standings`.

One finding: the lap length the estimate reads is 0x800d0e48. Only the race start (0x80061264) sets it, copying it from the best line, so it is still 0 in the saved states, which are mid-countdown. The tests poke it as the start would. If a race ended before the start, `random(0)` would hit the divide-by-zero `break`.

Verified in hle tests/laps.rs:

- `finish_estimates_match_the_original`: random laps, times, pace and progress over 3 states. 478 estimates were checked against 0x80061824: the laps record, the returned time and best, and the rand seed.
- `standings_match_the_original`: 0x80033aa0 with the snapshot and the sound's end hooked out. Every place (0x800d2688), every car's points (0x800d25f8), the laps and the rand seed match, including ties and cars with no time.

Docs: results.md gains a "The standings" section.

**Still unknown:** Who reads the fastest lap's car (0x800d0e69)
