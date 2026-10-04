---
title: The race's results: snapshots and the table
status: partial
discs: US
covers: US CCCPSX.EXE:0x8007fe48 snapshot_init, 0x8007fe7c snapshot_reset, 0x8007feb0 snapshot_take, 0x80080014 snapshot_count, 0x80080020 snapshot_put_back, 0x8004a8fc, 0x8004aef8, 0x8003b610, 0x8003bafc, 0x8007e6e0, 0x8007ec08, 0x8006ba48, 0x8007f6f8, 0x80064b8c hud_draw, 0x80064b64 hud_mode, 0x8006452c, 0x80064040, 0x80064294, 0x800644c0, 0x80063ab4, 0x80063bc4, 0x8009b5b8, 0x80135a18 snapshot buffer, 0x800d2718 snapshot count, 0x800d271c bytes used, 0x800d2720 last taken, 0x800d2688 standings order, 0x801399b0 player names
worklog: 43
---

# The race's results

When a race ends, `race_frame` (0x80033ed8) works out the standings
(0x80033aa0), sets the race phase (0x800d0de9) to 3 and the "draw mode"
byte (0x800d246c, through `iface_general+0x120`) to 2. The HUD switches
to the results table at once. Four seconds later (by the frame clock,
from 0x800d25e8) the race stands still (0x800d2607 = 1, mode byte 3) and
shows the snapshots taken during the race, two seconds each, round and
round: snapshot `((since - 4000) / 2000) % count` is put back every
frame. The results leave after 30 seconds, on Cross (action 18) pressed
after being let go, or on Start (26).

There is no replay. "Replay" in this game is this slideshow of up to
twenty frozen moments.

## Snapshots

The snapshot module was set up by 0x8007fe48 at race load ("INITIALIZING
SNAPSHOT MODULE..") and by 0x8007fe7c on a restart. Each clears the
4096-byte buffer at 0x80135a18, the count (0x800d2718) and the bytes used
(0x800d271c).

`snapshot_take` (0x8007feb0) is called:

| caller | when |
| --- | --- |
| 0x80033a78 | the start, before the race clock goes back to 0 |
| 0x80033aa0 | the end, before the standings |
| 0x8004023c (car_update) | a player's car at the top of a jump: airborne, in the air over 500 ms, rising before this step's integration and falling after it, at least 501 ms since it last touched anything; one time in two (`rand() % 2 == 0`) |
| 0x80040290 (car_update) | a player's car exactly 500 ms into a wreck; one time in two |
| 0x8004e428 (collision_update) | two players' cars touched (after the pair's impulse) |

A snapshot is skipped if one exists and the race clock (0x800d0e34) is
under 1000 ms past the last (0x800d2720, unsigned difference), or if any
part of it does not fit in the room left.

### Layout

```
header, 24 bytes
  u32 size[5]        bytes of each part below
  u32 total          sum of the five
cars, 24 bytes each (0x8004a8fc), count at 0x800d263c
  u16   flags_8      car +0x8, low half
  u8    wrecked      car +0x62c
  u8    floor_found  car +0x8b2
  s16   normal[3]    car +0x8c4 (only when floor_found)
  s16   d            (car +0x8d4 - normal . car +0x8b4) >> 12 (only when floor_found)
  s8    rot[2][3]    car +0x140, the first two rows, clamp((v*127) >> 12, -127, 127)
  s16   pos[3]       car +0x10c >> 12
moving volumes (0x8006ba48)   4 bytes, nothing written
track objects (0x8007f6f8)    4 bytes, nothing written
loose bodies (0x8007e6e0)     16 bytes for each of the 8 slots at 0x801323f4
                              (stride 680) in use, or 4 bytes if none
cameras, 16 bytes each (0x8003b610), count at 0x800d2633
  u16   mode         camera +0x48
  u16   car          camera +0x4a
  s8    rot[2][3]    camera +0x24, as for cars
  s16   pos[3]       camera +0x4 >> 12
```

A one-player race with six cars and no loose bodies takes 196 bytes a
snapshot (checked in a race under way: 0xc4), so twenty fit.

### Putting one back

0x80080020(k) walks to snapshot `k` and hands each part to its reader.

Cars (0x8004aef8): `flags_8 = saved | 8`; if `wrecked`, the wreck's
debris starts again (0x80029e10 with mode 2 at the origin), else it is
cleared (0x80029f04); the floor flag is written, and if set the normal
(as saved), origin 0 and `d << 12`; the two rows back to 4.12 as
`q = (b << 12) / (127 << 12)`, `r` its remainder, `(q << 12) + (r << 12) /
(127 << 12)`; the third row the cross product of the two (each product
`(a * b) >> 12`); `pos << 12`; steer 0, the body awake, each wheel's
compression and spin 0, the reset grace 0; then the car's boost flame is
put out (0x8002aff4). The wrecked byte itself is not written.

Cameras (0x8003bafc): rotation as for cars, `pos << 12`, the mode and car
bytes, shake 0.

## The table

`hud_draw` (0x80064b8c) draws nothing once the race is left (0x800d261c).
Each player has a mode (0x800d0e50, set through 0x80064b64 by
`race_frame`): 1 the race HUD, 3 done. While racing a player is done once
their car has run its laps (0x800d25f0, from 0x8003485c); after the end,
all are. When every player is done the table is drawn over the whole
screen (0x8006452c); with no players (the attract race) "DEMO MODE"
(string 214) blinks at (130, 20) instead (0x800644c0).

Text is the race's text font (font 0, ACTNFNT), left to right, each
letter's glyph upper case and its width plus the front end's kerning to
the next (0x80063bc4), in ordering-table layer 1, in the HUD's halved
colours.

Times table (0x80064040), for races without stunt scoring:

```
"CAR"   at (26, 60), cyan
head    at (248, 60), cyan: "TIME" (string 289), or "BEST" (290) in a race
        against the clock
one line for each of the race's cars, 20 pixels apart from y = 80, by
place (0x800d2688; -1 leaves the line empty):
  name  at (26, y): a player's car its player's name (0x801399b0, or
        0x801399bc when the car's slot is not 0), green; any other car
        its car's name (0x800c5d8c), white
  time  at (248, y), the same colour: the end of the car's last lap, or
        its best lap against the clock (0x80063ab4: "%02d:%02d.%01d%01d";
        no time is "DNF", string 215, for the race time and "--:--.--"
        for the best lap)
```

Points table (0x80064294), for stunt races (setup flag 2): "CAR" and
"POINTS" (string 291) the same way, then the players' names and their
stunt points (car +0x624, `%d`), all white, the player with more points
first (an unsigned compare). It also lowers 0x800d0e54 to the race clock
less player one's lap start when that is less.

The players' names are copied to 0x801399b0 from the two players'
profiles (+8) by 0x8009b5b8 as each race the players start is set up.

## Unknown

- What the draw mode byte (0x800d246c) changes beyond the car effects
  that test it (0x8001cd54, 0x800225c8, 0x80029fe4 and others).
- The loose bodies' 16-byte snapshot record (0x8007e6e0 / 0x8007ec08):
  kind, two bytes for kind 1, six rotation bytes, a position.
- 0x800d0e54, which the points table lowers.
