---
number: 75
title: Closing the questions later work answered
date: 2026-10-04
area: tooling
files: worklog/, docs/engine/controls.md, docs/engine/rigid-body.md
resolves: 1, 9, 10, 11, 14, 17, 18, 19, 20, 22, 23, 26, 27, 31, 33, 34, 36, 37, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 50, 51, 53, 54, 55, 56, 57, 60, 64, 65, 68, 69
---

# 75. Closing the questions later work answered

The site counted 70 of 74 entries as open. Most of those questions had been answered by later work, but never closed. No entry used `--resolves`, and a few wrote "None new" or "None for this change" where cairns wants the literal `nothing`.

I went through `cairns open` entry by entry and checked each question against the code and the reference (grepping for every address it named). Where it was answered, this entry resolves it, citing what answered it. Whatever part is still truly unknown is carried into this entry's own **Still unknown**, so nothing is lost by closing. Questions with no answer yet (2, 3, 4, 5, 6, 7, 12, 15, 16, 28, 30, 32, 35, 38, 52, 58, 59, 62, 63, 66, 67, 71, 72, 73) stay open where they are.

Entries 24, 25, 49, 61, 70 and 74 meant "nothing new" and now say `nothing`.

| entry | answered by | carried forward |
| --- | --- | --- |
| 1 | 35: SLUS_009.64 plays the movies, shows PSXLEGAL then PSXRFA1, then runs CCCPSX.EXE | |
| 9 | TIMs carry their own VRAM rectangle; 32: each sample's pitch comes from the note keyed | |
| 10 | 11 (the controls module reads libpad), 29 and 63 (vibration and jolts) | |
| 11 | 11 and controls.md: the curves (dead zone 10), the action table; 63: the vibration path | action 12 (Triangle)'s name; this entry names 9 to 11 in controls.md |
| 14 | 15: x, y, z, w | |
| 17 | 32 (sound) | analog stick against the digital pad in a race, unchecked |
| 18 | car-object.md: +0xec gravity's direction, +0x770 the origin; computer cars run their driver (ai.rs); the hook-cost question went with the emulator (25) | |
| 19 | car-object.md: +0x6b4 handling flags, +0x86a/+0x86b air control, surface 6's drag; the tuning bytes are TUNINGPRM | |
| 20 | 41/42 (0x8006b754, the moving volumes), 65 (0x8007c894, flying wheels), car-object.md (air control 0x8003d71c) | |
| 22 | recovery (race.rs `reset_car`, 0x80041384) and the player's effects (0x8003cb74, 48 and 50) ported | what damp_spin's 8 and 25 stand for |
| 23 | 68 (start placement, driver setup); the object (0x8004c478) and zone flags in the port; handling.rs and car-object.md (+0x04 steering lock, +0x6c the computer's pace, wheel +0x10 steering angle) | |
| 26 | 43 (results, snapshots), hud.rs (countdown 0x8001e060, power-up icon 0x80062534), 50 (stunts), 44 (computer cars), race_over (time limit), laps.rs `watch_way` (0x8005c9b4) | |
| 27 | 66, 41/42, 46, 65, 67, 48 and 63 | |
| 31 | 44: 0x800d2608 is the attract race's flag | |
| 33 | 48, 51, 60, 62 | 0x80015d04 (master volume for car 0's flag 0x40), not ported |
| 34 | 37 (garage previews and bars), 38 (cup screen 0x8008e310), 0x8009a89c ported | the preview's NCCT lighting (0x80032ae8); the main menu's stat bars (0x8011dfb4); the menu's engine sound; who sets 0x800d10b0 |
| 36 | 37 (the options screen's music lines) | the TOC entry past the last track; when the music volume is applied |
| 37 | 40 (controls screen, 0x8009495c) | menu effects still play at full volume |
| 39 | 44 (attract race), 40 (card manager); the front end now reports no unported actions | |
| 40 | 44 (attract race) | card screens can't be checked against hle (no card there); the 'card seen' flag (0x800d0f28) |
| 41 | 57 (triggered animations), 41/42 (moving volumes), 65 (flying wheels) | the pickup touch is taken after the collision step, not inside it |
| 42 | 46 (the knocked prop's debris), 62 (its sound at a spot) | no hle check of a knock |
| 43 | 71 (flying wheels in snapshots), 70 (0x800d0e54), 56 (a put-back wreck's look) | what the draw-mode byte 0x800d246c changes |
| 44 | 69 (0x800836e8) | trackside camera flags past bit 1; the front end's setup against hle |
| 45 | 46 (debris, embers, smoke), 47 (boost flame), 45 (puffs) | the wreck's screen flash is kept but not drawn; the billboard units; who sets fx_enable (0x800d0d98) |
| 46 | 65 (seen on screen; 0x8007c9b0) | the flash's blend mode; the volume record's +8 position |
| 47 | car-wheels.md: cvs +0x18 is twice the last wheel's lift | the turbo's own sound |
| 48 | 60 (+0x48), 62 (world sounds, VAB 2) | the two-player mixer (as 62); prop velocity (as 63) |
| 50 | 51 (commentary and the ten-turbo line) | |
| 51 | 62 (VAB 2) | what the dialog bank's tones say |
| 53 | 57 (trigger zones) | |
| 54 | car-lights.md: +0xbc glow strength, +0xc4 light targets | cvs +0xc0 |
| 55 | 58 (+0x130, the light colours) | the model scale cheats (options 2 and 4) |
| 56 | 58 | |
| 57 | 62 (looped trigger sounds, their follow and stop) | a differential check of the load-time marking |
| 60 | 62 (the world's records silenced); 0x80014a0c ported | |
| 64 | 73 (racecheck's matrix over all tracks includes wrecks) | |
| 65 | 71 (the snapshot records) | the rest of the 680-byte record (as 71) |
| 68 | 73 (all 11 tracks) | |
| 69 | 70 | |

I also fixed two stale pages found on the way:

- rigid-body.md still called 0x8006b754 and 0x8007c894 unidentified.
- controls.md now names actions 9 (reset), 10 (turbo) and 11 (change view).

**Still unknown:** action 12's name; analog stick against the digital pad in a race; what damp_spin's 8 and 25 stand for; 0x80015d04 (master volume, car 0's flag 0x40); the car preview's NCCT lighting (0x80032ae8), the main menu's stat bars (0x8011dfb4), the menu's engine sound and who sets 0x800d10b0; the TOC entry past the last track and when the music volume applies; menu effects at full volume; card screens against hle and the card-seen flag (0x800d0f28); the pickup touch taken after the collision step; no hle check of a knock; the draw-mode byte 0x800d246c; trackside camera flags past bit 1 and the front end's setup against hle; the wreck flash (not drawn, blend mode unknown), the billboard units and who sets fx_enable; the volume record's +8 position; the turbo's own sound; the dialog bank's tones; cvs +0xc0; the model scale cheats; a differential check of the load-time marking
