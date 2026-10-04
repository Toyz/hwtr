---
number: 31
title: The pause menu, and kits, registries and styles
date: 2026-10-03
area: ui
files: crates/hwtr-game/src/pause.rs, crates/hwtr-game/src/fsm.rs, crates/hwtr-game/src/front/screen.rs, crates/hwtr-game/src/front/mod.rs, crates/hwtr-game/src/front/boot.rs, crates/hwtr-game/src/front/title.rs, crates/hwtr-game/src/front/main_menu.rs, crates/hwtr-game/src/front/race_start.rs, crates/hwtr-game/src/race.rs, crates/hwtr/src/race.rs, crates/hwtr/src/main.rs
---

# 31. The pause menu, and kits, registries and styles

Start pauses the race now, with the game's own pause menu: Continue,
Restart Race (it asks "Are You Sure?"), Options, and Abort Race. Select no
longer quits a race; Abort does, as on the console. The two menu programs
also now share their machinery: a `Kit` trait runs a state machine, a
`Registry` holds each kit's actions, and a `Style` trait says how a family
of screens sets its letters.

## The pause menu

The race runs a second state machine for it (`fsm_second`, 0x800c6640, 37
states) over five small screens (0x800c60d4):
- the menu itself;
- Options, with Sound Volumes and Boom Box;
- Sound Volumes (FX, Music);
- Boom Box (Prev. Track, Next Track);
- "Are You Sure?" (No, Yes).

Their boxes and labels are the front end's 24-byte widgets, upper-cased,
with the pause menu's own way of setting them:
- the race's text font (`ACTNFNT`), raw glyph widths, and the front end's
  kerning table;
- every strip centred and cut to the race's 384-pixel screen (0x8009cc18);
- labels come on from 384 pixels to the right (0x8009d040);
- letters slide either way and jump home within 10 pixels (0x8009d3e4,
  0x8009d38c), where the front end's slide only rightward and downward;
- boxes are blue, labels white, the chosen one red (0x8009ddfc,
  0x8009decc).

The race stops while it is up: no steps, and none owed after, as 0x800346fc
sets the race's clock to now. The HUD is not drawn under it, and the
motors stop.

**How it comes up** (0x800345dc). Start (race action 13; it also counts as
pressed with no pad plugged in) arms a latch once it is let go, and pressing
it while armed asks for the pause. The pause comes up while the race is
starting or running. Leaving the menu clears the latch, so Start has to be
let go again.

**How it ends** (0x80034770, from 0x8009cb74's result):
- Continue: the race goes on.
- Restart: the race ends with code 2, and the front end sets up a new race
  (state 111 again, so a new grid and new opponents).
- Abort: before the start the race ends at once; otherwise it goes to the
  results and leaves them with code 3, and the front end returns to the menu.

Compared against the original paused on `desert1-drive`: the same lines,
colours and positions.

Not yet: the volume bars, the Boom Box's track names, and changing a volume
or track (the audio side).

## Kits, registries, styles

The front end and the pause menu are the same kind of program: a state
machine read from the executable, whose actions are named by the original's
function addresses, stepped until a frame is drawn. So:
- **`fsm::Kit`** has the per-program parts (`def`, `machine`, `actions`,
  `unported`, `blank_over`). Its provided `act` looks an action up, and its
  provided `run_blank` is the shared stepping loop.
- **`fsm::Registry<K>`** maps an address to a `fn(&mut K, &mut Poster)`.
  The front end's handlers register by feature: `front/boot.rs` (boot and
  the card), `title.rs`, `main_menu.rs`, `race_start.rs`. That replaces one
  long `match`. `Registry::missing(def)` lists what the machine names that
  nothing handles. For the front end that is 310 of 408 actions; the pause
  menu's are all registered.
- **`front::screen::Style`** is spacing, alignment, cut, entry and slide.
  `ScreenFont` implements the front end's, `pause::RaceText` the pause
  menu's, and the shared widget code takes either.

This mirrors the original, whose screen code calls through function tables
(`iface_game`, `iface_general`) so one framework serves both.

**Still unknown:** what 0x800d2608 guards in the pause trigger (the port takes it as 0, not checked)
