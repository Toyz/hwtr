---
title: The car's wheels as drawn
status: complete
discs: US
covers: US CCCPSX.EXE:0x80049ecc car_pose, 0x80020a14 wheel_pose, 0x80020888 car_set_pose
worklog: 54
---

# The car's wheels as drawn

A car's full model has one child node per wheel (4, or 6 on TOWJAM),
each with a 32-byte wheel record that holds its mount at half scale (see
[the car format](../formats/car.md)). Every frame that is not paused,
after the frame is drawn, the car pose (0x80049ecc) turns and places
those nodes. It does this only for cars within 600 units of a camera on
every axis.

## The pass (0x80049ecc)

For each such car, after setting the model roots (`car_set_pose`
0x80020888: the rotation, and twice the position), it goes through the
wheels.

1. **Spin.** Unless the car sleeps (+0x210), the wheel's angle (+0x80)
   grows by its spin rate (+0x84) times the frame's seconds,
   `(ms << 12) / 1000`. Asleep or not, the angle is then brought back
   within a hundred half turns (`π * 100`) either way.
2. **Steering.** A steering wheel (flag 2) takes the car's steering input
   (+0x10) as an angle in radians, so full lock is about 57 degrees. The
   original turns its wheels that far too. From the third wheel on the
   angle is reversed; others take 0.
3. **Lift.** The wheel's compression (+0x7c) less its axle's ride height
   (car +0x680 front, +0x698 rear, chosen by flag 1).
4. **Trail point.** The skid edge (see [the effects](effects.md)).
5. **Node.** `wheel_pose` (0x80020a14) is called with all of the above.

The car's just-reset mark (+0x928) is cleared after its wheels.

## The node (0x80020a14)

Nothing is done for a wrecked car (cvs +0x28 = 1): its wheels keep their
last pose and no trail point is pushed. Otherwise:

- **The model's lift.** cvs +0x18 becomes twice the lift. Each wheel
  overwrites it, so the last wheel's stays; the boost flame adds it.
- **Rotation.** The node's rotation is the identity, turned about z by
  the steering, then about x by the angle. Each angle is negated and
  converted from radians to 4096ths of a turn (`x * 2/π >> 14`, masked
  to a turn). The turns are libgte's RotMatrixZ and RotMatrixX, applied
  by the GTE (each column through the matrix, shifted down 12).
- **Place.** The node is placed at twice the record's mount,
  `(x << 13, y << 13, ((z << 12) + lift) << 1)`.
- **Trail point.** Unless the ground is kind 10, the trail point is
  pushed (0x80029230).

The renderer draws each wheel's vertices through its node's rotation and
place instead of the model's fixed child offset.
