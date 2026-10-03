---
number: 10
title: The platform layer: a window, a 4:3 presenter, and the DualSense as a PlayStation pad
date: 2026-10-03
area: input, render, build
files: crates/hwtr/src/main.rs, crates/hwtr/src/present.rs, crates/hwtr-input/src/lib.rs
---

# 10. The platform layer: a window, a 4:3 presenter, and the DualSense as a PlayStation pad

The port's binary, `hwtr`, now has its platform half: winit for the window,
wgpu for the picture, gilrs for the pad. It runs the PlayStation's NTSC
field rate (59.94 Hz) as its frame clock, independent of the display's
refresh, catching up at most four frames after a stall.

For now it only proves the plumbing: it reads the disc (the CUE under
`work/disc`, or `--cue`), takes the legal screen from the file system and the
title, main menu and loading backgrounds from `SCREENS.BIG`, and shows them
one after another on Cross or Start. Circle runs both motors.

## The picture

`Presenter` draws one picture at the PlayStation's resolution into the
window, scaled to the largest 4:3 rectangle that fits, letterboxed, with
nearest-neighbour sampling. The TV showed 640 x 240 and 320 x 240 alike as
4:3, so the presenter fixes the aspect rather than the pixel shape. Colours go
through an sRGB texture into an sRGB surface, which leaves the PlayStation's
values as they were.

## The pad

`hwtr-input` keeps the pad as the game will want it: the sixteen buttons in
the PlayStation's report order (Select bit 0 through Square bit 15, 1 meaning
pressed) and the four stick bytes (0x80 at rest, 0 left or up). It takes the
gamepad that last sent an event, falling back to the first connected one, and
merges the keyboard (arrows, Z X A S, Q W, 1 2, Enter, Shift). gilrs' own
filters are off, since its dead zone rescales the stick and the game applies
its own. Rumble uses gilrs force feedback, a strong and a weak effect for the
DualShock's large and small motors, the same scheme piney_apples uses.

Run on this machine, gilrs reports the DualSense on USB as "PS5 Controller"
and the window and GPU come up under Wayland.

## A correction to [[9]]

[[9]] says the engine banks hold "two programs and 32 tones". They hold two
programs with one tone each (`ELECTRC.VH`: 2 programs, 2 tones, 2 samples);
the 32 was the number of tone slots, 16 per program, not tones. The menu's
per-car banks hold one program with one tone, and `HWMENU.VH` two programs
with ten.

**Still unknown:** How the game reads the pad (PsyQ libetc or libpad, digital or analog mode) and drives vibration; until that is traced the pad layer only offers the raw state. The DualSense's right stick rests at 132-133 rather than 128 on this pad.
