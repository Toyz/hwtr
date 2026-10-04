---
number: 88
title: The controls into the car, checked for any stick
date: 2026-10-04
area: input
files: crates/hwtr-hle/tests/car.rs, docs/engine/controls.md
resolves: 38, 87
---

# 88. The controls into the car, checked for any stick

Since entry 17 one question had stayed open: does an analog stick reach the car the way a digital pad does? Half of it was already checked: `reads_match_the_original` (tests/pad.rs) covers the pad read for both kinds. The other half, the controls into the car (0x80034940), had no check. racecheck doesn't cover it because it copies the original's controls straight into the port's step.

**The new test.** `controls_into_the_car_match_the_original` (hle tests/car.rs) runs 400 rounds on each of three race states. Each round:

1. writes random action levels into player one's reader: nothing, 255 as a button gives, or anything between as a stick gives
2. sets random held buttons
3. sets a random speed, and in one round in ten marks the race over
4. calls 0x80034940(0)
5. compares the whole car with `PadReader::controls` followed by `Car::apply_controls`

It passed on the first run. The test requires over 500 rounds that actually steer. A stick and a pad both reach the car as levels, so the stick works like the pad. controls.md records this.

**Entry 38's unlock note is stale.** It said the race pickups that unlock cars weren't in the port, so a race's unlock words were always zero. They are in: `PowerUps::unlocked` (0x80067708) records the cars, and the app's `Race::result` packs them into the result's words for the front end. What stays open is that the unlock pickup has no hle check. That is carried here, along with 38's other questions.

**Still unknown:** what cheats 1 and 64 do (nothing in this build reads them); the car model's scale is drawn by the app, with no hle check; the unlock pickup (0x80067708) has no hle check; the standings, win and lose screens against an hle shot; whether the original saves the card after a cup; what damp_spin's 8 and 25 stand for; the car preview's NCCT lighting (0x80032ae8), the main menu's stat bars (0x8011dfb4), the menu's engine sound and who sets 0x800d10b0; the TOC entry past the last track; the card screens against hle and the card-seen flag (0x800d0f28); the pickup touch taken after the collision step; no hle check of a knock; the draw-mode byte 0x800d246c; trackside camera flags past bit 1 and the front end's setup against hle; the billboard units; the volume record's +8 position; the dialog bank's tones; cvs +0xc0; a differential check of the load-time marking
