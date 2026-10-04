---
title: Keeping bodies inside the track (the walls stages)
status: solid
discs: US
covers: US CCCPSX.EXE:0x800536b4 ground, 0x80054964 walls, 0x800572f0 computer_walls, 0x8012c6ec contacts, 0x800d2674 contact_count, 0x800d2670 computer_objects
worklog: 65, 66, 73
---

# Keeping bodies inside the track

Two stages of the collision step (0x8005148c) push bodies back inside
their zones. Each logs the contacts the impulse stage then answers
(0x8012c6ec, 40 bytes each, at most 16 a step, counted at 0x800d2674).

A zone is one of two kinds:

- **Plane zone.** Its sides are its planes, but not the portals (kind 0).
  A plane is `n · (p - origin) + d`, positive inside.
- **Road zone** (zone flag 0x800). It has four sides between the edges of
  its two sections, mixed by how far along the zone the point is. In
  order: the floor (the up of the right edge), the left wall, the roof and
  the right wall. The floor's contacts are surface 2, the others' 1.

For each side a point is outside of (`d <= 0`):

1. It is a contact if the body moves inward there (spin included).
2. If it is deeper than `0x32000`, it has gone through. A car that is
   flagged takes 0x800 and the wreck stage flips it. Anything else is
   put to sleep.
3. Otherwise the body moves out along the normal by the depth.

## The ground under each car (0x800536b4)

Just before the walls, each awake car (the cars' list, newest first) gets
two planes: the nearest surface (+0x8f0) and the nearest floor (+0x8b2,
with the zone origin it is measured from).

1. **Wheels.** A player's car starts from its wheels, last to first. Each
   wheel on the ground makes its contact plane the nearest surface. The
   first that faces up (normal z over 0x800), or any if the car's zone
   holds gravity (car flag 0x400), is also the floor (origin 0), and ends
   the car's search. Under 0x400 gravity turns toward it.
2. **Zones.** Every other car, and a player's with no floor yet, searches
   the zones its object is in:
   - A road zone gives its surface under the centre (the right edge's
     points, mixed between the sections). It is a floor if it faces
     against gravity (over 0x800). A road zone with flag 0x2000 turns
     gravity toward it.
   - A plane zone gives each plane but its portals and kind 1. It is a
     floor if its normal's z is over 0x800.

   A candidate is taken if the car has none yet, or if it is nearer than
   the best so far. **The best distances (nearest and floor) start at 0
   once and run on across the cars.** A car's first candidate takes its
   place regardless. But a player's car whose wheels gave it a nearest
   surface and no floor compares its zones' candidates with the best the
   car before it left.
3. **Floor wins.** A car with both ends with its nearest surface set to
   its floor, the floor's distance with its origin folded in.

## Points (0x80054964)

Every wall-hitting object's points are pressed: plane zones first for
every object, then road zones.

- A player's car skips its wheel points; the wheel stage handles them.
- A plane is tried only if the object's centre is within its radius of
  it.
- On a road a point well inside a side gets only a positive measure, not
  its distance.
- A player's car striking a plane of kind 3 or 4 takes 0x2000 or 0x1000.
- A flying wheel (kind 6) is a ball. Each side meets the centre less the
  normal times the reach, and every road side's distance is computed and
  less the reach. See [flying wheels](flying-wheels.md).

## Computer cars out of the race (0x800572f0)

A computer car (list 0x800d2670) that is wrecked (+0x62c) or has finished
(+0x5e3), and is awake, is a **box** instead. Its centre is the object's
centre, and its three half axes are the body's rotation columns times:

- the object's half width (+0x1c)
- the object's half length (+0x20)
- the height of the car's origin (+0x778)

Only the zone of its single point is tried. Plane zones come first for
every such car, then road zones.

- **The corner.** Each side meets the corner deepest toward it. Each axis
  is negated if it points along the side's normal (`a · n > 0`), and the
  corner is the centre plus the three. The axes stay negated for the next
  side, so a side square to an axis meets the same corner on that axis
  as the last side did.
- **Planes.** Every plane but the portals, with no radius test. A plane of
  kind 3 or 4 marks the car with 0x2000 or 0x1000, as for a player's car.
- **Road sides.** Every side's normal is computed, and its distance is
  taken at the centre, less the box's reach along the normal
  (`|a0 · n| + |a1 · n| + |a2 · n|`).
- **Through.** A computer car that goes through is flagged 0x800, not put
  to sleep.

## In the port

`Collision::walls` and `Collision::computer_walls` in
`crates/hwtr-game/src/collision/walls.rs`, with `Shape` (point, ball, box)
for the sides' reach. Tests in `crates/hwtr-hle/tests/collision.rs`:

- `walls_match_the_original`: 60 shoved steps over four states.
- `ground_matches_the_original`: 120 steps over nine states, with a
  player's wheels on a steep surface two steps in three, gravity up and the car
  before it raised, so the carried-over distance decides.
- `computer_walls_match_the_original`: 80 steps over four states, with
  computer cars wrecked or finished at random and half-turned about each
  axis. It checks every contact and car, about 1200 boxes, 300 of them on
  road zones.

## Unknown

- The meaning of the origin's height as the box's third half axis. It is
  the car's height from its origin, used where the object keeps none.
