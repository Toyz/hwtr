---
number: 40
title: Load/Save, the controls screen, the boot card checks and the pause volumes
date: 2026-10-04
area: ui
files: crates/hwtr-game/src/front/card_menu.rs, crates/hwtr-game/src/front/controls.rs, crates/hwtr-game/src/front/boot.rs, crates/hwtr-game/src/front/mod.rs, crates/hwtr-game/src/front/title.rs, crates/hwtr-game/src/front/main_menu.rs, crates/hwtr-game/src/pause.rs, crates/hwtr-game/src/race.rs, crates/hwtr-hle/tests/pause.rs, crates/hwtr/src/front.rs, crates/hwtr/src/main.rs, crates/hwtr/src/race.rs
---

# 40. Load/Save, the controls screen, the boot card checks and the pause volumes

The main front end's state machine has no unported actions left (0 of
about 800).

**Load/Save** (states 218 to 271; screens 19 to 24): each player playing
has a list.
- Left and right on the first line choose card slot 1 or 2. Up and down
  choose one of the card's four places.
- Cross on a place with a player opens Load / Delete / Save. Delete asks
  first. Save over a place asks "Do you wish to overwrite the existing
  game?".
- Cross on an empty place offers to save the player there with a new id
  (0x800695d8).
- Each operation writes the cards at once (0x8006917c) and shows "Memory
  Card: Save/Load/Delete Succeeded./Failed.".
- Loading makes the place's record the player and notes its password as
  saved. Saving over a place also copies the tables and settings in play
  onto that card. Both note the player as who last played on both cards.
- A slot with no card gives "ERROR ON MEMORY CARD IN SLOT n: UNABLE TO
  COMPLETE THIS OPERATION."
- The rename flow (states 220, 221, 260, 261, 264) has no way in; it is
  ported anyway.

The front end now holds two `slots`, each a `Save` as the card holds it,
and `card`, the tables and settings in play (0x80138f14), separately. The
app writes each due slot to work/memcard/slot1 or slot2. "Yes" on the
title's "save now?" prompt now goes to Load/Save, as in the original,
instead of writing straight to the card.

**Boot card checks** (states 2 to 43): each slot has a status, coded as
0x80068ca4 codes it:
- 0: the save is there.
- 1: no card (slot 2's folder missing), or a card set aside.
- 3: a card without the game's file.
- 8: a save that fails its checksum.

The ported dialogs follow from these codes. Slot 1 without a file asks
"Hot Wheels File Not Found. Create New File?". Yes makes the file (and new
settings and players). No sets the card aside and leads to "Continue
without saving?". The port used to make the file silently. The libcard
status bits behind the codes were read from 0x80024c68 and 0x80024f38: 1
none, 2 new card, 4 no file, 8 full, 16 unformatted, 32 error.

**Controls** (states 196 to 217; screens 13 to 17, `psxctrl`): each
player's list shows Vibration and the 13 actions, six rows at a time with
scroll arrows. Each row has the icon of its button:
- the button names come from 0x800bdc7c, by the lowest bit of the action's
  mapping word, shifted up a byte for the d-pad actions;
- the icons come from 0x800c282c.

On the list:
- Cross on Vibration turns the motors on or off (mapping word 28). Turned
  on, the pad buzzes for half a second; the app runs the motors.
- Cross on an action waits for all buttons to be let go, then takes the
  next press (0x8001b640): the d-pad for steering and the stunts (not
  Select, Start or the stick buttons), any shoulder or face button for the
  rest.
- Leaving checks for two actions on one button (0x8009495c). Only the pairs
  at 0x800c2704 may share: steer and spin each way, accelerate and back
  flip, brake and front flip. A clash shows "Error! A and B are same
  control."; otherwise each pad's buttons go into its player.

The front end now keeps a mapping per pad, loaded from the players at boot
(0x8001bafc), and the race takes player one's.

**Also:**
- Start on the main menu lets player two join. Player two's car steps on
  the car line (71, 72, 80 to 83).
- With no card to save to, each player's password is shown ("Valid Memory
  Card Not Found. Password for NAME:", state 52, 5 seconds).
- The pause menu's Sound Volumes:
  - FX, Music and Voice Over step by tenths (0x8009f388 up, 0x8009f4a4
    down), with key repeat (0x8009f0e8: at once, then after 11 blanks,
    then every 4).
  - FX and Music are drawn as percentages, the chosen one red.
  - A change applies to the race's effects or the CD at once, and the
    values go back to the settings when the race ends.
  - As in the original, stepping the music also sets the effects to the
    music's level (0x8001a6dc is called with it).
  - hle test `pause.rs` checks both steps from all 256 levels against the
    original. The pause menu has no unported actions left either.

**Still unknown:** None of the card screens can be checked against the original in hle: it has no memory card (libcard's events are stubbed), so it only ever shows 'Valid memory card not found'. They are ported from the code and checked by walking them in the example and by port shots. The attract mode's demo race is still not run. The original's 0x800d0f28 'card seen' flag is only set on a new-card event; here every card starts seen.
