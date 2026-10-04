---
number: 28
title: The front end runs: card dialog, title, main menu, race and back
date: 2026-10-03
area: ui
files: crates/hwtr-game/src/front/mod.rs, crates/hwtr-game/src/front/card.rs, crates/hwtr-game/src/front/screen.rs, crates/hwtr-game/src/front/font.rs, crates/hwtr-game/src/front/strings.rs, crates/hwtr-game/src/fsm.rs, crates/hwtr/src/front.rs, crates/hwtr/src/main.rs, crates/hwtr-hle/tests/front.rs
---

# 28. The front end runs: card dialog, title, main menu, race and back

The game starts at its front end now: the memory card dialog, the title, the
main menu with its car, track and mode, and from the menu a race with the
cars and track chosen; when the race ends the menu comes back. The flow is the
original's own state machine, not a copy of it.

## The state machine as the program

`fsm_main` (0x800c5bdc, 349 states) drives the whole front end, and
`hwtr_game::fsm` already ran it. `hwtr_game::front::Front` reads it out of
the executable and runs one step each vertical blank, as the console does
(one tick a frame at 60 Hz, the clock 17 ms a frame, checked with a watch on
0x800d240c). Each state names its actions by function address, and
`Front::act` is where each is ported. 408 different actions appear in the
table; the ones from boot to a race and back are ported (about 90), and any
other is reported once and does nothing.

Run headless from boot without a card (as the original was, to make the state
`menu-main`), with the same presses, the port goes through the states the
original does: 1, 2, 5, 6,
8 (the card dialog), 16, 8, 17, 32, 13, 44, 45 (the title), 47, 49, 50, 53,
63, 64 (the main menu), 70, 95, 111 (the race).

The memory card is always in, and always has a Hot Wheels file. Like the
IMOQ port, the card is a folder (`work/memcard/slot1`), and the game's card
file sits in it under its card name, `BASLUS-00964HTWHEELS`, byte for byte as
on a real card (no card filesystem). The first launch writes a new one. So
the boot goes 1, 2, 4 (0x80088828 loads the settings and the players last
playing), 13, 44 (the title), with no card dialog. Without a card, the game
says "Valid memory card not found", Retry or Continue.

The file (`hwtr_game::front::card`) is one 8 KiB block:
- a 512-byte title frame (0x80024aec): "SC", three icon frames, one block,
  "HOT WHEELS TURBO RACING" in Shift-JIS (0x800249b8's tables), and the icons
  from `MEM1.TIM` to `MEM3.TIM` with the last one's palette;
- the 2912-byte save (0x8006917c builds it):
  - the card's id (`random(400 000 000)`, not 0);
  - the settings record: 60 high scores, 60 best times, 15 cup winners, the
    options, and the ids of the two players last playing;
  - up to four player records of 180 bytes: name, unlocks, car and track,
    progress, records, cheats, button mapping;
  - the byte sum of the first 2904 bytes (0x80068c64 checks it).

A test runs the original's initialisers (0x80088544, 0x80088474) and compares
their records with the port's bytes. One quirk: the cup table is cleared as
if it had 60 lines, so the clearing runs past the record, into the player
ids and beyond. On a real card that leaves stack bytes at +0x87b; the port
writes zeros there.

After a race, if a player's password would differ from the last one saved,
the game asks "Player data has changed. Would you like to save now?"
(screen 3). No returns to the menu. Yes leads to the card screen (state 59
on), which is not ported yet, so the port writes the save at once and
returns as after No.

The difficulty a new game starts with is byte 0x20 of `TUNING.PRM` in
`SCREENS.BIG`, which the boot copies to 0x80136a18; it is 128.

## Screens

Thirty screen records (0x800c24e8) hold counts and pointers: 44-byte text
widgets the menus write (0x8008649c), 24-byte boxes and labels whose text is
a string-table line, list widgets, and 3D pieces. Each letter has a place
and a target; a widget lays its letters out (0x80081540, 0x8008173c,
0x800818f4), comes on from the left or the right (0x80081bec) and slides
(0x800828f8): each frame a letter covers `dt/100` of the way (a tenth after a
pause over 100 ms). Only rightward and downward motion slides; a letter going
the other way, or with less than a pixel to go, jumps. So text coming from
the right appears at once, and text leaving to the left vanishes.

Drawing (0x80086268) puts each letter at x and `y/2 - 8` on the 640 by 240
screen, in half the widget's colour; the chosen line flashes in a grey that
climbs 450 a second and wraps. While a screen fades out its texts are hidden.
The background picture (`psxmain1..3`, one at random) is drawn at a
brightness from the fade: black below level 51, then `level/2 + 1`.

## The font

`SCRNFNT.OVL` is the same overlay format as the race fonts. Letters draw
upper case from font 0 whatever the widget's font says; the font byte only
changes spacing (font 2 is fixed at 35). The pen moves by the first letter's
width plus a kerning adjustment (0x800876ac). The kerning table (0x800befe0)
stores percentages of the first letter's width, and the font load turns them
into pixels (0x80087628), marking each list done. Two quirks, kept:
- the comma is not in the alphabet, so its list is never marked and never
  applies;
- the load scales again every time the front end comes back from a race, so
  after the first race the kerning shrinks further. At these widths it is
  zero for nearly every pair already.

The binary searches read one entry past the stored counts, as the original
does; the port reads the same memory.

## Tests

`hwtr-hle/tests/front.rs`: the spacing of 4000 pairs (every listed pair
first) and each letter's width against 0x800876ac and 0x80087764, and every
letter of the main menu's eight texts and three labels against the live
`menu-main` state.

## Into a race and back

Race! (or Start) fades the menu and waits 1.8 s (0x8008baec), then
0x8009ba28 sets up the race. The player's car goes on a random grid place,
and five computer cars are drawn from the unlocked ones (starting at
`random(41) + 1`, then onward) until the field's average rating (0x800c289c)
is under 10213, at most 100 draws. The laps are 4: the grid is behind the
line, so crossing it the first time counts. The quick race `--track` gives
had 3, one short; it has 4 now. The program runs the race; when it is over
(or Select quits it), `Front::race_over` lets state 334 go on. The front end
loads the font again and returns through 49 to the menu.

Left and right on the car, track, mode and sign-in lines change them in
place, repeating after 500 ms and then faster down to 250 ms (0x8008b9e8).

## Not yet

- The 3D pieces: the dialog's box, the arrows, the button icons in the help
  line, the car and track previews. The help line keeps their room.
- The car select screen (Cross on Cars, state 98), options, sign-in,
  passwords, high scores, the cup, two players, the attract mode after 30 s
  idle (the title and menu just wait), the race-again prompt, unlock notices.
- Sounds: their ids are collected, not played.
- The memory card screen (load, save, delete, rename; states 218 on).
- The password the game compares to decide on saving (0x8006a140) is
  compared as what it encodes; the password text comes with the sign-in
  screen.

**Still unknown:** what the 3D pieces' 16-byte records hold beyond the model name; the attract mode's demo race
