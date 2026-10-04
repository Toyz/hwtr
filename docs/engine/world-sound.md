---
title: The track's sounds (sources, spots, knocks and triggers)
status: solid
discs: US
covers: US CCCPSX.EXE:0x8011aab0 world_sounds, 0x80016c98 world_alloc, 0x80016d0c world_place, 0x80016e2c world_level, 0x80016e54 world_key, 0x80016d5c world_stop, 0x80017208 world_mute, 0x80019abc voice_alloc, 0x8011aca0 voice_held, 0x800350d4 race_sound_start, 0x800215e0 world_sound_count, 0x800215f4 world_sound_get, 0x8007f5e0 anim_of_sound, 0x8007f59c anim_place, 0x800d0df4 track_sources, 0x80128ef4 spots, 0x80036174 spots_init, 0x80036270 spot_sound, 0x80036474 spot_place, 0x80036424 spot_stop, 0x800d0fd8 trigger_spots, 0x8006a4cc trigger_spots_follow, 0x8006a424 trigger_spots_stop, 0x80036758 race_sound_resume
worklog: 62
---

# The track's sounds

Besides the cars, the race plays sounds of the track's own. They come from
two places:

- the **sources** its world file lists, which loop where they stand, some
  following a moving object
- the two **spots**, which knocked props and trigger zones sound at

Both kinds play through 12 **world sound records** (0x8011aab0, 36 bytes
each):

| offset | type | what |
|---|---|---|
| +0 | s16 | the voice it plays on, -1 for none |
| +2 | s16 | its tone |
| +4 | s16 | its bank: 2 the track's, 0 the effects |
| +6 | s16 | its note |
| +8 | s16 | its program |
| +12 | u8 | it loops |
| +16 | s32[4] | where it is, 20.12 |
| +32 | s32 | its level, 4.12 |

## The track's bank

VAB 2 is named `"%s%d"` from the race's track and number (0x8001924c).
That gives `DESERT1`, for example, as `DESERT1VH`/`DESERT1VB` in the
track's archive. Every race track has one.

## Keying a sound (0x80016e54)

The arguments are a record, a sound, an importance, and whether it loops.

1. A looping record that already plays is left alone.
2. A voice is taken (0x80019abc):
   - A looping sound gets the first voice from the engines' count up to
     the first effect voice that is not held (0x8011aca0, set by every
     key through 0x8001a824 and cleared by 0x8001a8dc). Failing that, it
     gets the first voice there with a lower importance, keyed off.
   - A one-off sound gets an effect voice as other effects do.
3. The sound picks the bank, program, tone and note:

   | sound | bank | program | tone | note |
   |---|---|---|---|---|
   | 0 to 9 | 2 | 0 | the sound | sound + 60 |
   | 10 | 0 | 7 | 0 | 60 |
   | 11 | 0 | 3 | 8 | 68 |
   | 12 | 0 | 4 | 0 | 60 |
   | 13, 14 | 0 | 4 | 1 | 61 |
   | 15 | 0 | 4 | 2 | 50 |
   | 16 | 0 | 4 | 2 | 62 |

   Any other sound keys nothing, though the voice was taken already. Voice
   0 is never keyed either.
4. Keying:
   - A looping sound is keyed silent through 0x8001a824 at the
     importance. The record keeps the voice, the loop flag and the tone.
   - A one-off is keyed straight on libsnd, and its voice is not kept.
     It is heard where the record is (0x80018280) at the record's level.

Other operations on a record:

- **Stopping it (0x80016d5c).** A looping record's voice is let go through
  0x8001a8dc, another's keyed off, either only while it plays. The level
  is cleared to 0 and the voice forgotten.
- **Silencing it (0x80017208).** Its voice's volume is set to 0 while it
  plays. The voice keeps playing, and the next mixer pass raises it again.
- **Allocating one (0x80016c98).** This finds the first record with no
  voice. Nothing is marked, so two allocations in a row return the same
  record.

## The mixer's pass

After the cars, the one-player mixer (0x80017928) goes over the 12 records.
For each one whose voice plays:

- **Its volume.** The volume is the record's level times the side volumes
  from where it is. A looping record's side volumes are first cut to three
  fifths.
- **Its pitch.** The pitch is bent to its note plus the Doppler fine tune
  (0x80018760) for the listener's speed against a still source.

## Sources (0x800350d4)

The world file's 24-byte records (the table at +92) are the sources:
place, flags, sound, volume. As the race's sound starts, after the engines
are keyed, each source:

- takes a record (0x80016c98)
- stores record, flags and sound in the race's list (0x800d0df4, 16
  bytes each)
- with flag 1 follows an animation: the first whose second trigger word
  (the file record's +20) is the source's number (0x8007f5e0, else 0)
- gets the level `volume * 4096 / 255`
- is placed and keyed looping at importance 0

After each frame's mixer, the sound frame (0x800354ac) places each
following source where its animation is (0x8007f59c).

## Spots (0x80128ef4)

Two spots of 8 bytes: a record, a loop flag, the system clock when last
keyed.

- **Their record (0x80036174).** Both take a record after the sources do.
  It is the same record, since allocating marks nothing.
- **A spot sound (0x80036270).** The arguments are a place, a sound, a
  volume and a loop flag.
  1. Nothing plays while the race's sound is shut.
  2. Of the spots that are not looping, the one keyed longest ago is
     taken. If none is older than now, nothing plays.
  3. Its record is silenced, placed, given the level
     `volume * 4096 / 255`, and keyed at importance 1.
  4. The spot keeps the time, and the loop flag if looping.
- **Knocks.** A knocked world volume with flag 4 (0x8006b958) plays a
  one-off at its object: the volume's sound byte (file +76), at its
  volume byte (+80).
- **Triggers.** A trigger zone that starts an animation (0x8006a200)
  sounds where it stands:
  - With flag 8 or 0x20 the sound plays once.
  - With 0x10 it loops, and the spot is kept for that trigger and
    animation (0x800d0fd8, two bytes a trigger, zeroed when the track
    loads).
  - While the animation runs, its spot follows it (0x8006a4cc).
  - When the animation stops, the spot is silenced and freed (0x8006a424;
    the trigger keeps the number).
  - The second animation's spot does any of this only under flag 4.

## The pause, the end and going on

- **The pause (0x80036634).** It silences the sources after the cars'
  voices are let go.
- **The race's end (0x800364cc).** It silences the sources first.
- **Going on (0x80036758).** It keys each source again, which does nothing
  for one still playing.
- **The spots.** None of these touches them.

## In the port

`hwtr_game::world_sound` holds this, kept in `Engines` (`world`, `held`,
`sources`, `spots`, `trigger_spots`).

- The race emits `RaceEvent::Spot`, `SpotsFollow` and `SpotsStop` for
  knocks and triggers.
- The app loads the track's bank, starts the sources and keeps them
  following their animations.

Tests in `crates/hwtr-hle/tests/hits.rs` compare every key, let-go, record,
held flag and spot:

- 3000 keys against 0x80016e54
- 2000 stops and silences against 0x80016d5c and 0x80017208
- 3000 spot sounds against 0x80036270, with the clock hooked
- the mixer test, now with any world records
- DESERT1's sources started from its world file, against the race under
  way

`world_anim.rs` checks the triggers' sounds, volumes and loop flags
against 0x80036270's arguments.

## Unknown

- The two-player mixer's world pass (0x80017268 reads the records once);
  the port mixes the world only with one player.
- The flags' other bits on sources, and the anim's first trigger word.
