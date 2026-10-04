---
number: 38
title: Race results, unlocks and the cups
date: 2026-10-04
area: ui
files: crates/hwtr-game/src/front/results.rs, crates/hwtr-game/src/front/unlocks.rs, crates/hwtr-game/src/front/cup.rs, crates/hwtr-game/src/front/mod.rs, crates/hwtr-game/src/front/garage.rs, crates/hwtr/src/front.rs, crates/hwtr/src/main.rs, crates/hwtr/src/race.rs, docs/formats/car.md
resolves: 13
---

# 38. Race results, unlocks and the cups

A race now hands the front end a `RaceResult`, mirroring the original's
292-byte record at 0x80138d14. It holds how the race ended, each car's time,
best lap, laps, points and stunt score, and the six unlock words. 0x8009b0a4
(`take_result`) writes the records the race set: stunt scores when the race
counts them, and best laps for players who ran every lap, through the
five-line insert of 0x8009aadc / 0x8009ac64 / 0x8009adec. It then ORs the
unlocks into the profiles and notes which ones are new.

The unlock announcer (states 156, 344 to 348; screen 29, `psxunlk`) shows
each new item in turn under "<player> has unlocked": a car turning, a
track's map, or a cup. It moves on after five seconds or on Start.
0x80099ab8 is the cup prize. It puts the winner's points into the cup's
table and opens what the cup gives:
- Hot Wheels Cup: the Secret Car Cup, cars 0, 7, 21 and 33, tracks 7 to 9.
- Secret Car Cup: the TwinMill Cup, cars 38 and 39, tracks 10 and 11.
- TwinMill Cup: every car.

All of the cup is ported: the cup screen (62, 114 to 142), the confirm
dialog, race setup (0x8009c44c, with the TwinMill Cup's grid 2 and best line
"tcup"), the standings (145 to 149), and the win, lose and unlock check
(150 to 157). Tracks come from 0x800c2560. In the example run, the Hot
Wheels Cup goes Desert 1, Glacial 2, Desert 3, Glacial 1, Glacial 3,
Desert 2, as the table says. The port app runs the whole cup to the
announcer.

Car pictures: worklog 0037 said the decal drawn while a model loads never
applies. That was wrong. The cup screen loads the model only on the CAR
line (0x8008e11c) and draws the car's picture from DECALS.BMF everywhere
else (0x8008e310 → 0x8011dfb4 at (191, 61)), as the original's shot shows.
The front end now emits `DecalDraw`s wherever the original calls 0x80023660:
the main menu, the cup, the garage and the announcer. The app unpacks every
decal at load and copies one into VRAM slot (640, 256 + 64 × slot) when it
is drawn. Checked against the hle shot of the cup screen. DECALS.BMF has no
KYLE/JETHREAT swap: part 10 is Kyle Petty's car and part 11 the Jet Threat.

To shoot the screens that come after a race, the app takes a shot-only
`--race-frames N`. Each race ends after N frames as if it were run in full,
with the cars placed in slot order. With `RUST_LOG=hwtr=debug` the app logs
each front state change with the clock.

**Still unknown:** The standings, win and lose screens are checked against the decompiled code and port shots only, not an hle shot (the original would have to finish six races). Whether the original saves the card after a cup. Race pickups that unlock cars (uncar1/2.pup) are not in the port's race yet, so a race's unlock words are always zero.
