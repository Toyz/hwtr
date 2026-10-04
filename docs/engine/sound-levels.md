---
title: Sound levels (the music, the menu's effects, the sliders)
status: solid
discs: US
covers: US CCCPSX.EXE:0x80014b6c music_level, 0x800a5c58 CdMix, 0x800abc08 cd_attenuation, 0x80015908 menu_effect, 0x8001a6dc effects_level, 0x8001a728 voice_level, 0x80092fd0 options_slide, 0x8009311c options_slide, 0x80088828 card_settings, 0x8008a184 default_settings, 0x800d2488 music_level_kept
worklog: 76
---

# Sound levels

Three levels set how loud the game is. The settings record (0x800d1184)
keeps each as 0 to 255: the music at +0x877, the sound effects at +0x878
and the voice at +0x87a.

## The music (0x80014b6c)

The CD audio's level comes from the music setting `m`, in two steps that
each round down:

```
level = min(((m << 12) / 255 * 127) >> 12, 128)
```

The level is kept at 0x800d2488. libcd's `CdMix` (0x800a5c58) then writes
it to the CD drive's attenuation registers (0x800abc08: index 2, then 3,
then apply 0x20): the left into the left and the right into the right, at
`level`, and nothing across. 128 plays the disc as it is.

It is set:

- **On boot and after reading a card.** With the defaults (0x8008a184), or
  with a card's settings when one is read (0x80088828; a failed read sets
  nothing).
- **On the options screen.** Whenever the music slider moves (0x80092fd0,
  0x8009311c), through `iface_game+0xc4`.
- **On the pause menu's music line.** 0x8009f7c8, 0x8009f86c.

## The menu's sound effects (0x80015908)

A menu sound is played at the level its caller passes, in 4096ths, made
127ths (`level × 127 >> 12`), then keyed with `SsUtKeyOn` (0x800a67c4).
Every menu caller passes 4096, so **menu sounds play at full volume
whatever the Sound FX slider says**. The only exception is the slider
itself: letting go of it plays a sound at the new level. The effects
setting is the race's (below).

## The race's levels

- **Effects.** 0x8001a6dc keeps the effects level in 127ths
  (`v × 127 / 255`) at 0x800d24c8. The boot and card paths set it to full
  (255); a race sets it from the effects setting.
- **Voice.** 0x8001a728 keeps half the voice setting at 0x800d24cc. The
  defaults path sets it.

## In the port

`hwtr_game::cd::music_level` is 0x80014b6c's arithmetic. The front end
asks for it (`CdAsk::Volume`) on the same occasions, and the app hands it
to the sound chip's CD input.

`the_music_level_matches_the_original` (hle tests/sound.rs) calls
0x80014b6c with every setting from 0 to 255 and compares the level it
keeps. Settings 249, 251 and 253 come out one lower than a direct
`m × 127 / 255`, which is what the port used before.

## Unknown

- The pause menu's own path to the music level was not checked against
  hle here.
