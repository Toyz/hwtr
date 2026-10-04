---
number: 85
title: Button codes and the cheats they give
date: 2026-10-04
area: ui
files: crates/hwtr-game/src/front/mod.rs, crates/hwtr-game/src/effects.rs, crates/hwtr-game/src/race.rs, crates/hwtr-game/src/car/load.rs, crates/hwtr-hle/src/original/effects.rs, crates/hwtr-hle/tests/password.rs, crates/hwtr-hle/tests/effects.rs, docs/engine/cheats.md, docs/engine/effects.md
---

# 85. Button codes and the cheats they give

The cheats had no way in. The port stored a profile's cheat word, carried it into the race and saved it to the card, but nothing ever set it. The main menu gathered code buttons and never checked them.

**How the original takes a code.** On the main menu each pad keeps its last eight code buttons as digits: Square 1, Triangle 2, L1 3, R1 4, L2 5, R2 6. After any press (0x8008d010, 0x8008d14c), 0x8007f87c checks them for that pad's player:

1. **Cheat codes** (0x8007f710). Ten slots, one per bit, from 0x800bee6c. The first match sets the profile's cheats to that bit alone.
2. **The car code** (0x8007f79c). 12345612 gives the car `towjam`.

A match chimes (effect 52) and fills the buffer with spaces.

**The codes.** Five slots (0, 1 and 7 to 9) hold 77777777, so bits 2, 128, 256 and 512 can't be had. The reachable ones:

| code | cheat |
| --- | --- |
| 77777777 | 1 |
| 16522561 | 4: small cars |
| 34563456 | 8: flat textures |
| 64561234 | 16: DUDE sounds |
| 12124455 | 32: big wheels |
| 63124536 | 64 |

**In the port:** `Tables::apply_code` is 0x8007f87c, and the menu runs it after each press. Entering 16522561 in the app's front end logs "player 1: code 16522561 taken, cheats 0x4".

**Checked:**

- `codes_are_taken_as_the_original_takes_them`: 400 codes against random profiles, comparing the answer, cars and cheats with 0x8007f87c.
- `a_code_pressed_on_the_main_menu_is_taken`: presses the small-car code frame by frame on the original's main menu from `menu-main`, and checks player one's cheats and the cleared buffer. (hwtr-hle's `--peek` seems to read before the frames run. It showed nothing until I drove the frames from a test.)

**On the way:**

- **The wheels constant.** `HALF_WHEELS` in car/load.rs multiplied the diameter by 0x2000 (two). Cheat 32 doubles the wheels, so it is now `BIG_WHEELS`, and the comment says so.
- **The small cars' puffs.** `effects_init` (0x8002bdb4) ends by setting 0x800d0db6 to 28 under cheat 4. The puff draw (0x8002f618) takes it off each puff's growth. Ported as `Effects::set_small`/`puff_shrink`. `the_small_cars_shrink_the_puffs_as_in_the_original` checks the value under seven cheat bytes. It also checks that one puff drawn with 0 and with 28 moves the original's quad template and the port's corners by the same 28.

docs/engine/cheats.md now covers the codes and every bit's readers.

**Still unknown:** what cheats 1 and 64 do (nothing in this build reads them); the model scale in the car's draws (car, wheels, light points, glows, shadow) and cheat 8's flat textures are not ported; which car-draw matrix the big-wheel scale goes to
