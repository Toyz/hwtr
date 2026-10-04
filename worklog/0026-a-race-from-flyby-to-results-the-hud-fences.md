---
number: 26
title: A race from flyby to results; the HUD; fences
date: 2026-10-03
area: race, ui, physics, render, input
files: crates/hwtr-game/src/race.rs, crates/hwtr-game/src/laps.rs, crates/hwtr-game/src/hud.rs, crates/hwtr-game/src/collision/fences.rs, crates/hwtr-game/src/camera.rs, crates/hwtr-render/src/renderer.rs, crates/hwtr/src/race.rs
---

# 26. A race from flyby to results; the HUD; fences

A race now runs the way the original's does, from the flyby over the track
to the results, with the HUD over it. A car can no longer stick in a sign.

## The phases (race_frame, 0x80033ed8)

- **Starting.** The cameras fly the SCP's flyby, 200 ms a keyframe; the
  countdown starts when it ends (DESERT1: 50 keyframes, 9800 ms). Cross cuts
  it short. The cameras then sweep down to the cars over 5 s: the eye eases
  on a half sine from the last keyframe to the chase view (0x800369e4's
  intro branch, lerped by 0x8006b1a8). The countdown calls 3, 2, 1 at +1200,
  +2550 and +3800 ms, at most one a frame. At +5000 the race starts: the
  clock goes back to 0, and every car's laps start.
- **Racing.** Only now do the cars and the collision step (the cameras
  always do, and the race clock always runs). Each frame checks for the end:
  past 0x1b773f ms, or every player's car finished.
- **Finished.** The standings (0x80033aa0) order cars by race time (no time
  last) and pay 10/8/7/6/5/4 points. After 4 s by the system clock the race
  stands still; the results leave after 30 s, on Start, or on Cross pressed
  afresh.

An original run from a save at race load gives the same times frame for
frame: countdown from 9800, then 3, 2, 1, and GO at 14800.

## Laps (0x8006137c)

A checkpoint counts only when every one before it this lap has. The last
checkpoint is the finish line, which ends the lap: the lap's time, the best
lap, and the car's race once its laps are run. The grid is behind the line,
as the original's GO screenshot shows, and the first crossing is quietly
ignored (passed nothing yet, so it is not the wrong way either). The lap
distance carries past the lap's length while a car is short of its next
checkpoint (0x8005c3a4, with collision_load's per-checkpoint starts). The
checkpoints a lap come from a front-end table by world and track
(0x800c5c64). The lap length is the BLD header's word, not a lap time.

A first port counted laps in a quiet race (flag 2). The code jumps from the
finish line straight to clearing the checkpoints; the test now randomises
the flag and a mutation confirms it catches this.

## The HUD (0x800638ec)

Glyphs of three overlay fonts (`ACTNFNT`, `ACTNOVL1`, `ACTNOVL2/3`): a glyph
count, 12-byte glyphs (size and the four corners' texture coordinates, the
textures stored turned a quarter), and a 4-bit TIM. They load into VRAM at
the original's slots (0x800bdcc8 tables), so the renderer draws them through
the same texture-page and CLUT path as the track. The HUD shows the
speedometer (mph, compressed past 80, at most 180), the lap's time, a
finished lap's time blinking for 5 s (yellow for the best), LAP n/m, the
place (ranked each step, 0x800408cc), the score in a scoring race, and the
turbo meter with won turbos arriving bar by bar.

Two things only showed when drawn:
- The turbo bars were hidden. The PlayStation adds a primitive at the head
  of its ordering-table entry, so within a layer the last added is drawn
  first: the meter's frame, added after the bars, sits under them. Sprites
  now carry their layer and draw in that order.
- At window sizes other than 4:3, a glyph's far edge could sample the next
  glyph in the atlas, tinted by the text's colour. The PlayStation never
  reaches a sprite's far edge, so sampling is clamped to each sprite's texels.

## Fences (0x8005a4cc, 0x8005a548)

Playing, a car drove between a sign's posts and stuck. SCP table D, never
explained, is the answer: each 20-byte record is a fence, a straight barrier
(centre, direction, normal, half length), listed per sector (+11 count, +14
first). A player's car spanning sectors is tested against their fences: a
separating-axis test (0x8005bcec), then the fence cut to the car's box. The
car moves out along the box's axis of least overlap, and while it moves into
the fence the cut ends become contacts. The differential test places the
car on fences of four saves: 791 pushes and 1098 contacts, all exact.

## Feel

- The grid's computer cars were posed with a guessed quaternion convention,
  so some faced sideways. Every entrant is now built by `Car::load`, whose
  convention loadcheck verified.
- The original reads both pads every 25 ms step (0x8005fba8 inside the step
  loop), as the port does. What judders is the display: 40 steps a second on
  a 60 or 144 Hz screen. The cars and the camera are now drawn between the
  last two steps. Game logic is untouched.
- Real time was counted in whole milliseconds a frame, so the race ran 4%
  slow at 60 Hz and 13% slow at 144 Hz. The remainder now carries over.

**Still unknown:** the results screen (0x8006452c, 0x800644c0) and the replay it plays (0x8007feb0, 0x80080020); the countdown's sprites (0x8001e060); the stunt messages (0x80064dec, 0x80064724, 0x80064564); the power-up icon; computer cars; time-limit races (0x800d0de0); the players' 0x8005c9b4
