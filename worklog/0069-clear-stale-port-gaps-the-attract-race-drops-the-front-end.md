---
number: 69
title: Clear stale port gaps; the attract race drops the front end's picture
date: 2026-10-04
area: engine
files: crates/hwtr-game/src/race.rs, crates/hwtr-game/src/collision/pairs.rs, crates/hwtr-game/src/snapshot.rs, crates/hwtr-game/src/front/race_start.rs
---

# 69. Clear stale port gaps; the attract race drops the front end's picture

Went through the remaining "not yet ported" notes. Three of them had been ported already, and one small gap was real.

Stale notes, now corrected:

- The knocked props' debris (0x8002e27c) is already thrown at race.rs's knock handling. The stage that steps flying wheels (0x8007c894) is in since worklog 65. The trace is removed.
- Props in the crash check (0x8007e000): flag 2 knocks a prop over and flag 4 bumps the car. Both already go through the port's knock (0x8006b958), which marks the object, records the volume and draws the debris numbers, and the race plays its sound at a spot. The doc comment now says so.
- Snapshot put-back: the wreck's charred or cleared look (0x80029e10 / 0x80029f04) is set by the race's results code after each put-back, so the trace in `put_back` was stale.

Real gap, now fixed:

- The attract race's setup (0x8009b254) first calls 0x800836e8. That calls through 0x8011dfc0, which a menu-state peek shows is 0x80015648, the background picture loader. Given no name, it clears the picture flag 0x800d2401. The port now clears `Front::background` there, as it already does for the password screen (0x800836c0 with 0, the same loader).

A menu-to-race smoke run (shot mode, 4000 frames, cross pressed every second, races cut at 600 frames) runs the attract race, then several DESERT1 races and their results, without errors.

Still open: the points table lowering 0x800d0e54, camera modes other than 0 to 4, and object kinds other than those listed.

**Still unknown:** The points table lowering (0x800d0e54)
