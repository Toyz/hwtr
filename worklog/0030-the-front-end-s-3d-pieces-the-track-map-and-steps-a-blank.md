---
number: 30
title: The front end's 3D pieces, the track map, and steps a blank
date: 2026-10-03
area: ui
files: crates/hwtr-data/src/scr.rs, crates/hwtr-game/src/front/screen.rs, crates/hwtr-game/src/front/mod.rs, crates/hwtr/src/pieces.rs, crates/hwtr/src/front.rs
---

# 30. The front end's 3D pieces, the track map, and steps a blank

The front end's 3D pieces draw now: the arrows beside the main menu's
changeable lines, the button icons in the scrolling help line, the dialog box
behind the card and save prompts, and the spinning track map. And the stutter
when changing a car is gone.

## The stutter: steps a blank

The original's main loop (0x80010a5c) calls `game_tick`, and so `fsm_tick`,
as fast as it can. Only finishing a frame (0x800839f8, then 0x800122f4)
waits for the vertical blank. So when a state changes through steps that draw
nothing, all of them run within one blank. A car change is states 69, 88, 93
and back to 64, and the original draws every blank through it: the GPU count
in hle goes up about 220 primitives a blank.

The port ran one step a blank, so the picture froze for about six blanks,
then the help line jumped to catch up. `Front::tick` now steps until a frame
is finished, with a cap of 64 steps for states that never draw (loading, or
waiting on a race).

## SCREENS.SCR and SCREENS.GLM

`hwtr_data::scr` reads `SCREENS.SCR` (header, object list, model table,
names; the layout is in its docs):
- 36 named models: the ten track maps, the arrows, the button icons, the
  popup box, and others;
- each model is a tree of objects (rotation, position, child, sibling);
- each object holds 76-byte textured quads: four corners with uv, four
  normals, a colour, a texture index, and flags (both-sided, depth bias).

`SCREENS.GLM` is seven 8-bit 128x64 TIMs. Image `i` goes where the table at
0x800bddbc says, with its palette at (640, 450 + i). At load, 0x800280d4
shifts each quad's uv by its image's place in the texture page.

## Pieces on the screens

A screen's pieces (16-byte records) name a model, a resting place in the
front end's 1000-wide coordinates (0x800813fc and its twins turn that into
world units), an entry and a size in percent. They come on, slide (the same
fraction as the letters) and go off with the screen (0x80082590, 0x80082c20,
0x800827b8). Drawing them is 0x80083ac0:
- a camera 866 units back;
- the place pulled toward the camera in proportion
  (`k = (866·4096 − z) / 866`), so a piece's x and y land where its
  coordinates say whatever its depth;
- the scale, then the turn about x, y and z;
- each object's own placement;
- the front end's view, diag(1, −1/2, −1), with H 554 and centre
  (320, 120), all read from the live state.

The renderer (`hwtr/src/pieces.rs`) does that chain in the game's fixed
point (products shifted 12 and saturated, as the GTE does), then divides in
float for the screen. It culls quads facing away unless both-sided, drops
far ones, and draws back to front under the letters. Each corner is lit as
NCC does:
- the light is (0, −4096, −7094) (0x80015584), turned by the object;
- the colour matrix gives half of that light on each channel (0x800bd070);
- the back colour is 32 (0x800a2e08 at boot).

The main menu's arrows (0x8008be00, 0x8008bc94) sit beside the car, track,
sign-in and mode lines and shrink away elsewhere. Compared against the
original with the cursor on Cars, they match in place and size.

The help line's `^ * # % ]` are Triangle, Circle, Square, Cross and R1
icons (0x8008664c), placed from the line's pixel position.

## The track map

The main menu's track map (0x800859ac) is the chosen track's model
(0x8008c7fc, by its file name; the lookup ignores case). It has its own
motion:
- it turns 2/5 of a turn a second;
- it eases to half size at (1070, 950), and grows to 3/4 at (875, 800)
  while the track line is chosen;
- it is scaled, then placed (the place scaled too), tilted −1.21 radians
  about x, and turned about z.

## Not yet

The car preview in the player's box (0x80084bcc) draws the car's own model
with its textures, which needs the race's car loading in the front end's
VRAM layout.

**Still unknown:** the car preview's own transform past its spin and tilt
