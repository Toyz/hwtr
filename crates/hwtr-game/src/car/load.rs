//! Building a car for a race (`cars_load`, 0x8003beec, one car at a time).

use super::handling::{EngineSpec, Handling};
use super::{Car, Engine, Tuning, Wheel};
use crate::body::Body;
use crate::math::{Vec3, add, div, div_fx, fx, quat_to_matrix, sub, transpose};
use crate::race::{Entrant, RaceSetup};

/// Race option bits.
const THIRD_SIZE: u32 = 4;
const HALF_WHEELS: u32 = 32;

/// The gravity the springs hold the car up against, in/s².
const G: i32 = 0x18_2000;

impl Engine {
    /// The engine as its specification sets it up, everything else at rest.
    fn from_spec(spec: &EngineSpec) -> Engine {
        let mut torque_curve = [0; 17];
        torque_curve[..16].copy_from_slice(&spec.torque_curve);
        Engine {
            redline: spec.redline,
            idle: spec.idle,
            gears: spec.gears,
            final_drive: spec.final_drive,
            gear_ratios: spec.gear_ratios,
            reverse_ratio: spec.reverse_ratio,
            peak_torque: spec.peak_torque,
            torque_curve,
            ..Engine::default()
        }
    }
}

/// The skill a car drives with: its handling's, scaled by the difficulty
/// into the tuning's range for its kind of driver, at most 4.
fn skill(base: i32, difficulty: u8, [low, high]: [u8; 2]) -> i32 {
    let along = div((difficulty as i32) << 12, 255).0;
    let (low, high) = ((low as i32) << 12, (high as i32) << 12);
    let percent = div_fx(fx(along, high.wrapping_sub(low)).wrapping_add(low), 100 << 12);
    fx(base, percent).min(0x4000)
}

impl Car {
    /// 0x8004528c: the car's springs and wheels from its handling. The
    /// springs share the car's weight; each axle's extension is that over its
    /// stiffness. The wheels take their mounts, diameters and drive layout
    /// from the handling, lowered by their axle's ride height; the car is as
    /// wide as its widest wheel track. Race options can shrink the car to a
    /// third, and its wheels by half.
    fn fit_handling(&mut self, options: u32) {
        let h = &self.handling;
        let count = h.wheel_count;
        self.spring_preload = div_fx(fx(h.mass, G), (count as i32) << 12);
        self.extension =
            [div_fx(self.spring_preload, h.front.stiffness), div_fx(self.spring_preload, h.rear.stiffness)];
        self.origin = h.origin;
        [self.width, self.length, self.height] = h.size;
        let third = div_fx(0x1000, 0x3000);
        if options & THIRD_SIZE != 0 {
            self.origin = self.origin.map(|c| fx(c, third));
            let size = [self.width, self.length, self.height].map(|c| fx(c, third));
            [self.width, self.length, self.height] = size;
        }
        self.front_wheels = 0;
        self.rear_wheels = 0;
        let handling = self.handling.clone();
        self.wheels = (0..count as usize)
            .map(|i| {
                let mut mount = handling.mounts[i];
                let mut diameter = handling.diameters[i];
                if options & THIRD_SIZE != 0 {
                    mount = mount.map(|c| fx(c, third));
                    diameter = fx(diameter, third);
                }
                if options & HALF_WHEELS != 0 {
                    diameter = fx(diameter, 0x2000);
                }
                let rear = i >= 2;
                let (driven, steers) = if rear {
                    self.rear_wheels += 1;
                    (handling.rear_driven, handling.rear_steers)
                } else {
                    self.front_wheels += 1;
                    (handling.front_driven, handling.front_steers)
                };
                let wheel = Wheel { mount, diameter, rear, steers, driven, ..Wheel::default() };
                let ride = handling.axle(wheel.rear).ride_height;
                mount[2] = mount[2].wrapping_sub(ride);
                Wheel { mount, ..wheel }
            })
            .collect();
        for wheel in &self.wheels {
            let track = fx(wheel.mount[0].wrapping_sub(self.origin[0]).wrapping_abs(), 0x2000);
            self.width = self.width.max(track);
        }
    }

    /// The car in race slot `slot`, from its handling (the CWH), placed on
    /// its start grid point `grid` (position and quaternion, from the SCP),
    /// for the race `setup`. Computer cars start on the AI's line instead,
    /// which is not yet ported; this places every car on the grid.
    pub fn load(
        slot: u8,
        entrant: &Entrant,
        setup: &RaceSetup,
        (handling, spec): (&Handling, &EngineSpec),
        grid: (Vec3, [i32; 4]),
        tuning: &Tuning,
    ) -> Car {
        let mut car = Car { slot, handling: handling.clone(), engine: Engine::from_spec(spec), ..Car::default() };
        car.fit_handling(setup.options);
        car.body = Body::new(car.handling.mass, [car.width, car.length, car.height], [0; 3]);
        car.body.rot = transpose(&quat_to_matrix(grid.1));
        // On its grid point, the wheels' origin raised by the higher ride
        // height.
        let ride = car.handling.front.ride_height.max(car.handling.rear.ride_height);
        let local = [car.origin[0], car.origin[1], car.origin[2].wrapping_add(ride)];
        let turned = car.body.rot.map(|row| (0..3).fold(0i32, |s, k| s.wrapping_add(fx(row[k] as i32, local[k]))));
        car.body.pos = sub(add(grid.0, turned), car.body.centre);
        car.turbos = 3;
        car.flags |= match entrant.driver.byte() {
            1 => 1,
            2 => 2,
            _ => 0,
        };
        car.player = entrant.player;
        let player = car.flags & 3 != 0;
        car.state = if player { 2 } else { 1 };
        let range = if player { tuning.player_skill } else { tuning.computer_skill };
        car.handling.skill = skill(car.handling.skill, setup.difficulty, range);
        car.respawn.zone = None;
        car
    }
}
