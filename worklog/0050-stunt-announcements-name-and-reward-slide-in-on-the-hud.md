---
number: 50
title: Stunt announcements: name and reward slide in on the HUD
date: 2026-10-04
area: ui
files: crates/hwtr-game/src/hud.rs, crates/hwtr-game/src/race.rs, crates/hwtr-game/src/car/stunt.rs, crates/hwtr/src/race.rs, crates/hwtr-hle/tests/announce.rs, docs/engine/stunt-announcement.md
---

# 50. Stunt announcements: name and reward slide in on the HUD

Landing a stunt now shows its name and its reward on the HUD, sliding in
as the original does.

**Announce (0x80064dec).** Ported as `Hud::announce`.

- The stunt's name goes on the middle line.
- The last line is `"N Points"` or `"+N Turbo(s)"` (strings 217, 292,
  293).
- The lines are centred on the widest with a bubble sort, and each gets
  a left-moving and a right-moving half.

**Each frame (0x80064724, with 0x80063c70 and 0x80063e5c).** Ported as
`Hud::announcement`.

- The left half draws the even letters from the right; the right half
  draws the odd letters from the left. Both move 20 px a frame.
- Both snap to the clip's middle, stand 1001 ms, then the whole line
  slides off left.
- It is drawn after the HUD and before the wrong-way warning, when the
  HUD shows messages (bit 0x400).

**Plumbing.**

- `Race` announces on the car's player's HUD when a stunt is awarded.
- `ResultsText` now carries the whole string table and the three words.
- `Hud` and `MeterTables` derive Default.

**Verified.** crates/hwtr-hle/tests/announce.rs runs 60 announcements,
with random stunts, points and turbos, through 150 frames each, against
the original. It compares:

- the random seed after the name is drawn
- the record each frame (lines, halves, rest time, active)
- every letter drawn and its place

The race clock and the glyph draw were stood in for; the glyph draw
returns the font's width.

**Not seen on screen.** A Cross-only shot run lands no stunts.

**Still unknown:** the stunt commentary lines (0x80036484); the ten-turbos hint line
