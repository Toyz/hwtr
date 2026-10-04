---
number: 32
title: Sound: libsnd's key-on, a sound chip, the menus' effects and music
date: 2026-10-03
area: audio
files: crates/hwtr-game/src/snd.rs, crates/hwtr/src/race.rs, crates/hwtr-game/src/race.rs, crates/hwtr/src/spu.rs, crates/hwtr/src/front.rs, crates/hwtr/src/main.rs, crates/hwtr-game/src/front/title.rs, crates/hwtr-game/src/front/race_start.rs, crates/hwtr-hle/tests/sound.rs
---

# 32. Sound: libsnd's key-on, a sound chip, the menus' effects and music

The game makes sound now: the menus' effects and the music. The race's
sounds come next.

## How the game plays a sound

Everything goes through libsnd's `SsUtKeyOnV` (0x800a67c4):
`(voice, bank, program, tone, note, fine, left, right)`. It works out the
SPU voice's settings and keys the voice on.

**Volume** (0x800ae7e0):
- start from the louder of left and right;
- times the bank's master volume × 16383, over 127²;
- times the program's and the tone's volumes, over 127²;
- panned three times, by the tone's pan, the program's, and the voice's own
  balance (64 when left equals right). A pan under 64 scales the right side
  by pan/63; otherwise the left by (127 − pan)/63.

**Pitch** (0x800ae6b4): the note against the tone's centre note and fine
tune, split into octave and semitone. The semitone's entry (0x800c8f54) is
multiplied by the fine step's (0x800c8f6c), then shifted down by the octave,
rounding; at or above the sample's own octave it is pinned at 0x3fff.

**Start and envelope** (0x800adf0c): the tone's sample start in SPU RAM,
and its ADSR, the release rate made slower by a global (0 here). Each
program's tones live in its own 16-tone block, in the order the programs are
present.

`hwtr_game::snd` ports this. `hwtr-hle/tests/sound.rs` keys random
programs, tones, notes and volumes on the original in `menu-main` (where
`HWMENU` is bank 0) and compares the shadow registers it writes: volume left
and right, pitch, start (allowing for where the bank sits in SPU RAM), and
both ADSR words. All 900 voices that sounded matched.

## The sound chip

`hwtr/src/spu.rs` stands in for the SPU's voices, as the renderer does for
the GPU. It has 24 voices. Each voice:
- decodes PS-ADPCM blocks with their predictor filters and loop flags;
- steps through them at its pitch (0x1000 is 44.1 kHz);
- runs an ADSR envelope through attack, decay, sustain and release, by the
  usual rate, step and exponential rules;
- is mixed at its volume left and right.

It is an rrt `Source` that rrt's audio output plays, and the game sets
voices between ticks. On a PC there is no reason to share 512 KiB of sample
memory, so the port doesn't: each bank's samples are their own buffer, every
bank stays loaded, and a voice plays from a bank at an offset (libsnd's
sample starts, counted from the bank's own beginning). Nothing is reloaded
after a race, as the console has to.

Two differences from the real chip: interpolation is linear rather than the
chip's 4-point Gaussian, and there is no reverb. Sound is only on with a
window (`--shot` runs are silent).

## The menus

- **Effects** (0x80015908): effect `id`'s tone, program and note come from
  a static table (0x800bd5f8, 61 records, copied to 0x8011aec0 by
  0x8001a73c). Every effect plays on voice 1 of bank 0 (`HWMENU`) at full
  volume, so a new one cuts the last. The front end's sound ids (moving,
  Cross, Start, the car and track changes, the race wait) now sound.
- **Music:** the boot picks one of six banks at random (0x800180ac, names
  at 0x800bdbac: electric, hamster, herekity, mondra, outee360, pistel) as
  bank 1. The music is that bank's one long looping sample: voice 0,
  program 0, tone 0, note 60, at 2/5 of the music volume (0x80018118;
  127 from the settings' 255). It starts at the title, stops when a race
  starts (0x8008a6a0, 0x800181ac), and starts again when the race is over.

Tests: the Cross sound sounds for about 0.3 s and stops; `ELECTRIC`'s music
is still playing after 30 seconds.

## The race's effects

The race's bank 0 is `MAINSFX2` from the track's archive (0x8001924c), with
the same effects table. Effects play at 3/8 of the effects volume:
0x8001a6dc turns the settings' 200 into 99 in 127ths, so 37.

Effects take a voice from 0x80019abc:
- the first free one from ten past the number of cars (the voices below are
  the engines') up to 19;
- otherwise the first playing something less important, which is let go.

The pause menu's effects always use voice 19 (0x80015990).

Wired from the race's events, with each call site's id and importance:

| event | effect | importance |
|---|---|---|
| countdown 3, 2, 1 | 6, 7, 8 | 1 |
| GO | 9 | 1 |
| checkpoint | 10 | 0 |
| wrong way | 11 | 0 |
| lap (the line) | 12 | 0 |
| pause | 14 | 0 |
| a player's wreck | 29 | 1 |

Laps and wrong way sound only for players' cars, and not on quiet courses
(0x8006137c).

## Not yet

- The engines (0x800354ac each frame, 0x80045a84 for each car's RPM,
  throttle and place; the engine mixer 0x80017268, 0x80017928).
- Skids, crashes (`CRASHES` 1, 3 or 4, chosen at random), the track's
  ambient bank, the announcer (`DIALOG1` to `DIALOG12`).
- The boom box, the volume settings, reverb.

**Still unknown:** the SPU's master volume as libsnd sets it (0x800a5c58); the Gaussian interpolation table
