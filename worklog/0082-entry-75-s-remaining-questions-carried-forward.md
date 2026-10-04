---
number: 82
title: Entry 75's remaining questions, carried forward
date: 2026-10-04
area: tooling
files: worklog/
resolves: 75
---

# 82. Entry 75's remaining questions, carried forward

Entry 75 carried a list of questions it had not answered. Six of them have been answered since:

| question | answered by |
| --- | --- |
| action 12's name | 79: the HUD on/off button, now ported |
| menu effects at full volume | 76: so are the original's (0x80015908 takes 4096 from 43 of 44 callers) |
| when the music volume applies | 76: on boot, after a card read, and from the options and pause menus |
| the turbo's own sound | 79: there is none; only an empty turbo sounds (effect 44) |
| the wreck flash's blend mode, and drawing it | 80: mode 0, drawn as a flat quad under the HUD |
| who sets fx_enable | 81: 0x80028b34, from the views and the scale cheats |

This entry resolves 75. The rest of its list carries here, sharpened where the work above learned more.

**The scale cheats.** I looked at them while doing fx_enable. `car_bind_state` (0x80021888) sets the model's scale in cvs +0x14:

- option 2 or 32: 2.0
- option 4: a third
- otherwise: 1.0

`car_draw` (0x80022064) and the shadow use it. The port scales the cars' handling under the cheats but draws every car at its normal size.

**Question 66 is still open.** On the grid, car 0's body sits 141,000 to 146,700 above its wheels' contacts. Its origin height is 117,590 and its ride height 20,480, which sum to 138,070; the remainder looks like spring sag. That fits the third half-axis being the drop from the body's centre to the wheels' mounts, but it is not shown, so 66 stays open.

**Still unknown:** the analog stick against the digital pad in a race; what damp_spin's 8 and 25 stand for; 0x80015d04 (master volume for car 0's flag 0x40), not ported; the car preview's NCCT lighting (0x80032ae8), the main menu's stat bars (0x8011dfb4), the menu's engine sound and who sets 0x800d10b0; the TOC entry past the last track; the card screens against hle and the card-seen flag (0x800d0f28); the pickup touch taken after the collision step; no hle check of a knock; the draw-mode byte 0x800d246c; trackside camera flags past bit 1 and the front end's setup against hle; the billboard units; the volume record's +8 position; the dialog bank's tones; cvs +0xc0; the scale cheats' drawing (car_bind_state's cvs +0x14: 2.0 under option 2 or 32, a third under 4) in car_draw and the shadow; a differential check of the load-time marking
