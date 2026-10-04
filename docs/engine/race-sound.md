---
title: The race's sound: engines, hits and tyres
status: partial
discs: US
covers: US CCCPSX.EXE:0x8001924c sound_race_init, 0x80019870 iface_sound_fill, 0x80019abc voice_alloc, 0x800354ac sound_frame, 0x80045a84 car_sound_input, 0x80017928 mixer_one, 0x80035a88 contact_sound, 0x80035c7c pair_sound, 0x80016280 impact_on, 0x80016820 scrape_on, 0x800169a8 scrape_off, 0x80016a18 crash_on, 0x80016490 tyres_on, 0x800167b0 tyres_off, 0x80015b88 tyres_volume, 0x80015bcc scrape_volume, 0x800364cc race_sound_off, 0x80036634 pause_sound_off, 0x80016004 engine_off, 0x80016078 wreck_on, 0x800161f4 wreck_off, 0x80015b44 wreck_volume, 0x80016c0c crash_off, 0x80016404 impact_off, 0x8001a824 voice_key, 0x8001a8dc voice_off, 0x8003bebc camera_shake, 0x80036484 commentary_ask, 0x80019754 dialog_play, 0x800d2628 commentary, 0x8011a840 car_sound, 0x80128e94 car_sound_state, 0x800be888 tyre_effects, 0x800be8a4 contact_effects, 0x800bea6c ground_priority, 0x800d0c28 tyre_voices, 0x800d0c2c scrape_voices
worklog: 48, 51, 60, 62
---

# The race's sound

During a race every sound but the music goes through the sound
interface table at 0x8012fe54, filled by 0x80019870. Each car has two
records:

- a 104-byte one at 0x8011a840: where the car is heard from, levels and
  voices
- a 16-byte one at 0x80128e94: the tyres' and scrape's state

The port keeps both as `engines::CarSound`.

## Set-up

`race_load` calls 0x80034fd0 before anything else that draws random
numbers. Through 0x8001924c it opens these banks:

| VAB | bank |
| --- | --- |
| 0 | `MAINSFX2`, the effects |
| 1 | one of `CRASHES1`, `CRASHES3`, `CRASHES4`: `rand()%4` is drawn until it is not 1, then 1 is added |
| 2 | the track's own, named `"%s%d"` from its name and number (`DESERT1`); see [the track's sounds](world-sound.md) |
| 3 | `DIALOG1` to `DIALOG12` (`rand()%12 + 1`) |
| 4+ | each car's engine bank |

The port draws both random numbers in `Race::new` before `fakeai_load`'s,
and the app loads the crashes bank.

## Voices

| voices | for |
| --- | --- |
| 0 to cars-1 | each car's engine |
| cars + player | a player's overrun |
| 10 past the cars' count (0x800d24d2) to 19 | effects |
| 20, 21 | player one's and player two's scrape (0x800d0c2c) |
| 22, 23 | player one's and player two's tyres (0x800d0c28) |

`voice_alloc` (0x80019abc, first argument 0) takes:

1. the first stopped voice in the effects range
2. failing that, the first one whose recorded importance (0x8011acc0) is
   below the asked one, letting it go

Keys made through 0x8001a824 record an importance: 1 for the engines
(0x80015ebc), 0 for the tyres and the scrape, the asked one for an effect
(0x800157f8). The impact, the crash and the wreck key libsnd directly, so
their voices keep whatever importance was there before.

Voices are let go in two ways:

- **0x8001a824's partner (0x8001a8dc).** It clears the importance to 0.
  The engines (0x80016004, only while the engine voice plays), the tyres
  (0x800167b0) and the scrape (0x800169a8) are let go this way.
- **The release.** The wreck (0x800161f4), crash (0x80016c0c) and impact
  (0x80016404) voices are let go only if they play. Each is given back at
  importance 255 and forgotten.

## A wreck (0x8004619c)

1. For a player's car, after its jolt, effect 29 plays at importance 1
   (0x800157f8), and its camera shakes for 250 ms (0x8003bebc).
2. For any car:
   - Its engine is let go (0x80016004).
   - Its wreck level (car sound +0x34) is set to 4096 (0x80015b44).
   - Effect 29 is keyed on an effect voice at importance 0 (0x80016078).
     It is heard from the car: the level and Doppler of 0x80018280 and
     0x80018760, scaled by the wreck level and halved. The voice is kept
     at +0x48 (-1 for none).
3. The mixer keeps that voice at the wreck level scaled each frame.
4. A contact that changes the car's scrape releases the wreck voice.

## The pause and the race's end

The pause (0x80036634, then effect 14) and the race's end (0x800364cc)
let go every car's engine, wreck, tyres, scrape, crash and impact voices,
in that order. The world's sounds stop too; the pause then pauses the
music (0x80014a0c).

## Each frame (0x800354ac)

For each car, in order:

1. **Input.** `car_sound_input` (0x80045a84) reads the revs, centre and
   velocity, and the tyres:
   - `ground`: of the wheels that are down, the ground kind ranked highest
     by 0x800bea6c
   - `roll`: the share of wheels down, times speed over 100 mph, at most
     4096
   - `skid`: the share of wheels down and slipping
   - A wrecked car has no revs and no tyres.
