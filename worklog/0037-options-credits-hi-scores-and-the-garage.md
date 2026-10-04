---
number: 37
title: Options, credits, hi-scores and the garage
date: 2026-10-04
area: ui
files: crates/hwtr-game/src/front/options.rs, crates/hwtr-game/src/front/credits.rs, crates/hwtr-game/src/front/hiscores.rs, crates/hwtr-game/src/front/garage.rs, crates/hwtr-game/src/front/mod.rs, crates/hwtr/src/front.rs, crates/hwtr/src/spu.rs
---

# 37. Options, credits, hi-scores and the garage

The main menu's Cross on CARS and on OPTIONS, and the options' sub-screens, now work like the original. Each was mapped from the fsm_main states and the actions' decompilation, then checked against `hwtr-hle` shots of the original. The example `front` now prints each unported action as it's reached, which is how the work is found.

**Options** (states 108, 184 to 195; screen 12):
- Lines:
  - 0 to 2: Difficulty, Music and Sound FX sliders (settings bytes 0x878, 0x877, 0x876). Held left or right moves them 10 a frame with clicks 48 and 49. The music slider sets the CD volume as it moves (`CdAsk::Volume`), and letting go of Sound FX plays effect 56.
  - 3: Audio Mode, mono or stereo (0x879). It's libsnd's mono switch via slot 0x8011e008, and now applies to every effect keyed in the menus and in races.
  - 4 to 6: Controls, Hi-Scores and Credits, by Cross (events 96, 98, 97).
  - 7: the Boom Box mode: Random, Default (the track's own song) or Selected, using strings 255 to 257.
  - 8: the song line, which appears only in Selected mode. Moving onto it stops the menu music and plays the song; moving off brings the music back (0x80093274, 0x800932a8).
- The knobs are the screen's first four pieces, targeted by 0x80093418: x = 485 + v × 0x1afa / 4096, or 485/915 for mono/stereo.
- The arrows beside the chosen line come from 0x800927cc, the same routine as the main menu's (`place_piece`, generalised from `place_arrow`).
- The pad masks per line are from 0x80092be8. Holding a direction on a slider is 0x80092d50: events 94/95, else 10.
- The music mode and song feed the race's song choice (worklog 36).
- The string table's RAM base is 0x80135134 (from 0x8007fbcc), so a pointer at 0x801352bc is string 98, "Song".

**Credits** (states 197 to 201; screen 18): one of three pictures `psxcrde1-3`, and `psxcrds` for the second page. Each page is a scroller record (0x800bfd9c, 0x800bfdbc): 215 and 100 lines, each bright (255) or not (170), centred between x 220 and 620, 24 px apart, shown between y 140 and 380, rising 30 px a second and starting again when all are past the top (0x80080ffc, 0x80080e08). Up and down switch pages.

**Hi-Scores** (states 109, 316 to 324; screen 27):
- Three kinds of table (strings 66 to 68): High Scores and Best Times per track, Top Cup Winners per cup (strings 59 to 61). They come from the save: the card's tables, five 16-byte records each (a 12-byte name, then the value).
- Times print as `%02d:%02d.%02d` from milliseconds (0x8008acb0, 0x8008ace0, 0x8008ad20).
- Row 0 picks the kind, row 1 the track or cup. Left and right skip tracks neither player has (0x8008a2f0) and Haunted 1's empty slot. Cups need 0x8008a33c: the first is always open, the second needs player one's track bits 7 and 8, the third car 38.
- After 30 s idle (0x80086c34) it shows random tables, revealing a row each second (0x80086bc0), and moves to another after 40 s.

**Garage** (states 107, 159 to 183; screens 10 and 11):
- Background `psxgar1` for one player, `psxgar` for two. The car preview comes in at 1.5×.
- The panel shows the previous, current and next car's names, the others dimmed (100). After 3 s it switches to the car's facts (ENGCARS.CDT: the name, then pairs 1-2 and 3-4, alternating every 3 s).
- A locked car reads "MYSTERY CAR / This car has not / been unlocked." (strings 97, 236, 237). In place of the car it shows the SCR model `mystery`: twice size, tilted −1.39 rad, turning a turn every 5 s.
- Up and down held step the car, with the main menu's held repeat, now a shared `held_repeat`. Cross picks: the preview moves aside at full size, or buzzes (effect 53) if the car is locked. Triangle un-picks, or leaves with the cars as they were. Once everyone has picked, event 88 returns to the main menu.
- The quality bars (0x80090974) are glyph 153, a block per 5 points: top speed red, stunts green, durability blue, plus the class (green, yellow, red, pink; 12 blocks per class).
  - The values are CWHS block A bytes +0x130 to +0x133. 0x8008c544 copies them in the order 0, 2, 1, 3, and a locked car gets class −1 (no bars).
  - CWHS now reaches the front end through `Files::cwhs`.
- 0x80023660, the car's decal drawn while its model loads, never applies: every model is in memory.

Checked against `hwtr-hle` shots of the original options, hi-scores and garage screens (layout, texts, knobs, bars), and by walking each flow in the `front` example with no unported action reached.

**Still unknown:** The Controls screen (states 196, 202 to 216; screens 13 to 16): the button-mapping editor and its duplicate check (0x8009495c), which need the iface_controls slots (0x8012fcec assigns a button, 0x8012fd1c, 0x8012fd24/28 the vibration and analog flags) ported; its states remain unported. The menu effects' volume (0x8011df44's third argument) is not applied: menu sounds play at full volume whatever the Sound FX slider says, though races use it.
