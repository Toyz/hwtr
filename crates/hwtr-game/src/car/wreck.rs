//! Wrecking a car (0x8004619c): it is thrown up and set spinning, and stays
//! wrecked until put back on the road.

use super::Car;
use crate::math::{div_fx, fx, mul_16_64, mul_64_16, transpose};
use crate::rand::Rand;

impl Car {
    /// 0x8004619c: wrecks the car, once. A flip (from going through a
    /// wall) stops it and turns its spin about; then it loses half its
    /// speed, is thrown up at 10 to 19 mph, and spins up to 12 radians a
    /// second faster about each axis, its angular momentum following from
    /// its inertia. A player's wreck also picks a camera view.
    ///
    /// Not yet ported: the wheels flying off as debris (0x8007c9b0, a
    /// player's car, which draws random numbers before the throw), the
    /// model crumpled at random (0x8002e574, through 0x80029e10, a few
    /// hundred draws after the throw), and the sounds, the HUD and the
    /// camera.
    pub fn wreck(&mut self, flip: bool, rand: &mut Rand) {
        if self.wrecked != 0 {
            return;
        }
        self.wrecked = 1;
        let body = &mut self.body;
        if flip {
            body.vel = [0; 3];
            body.spin = body.spin.map(i32::wrapping_neg);
        }
        body.vel = body.vel.map(|c| fx(c, 0x800));
        if self.flags & 1 != 0 {
            tracing::trace!("car {}: the wheels fly off (0x8007c9b0), not yet ported", self.slot);
        }
        let mph = div_fx(176 << 12, 10 << 12);
        let up = (rand.below(10) as i32 + 10) << 12;
        body.vel[2] = body.vel[2].wrapping_add(fx(up, mph));
        body.momentum = body.vel.map(|c| fx(c, body.mass));
        for k in 0..3 {
            body.spin[k] = body.spin[k].wrapping_add((rand.below(24) as i32 - 12) << 12);
        }
        let rot = body.rot;
        let inertia = mul_64_16(&mul_16_64(&rot, &body.inertia), &transpose(&rot));
        let spin = body.spin;
        body.ang_momentum = inertia.map(|row| {
            (0..3).fold(0i64, |s, k| s.wrapping_add(row[k].wrapping_mul(spin[k] as i64) >> 12))
        });
        self.flags_8 &= !2;
        if self.flags & 1 != 0 {
            if rand.below(2) != 0 {
                self.wreck_view = 1;
            }
            tracing::trace!("car {}: the wreck's sound, rumble and HUD, not yet ported", self.slot);
        }
    }
}
