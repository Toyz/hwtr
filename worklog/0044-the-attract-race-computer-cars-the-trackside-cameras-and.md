---
number: 44
title: The attract race: computer cars, the trackside cameras and the director
date: 2026-10-04
area: race
files: crates/hwtr-game/src/camera.rs, crates/hwtr-game/src/race.rs, crates/hwtr-game/src/hud.rs, crates/hwtr-game/src/math.rs, crates/hwtr-game/src/front/race_start.rs, crates/hwtr-game/src/front/main_menu.rs, crates/hwtr-data/src/world.rs, crates/hwtr/src/main.rs, crates/hwtr/src/race.rs, crates/hwtr-hle/tests/camera.rs, crates/hwtr-hle/src/original/camera.rs, docs/engine/attract.md, docs/formats/world.md
---

# 44. The attract race: computer cars, the trackside cameras and the director

The title's 30-second idle used to bounce straight back to the title: the
attract race was a stub, because the race could not run without a player.
It can now, and the main menu's idle (0x80088428, which had been a
`note_unported`) leads to it too.

`0x8009b254` picks a track a player has, never a Volcano one, and six
computer cars from player one's garage until the field is fair, one lap
against a minute. Two oddities kept: the checkpoints are read with the
track's number alone (`0x800c5c64[number - 1]`, the Desert row), and the
car-number byte of each entrant is never written (the port writes the car's
own, since it has no stale setup to inherit).

The race side is small. Setup flag 1 lands at 0x800d2608; `race_frame`
checks it twice: no pause, and at `race_over` the end byte goes straight
to 2, no standings. `camera_load` always makes one camera; with no players
it follows car 0 with the demo views (0x800bea3c, two of them, both
chase). The HUD with no players draws "DEMO MODE", blinking every 750 ms
of the race clock (0x800644c0, `(t * 0x057619f1) >> 36`).

The interesting part was the camera. The demo calls 0x8003a690 after each
camera step instead of reading the view button. Every four seconds it
looks at the world's 40-byte records at header +56/+60, which the format
page had as "rec40, meaning unknown": they are trackside cameras (flags,
field of view, rotation, position), read by 0x80021390 through
iface_general+132. The camera that sees the most cars coming toward it,
within twelve times the track's range (0x800be934), follows the farthest
of them in mode 3; with none, a random non-mounted view on a random
unwrecked car. Mode 3 stands at the trackside camera and turns to the
car; mode 2 (never chosen: the director ORs bit 0 into the flags before it
tests them) stands as stored; mode 4 does nothing. In the demo the chase
spring's share and both limits are tripled.

The hle test sets the demo flag, views and count as `camera_load` would,
then for 400 rounds puts the camera in a chase view or at a trackside
camera with the director due or not, sends half the cars toward some
trackside camera, and calls the original's 0x800369e4: the camera and the
random seed match the port's step plus director every time, 97 of the
rounds cutting to a trackside camera.

In the app the front end is ticked while the attract race runs, as the
original runs the race from state 326's update; 0x8009b578 counts as that
blank's frame so the front end does not run ahead. A button posts 135 and
0x8009b510 lets the race go. Because the port loads the menus at once
where the original reads the CD, the buttons held at that moment are
latched as already pressed, or the same press would choose "Race!" on the
main menu as it appears.

**Still unknown:** What 0x800836e8 (iface_game+0x98 with 0) does at the attract race's setup; trackside camera flags other than bit 1 (none set on any shipped track); the front-end setup itself is not checked against hle (no state sits at the title's idle).
