//! A car's wheels as drawn: each turns with its spin, and its node in the
//! model is steered, turned and lifted by its suspension (0x80049ecc,
//! 0x80020a14).

use super::Car;
use crate::math::{Matrix, Tables, fx, gte_mul};

/// A wheel's node as the model draws it (0x80020a14): its rotation, steered
/// about z then turned about its axle, and how far its suspension lifts it
/// off its mount (4.12; the node sits at twice the mount, raised by twice
/// this).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WheelPose {
    pub rot: Matrix,
    pub lift: i32,
}

/// Radians (4.12) as 4096ths of a turn, negated, as 0x80020a14 turns them
/// (`x * 2/π >> 14`, masked to a turn).
fn turn_of(radians: i32) -> i32 {
    let v = radians.wrapping_neg().wrapping_shl(12);
    let hi = ((v as i64 * 0xa2f9_6525u32 as i32 as i64) >> 32) as i32;
    ((hi.wrapping_add(v) >> 14).wrapping_sub(v >> 31)) & 0xfff
}

/// A hundred half turns: the wheels' angles wrap there.
fn wrap() -> i32 {
    fx(12868, 0x6_4000)
}

impl Car {
    /// 0x80049ecc's wheels, a frame of `frame_ms` on: each wheel turns by
    /// its spin unless the car sleeps, its angle kept within a hundred half
    /// turns either way.
    pub fn turn_wheels(&mut self, frame_ms: u32) {
        let seconds = ((frame_ms as i32) << 12) / 1000;
        let asleep = self.body.asleep;
        for w in &mut self.wheels {
            if !asleep {
                w.angle = w.angle.wrapping_add(fx(w.spin_rate, seconds));
            }
            while w.angle < wrap().wrapping_neg() {
                w.angle = w.angle.wrapping_add(wrap());
            }
            while wrap() < w.angle {
                w.angle = w.angle.wrapping_sub(wrap());
            }
        }
    }

    /// 0x80020a14 for each wheel: a steering wheel takes the steering
    /// (reversed on the rear axle's wheels and past), every wheel its angle,
    /// and its lift is its compression less its axle's ride height.
    pub fn wheel_poses(&self, t: &Tables) -> Vec<WheelPose> {
        self.wheels
            .iter()
            .enumerate()
            .map(|(k, w)| {
                let mut steer = if w.steers { self.steer } else { 0 };
                if k >= 2 {
                    steer = steer.wrapping_neg();
                }
                let ride = if w.rear { self.handling.rear.ride_height } else { self.handling.front.ride_height };
                WheelPose::new(t, steer, w.angle, w.compression.wrapping_sub(ride))
            })
            .collect()
    }
}

impl WheelPose {
    /// 0x80020a14's node: the identity steered by `steer` about z, then
    /// turned by `angle` about x (both radians, 4.12), and `lift`.
    pub fn new(t: &Tables, steer: i32, angle: i32, lift: i32) -> WheelPose {
        let identity: Matrix = [[4096, 0, 0], [0, 4096, 0], [0, 0, 4096]];
        let rot = gte_mul(&identity, &t.rot_axis(2, turn_of(steer)));
        WheelPose { rot: gte_mul(&rot, &t.rot_axis(0, turn_of(angle))), lift }
    }
}
