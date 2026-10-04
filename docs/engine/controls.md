---
title: Controls and the pad
status: partial
discs: US
covers: US CCCPSX.EXE:0x8001bec0 controls_frame, 0x8001bf1c, 0x8001c998, 0x8001cee4 controls_read_analog, 0x8001d9ec curve, 0x8001b170 action_level, 0x8011b2b8 action_levels, 0x8001d320 controls_init, 0x8001c344 controls_read_port, 0x8001c480, 0x8001cee4, 0x8001abe8 controls_action_held, 0x8011b388 pad_buffer, 0x8011b3d8 control_mapping, 0x800bdc08 control_mapping_default, interface table 0x8012fcdc, 0x8001b7b4 motors_road, 0x8001b934 motors_jolt, 0x8001ccc4 motors_fade, 0x8005fba8 race_controls_frame, 0x8005fd4c wall_jolt, 0x8005fed0 pair_jolt, 0x800d0e2c jolt_wait, 0x8006545c hud_button, 0x80034940 race_controls
worklog: 11, 63, 79, 88
---

# Controls and the pad

The controls module publishes itself in the interface table at 0x8012fcdc
(21 slots, filled by 0x8001bd88) and reads the controllers through PsyQ's
libpad, which polls both ports from the vertical blank interrupt. The game
never calls the BIOS pad functions.

## Start-up

`controls_init` (0x8001d320, from system init):

```
PadInitDirect(0x8011b388, 0x8011b3aa)   receive buffers, 34 bytes per port
PadStartCom()
0x800a38cc(5)
controls_port_reset(0), controls_port_reset(1)
copy control_mapping_default (0x800bdc08, 116 bytes) to both players'
    control_mapping (0x8011b3d8 and 0x8011b44c)
```

## The receive buffer

libpad's direct-mode buffer, one per port (port 1 at 0x8011b388, port 2 at
0x8011b3aa):

```
+0   u8   status        0xff: no controller
+1   u8   type          high nibble the kind, low nibble half-words of data
+2   u8   buttons lo    inverted: Select L3 R3 Start Up Right Down Left (bit 0..7)
+3   u8   buttons hi    inverted: L2 R2 L1 R1 Triangle Circle Cross Square
+4.. u8   analog data   by type
```

`controls_read_port` (0x8001c344) dispatches on the type byte:

| type | controller | handled by |
| --- | --- | --- |
| 0x41 | digital pad | 0x8001c480 |
| 0x73 | DualShock, analog mode | 0x8001cee4 |
| 0x53 | analog joystick | 0x8001cee4 |
| 0x23 | neGcon | 0x8001cee4 |

Any other type: the port is reset (0x8001d3fc) unless the byte at
`0x800d24f4 + port` is set, it is read as digital, and then also as analog
(with its type) when the type's low nibble is 2 or more.

## Actions

`controls_action_held(action)` (0x8001abe8) answers whether game action
0-27 is held, through a 28-case switch; the cases seen test port 1's pad
and then port 2's.
Each case picks a byte of the receive buffer, inverts it and ANDs it with the
action's mask in the player's mapping table, `control_mapping + config *
0x74 + action * 4` (the config index is the byte at `$gp + 280` for player 1
and `$gp + 281` for player 2). Some cases read an analog byte instead when the
type is 0x23, the neGcon.

The default masks (`control_mapping_default`, 29 words) are single bits.
For the race's actions 0 to 12 they are, checked by holding each button in
the original and reading the car's control fields:

| action | default | level kept at (0x8011b2b8 +) | car field |
| --- | --- | --- | --- |
| 0 steer left | Left (low byte 0x80) | +6 | steer, negative |
| 1 steer right | Right (0x20) | +4 | steer, positive |
| 2 accelerate | Cross (high byte 0x40) | +2 | +0x14 |
| 3 brake | Square (0x80) | +0 | +0x18 |
| 4, 5 stick right, left | Right, Left | +8, +0xa | +0x1c (right - left) |
| 6, 7 stick down, up | Down, Up | +0xc, +0xe | +0x20 (down - up) |
| 8 handbrake | L2 (0x01) | +0x10, 0 or 1 | +0x24 |
| 9 reset (back on the road) | R1 (0x08) | +0x11 | +0x25 |
| 10 turbo | R2 (0x02) | +0x12 | +0x26, +0x27 (held) |
| 11 change view | Circle | +0x13 | the camera's view button |
| 12 HUD on/off | Triangle | +0x14 | the HUD's show mask (0x8006545c) |

## The levels

