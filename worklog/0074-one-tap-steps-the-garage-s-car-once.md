---
number: 74
title: One tap steps the garage's car once
date: 2026-10-04
area: ui
files: crates/hwtr-game/src/front/mod.rs, crates/hwtr-game/src/front/garage.rs, crates/hwtr-game/src/front/cup.rs
---

# 74. One tap steps the garage's car once

Reported in play: one tap of up in the garage stepped the car two or three times.

Reproduced with the front-end example: one 4-frame up press in the garage stepped at frames 1000 and 1003.

**Cause.** When a car's model arrives, the original clears pad 0's held-step delay (0x8008b9d0, called from the garage draw 0x80091cf0; the main menu's car row and the cup screen do the same). In the original that is harmless: it reads the car from the CD only a second and a half after the last step, so the button is always up by then. The port has every model in memory and shows the new car within a frame or two, while the tap is still held. With the delay back at 0, the next blank stepped again.

**Fix.** `Front::car_arrived` clears the delay only when the pad is not stepping, i.e. with the button up. Letting go has already cleared it then, as in `held_repeat`. Used in the garage, the main menu and the cup screen.

**Checked:**

- One tap now steps once.
- Held, the car still steps at 0 ms, then after 500 ms, then every 250 ms (frames 1000, 1030, 1045, 1060...), as 0x8008ba30 does.

**Still unknown:** nothing
