//! A player's controls into their car (0x80034940).

use super::{Car, Tuning};
use crate::math::{div, div_fx, fx};

/// The race's actions as the controls read them, each 0 to 255 (a button
/// held is 255; an analog stick or trigger anything between).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Controls {
    /// Actions 0 and 1.
    pub steer_right: u8,
    pub steer_left: u8,
    /// Actions 2 and 3.
    pub accelerate: u8,
    pub brake: u8,
    /// The stick, across (actions 4 and 5) and along (6 and 7), each the
    /// first action less the second (which way each is, is not yet known).
    pub stick_across: [u8; 2],
    pub stick_along: [u8; 2],
    pub handbrake: u8,
    /// Actions 9 and 10, meanings not yet known.
    pub action_9: u8,
    pub action_10: u8,
}

impl Car {
    /// 0x80034940: the controls into the car. Steering is the difference of
    /// the two steering actions times the car's steering angle, reduced
    /// above the tuning's speed: from its first percentage there toward its
    /// second at top speed. Pedals and the stick are the actions over 255.
    /// A car driving itself takes no controls (it steers full right, the
    /// handbrake on, as the original leaves it).
    pub fn apply_controls(&mut self, c: &Controls, tuning: &Tuning) {
        if self.unknown_5e3 != 0 {
            self.steer = 0x1000;
            self.accel = 0;
            self.brake = 0;
            self.handbrake = 1;
            self.unknown_25 = 0;
            self.unknown_27 = 0;
            return;
        }
        let over = |a: u8, b: u8| div(((a as i32) - (b as i32)) << 12, 255).0;
        // Positive steers left.
        let mut steer = fx(over(c.steer_left, c.steer_right), self.handling.unknown_04);
        let mph = div_fx(176 << 12, 10 << 12);
        let threshold = fx((tuning.steer_mph as i32) << 12, mph);
        let speed = self.body.speed;
        if threshold < speed {
            let along = div_fx(speed.wrapping_sub(threshold), 0x90_0000i32.wrapping_sub(threshold));
            let k = (0x1000 - along).clamp(0, 0x1000);
            let [near, far] = tuning.steer_percent.map(|p| (p as i32) << 12);
            let percent = div_fx(fx(k, near.wrapping_sub(far)).wrapping_add(far), 100 << 12);
            steer = fx(percent, steer);
        }
        self.steer = steer;
        self.accel = over(c.accelerate, 0);
        self.brake = over(c.brake, 0);
        self.stick = [over(c.stick_across[0], c.stick_across[1]), over(c.stick_along[0], c.stick_along[1])];
        self.handbrake = c.handbrake;
        self.unknown_25 = c.action_9;
        self.unknown_26 = c.action_10;
        self.unknown_27 = c.action_10;
    }
}
