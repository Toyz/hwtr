---
title: Controls and the pad
status: partial
discs: US
covers: US CCCPSX.EXE:0x8001d320 controls_init, 0x8001c344 controls_read_port, 0x8001c480, 0x8001cee4, 0x8001abe8 controls_action_held, 0x8011b388 pad_buffer, 0x8011b3d8 control_mapping, 0x800bdc08 control_mapping_default, interface table 0x8012fcdc
worklog: 11
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

The default masks (`control_mapping_default`, 29 words) are single bits, for
example action 0 = 0x80 and action 7 = 0x10; which buffer byte each applies
to depends on the action's case, so the button each action means is not yet
named.

## Notes for the port

The game reads analog sticks itself, so a modern pad is best presented as a
DualShock in analog mode (type 0x73). The neGcon path reads analog values for
more than steering (its I, II and L buttons are analog), so a neGcon
presentation could carry the DualSense's analog triggers; which actions use
them is in 0x8001cee4.

## Unknown

- The names of the 28 actions and which byte each case tests.
- The analog read in 0x8001cee4: dead zone, scaling, which stick.
- Vibration: the option string "Vibration" (0x800cf428, used by 0x80094164)
  shows the game drives the DualShock motors; the code path is untraced.
