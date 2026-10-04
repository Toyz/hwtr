---
number: 43
title: The results: the snapshot slideshow and the standings table
date: 2026-10-04
area: race
files: crates/hwtr-game/src/snapshot.rs, crates/hwtr-game/src/race.rs, crates/hwtr-game/src/hud.rs, crates/hwtr-game/src/car/update.rs, crates/hwtr-game/src/collision/pairs.rs, crates/hwtr-game/src/pause.rs, crates/hwtr-game/src/front/race_start.rs, crates/hwtr/src/race.rs, crates/hwtr-hle/tests/snapshot.rs, crates/hwtr-hle/src/original/race.rs, docs/engine/results.md
---

# 43. The results: the snapshot slideshow and the standings table

The race's end had been a four-second wait and a frozen frame. Reading
`race_frame` (0x80033ed8) past the finish showed what the original does in
those thirty seconds, and it is not a replay: after four seconds by the
frame clock it calls 0x80080020 every frame with
`((since - 4000) / 2000) % 0x80080014()`, which puts back one of a handful
of snapshots. The strings beside the loader name it: "INITIALIZING
SNAPSHOT MODULE..". The "replay's snapshots (0x8007feb0)" traces were right
about the function and wrong about the word.

`snapshot_take` (0x8007feb0) has five callers. Two were known (the start,
the end). Two sit in `car_update` and had been ported as traces named "the
big air line" and "the wreck line": the top of a long jump and half a
second into a wreck, each one time in two through `iface_general+4`, which
is `rand() % n`. The fifth is in `collision_update` after a pair's impulse,
when both cars have flag 1: two players touching. The car snapshot is taken
mid-step, before the rotation is orthonormalised, so the port passes the
rotation as it was then.

Each part of a snapshot has its own writer and reader. Cars keep 24 bytes:
the low half of +0x8, the wreck byte, the floor plane folded to the origin,
two rotation rows in 127ths, the position in whole units; the third row is
rebuilt as a cross product. Cameras keep 16. Two writers (0x8006ba48,
0x8007f6f8) are stubs that reserve a word; the loose bodies (0x8007e6e0)
keep 16 bytes a body in flight or a word. The buffer is 4096 bytes, so a
six-car race keeps at most twenty; one is skipped when the last is under a
second old by the race clock.

The test calls the original's 0x8007feb0 and 0x80080020 on the
desert1-drive and desert1-air states: the bytes match the port's snapshot
(196 bytes, as the state's counter said: 0xc4), and putting it back over
cars and a camera moved since gives the same Car and Camera, whole, as the
port's.

The hle shot of a race forced into phase 3 showed what else is on screen:
"CAR" and "TIME" over the frozen start. That is `hud_draw` (0x80064b8c) in
mode 3 drawing 0x8006452c, which picks a times table (0x80064040) or, in a
stunt race (setup flag 2), a points table (0x80064294). The column heads
come from the string table (289 TIME, 290 BEST against the clock, 291
POINTS, 215 DNF); the cars' names from 0x800c5d8c; the players' names from
0x801399b0, which 0x8009b5b8 copies from the profiles when each race is
set up, so RaceSetup now carries them. A player's line is green, time and
all. The text is the race font with the front end's kerning, the same
style the pause menu already had.

In the port, aborting from the pause menu now shows the table, then the
start and end snapshots in turn. Notes: the app no longer eases between
steps while the race stands still, so the slides cut; the wreck debris a
snapshot restarts and the trail it clears are traces until the particles
are ported.

**Still unknown:** What the draw-mode byte 0x800d246c changes beyond the car effects that read it; the loose bodies' snapshot record (0x8007e6e0), not kept since the loose bodies are not ported; 0x800d0e54, which the points table lowers; the wreck debris a put-back snapshot restarts (0x80029e10) waits on the particle port.
