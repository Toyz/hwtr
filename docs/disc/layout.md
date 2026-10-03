---
title: The CD, its tracks and its file system
status: solid
discs: US
covers: US every track, SYSTEM.CNF, every file in the ISO 9660 file system, SLUS_009.64, CCCPSX.EXE header
worklog: 1, 3
---

# The CD, its tracks and its file system

Hot Wheels Turbo Racing, NTSC-U, `SLUS-00964`, published by Electronic Arts
(1999), developed by Stormfront Studios. One CD-ROM XA disc: a Mode 2 data track
and thirteen Red Book audio tracks that are the in-game music.

## Tracks

As dumped: one BIN per track, raw 2352-byte sectors, with the CUE sheet giving
each audio track a 150-sector (2 second) INDEX 00 pregap inside its BIN.

```
track  type          sectors in BIN   playable (after INDEX 01)
 1     MODE2/2352        43786          43786  (the data track, 9:43.81)
 2     AUDIO             18287          18137
 3     AUDIO             24546          24396
 4     AUDIO             14650          14500
 5     AUDIO             25727          25577
 6     AUDIO             17415          17265
 7     AUDIO             18452          18302
 8     AUDIO             21465          21315
 9     AUDIO             14460          14310
10     AUDIO             18492          18342
11     AUDIO             18654          18504
12     AUDIO             18363          18213
13     AUDIO             23369          23219
14     AUDIO             20323          20173
```

Absolute disc LBA = sectors of every earlier BIN + offset in this BIN. Track 2
INDEX 01 is therefore LBA 43786 + 150 = 43936.

## Data track sectors

Every sector of track 1 is Mode 2 (header byte 15 = 2). Measured over all 43786:

| subheader (file, channel, submode, coding) | sectors | where |
| --- | --- | --- |
| `00 00 08 00` (Form 1, data) | 43632 | everything else |
| `00 00 20 00` (Form 2, nothing set) | 154 | LBA 12-15 and the 150-sector postgap 43636-43785 |

No file on the disc uses Form 2: there is no XA audio and no STR video. The
movies are `.WVE` files read as ordinary data.

## ISO 9660

Primary volume descriptor at LBA 16:

```
system id       "PLAYSTATION"
volume id       "HOT_WHEELS"
volume blocks   297989          (covers the audio tracks too)
publisher       "ELECTRONIC ARTS"
application     "PLAYSTATION"
created         1999-07-15 05:12:30.00
```

Every directory record carries the 14-byte CD-XA system use field (`"XA"` at
+6, attributes big-endian at +4, file number at +8). Only the root directory
exists; there are no subdirectories.

```
   lba       size  xa attr  path
    23         68  0d55     SYSTEM.CNF
    24     124928  0d55     SLUS_009.64      boot loader, PS-X EXE
    85   78272596  0d55     CCCPSX.BIG       all game data, see formats/big.md
 38305     763328  0d55     EA_LOGO.WVE      EA logo movie
 38678    9043000  0d55     ONLINE.WVE       intro movie
 43094     154144  0d55     PSXLEGAL.TIM     legal screen
 43170     154144  0d55     PSXRFA1.TIM      title screen
 43246     798720  0d55     CCCPSX.EXE       the game, PS-X EXE
 43936   37144576  4555     AVENUEX.DA       = track 2
 62223   49963008  4555     ECLECTIC.DA      = track 3
 86769   29696000  4555     SANJCNTO.DA      = track 4
101419   52381696  4555     BATCAR.DA        = track 5
127146   35358720  4555     OUTEE360.DA      = track 6
144561   37482496  4555     MONDRAGO.DA      = track 7
163013   43653120  4555     CHEATER.DA       = track 8
184478   29306880  4555     HAMSTERS.DA      = track 9
198938   37564416  4555     HOTRACR.DA       = track 10
217430   37896192  4555     HEREKITT.DA      = track 11
236084   37300224  4555     SMELLMYF.DA      = track 12
254447   47552512  4555     YELOFLAG.DA      = track 13
277816   41314304  4555     FUEL.DA          = track 14
```

XA attribute `0d55` is Form 1 data; `4555` sets the CD-DA bit (0x4000). Each
`.DA` entry's LBA is exactly its audio track's INDEX 01 and its size is the
track's playable sector count times 2048, so the file system names every music
track. The names are the bands' songs, shortened to 8.3.

## SYSTEM.CNF

```
BOOT = cdrom:\SLUS_009.64;1
TCB = 4
EVENT = 10
STACK = 801FFF00
```

## The two executables

Both are PS-X EXE with the North America marker ("Sony Computer Entertainment
Inc. for North America area"), gp0 = 0, no data or bss fields set, and stack
0x801ffff0.

```
file          load        size      entry
SLUS_009.64   0x80100000  122880    0x80117a60
CCCPSX.EXE    0x80010000  796672    0x8009fc10
```

`SLUS_009.64` loads high in RAM so that it can load `CCCPSX.EXE` underneath
itself. Its strings name `\EA_LOGO.WVE;1`, `\ONLINE.WVE;1`, `\PSXLEGAL.TIM;1`,
`\PSXRFA1.TIM;1` and `\CCCPSX.EXE;1`, and its movie player is called
`playVAGmovie` in its own debug output; it plays the two movies, shows the
legal screen and starts the game (inferred from the strings, not yet traced).

`CCCPSX.EXE` is all of the game's code. The `.OVL` members of `CCCPSX.BIG` are
sprite sheets, not code (see [the BIG archive](../formats/big.md)).

## Unknown

- The order and timing of what `SLUS_009.64` does before it chains to
  `CCCPSX.EXE`, and how it chains (BIOS `LoadExec` or its own loader).
- Whether the game ever reads `PSXLEGAL.TIM` and `PSXRFA1.TIM` from the file
  system; `CCCPSX.BIG` carries its own copy of `PSXRFA1` (byte-identical).
