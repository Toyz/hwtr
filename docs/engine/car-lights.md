---
title: The car's lights (exhaust glows, headlights, tail lights, darkening)
status: solid
discs: US
covers: US CCCPSX.EXE:0x80029fb0 car_draw_glows, 0x8002a81c car_draw_beams, 0x8002ad48 car_fade_light, 0x8002bb0c car_aim_lights, 0x8002bad0 car_set_glow, 0x80021f60 car_upload_lamp_clut, 0x800291b4 car_light_mask, 0x80028fb0 car_effect_matrix, 0x80010678 quad_list_draw, 0x80022cd0 fxp_parse
worklog: 58
---

# The car's lights

A car has four kinds of light. Its FXP gives the lamps (see
[the car formats](../formats/car.md)), and its view state (cvs) holds what
they are doing.

| cvs | type | what |
|---|---|---|
| +0x14 | s32 | the model's scale, 4.12 (4096; 8192 or a third under two cheats) |
| +0x20 | u32 | what the car shows: bit 0x10 glows, bit 0x20 headlights |
| +0x2c, +0x2d, +0x2e | u8 | how many glows, headlights and C records |
| +0x30 | 16 bytes each | a glow: pointers to its place, its stretch, and two words |
| +0x130 | s32 | the glows' strength, 4.12 |
| +0x134 | 12 bytes each | a headlight: pointers to its place, its direction, a third vector |
| +0x168 | 20 bytes each | a C record (unused by the draw) |
| +0x1e9 | u8 | the body colour the fade heads for |
| +0x1ea, +0x1eb | u8 | the headlights' level: the target, then the level now |
| +0x1f1 | u8 | the headlights' fade runs |
| +0x1f2 | u8 | the brake lights are on |

0x800291b4 sets +0x20 when the car loads. A player's car gets every bit
except those of lamps it lacks: 0x10 with no glows, 0x20 with no headlights,
0x100 with no C records. Every other car gets 0xfffffe5f: glows, but no
headlights.

## Each frame's end (0x80049ecc)

For every car, shown or not, after its pose:

- **Glow strength (+0xbc, 0x8002bad0).** The engine's share of its band,
  `(rpm - idle) / (redline - idle)` in 4.12, doubled less one. Nothing
  shows below half revs, and full strength comes at the redline. A wrecked
  car (car +0x62c) has none.
- **Light targets (+0xc4, 0x8002bb0c).** Only for a player's car (cvs
  +0x1ee) that is not wrecked (cvs +0x28 = 1). The input is the car's word
  +0x8, whose bits the zone it is in sets (1 from zone flag 0x10, 2 from
  0x8) along with the brake (4):
  - With bit 1, the body heads for 48 and the headlights for 112.
  - With bit 2, the headlights head for 112 and are shown (+0x20 bit 0x20).
  - Bit 4 is the brake lights.
  - With the word 0, the body heads for 128, the headlights for 0, and the
    brake lights go off.
  - Then the headlights' fade runs (+0x1f1 = 1).

## The car draw (0x80022064)

After the shadow, in this order:

1. **The glows (0x80029fb0).** These need fx_enable bit 0x10, +0x20 bit
   0x10, a shown car and no wreck. For each glow with strength:
   - Its size is `15 * strength`, times a flicker, times the scale.
   - The flicker is `rand(1375) * 4096 / 1000 + 512`: one random draw per
     glow per draw. It is 4096 while paused (0x800d261c = 1) or in the
     frozen results (draw mode 3), and then nothing is drawn from the
     random sequence.
   - The glow is a triangular prism (0x800bde14, sides 0x800bde5c). Its
     matrix is the car's moved to the glow's place plus (2, -15, 1.5). The
     back three corners move by the glow's stretch vector times the size.
   - Car 36 has a hexagonal prism instead (0x800bde68, 0x800bdef8), at
     eight times the size and placed by its own offset.
   - It is drawn as three quads (six for car 36) through 0x80010678:
     sheet 4, grey 128, additive, two-sided.
2. **While the headlights' level (+0x1eb) is above 0:**
   - **The beams (0x8002a81c).** These need fx_enable bit 0x20, +0x20 bit
     0x20, a shown car and no wreck. Each headlight's matrix is placed 40
     units ahead of the lamp.
     - The beam has three quads (0x800bdf10, faces 0x800bdf70) on sheet 17,
       coloured at the level, additive.
     - The far corners (2, 3, 6, 7) are pushed 170 units along the lamp's
       direction, then scaled 3.5 across and 1.5 along and up.
   - **The body's fade (0x8002ad48 with 0).** The drawn model's root colour
     steps 5 toward +0x1e9, to at most 128.
3. **The headlights' fade (0x8002ad48 with 1), while +0x1f1 is set.** The
   level steps 4 toward +0x1ea, to at most 112.
4. **The tail lights (0x80021f60).** The car's palette's first seven
   colours are loaded at (384, 464 + slot), as the skin has them while
   +0x1f2 is set. Otherwise each colour is halved:
   `(c >> 1) & 0x3def | 0x8000`. 0x80021b88 builds both tables when the
   car loads (0x8011d410, 0x8011d470).

The fade (0x8002ad48):

- **Wrecks and the frozen results.** On a wreck nothing moves and a
  headlight fade stops. In the frozen results nothing moves, though a
  fade already at its target still stops.
- **At the target.** Once the value equals the target, a headlight fade
  stops (+0x1f1 = 0).
- **Going up.** The value steps up until a step would reach the limit.
  Then it is set to the limit, the target is cleared to 0, and a headlight
  fade stops.
- **Going down.** The value steps down until a step would reach the
  target. Then it is set to the target and the target is cleared to 0.
  For the headlights, the level goes to 0, they stop being shown (+0x20
  bit 0x20 cleared), and the fade stops.
- **The next frame.** The next frame's targets set the target again, so a
  value at rest stays put.

The body colour is the root node's colour (+0x44) of the model drawn, and
each detail level has its own. Charring and the boost pulse touch only the
full model's.

## In the port

`hwtr_game::lights` has all of this:

- `Lamps` (from `hwtr_data::car::fxp`) and `Lights`, kept per car in the
  effects as `Effects::lights`.
- `glow_level` and `Lights::aim` run at the frame's end.
- `glows`, `beams`, `Lights::fade` and `tail_lights` run in the race's car
  draw.
- The app loads the tail lights' palette into the renderer's VRAM.

Tests: `crates/hwtr-hle/tests/lights.rs`.

- **The FXP's records.** The test parses the FXP from memory, as
  fxp_parse left it, and checks its records against the view state's.
- **The off palettes.** The test checks them against 0x8011d470.
- **The frame's end.** 900 rounds of 0x80049ecc for every car, with any
  revs, zone bits, brake, wreck and light bytes, compare the glow strength
  and every light field.
- **The car draw.** 4500 rounds compare:
  - each glow's and beam's matrix place and corners, hooked at 0x80015128
    and 0x80010678
  - the lights after the draw
  - the drawn model's colour
  - the palette loaded
  - the random seed

  The rounds cover any lights, glow strength, wreck mark, body colour,
  pause, draw mode and the medium or low model.

## Unknown

- What the C records (44 bytes, +0x168) light; the draw never reads them.
- Why only car 36 has a hexagonal glow.
