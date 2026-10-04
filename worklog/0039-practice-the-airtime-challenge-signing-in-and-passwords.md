---
number: 39
title: Practice, the airtime challenge, signing in and passwords
date: 2026-10-04
area: ui
files: crates/hwtr-game/src/front/race_start.rs, crates/hwtr-game/src/front/sign_in.rs, crates/hwtr-game/src/front/password.rs, crates/hwtr-hle/tests/password.rs, crates/hwtr-game/src/race.rs, crates/hwtr-game/src/fsm.rs, crates/hwtr-game/src/front/options.rs, crates/hwtr/src/main.rs, crates/hwtr/src/front.rs, crates/hwtr/src/intro.rs
---

# 39. Practice, the airtime challenge, signing in and passwords

**Race modes.** Practice (0x8009b614) and the airtime challenge (0x8009c058)
set up a race for the players alone: two laps, the player-one track, and a
clock. Practice runs 30 minutes with flags 12, or 6 in practice airtime
(stunts counted). The airtime challenge runs 3 minutes with flags 6. Their
end and wait actions (0x8009b904, 0x8009c2f4; 0x8009b9e8, 0x8009c3d8) are
0x8009bf34 and 0x8009c018 again. Both setups match the original's record at
0x80138c94 field for field, peeked in hle. The race now ends on the clock
when flag 4 is set (0x8003485c through race_load's 0x800d0de0), which it
never did before.

**Passwords.** `front::password` ports 0x80069ae4 (make) and 0x80069d78
(read):
- A password packs 24 car bits (ordered by 0x800becfc, whose last four
  entries are zeros and so stand for car 0), the low 16 track bits, and the
  six records' low 7 bits with the progress bits on top. With no cup
  progress, the records' six bytes are a fixed pattern instead.
- Each letter is 5 bits plus the running sum mod 32, in the alphabet at
  0x800bed14. There are 18 letters, then two check letters, then six swaps
  (0x800bee54).
- Reading also takes "TWJM" and the 24 cheat words at 0x800bed34.

hle test `password.rs` compares 500 made passwords and 600 reads (made,
random and cheat words) against the original's own functions. All match.

**Signing in.** 0x8008db1c sends the main menu's sign-in line one of three
ways:
- "Sign In" asks "Player data will be reset! Are you sure?" on screen 24,
  then opens the board.
- "Password" opens the board with ENGPWD.CHM's letters.
- "Load/Save" goes to the card, which is not ported yet.

The board is screen 26 (psxreg or psxpwd):
- Its letters come from ENGNAME.CHM or ENGPWD.CHM, followed by back, a space
  key (names only) and DONE.
- Key places come from 0x800c257c and 0x800c2640 in 240-line pixels; DONE
  is entry 48.
- Letters are drawn with the raw glyph call (0x8011dfa4 at x−5, y−4,
  colour halved). The slider aims at x·1000/640, y·1000/480.
- Typing over a "PLAYER n" name clears it first.
- Accepting a name starts the player afresh (0x80088474, keeping the
  record's tag and id) and notes its password as saved. A password that
  reads adds what it carries; one that doesn't buzzes and shows screen 25.

Start on the main menu (0x8008c050) lets a second player join (0x8008c0bc,
with their car's picture in slot 1). Start on the sign-in line signs in the
second player. Checked against hle shots of the dialog and the board.

**Fixes.**
- Action 0x80086c80 and 0x80086c34 were registered twice (title and
  hi-scores), which panicked debug builds. The registry's duplicate check is
  now on in release builds too. The single 0x80086c34 posts 37 as the
  original does. Until the demo race exists, the attract setup takes the
  original's "race would not start" path (event 134) back to the title.
- 0x8008ada8 (fade to half) hides the texts, as the original does.
- Menu sounds carry a volume. Letting go of the Sound FX slider plays effect
  56 at the new volume, as 0x80092f50 does. In the original that slider
  otherwise only sets the race's effects (0x8008ab00, the pause menu).
- The help lines run to 9 (the board's is the ninth).
- The app hands from the intro to the front end in one tick. Before, one
  frame drew nothing, which showed as a flicker.

**Still unknown:** The attract mode's race (0x8009b254) is not run: the port's race cannot yet run a field of computer cars with the demo views and stop on a button, so the title loops back after 30 seconds. The card manager (Load/Save, states 218 to 271) is not ported yet. The sign-in board and dialogs are checked against hle shots; the wrong-password screen only against the code.
