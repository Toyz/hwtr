---
title: The attract race
status: partial
discs: US
covers: US CCCPSX.EXE:0x8009b254, 0x8009b1e0, 0x8009b578, 0x8009b510, 0x80088428, 0x80036830 camera_load, 0x800369e4 camera_step (modes 2 to 4), 0x8003a690 director, 0x80021390, 0x8002137c, 0x800644c0, 0x800be934 camera ranges, 0x800bea3c demo views, 0x800d0e20 demo view count, 0x800d2608 demo flag, 0x800d2632 no-players flag
worklog: 44
---

# The attract race

Left alone for 30 seconds on the title (state 48) or the main menu
(0x80088428: state 64 entered more than 30 s ago, event 37 to state 77),
the front end sets up a race of computer cars and shows it with a TV-style
director until it ends or a button is pressed.

## Front end

State 326 runs `0x8009b578` (the race's frame, iface_game+0x18; event 10
once the race is over) and `0x8009b1e0` (action 27 held on either pad:
event 135). 135 leads to state 327 and the main menu (49), 10 to state 328
and the title (44); both enter `0x8009b510`, which stops the song and drops
the result.

`0x8009b254` sets the race up in the setup block (0x80138c94):

```
loop: world = rand(4); number = rand(3) + 1; t = world*3 + number - 1
      until a player has track t (0x8008a2f0), it has an archive name
      (0x800c5bf4), and t is not 9, 10 or 11 (the Volcano tracks)
track = world_names[world]; cars = 6; laps = 1
checkpoints = 0x800c5c64[number - 1]   (the Desert row, whatever the world)
difficulty = TUNING.PRM byte 0x20 (0x80136a38)
up to 100 times: every car free (0x80139984); each of the six entrants:
    c = rand(41), then the next of c+1, c+2, ... (wrapping at 41) that
    player one has (0x8008a298) and is free; driver 0, player 0, grid k,
    name = car file name (0x800c5ce8); the car number byte is not written
  until the field is fair (0x8009c9f8)
time limit = 60000; flags = 5 (1 the demo, 4 against the clock)
```

`rand(n)` here is iface_game+188. Options, best line and the car numbers
keep whatever the last race left there.

## The race

Flag 1 is kept at 0x800d2608. With it the race cannot be paused, and when
`race_over` (0x8003485c) says the time is up, `race_frame` sets the end
byte (0x800d261c) to 2 at once: no standings, no results.

`camera_load` (0x80036830) makes `max(1, players)` cameras. With no
players (0x800d2632 = 1) the one camera follows car 0 with the demo views
(0x800bea3c, 2 of them by 0x800d0e20: both chase views, one behind and
above, one ahead looking back), and the track's range comes from
0x800be934 (u16 by world*3 + number - 1: 350, or 250 for the fourth
track).

The HUD (0x80064b8c) with no players draws only "DEMO MODE" (string 214)
at (130, 20) in red, hidden whenever bit 0 of
`(race_clock * 0x057619f1) >> 36` is set (every 750 ms).

## Camera modes 2 to 4

The camera step's mode table (0x800ce0f4) has five entries.

- 2: the camera stands at trackside camera `view` (0x80021390: the world's
  40-byte records at header +60, count +56): its position, rotation and
  field of view; velocity 0.
- 3: the same place and field of view, turned to look at the car's centre
  (body position + centre): look = normalised (centre - eye), right =
  normalised (look.y, -look.x, 0), up = normalised cross(right, look);
  rows `[right, look, up]`.
- 4: nothing (0x8003abe4).

In the demo the chase view's spring (0x80039d54) gets three times its
share of the gap, its speed limit and its acceleration limit
(each `(v * 0x3000) >> 12`, 0x800383d0).

## The director

After each camera step in the demo, 0x8003a690(camera, dt) runs in place
of the view button. While the camera's timer (+0x50) is at least `dt` it
only counts down. Otherwise it picks a shot:

```
reach = ((range << 12) * 0xc000) >> 12          12 x range, 20.12
for each trackside camera k (flags |= 1; skipped if flags & 2):
    seen = 0; far = 0
    for each car i:
        d = camera.pos - car.pos (body position, not centre); len = |d|
        if len < reach and dot(d / len, car.velocity) >= 2049:
            if far < len: far = len; candidate = i
            seen += 1
    if best < seen (unsigned): best = seen; spot = k; car = candidate
if best != 0:
    mode = 3 (2 if the last camera looked at lacked bit 0, which never
    happens); camera car = car; view = spot
else:
    repeat view = rand(view count) until that view's mode is not 0
    mode = the view's mode
    up to ten times: camera car = rand(cars), until that car is not wrecked
snap = 1; timer = 4000; then timer -= dt
```

`candidate` carries over from one trackside camera to the next. `rand(n)`
is iface_general+4 (`rand() % n`).

## Unknown

- What 0x800836e8 (iface_game+0x98 with 0), called first by 0x8009b254,
  does.
- Trackside camera flags other than bit 1; no shipped track sets any.
