---
title: What is on the disc
status: partial
discs: US
covers: US CCCPSX.BIG (all 3409 members), SCREENS.BIG::ENGLISHHWT, SCREENS.BIG::ENGCARSCDT, the 13 CD-DA tracks
worklog: 1, 2, 4
---

# What is on the disc

The game's content, counted. Layout of the disc itself is in
[the disc page](../disc/layout.md); the archive format in
[the BIG archive](../formats/big.md).

## Member types in CCCPSX.BIG

Counts are entries across all twelve nested archives (3397 entries; the
twelve archives themselves are not counted). "Evidence" says how the type is
known; anything marked *inferred* has not been parsed yet.

| ext | entries | what | evidence |
| --- | ---: | --- | --- |
| `TIM` | 611 | Sony TIM images | standard TIM header; car skins are 33312 bytes = 8bpp, 256-colour CLUT, 32768 pixel bytes |
| `VH` / `VB` | 499 / 499 | Sony VAB sound banks, header and body | `VH` starts `pBAV`; loader strings `"%s.vh"`, `"%s.vb"` |
| `CAR` | 492 | a car | loader string `"loading %s.car"`; *inferred*: model or handling data |
| `BMF` | 494 | a model | *inferred* from being paired with every car and with `DECALS`, `CWHS` |
| `SHD` | 492 | a car's shadow | *inferred* from the name; every one is 2112 bytes |
| `PUP` | 143 | a power-up definition | 180 bytes each, starts with its name (`"4x4"`, `"4x4 Power"`); `PwrupLoadPowerUp` strings |
| `OVL` | 45 | HUD and font sprite sheets | *inferred*: tables of small records, no code; `ACTNFNT`, `ACTNOVL1-3`, `SCRNFNT` |
| `GLM` | 23 | *unknown*, per world and `SFX`, `SCREENS` | loader string `"%s.glm"` |
| `GLB` | 11 | *unknown*, one per track | `"%s.glb"` |
| `WLD` | 11 | the track's world | `"%s.wld"`, "LOADING WORLDS.." |
| `WLB` | 11 | *unknown*, one per track | `"%s.wlb"` |
| `DLW` | 11 | *unknown*, one per track, near `WLD` in size | `"%s.dlw"` |
| `SCP` | 11 | *unknown*, one per track | `"%s%d.scp"` |
| `BLD` | 14 | *unknown*, one per track plus `TCUP`, `SCCUP1`, `SCCUP2` | `"%s%d.bld"` |
| `PRM` | 13 | `TUNING.PRM`, 256 bytes, identical everywhere | `"Tuning.prm"` |
| `CWH` | 12 | `DEFAULT.CWH`, identical everywhere | `"loading %s.cwh"`, "HWC file's are wrong version!" |
| `SCR` | 1 | `SCREENS.SCR`, the front end's screen layouts | `"screens.scr"`; *inferred* |
| `HWT` | 1 | `ENGLISH.HWT`, CR LF separated text | read |
| `CDT` | 1 | `ENGCARS.CDT`, one line per car, `|` separated | read |
| `CHM` | 2 | `ENGNAME.CHM`, `ENGPWD.CHM`: character lists for name and password entry | read |

## Worlds and tracks

`ENGLISH.HWT` opens with the world names and then the tracks:

```
Desert          Desert World 1-3
Glacial         Glacial Rift 1-3
PTest           Physics Test 1-3
Haunted         Haunted Highway 1-3
Volcano         Volcano Island 1-3
```

Archives exist for Desert 1-3, Glacial 1-3, Volcano 1-3 and Haunted 2-3:
eleven tracks. Missing: [the Physics Test world](../cut/physics-test-world.md),
[Haunted Highway 1](../cut/haunted-highway-1.md), and a world named only in
code, [FCity](../cut/fcity-world.md).

## Cars

41 cars, each with `CAR`, `TIM`, `BMF` and `SHD`, copied into the front end
archive and into every track archive. The front end also has one `VH`/`VB`
pair per car, named after the car. By archive name:

```
BISECTOR CATAPULT DEORA DOUBLEV DRAGSTER EVILWEVL FORM5G GOCART GULCHSTP HW500
JETHREAT KYLE LAKESTER MONGOOSE PPASSION PWRPIPES PWRPSTN RASH1 RDROCKET
REDBARON RIGOMTR ROCKET RODDER ROKBUSTR SHDOWJET SILHOUET SLIDEOUT SNAKE
SOLAIRE SPDBLSTR SPLTIMG STEALTH STGFRT STRIP_T SUPERVAN SWT16 THUNDER TOWJAM
TWINMIL2 TWINMILL WAY2FAST
```

`ENGCARS.CDT` has 41 lines, one per car: display name, year, then three
slogan lines, e.g. `Deora|Cast in 1968|Hit the Surf|Low and steady|Wins the race!`.

## Power-ups

13 per track archive:

```
4X4 BOING BRAKES CAR01 CAR02 GYRO HANDLING RUBBER STEEL STICKY TURBO UNCAR1 UNCAR2
```

## Sound

Per track archive: engine banks by engine type rather than by car (`BIG8`,
`ELECTRC`, `FLAT4`, `GENER8`, `GENER12`, `GENER16`, `ROCKETE`, `ROTARY`,
`SUPER8`, `SUPER12`, `TURBDSL`, each also with an `O`-suffixed variant whose
`VB` is under 4 KB), `MAINSFX2` (the effects bank,
271424 bytes of samples in Desert 1), `CRASHES1-4`, `DIALOG1-12` (voice),
`DUDE`, and the track's own bank. The front end has banks named after six of
the songs (`ELECTRIC`, `HAMSTER`, `HEREKITY`, `MONDRA`, `OUTEE360`, `PISTEL`)
and `HWMENU`.

## Music

The thirteen CD-DA tracks, by their file system names, in track order:

```
 2 AVENUEX    3 ECLECTIC   4 SANJCNTO   5 BATCAR     6 OUTEE360
 7 MONDRAGO   8 CHEATER    9 HAMSTERS  10 HOTRACR   11 HEREKITT
12 SMELLMYF  13 YELOFLAG  14 FUEL
```

## Movies

`EA_LOGO.WVE` (763328 bytes) and `ONLINE.WVE` (9043000 bytes), outside the
archive, played by the boot loader.

## Unknown

- The formats of `CAR`, `BMF`, `SHD`, `GLM`, `GLB`, `WLD`, `WLB`, `DLW`,
  `SCP`, `BLD`, `OVL`, `SCR`, `CWH`, `PRM`, `PUP` and `WVE`.
- What the song-named VAB banks in `SCREENS.BIG` hold, given the music is CD-DA.
