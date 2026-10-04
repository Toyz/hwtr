---
number: 71
title: The results' snapshots keep the flying wheels (0x8007e6e0, 0x8007ec08)
date: 2026-10-04
area: ui
files: crates/hwtr-game/src/snapshot.rs, crates/hwtr-game/src/race.rs, crates/hwtr-hle/tests/snapshot.rs, docs/engine/results.md, docs/engine/flying-wheels.md
---

# 71. The results' snapshots keep the flying wheels (0x8007e6e0, 0x8007ec08)

The results' snapshots now keep the flying wheels, so the slideshow shows a wrecked player's wheels where they were at each moment.

**Saving (0x8007e6e0).** Each in-use slot of the table at 0x801323f4 is written in slot order as a 16-byte record:

- the in-use byte as a u16 (1 for a wheel)
- the car and the wheel
- the body rotation's first two rows in 127ths, clamped as for cars
- the body place >> 12

With no wheel flying, the part takes 4 bytes and writes nothing. If the records do not fit, the whole snapshot is not taken.

**Putting back (0x8007ec08).**

- With nothing kept, every slot is freed.
- Otherwise record i goes into slot i from the first, so the wheels close up. Each slot gets the in-use byte, the rotation (third row the cross product, as for cars), the place << 12, and the car and wheel. Nothing else in the slot is written.
- The slots past the last record are freed.

Port changes:

- `Snapshot.wheels: Vec<WheelShot>`.
- `Snapshots::take` and `put_back` take the collision's flying table, and the race passes `collision.flying`.
- A slot the port had empty is filled with a default body. Only car, wheel, rotation and place matter while the results stand still.

Verified: the snapshot test in hle tests/snapshot.rs now places random wheels in RAM and the port before a snapshot. It compares the snapshot bytes, the wheels section included, and the size. Then it moves the wheels into other slots, puts the snapshot back, and compares every slot's car, wheel, rotation and place with the original's table. A third run with no wheels checks the 4-byte empty part.

Docs: results.md's layout and put-back sections; flying-wheels.md.

**Still unknown:** Any in-use value but 1 in the flying table
