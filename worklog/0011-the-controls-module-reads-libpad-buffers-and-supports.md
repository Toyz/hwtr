---
number: 11
title: The controls module reads libpad buffers and supports digital, DualShock analog, analog joystick and neGcon
date: 2026-10-03
area: input, decomp
files: docs/engine/controls.md, symbols/cccpsx.txt
---

# 11. The controls module reads libpad buffers and supports digital, DualShock analog, analog joystick and neGcon

The game imports no BIOS pad function, so it uses PsyQ's libpad, which polls
the controller ports from the vertical blank interrupt. Its SIO routines
(0x800b0554, 0x800b07b0) have no callers in the call graph for that reason.

The game side is the module whose interface table is at 0x8012fcdc, now
named `iface_controls`. `controls_init` (0x8001d320, called during system
init) hands libpad two 34-byte receive buffers at 0x8011b388 and 0x8011b3aa,
starts it, resets both ports, and copies a default table of 29 action masks
from 0x800bdc08 into each player's mapping (0x8011b3d8, 0x8011b44c).
`controls_action_held` (0x8001abe8) tests an action through a 28-case switch
against those masks.

`controls_read_port` (0x8001c344) switches on the controller type byte:
0x41 to the digital reader, and 0x73 (DualShock analog), 0x53 (analog
joystick) and 0x23 (neGcon) to the analog reader 0x8001cee4. The game was
built to steer with a stick or a neGcon's twist. For the port this settles
the mapping: present the DualSense as a DualShock in analog mode, and keep
the neGcon path in mind for analog triggers.

## A correction to [[6]]

[[6]] gives iface_b's per-index records as "98-byte records at 0x8012b2b8".
The address is 0x8011b2b8: `lui 0x8012; addiu -19784` is 0x80120000 -
0x4d48. The same slip would have put the pad buffers at 0x8012b388. The
architecture page has been fixed; the tools always computed it correctly, the
error was reading the instructions by hand.

**Still unknown:** The 28 actions' names; the analog read's dead zone and scaling; the vibration path.
