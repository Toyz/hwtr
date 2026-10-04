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
    /// its inertia. Player one's wreck may get a line, at random, and a
    /// player's jolts the pad.
    ///
    /// The wreck's smoke, embers and flying faces (0x80029e10 with
    /// 0x8002e574) draw their random numbers here, after the throw; the
    /// race gives them to the effects, and runs the sounds and a player's
    /// camera shake (`jolted`, `crashed`). Not yet ported: the wheels
    /// flying off as debris (0x8007c9b0, a player's car, which draws
    /// random numbers before the throw).
    pub fn wreck(&mut self, flip: bool, rand: &mut Rand) {
        if self.wrecked {
            return;
        }
        self.wrecked = true;
        let body = &mut self.body;
        if flip {
            body.vel = [0; 3];
            body.spin = body.spin.map(i32::wrapping_neg);
        }
        body.vel = body.vel.map(|c| fx(c, 0x800));
        let fx_vel = body.vel;
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
        body.ang_momentum =
            inertia.map(|row| (0..3).fold(0i64, |s, k| s.wrapping_add(row[k].wrapping_mul(spin[k] as i64) >> 12)));
        // 0x80029e10 (mode 0): the wreck's smoke, embers and chunks, their
        // random numbers drawn now.
        let human = self.flags & 3 != 0;
        self.wreck_draws.0 =
            Some(crate::effects::WreckDraws::take(rand, self.slot, fx_vel, human, self.model_faces as usize));
        self.flags_8 &= !2;
        if self.flags & 1 != 0 {
            if rand.below(2) != 0 {
                self.wreck_line = true;
            }
            self.jolted = true;
        }
        self.crashed.0 = Some(());
    }
}

impl Car {
    /// 0x80045ff4: how the road feels through the pad: the average, over
    /// all the wheels, of how rough the ground under each one touching it
    /// is (`rumble` by surface, 0x800bea7c), and the speed, 0 to 255.
    pub fn road_feel(&self, rumble: &[u8]) -> (u8, u8) {
        let speed = (fx(0xff000, self.body.speed / 2304) >> 12).clamp(0, 255) as u8;
        let feel: u32 = self
            .wheels
            .iter()
            .filter(|w| w.on_ground)
            .map(|w| rumble.get(w.surface as usize).copied().unwrap_or(0) as u32)
            .sum();
        (feel.checked_div(self.wheels.len() as u32).unwrap_or(0) as u8, speed)
    }
}
