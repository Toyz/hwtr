---
number: 36
title: The CD's music: race songs and the Boom Box
date: 2026-10-04
area: audio
files: crates/hwtr-game/src/cd.rs, crates/hwtr-game/src/pause.rs, crates/hwtr-game/src/race.rs, crates/hwtr-game/src/front/mod.rs, crates/hwtr-game/src/front/race_start.rs, crates/hwtr/src/cd.rs, crates/hwtr/src/spu.rs, crates/hwtr/src/main.rs
---

# 36. The CD's music: race songs and the Boom Box

The races now have their music: the 13 songs on the CD's audio tracks 2 to 14. The pause menu's Boom Box plays, steps through and names them.

**The drive** (`iface_sound` slots 2 to 6, filled at 0x80019870):
- 0x80014804 plays song n. It checks n against the TOC count, then plays from the TOC's track n + 2 to the start of the next one, in CD-DA mode with reports. The report callback (0x80014754) jumps back to the start when the position passes the end, so every song loops.
- 0x800149b8 stops and rewinds (CdlStop); 0x80014a0c pauses (CdlPause); 0x80014a94 plays on from the last reported position.
- 0x80014b6c sets the volume: the setting × 127 / 255, at most 128, through CdMix.
- In `hwtr-game` these are `cd::CdAsk` {Play, Stop, Pause, Resume}. The front end and the race collect them for the app, which reads each track whole off the image (`CdPlayer`, track n + 2's file from INDEX 01) and plays it, looping, through the SPU mixer at the CdMix volume.

**When the music plays:**
- The race setup (0x8009ba28, as 0x8009b254 and the other setups do) ends with 0x8008a808, which picks the song by the music mode at 0x800d276d:
  - 0: at random, 0x800145f0(13), the front end's own rand;
  - 1: the track's own, from the table at 0x800be8c0 (song k for track k, read off the exe);
  - 2: the song chosen, 0x800d11a0.
  It then plays it. The mode defaults to 0 (0x800d276d is 0 in the executable) and is kept only in RAM.
- During the race only the pause menu touches it. Bringing the menu up pauses the CD (0x80036634 calls slot 5); leaving with Continue plays on (0x80036758, slot 6).
- Back in the front end, 0x8009bf34 stops it (0x8008a7e0) as it reads the race's end.

**The Boom Box** (the pause menu's fourth screen):
- Entering it (0x8009f910) plays the current song (0x800d2484, slot 3).
- Next (0x8009fb24) and Prev. (0x8009fba0) wrap through the 13, stopping first.
- Entering Options (state 150) and leaving the Boom Box (state 203) stop it (0x8009fafc).
- 0x8009f98c / 0x8009fa44 draw the artist at (192, 155) and the title at (192, 175), centred, in white (255). They come from the tables at 0x800be8cc and 0x800be900; "Alex Skolnick / Avenue X" is song 0.
- The text routine is 0x8009e160 / 0x8009e0a0: each glyph's width plus the kerning to the next letter, centred on half the total. The glyph draw (0x8001ddc4) upper-cases, while the kerning (0x8007f8d4) gets the letters as written.
- The race keeps the song (`Race::song`). The pause menu starts from it and hands it back.

**Verified.** `hwtr-hle` from desert1-race, through Pause > Options > Boom Box, shows the same layout as the port. The original's song doesn't change there because `hwtr-hle` has no drive (no TOC, so 0x80014804 returns early). The app test `a_song_plays_and_pauses` plays song 0 off the image, holds silent when paused, and goes on when resumed. A shot after Next Track shows song 1 (Meat Beat Manifesto, "Eclectic People").

**Still unknown:** The options screen's music line (0x800932d8, 0x80093368: the mode at line 7, the song at line 8, previewed with 0x80093274) is in the unported options screen, so the music mode is always 0 (random) for now. Whether the console's TOC entry past the last track (song 12's end, toc[15]) makes song 12 end early or late; the port plays each track's file to its end. The music volume comes from settings.music (0x80014b6c); when the original applies it is not traced.