2. **Tyres** (slots up to the players' count only):
   - The effect is picked from 0x800be888 by ground: its first entry
     while rolling, its second while `skid >= roll`.
   - When the effect changes, the old voice is let go (0x800167b0) and
     the new one is keyed by `tyres_on` (0x80016490).
   - `tyres_on` keys only with one player, and only for a player's car.
     It keys at:
     - a fifth of the level for effect 16
     - three fifths for effect 23
     - a third otherwise
   - The volume is `max(roll, skid)`, at most 4096 (0x80015b88).
3. **Scrape timer** (16-byte record +12): it counts down by the frame's
   milliseconds. When it runs out, the scrape voice is let go
   (0x800169a8).

After the cars, the listener is taken from player one's camera. Then the
mixer runs (0x80017928 with one player, 0x80017268 with two).

## Hits

- **A contact with the track:** at a car's first contact of a collision
  step, `collision_update` calls `contact_sound` (0x80035a88). It is
  skipped once the race's sound is shut (6875(gp), set by 0x800364cc at
  the race's end).
  - The volume is the car's speed in mph (`speed * 232 >> 12`), clamped
    to 20..100, over 100.
  - 0x800be8a4 gives the surface's pair of effects: impact and scrape.
  - **With the scrape timer at 0:** the impact plays (0x80016280), then
    the scrape is keyed (0x80016820).
  - **With the timer running:** only a change of scrape matters. The
    original then stops voice +0x48 (0x800161f4, which nothing keys
    here) and keys the new scrape.
  - Either way the scrape volume becomes the contact's volume and the
    timer 200 ms.
- **The impact (0x80016280):** keyed on an allocated voice at importance
  0, at half its level. With no voice to be had it keys voice -1, which
  plays nothing.
- **The scrape (0x80016820):** player cars only, on their own voice, at
  an eighth of the scrape volume. It uses the volume from before this
  contact.
- **Two bodies meeting:** the first thing the pair loop does is
  `pair_sound` (0x80035c7c).
  - The sound is heard on the later car of the two (the larger car
    pointer). The bodies' relative speed decides the crash:

    | speed | kind | volume | tone |
    | --- | --- | --- | --- |
    | 10 mph or less | none | | |
    | under 20 mph | 2 | speed over 20 mph | `rand()%4` |
    | under 50 mph | 3 | speed over 50 mph | `rand()%2 + 4` |
    | 50 mph or more | 4 | `speed / 2304` | `rand()%2 + 6` |

  - The tone is drawn in 0x80016a18 before a voice is looked for, so it
    is drawn whenever the crash sounds. A voice is taken at importance 1.
  - The crashes bank is keyed with effect `kind`'s program, the drawn
    tone, and note and fine tune both `tone + 60`. The volume is a third
    of the level.

## The commentator

The commentator's state is at 0x800d2628: the line, a byte that is 0
while a line waits, and the time since the line was asked for or spoken.
`race_load` sets it to nothing waiting.

`commentary_ask` (0x80036484) is called with one of these lines:

| line | asked for by |
| --- | --- |
| 0 | a player's wreck, half a second in (`cars_update`) |
| 1 or 2 | a scored stunt, after drawing two random numbers (0x8003d4e8) |
| 3 | the tenth turbo before any turbo was used (0x8003c850) |

- Line 3 always takes the place.
- Any other line is ignored while a line waits, or within 1000 ms of
  the last.

The race's sound frame (0x800354ac) runs after each frame's steps. It
skips everything once the race's sound is shut. Otherwise:

1. It adds the frame's milliseconds to the timer.
2. A line that has waited 301 ms is spoken, and the timer is reset.
3. Line 3 plays effect 57 at importance 1.
4. Any other line goes to `dialog_play` (0x80019754):
   - It takes an effect voice at importance 0.
   - It draws `rand()%2`: lines 0, 1 and 2 use tones r, r + 2 and r + 4.
     Any other line draws `rand()%6` for its tone.
   - It keys the dialog bank (VAB 3) at program 0 with that tone, note
     tone + 60, fine tune 0, at three eighths of the effects volume.
   - It draws the numbers even when no voice is free.

These random numbers come from the game's generator, so the port draws
them in `Race::sound_frame`, which the app calls between the steps and
the rest of the frame.

## Levels

A sound's level comes from where its car was heard last:

- 0x80018280 gives the side volumes from the distance and side to the
  listener.
- 0x80018760 gives the fine tune, a Doppler shift of the car's last bend.

Each frame the one-player mixer:

1. skips a car none of whose voices play
2. otherwise takes the car's level times 2/5 a side
3. sets the voices that play from it:

| voice | volume |
| --- | --- |
| engine | as before (0x8001a36c) |
| tyres | tyre level scaled; for the first two cars, a fifth if keyed by effect 16, three fifths if by 23 |
| scrape | scrape level scaled, on the player's scrape voice |
| wreck | wreck level scaled (set right after the engine's bend) |
| crash | a third |
| impact | impact level scaled, halved |

## Unknown

- The two-player mixer's handling of the hits' voices.
- What the dialog bank is played for.
- Whether props' bodies give a velocity (+100) to the pair sound; the
  port takes none.
