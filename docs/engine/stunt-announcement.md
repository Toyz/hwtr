---
title: The stunt announcement
status: complete
discs: US
covers: US CCCPSX.EXE:0x80064dec stunt_announce, 0x80064724 stunt_text, 0x80063c70 slide_left, 0x80063e5c slide_right, 0x80064d44 text_width, 0x80080d6c stunt_name, 0x800bead0 hud_player, 0x800bf114 stunt_names
worklog: 50
---

# The stunt announcement

When a player's car lands a stunt, `stunt_announce` (0x80064dec) is
called with the car's player number, the points (0 when the race does
not score), the stunt and the turbos won. The HUD then shows the stunt's
name and its reward in yellow, slid in from both sides of the screen.

The announcement lives in the player's HUD record at 0x800bead0
(240 bytes a player). That record also holds the wrong-way warning
(+0, +236) and the turbo meter (0x800becb0).

| offset | holds |
| --- | --- |
| +4 | when the halves came to rest (0 before) |
| +16 | the points |
| +20 + 32k | line k's left-moving half (16 bytes) |
| +36 + 32k | line k's right-moving half |
| +116 + 40k | line k's text |
| +237 | active |

Each half is 16 bytes:

| offset | holds |
| --- | --- |
| +0, +2 | clip left, right |
| +4 | where the text starts |
| +6 | row |
| +8 | speed |
| +10 | text width |
| +12 | at rest |
| +13 | gone |
| +14 | squeeze, taken off each letter's advance |

## Set-up (0x80064dec)

1. **The meter.** The turbo meter's part is filled (see the turbo meter
   in `Hud::turbos_given`).
2. **The lines.** All three lines are cleared, and each line's left half
   gets:
   - a clip from 65 to 275 (260 with two players)
   - its text starting at the clip's right
   - speed -20
   - rows 160, 180 and 200 (54, 74 and 94 for player one of two)
3. **The name.** `stunt_name` (0x80080d6c) draws one of the stunt's
   names at random (table 0x800bf114: a count, then up to six text ids)
   and prints it into the middle line.
   - A name of 40 letters or more would run on into the last line, and
     both lines would then move up one. None of the game's names is that
     long.
4. **The reward.** The last line is `"%d Points"` (string 217, the
   points as a 16-bit number) when the stunt scores. Otherwise it is
   `"+%d Turbo"` (string 292) for one turbo, or `"+%d Turbos"` (293).
5. **Centring.** Each line's width is the sum of its letters' advances,
   width plus kerning (0x80064d44). A bubble sort puts the widest first,
   and the other two lines start half the difference further right.
6. **The right halves.** Each right half is a copy of the left, starting
   at `lo - width - (x - hi)`, as far left of the clip as the left half
   is right of it.
7. **Two players.** Only the last line is kept, and only if it shows
   points.

## Each frame (0x80064724)

The frame is drawn when the HUD shows messages (bit 0x400), after the
rest of the HUD and before the wrong-way warning.

1. **End.** Once all six halves are gone, the announcement ends.
2. **Rest.** When the first half comes to rest, the time is noted and
   every half's speed becomes 0.
3. **Leave.** 1001 ms later, the left halves' speed becomes -20 again
   and the right halves are marked gone.
4. **Drawing.** For each line with text:
   - While neither half is gone, the left half draws the even letters
     and the right half the odd ones. Each letter is drawn only if its
     pen is within the clip.
   - Once either half is gone, the left half draws every letter.
5. **Moving.** After drawing, a half moves by its speed: the left half
   leftward by adding it, the right half rightward by subtracting it.
   - It comes to rest when the next move would carry its middle past the
     clip's middle. It then snaps to the middle.
   - At rest it still moves by its speed.
   - The left half is gone once its text ends at or left of the clip's
     left. The right half is gone once its start is at or past the
     clip's right.

A line with no text never has its halves drawn, so they are never marked
gone. The announcement then stays active, drawing nothing, until the next
one replaces it.
