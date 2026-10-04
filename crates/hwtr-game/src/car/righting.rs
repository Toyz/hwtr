//! Turning a car that lands on its side, its end or its roof back onto its
//! wheels (0x80046ac0).

use super::{Car, Tuning};
use crate::math::{Vec3, add, column, dot, fx};
use crate::rand::Rand;

/// An axis more than 45 degrees from the floor's plane points into it.
const UPRIGHT: i32 = 2896;

/// What a step of righting leaves to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Righting {
    Done,
    /// The car stayed on its roof too long: wreck it (0x8004619c).
    Wreck,
}

impl Car {
    /// 0x80046ac0, for a player's car touching the floor: the air control
    /// ends; then for each way it can lie wrong (its side, its nose or tail,
    /// its roof) a timer runs while it lies so, 25 a step, and once past
    /// the tuning's time a couple of forces turns it back about the axis
    /// that is wrong. On its roof the car first decides, at random, whether
    /// to be turned back (and which way) or to be wrecked when the time is
    /// up.
    pub fn right_itself(&mut self, tuning: &Tuning, rand: &mut Rand) -> Righting {
        self.air_armed = Default::default();
        self.air_lock.active = false;
        let n = self.ground.floor.normal;
        let rot = self.body.rot;
        let axis = [0, 1, 2].map(|j| column(&rot, j));
        let square = |a: i32, b: i32| fx(a, a).wrapping_add(fx(b, b));
        // On its side: the x axis stands up out of the floor's plane.
        let side = dot(axis[0], n);
        if side.wrapping_abs() > UPRIGHT {
            self.righting[0] = self.righting[0].wrapping_add(25);
            if (tuning.right_side_ms as u32) < self.righting[0] {
                let size = square(self.width, self.height);
                let size = if side > 0 { size.wrapping_neg() } else { size };
                self.turn(axis[0], axis[2].map(|c| fx(c, size)));
            }
        } else {
            self.righting[0] = 0;
        }
        // On its nose or tail: the y axis.
        let end = dot(axis[1], n);
        if end.wrapping_abs() > UPRIGHT {
            self.righting[1] = self.righting[1].wrapping_add(25);
            if (tuning.right_end_ms as u32) < self.righting[1] {
                let size = square(self.length, self.height);
                let size = if end > 0 { size.wrapping_neg() } else { size };
                self.turn(axis[1], axis[2].map(|c| fx(c, size)));
            }
        } else {
            self.righting[1] = 0;
        }
        // On its roof: the z axis points down into the floor.
        if dot(axis[2], n).wrapping_neg() <= UPRIGHT {
            self.righting[2] = 0;
            return Righting::Done;
        }
        if self.righting[2] == 0 {
            self.rights_itself = (tuning.stay_roof_percent as u32) < rand.below(100);
            if self.rights_itself {
                self.roll_way = rand.below(2) != 0;
            }
        }
        self.righting[2] = self.righting[2].wrapping_add(25);
        if !self.rights_itself {
            if (tuning.wreck_roof_tens as u32) * 10 < self.righting[2] {
                return Righting::Wreck;
            }
            return Righting::Done;
        }
        if (tuning.right_roof_ms as u32) < self.righting[2] {
            let size = square(self.width, self.height);
            let size = if self.roll_way { size.wrapping_neg() } else { size };
            self.turn(axis[2], axis[0].map(|c| fx(c, size)));
        }
        Righting::Done
    }

    /// A couple: `force` at the body's centre plus `lever` (the axis times
    /// the body's mass, as the game takes it), and its opposite at the
    /// centre less it.
    fn turn(&mut self, lever: Vec3, force: Vec3) {
        let body = &mut self.body;
        let centre = add(body.pos, body.centre);
        let lever = lever.map(|c| fx(c, fx(body.mass, 0x1000)));
        body.apply_force(add(centre, lever), force);
        body.apply_force(add(centre, lever.map(|c| fx(c, -0x1000))), force.map(|c| fx(c, -0x1000)));
    }
}
