# The port: how it gets from here to playable

Written 2026-10-03, after the first look at the disc (worklog 1-4). The
approach is the one that took piney_apples from disc to playable, sized for a
much smaller game: one 796 KB executable, no overlays, ~1540 functions of
which a few hundred are PsyQ library.

## The shape of the game

- `SLUS_009.64` boot loader: two EA `.WVE` movies, a legal screen, then
  `CCCPSX.EXE`.
- `CCCPSX.EXE`: front end (`SCREENS.BIG`) and races (one track BIG each).
  The race loader's progress strings name the subsystems in load order:
  overlays (HUD sheets), audio, worlds, anim, best line, controls, collision,
  render, fake AI, cars, camera, world collision, power-ups, game flow, damage
  model, snapshot.
- Music is CD-DA; sound effects and voice are VAB banks on the SPU; no XA, no
  STR.

## Phases

### 1. Tooling to read the code (started)

- [x] Disc, ISO 9660, BIG, PS-X EXE, disassembler, function and string scan.
- [x] `$gp`-relative reference resolution (gp = 0x800d0b48).
- [x] Jump tables and `jalr` targets through interface slots.
- [ ] PsyQ library identification. No signature files are on the machine;
      build our own by recognising each library function by its `$Id`
      string, its BIOS calls, its hardware registers (0x1f801xxx) and its
      shape, then name everything above 0x8009fc10.
- [x] A call graph from `main` (0x80010a5c) down, to order the work; the
      game flow is two state machines (worklog 6), ported (worklog 8).

### 2. An R3000A + GTE interpreter (`hwtr-cpu`) (done) and the whole game in it (`hwtr-hle`) (done)

`hwtr-hle` boots the original from its entry point to a race (worklog 16):
ground truth for any frame. Next for it: sound (libsnd/SPU), the analog pad,
saving and restoring its state to start tests mid-race.

The verification harness. Loads `CCCPSX.EXE`, runs one function with chosen
arguments and memory, stops at return, and reports memory and register
effects. Library calls and hardware access are stubbed or recorded. The GTE
must be exact (flags, saturation, the UNR divide table), since the track and
car geometry go through it.

The test pattern for every ported function: run the original in
`hwtr-cpu` and the Rust port on the same inputs, compare outputs.

### 3. The asset formats (`hwtr-data`, `hwtr-viewer`)

In the order a race needs them: TIM, VAB, `CAR`/`BMF`/`SHD` (cars), `WLD`,
`WLB`, `GLM`, `GLB`, `DLW` (the track), `SCP` and `BLD` (inferred track
splines or build data), `PUP`, `TUNING.PRM`, `DEFAULT.CWH`, the `.OVL` HUD
sheets, `SCREENS.SCR`. Each gets a docs page and a viewer mode. The `WVE`
movies need an EA video decoder (the loader's MDEC path).

### 4. The race

Port the race subsystems in the loader's order, each checked against the
original under `hwtr-cpu`: the game flow and timing, car handling and
physics, collision against the world, the AI and its best line, power-ups,
damage, the camera, then the renderer (GTE math reproduced exactly, drawing
through wgpu rather than a GPU emulation).

### 5. The front end, sound and saves

Menus from `SCREENS.SCR`, the VAB/SPU sound driver and CD-DA music,
memory-card saves mapped to a file.

### 6. The PC side

- Window and rendering: winit + wgpu, native resolution, widescreen as an
  option (the PS1's fixed 4:3 projection is a parameter of the GTE `H`
  register).
- Input: the DualSense on this machine through gilrs (it shows up as a
  hidraw and evdev joystick). The game's pad code maps onto it; the analog
  sticks and triggers map to steering and throttle, rumble to the DualShock
  vibration the game already drives ("Vibration" option string at
  0x800cf428).
- Audio: cpal.
- Frame rate: the game logic runs at the PS1's tick; rendering may
  interpolate later, but only after the logic is exact.

## Cut content

Tracked under `docs/cut/`. Restoring it (the Haunted 1 slot, a fifth world)
is possible once the port owns the track tables, but only from what is on the
disc.
