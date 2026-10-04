---
title: Cheats (the button codes and the race options)
status: partial
discs: US
covers: US CCCPSX.EXE:0x8007f87c code_apply, 0x8007f710 cheat_code, 0x8007f79c car_code, 0x800bee6c cheat_codes, 0x800d1004 car_codes, 0x80088370 car_name_to_id, 0x80086ad0 code_press, 0x80086a70 codes_clear, 0x80086aa0 codes_clear_2, 0x80136c70 codes, 0x8008cff4 menu_pad, 0x8008d12c menu_pad_2, 0x80013ff8 cheat_on, 0x80013f00 race_setup_apply, 0x800d2468 cheats, 0x80021888 car_bind_state, 0x80022064 car_draw, 0x80022cd0 fxp_parse, 0x8002bdb4 effects_init, 0x80012a10 tim_upload, 0x800d244c tim_fill_on
worklog: 84, 85, 86, 87
---

# Cheats

A player's cheats are a word in their profile (+0x3c). The race takes
player one's, or player two's if one has none, as its options (the race
setup's +0x1c). 0x80013f00 (reached through an interface table, set at
0x8001e660) copies the setup's options to 0x800d2468, and `cheat_on(bit)`
(0x80013ff8) tests the low byte there.

## Entering a code

On the main menu each pad keeps its last eight code buttons as digits
(0x80136c70 for the first pad, 0x80136c80 for the second; 0x80086ad0
shifts one in):

| digit | button | action |
| --- | --- | --- |
| 1 | Square | 20 |
| 2 | Triangle | 21 |
| 3 | L1 | 22 |
| 4 | R1 | 23 |
| 5 | L2 | 24 |
| 6 | R2 | 25 |

After any press (0x8008d010 for the first pad, 0x8008d14c for the second),
the eight go to 0x8007f87c with that pad's player:

1. **The cheat codes** (0x8007f710). The table at 0x800bee6c holds ten
   codes, one per bit. The first that matches makes the player's cheats
   that bit alone; any cheat before is dropped.
2. **The car code** (0x8007f79c). One entry at 0x800d1004: a code and a
   car's file name. A match gives the player the car (its bit in +0x14 or
   +0x18, by `car_name_to_id`, 0x80088370).

On a match the menu plays effect 52 and fills the eight with spaces
(0x80086a70, 0x80086aa0).

| code | gives |
| --- | --- |
| 77777777 | cheat 1 |
| 16522561 | cheat 4: small cars |
| 34563456 | cheat 8: flat cars |
| 64561234 | cheat 16: the DUDE sounds |
| 12124455 | cheat 32: big wheels |
| 63124536 | cheat 64 |
| 12345612 | the car `towjam` (also the password TWJM) |

Slots 1 and 7 to 9 hold 77777777 too, so slot 0 always wins. **Cheats 2,
128, 256 and 512 cannot be had.**

## What each bit does

| bit | what it does | where |
| --- | --- | --- |
| 1 | nothing found: no `cheat_on(1)`, and no other read of the bit | |
| 2 | (cannot be had) a model scale of 2, the scaled draws below, and the effects turned off under 2 or 4 | |
| 4 | **small cars**: a third of the size (below) | |
| 8 | **flat cars**: each car's skin is one colour, its first texel's | 0x80012ac4 |
| 16 | **the DUDE sounds**: the race's effects bank is DUDE, every effect one of its eight tones | [race sound](race-sound.md) |
| 32 | **big wheels**: wheels twice the diameter, and drawn twice the size; glows and headlight beams twice as big | |
| 64 | nothing found: no `cheat_on(64)` | |
| 128 | (cannot be had) the race's flag 16; the collision's 0x80 (0x8005c470) | |
| 256 | (cannot be had) the race's flag 0x40 | |

**Small cars (4).**

- **The body.** 0x8004528c makes the car's size, its origin, its wheel
  mounts and its wheel diameters a third (`4096 / 3` in 4.12).
- **The model's scale.** The draw's scale (cvs +0x14, set by 0x80021888)
  is 1365, a third. Under cheat 2 it is 8192 instead.
- **Drawn scaled.** Under 2 or 4, these are drawn at the model's scale:
  the car (0x80022274, a matrix times the scale), its light points
  (0x80022cd0) and its shadow (0x80029728). The glows (0x80029fb0) are
  always drawn times the scale, whatever the cheat.
- **Effects.** The skid marks, headlight beams and boost flame are off
  under 2 or 4 (see [effects](effects.md)). Under 4 the puffs are drawn
  28 smaller.

**Big wheels (32).** 0x8004528c doubles each wheel's diameter. The scale
(cvs +0x14) is 8192. The car draw (0x8002269c) scales each wheel node's
own rotation (the model's +0x8 nodes, 72 bytes each) by it under 32
alone. The glows are twice the size, and the headlight beams reach twice
as far (0x8002a81c).

**How the car draw scales (0x80022064).** Under 2 or 4 the body node's
rotation (the model's +0x4) is scaled in place, each column times the
scale. A wheel on the car is drawn through the body's matrix, so it
shrinks or grows with it, its place included. A wheel off the car (its
bit in cvs +0x1e8) is drawn through the car's world matrix alone
(0x8002295c), at its own size.

## In the port

`Tables::apply_code` (hwtr-game, front/mod.rs) is 0x8007f87c. The main
menu's pad handling runs it after each press, as 0x8008d010 does.

`codes_are_taken_as_the_original_takes_them` (hle tests/password.rs)
calls 0x8007f87c with 400 codes against random profiles: every cheat
code, the car code, and random digits of every length. It compares the
answer, the cars and the cheats. `a_code_pressed_on_the_main_menu_is_taken`
presses 16522561 on the original's main menu, frame by frame from
`menu-main`, and checks player one's cheats and the spaced buffer.

Ported:

- the small cars' and big wheels' bodies (car/load.rs)
- the effects turned off, and the small cars' puffs
- the DUDE sounds
- the model's scale in the draws (`car::draw::model_scale`,
  `body_scaled`, `wheels_scaled`): the car and its wheels (the app's
  `car_triangles`), its shadow, its glows' places and sizes, and its
  beams

The shadow, glows, beams and glow places are checked against the
original under the cheats (car.rs, lights.rs). The car model itself is
drawn by the app's renderer, so its scale is not checked against hle.

`hwtr --track NAME --cheats BITS` races under cheats directly.

**The flat cars (8).** `tim_upload` (0x80012a10) fills the image with
its first byte before LoadImage when all of these hold:

- its fill is on (0x800d244c; off only around SFX.GLM and one sprite
  upload)
- cheat 8
- the caller's last argument is 0

Only two uploads pass 0, both of a car's skin: car_load_race's
(0x80021c74) and 0x80023aa8's. So the cars go one colour each. The
track's textures go up through glm_load's own LoadImage and are not
touched. The fill counts bytes from the image's halfwords: 4-bit (the
first texel's nibble twice) and 8-bit fill the whole image, 16-bit only
its first half. The skins go up as 8-bit.

`car::draw::flat_skin` is the fill. The app puts each car's flattened
skin in place under cheat 8. `the_flat_cars_fill_as_the_original` (hle
tests/tim_upload.rs) runs tim_upload on the Deora's skin at each depth,
with the cheat, the fill and the keep argument on and off, and compares
the image it leaves.

## Unknown

- What cheats 1 and 64 were for; nothing in this build reads them.
