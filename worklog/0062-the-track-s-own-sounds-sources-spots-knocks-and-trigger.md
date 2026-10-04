---
number: 62
title: The track's own sounds: sources, spots, knocks and trigger loops
date: 2026-10-04
area: audio
files: crates/hwtr-game/src/world_sound.rs, crates/hwtr-game/src/engines.rs, crates/hwtr-game/src/race.rs, crates/hwtr-game/src/world_anim.rs, crates/hwtr-data/src/world.rs, crates/hwtr/src/race.rs, crates/hwtr/src/main.rs, crates/hwtr-hle/tests/hits.rs, crates/hwtr-hle/tests/world_anim.rs, docs/engine/world-sound.md
---

# 62. The track's own sounds: sources, spots, knocks and trigger loops

The tracks now sound. Each has its own looping sounds where its world puts
them: DESERT1 has nine, and every track has between four and nine. Knocked
props and trigger zones sound at the spots the original uses, in 3D, and a
trigger's looping sound follows its moving object. Before this, the port
played no track sound at all, and knocks played flat as plain effects.

Docs: docs/engine/world-sound.md.

**What was found.**

- **The world file's sound table.** The world file's "24-byte records,
  meaning unknown" (+88/+92) are the track's sound sources. Each holds a
  place, flags (1: follows the animation whose file record's +20 word is
  the source's number), a sound and a volume (0x800215e0, 0x800215f4,
  0x8007f5e0).
- **The track's bank.** VAB 2, the "bank named by the track", is
  `"%s%d"`: `DESERT1VH`/`VB` in the track's archive.
- **12 world sound records (0x8011aab0)** hold voice, tone, bank, note,
  program, loop flag, place and level, with these operations:
  - alloc 0x80016c98, place 0x80016d0c, level 0x80016e2c
  - key 0x80016e54, stop 0x80016d5c, silence 0x80017208
- **Which bank a sound plays from.** Sounds below 10 play the track's bank
  (program 0, tone = sound, note + 60). Sounds 10 to 16 play effects-bank
  tones from a jump table. Looping keys take a voice from the band
  between the engines and the effects, chosen by the held table
  (0x8011aca0: set by 0x8001a824, cleared by 0x8001a8dc).
- **The mixer's world pass.** After the cars, each playing record gets its
  level times the side volumes (three fifths for a loop), and a pitch bent
  by the listener's Doppler.
- **Two spots (0x80128ef4) for knocks and triggers.**
  - A sound takes the non-looping spot keyed longest ago (0x80036270).
  - Both spots share one record: allocating marks nothing.
  - Triggers keep their looping spot per animation (0x800d0fd8). It
    follows the animation while it runs (0x8006a4cc) and is freed when it
    stops (0x8006a424).
- **Pause, end and resume.** The pause and the race's end silence the
  sources; going on keys them again (0x80036758).

**The port.**

- **The data.** `hwtr_data::world::SoundSource` is the parsed record.
- **The engine side.** `hwtr_game::world_sound` holds the records, key,
  stop, silence, mixer pass, sources, spots and trigger spots, kept in
  `Engines`. The engines also gained the held table, kept by every key
  and let-go, and `Bank::Track`. `Change::Bend` now carries its bank.
- **The race.** It emits `Spot`, `SpotsFollow` and `SpotsStop` (knock and
  trigger, with their volumes and loop flags) in place of the flat
  `Knock`.
- **The app.** It loads the track's bank, starts the sources after the
  engines, has the following ones track their animations each frame, and
  routes the spot events. The pause and the end silence the sources
  around the cars, as the original orders them.
- **Testing aid.** Shot mode takes `HWTR_SHOT_SOUND=1` to run the sound
  code on a silent chip.

**Also: the reset's boost flame (0x80041384).** A car put back on the road
now puts its boost flame out (iface_general+0xd8) right after its effects
reset (+0xcc), as the original orders them. The flying wheels' removal
there (0x8007da78) waits for the wheels.

**Checked against the original** (tests/hits.rs, world_anim.rs):

- `world_sounds_key_as_in_the_original`: 3000 rounds over any record,
  sound, importance and loop flag, with any voices playing and held.
- `world_sounds_stop_as_in_the_original`: 2000 rounds.
- `spot_sounds_as_in_the_original`: 3000 rounds with the system clock
  hooked, sometimes with the sound shut.
- The mixer test now runs with any world records and matches.
- `track_sounds_start_as_in_the_original`: DESERT1's sources and the
  spots' record against the race under way.
- The trigger test now checks each sound's volume and loop flag.
- The shared check compares the held table and all world records and
  spots in every sound test.
- **On every track.** With the silent chip, all 11 tracks load their bank
  and key their sources with no errors.

**Still unknown:** The two-player mixer's world pass (0x80017268); the sources' other flag bits.