Once a frame per port, `controls_frame` (0x8001bec0, from 0x8001b0ec with
the milliseconds since the last read) reads actions 8 to 12 as on or off
(0x8001bf1c), then by the pad's type:

- **Digital (0x41), 0x8001c480 and 0x8001c998.** Accelerate and brake are
  255 or 0. Steering ramps while held: +40 a frame below 75, then
  +floor(1.8 x ms), at most 255; pressing one way drops the other's level,
  and left is tested after right, so it wins. The stick actions ramp +45 a
  frame.
- **DualShock analog (0x73), 0x8001cee4.** The d-pad, Cross and Square are
  not read for actions 0 to 7. Each stick axis splits into its halves,
  `(v - 128) x 2` and `(127 - v) x 2`, read through a 7-point curve
  (0x8001d9ec, straight lines between points): steering (left stick x) and
  the pedals (right stick y, up accelerates) through 0x800bdbdc, (0,0)
  (10,0) (50,5) (100,17) (190,70) (245,255) (255,255); the stick actions
  through 0x800bdbec, (0,0) (50,30) (100,45) (175,80) (220,145) (245,255)
  (255,255).

Both then ease a pedal level at +0x16 toward accelerate's level (else
brake's, else 0) by 7 a frame. `action_level` (0x8001b170, interface slot
0x8012fce4) returns an action's level; the race's controls (0x80034940)
read actions 0 to 10 through it. Ported as `hwtr_game::pad`, checked by a
differential test against 0x8001bec0.

The controls into the car (0x80034940) are `PadReader::controls` and
`Car::apply_controls`. `controls_into_the_car_match_the_original` (hle
tests/car.rs) checks them over 1200 rounds:

- any action levels: a button's 255, nothing, or an analog stick's
  anything between
- any speed, the race over or not
- the whole car compared after the call

So an analog stick reaches the car as a digital pad does, through the same
levels.

## Notes for the port

The game reads analog sticks itself, so a modern pad is best presented as a
DualShock in analog mode (type 0x73). The neGcon path reads analog values for
more than steering (its I, II and L buttons are analog), so a neGcon
presentation could carry the DualSense's analog triggers; which actions use
them is in 0x8001cee4.

## The HUD button (0x8006545c)

The race's pad read (0x80034940) ends each player's read with action 12.

- **No change.** Its level is kept per player at 0x800d0e60; if the level
  is unchanged, nothing happens.
- **A press.** The player's show mask (0x800d0e58) goes to 0 if it showed
  everything the HUD shows when on (0x800d0e5c, set with it at the race's
  setup), and back to that otherwise.
- **Any change.** Press or release, all three lines of the player's stunt
  announcement are marked done, both halves (0x800beae4 + 32 a line, +0xd),
  so one under way is cut short.

## Vibration

The controls screen's "Vibration" line (the mapping's last word) turns the
DualShock's motors on or off. Each port keeps its two actuator bytes
(0x8011b2b8 +0x18, +0x19): the small motor on or off and the large one's
power. Five things drive them:

- **The road's feel (0x8001b7b4, from 0x8005fba8 each frame while
  racing).** Rough ground at more than a crawl runs the large motor.
- **A jolt (0x8001b934, iface_controls+0x28) of a level.** The large motor
  runs at 2.5 times the level, and from 110 the small motor too, for half a
  second.
  - A player's wreck jolts at TUNING +0x32.
  - A wall jolts by the speed into it (0x8005fd4c): `-(vel . normal) /
    2304 * 255 / 4096`, at most 255. It comes at the step's first contact,
    then not again for 10 frames.
  - Another object jolts (0x8005fed0):
    - A prop it knocks over gives TUNING +0x30, and one that lifts it +0x31.
    - The exception is a heavy prop (knockable, weight over 10000) meeting
      a car that is not all-terrain (car +0x865 or the handling's flag
      1). That, and anything else, jolts by the speed between the two, as
      a wall does, capped at 4096 before the scaling, then not again for
      20 frames.
  - The wait is per player (0x800d0e2c), counted down at the top of
    0x8005fba8 each frame.
- **The wind-down (0x8001ccc4, at each read of the pad).** The large motor
  steps down every 8 ms, and the small one stops after half a second.

`hwtr_game::pad::Motors` holds the motors.

- The collision queues the contact jolts (`Collision::jolts`, with
  `jolt_wait`), and the race applies them.
- Tests: `wall_jolts_match_the_original` and
  `pair_jolts_match_the_original` (tests/collision.rs) call 0x8005fd4c and
  0x8005fed0 with the jolt hooked.

## Unknown

- The meaning of actions 9 to 12 and 13 to 27 (menus).
- The neGcon (0x23) path's analog buttons.
