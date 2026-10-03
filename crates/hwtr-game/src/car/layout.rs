//! Where the original keeps a car: one 0x930-byte record each, in an array
//! at 0x80128fcc, with the count at 0x800d263c. `cars_update` (0x8004064c)
//! walks it each frame and dispatches on the state byte at +0x891.
//!
//! The codec here converts between that layout and [`Car`]; the port's logic
//! never sees it. Tests and the reference's shadow checks use it to hold the
//! port to the original byte for byte.

use super::{Axle, Car, Engine, Tuning, TuningSet, Wheel};
use crate::body::{self, Body};
use crate::ram::Ram;

pub const CARS: u32 = 0x8012_8fcc;
pub const CAR_SIZE: u32 = 0x930;
pub const CAR_COUNT: u32 = 0x800d_263c;
/// The update state: 2 full physics, 1 and 0 simpler updates.
pub const STATE: u32 = 0x891;

pub const FLAGS: u32 = 0x4;
pub const STEER: u32 = 0x10;
pub const ACCEL: u32 = 0x14;
pub const BRAKE: u32 = 0x18;
pub const HANDBRAKE: u32 = 0x24;
/// The rigid body; the car's position, velocity and the rest are its.
pub const BODY: u32 = 0x30;
pub const GRAVITY_DIR: u32 = BODY + body::layout::GRAVITY_DIR;
pub const DRAG_POINT: u32 = 0xfc;
pub const POS: u32 = BODY + body::layout::POS;
pub const VEL: u32 = BODY + body::layout::VEL;
pub const SPEED: u32 = BODY + body::layout::SPEED;
pub const ROT: u32 = BODY + body::layout::ROT;
pub const SPIN: u32 = BODY + body::layout::SPIN;
pub const FORCE: u32 = BODY + body::layout::FORCE;
pub const TORQUE: u32 = BODY + body::layout::TORQUE;
pub const WHEELS: u32 = 0x218;
pub const WHEEL_SIZE: u32 = 0x88;
pub const WHEEL_COUNT: u32 = 0x548;
pub const FRONT_WHEELS: u32 = 0x549;
pub const REAR_WHEELS: u32 = 0x54a;
pub const GROUNDED: u32 = 0x54b;
pub const GROUNDED_LEVEL: u32 = 0x54c;
pub const ENGINE: u32 = 0x550;
pub const MASS: u32 = 0x644;
pub const CG_ALONG: u32 = 0x648;
pub const CG_UP: u32 = 0x64c;
pub const BRAKE_GRIP: u32 = 0x650;
pub const BRAKE_BIAS: u32 = 0x654;
pub const DRAG: u32 = 0x658;
pub const DOWNFORCE_FRONT: u32 = 0x65c;
pub const DOWNFORCE_REAR: u32 = 0x660;
pub const DOWNFORCE_FRONT_SCALE: u32 = 0x664;
pub const DOWNFORCE_REAR_SCALE: u32 = 0x668;
pub const GRIP_FRONT: u32 = 0x670;
pub const DAMP_FRONT_IN: u32 = 0x678;
pub const DAMP_FRONT_OUT: u32 = 0x67c;
pub const GRIP_REAR: u32 = 0x688;
pub const DAMP_REAR_IN: u32 = 0x690;
pub const DAMP_REAR_OUT: u32 = 0x694;
pub const UNKNOWN_6B4: u32 = 0x6b4;
pub const ORIGIN: u32 = 0x770;
pub const WIDTH: u32 = 0x780;
pub const LENGTH: u32 = 0x784;
pub const HEIGHT: u32 = 0x788;
pub const UNKNOWN_865: u32 = 0x865;
pub const UNKNOWN_86A: u32 = 0x86a;
pub const UNKNOWN_86B: u32 = 0x86b;

/// The shared tuning bytes.
pub const TUNING: u32 = 0x8013_6a18;

/// Offsets within a wheel.
pub mod wheel {
    pub const MOUNT: u32 = 0x00;
    pub const DIAMETER: u32 = 0x14;
    pub const FLAGS: u32 = 0x18;
    pub const HEADING: u32 = 0x1c;
    /// The heading's padding word, which the original fills from
    /// uninitialised stack.
    pub const HEADING_PAD: u32 = 0x28;
    pub const WORLD: u32 = 0x2c;
    pub const GROUND: u32 = 0x3c;
    pub const SURFACE: u32 = 0x3d;
    pub const NORMAL: u32 = 0x40;
    pub const CONTACT: u32 = 0x50;
    pub const CONTACT_VEL: u32 = 0x60;
    pub const FRICTION: u32 = 0x70;
    pub const SPRING: u32 = 0x74;
    pub const SLIP: u32 = 0x78;
    pub const SPIN_RATE: u32 = 0x84;
}

/// Offsets within the engine record.
pub mod engine {
    pub const RPM: u32 = 0x00;
    pub const WHEEL_RPM: u32 = 0x04;
    pub const REVERSE: u32 = 0x08;
    pub const GEAR: u32 = 0x09;
    pub const RATIO: u32 = 0x0c;
    pub const DRIVE: u32 = 0x10;
    pub const REDLINE: u32 = 0x18;
    pub const IDLE: u32 = 0x1c;
    pub const GEARS: u32 = 0x20;
    pub const FINAL_DRIVE: u32 = 0x24;
    pub const GEAR_RATIOS: u32 = 0x28;
    pub const REVERSE_RATIO: u32 = 0x40;
    pub const PEAK_TORQUE: u32 = 0x44;
    pub const TORQUE_CURVE: u32 = 0x48;
}

/// Offsets within the tuning bytes: three sets of three from +33.
pub mod tuning {
    pub const DOWNFORCE_MPH: u32 = 10;
    pub const SETS: u32 = 33;
    pub const SURFACE_DRAG: u32 = 51;
}

impl Wheel {
    pub fn read(ram: &Ram, w: u32) -> Wheel {
        use wheel::*;
        Wheel {
            mount: ram.vec3(w + MOUNT),
            diameter: ram.i32(w + DIAMETER),
            flags: ram.u8(w + FLAGS),
            heading: ram.vec3(w + HEADING),
            world: ram.vec3(w + WORLD),
            ground: ram.u8(w + GROUND),
            surface: ram.u8(w + SURFACE),
            normal: ram.vec3(w + NORMAL),
            contact: ram.vec3(w + CONTACT),
            contact_vel: ram.vec3(w + CONTACT_VEL),
            friction: ram.i32(w + FRICTION),
            spring: ram.i32(w + SPRING),
            slip: ram.u8(w + SLIP),
            spin_rate: ram.i32(w + SPIN_RATE),
        }
    }

    pub fn write(&self, ram: &mut Ram, w: u32) {
        use wheel::*;
        ram.set_vec3(w + MOUNT, self.mount);
        ram.set_i32(w + DIAMETER, self.diameter);
        ram.set_u8(w + FLAGS, self.flags);
        ram.set_vec3(w + HEADING, self.heading);
        ram.set_vec3(w + WORLD, self.world);
        ram.set_u8(w + GROUND, self.ground);
        ram.set_u8(w + SURFACE, self.surface);
        ram.set_vec3(w + NORMAL, self.normal);
        ram.set_vec3(w + CONTACT, self.contact);
        ram.set_vec3(w + CONTACT_VEL, self.contact_vel);
        ram.set_i32(w + FRICTION, self.friction);
        ram.set_i32(w + SPRING, self.spring);
        ram.set_u8(w + SLIP, self.slip);
        ram.set_i32(w + SPIN_RATE, self.spin_rate);
    }
}

impl Engine {
    pub fn read(ram: &Ram, e: u32) -> Engine {
        use engine::*;
        Engine {
            rpm: ram.i32(e + RPM),
            wheel_rpm: ram.i32(e + WHEEL_RPM),
            reverse: ram.u8(e + REVERSE),
            gear: ram.u8(e + GEAR),
            ratio: ram.i32(e + RATIO),
            drive: ram.i64(e + DRIVE),
            redline: ram.i32(e + REDLINE),
            idle: ram.i32(e + IDLE),
            gears: ram.i32(e + GEARS),
            final_drive: ram.i32(e + FINAL_DRIVE),
            gear_ratios: std::array::from_fn(|g| ram.i32(e + GEAR_RATIOS + 4 * g as u32)),
            reverse_ratio: ram.i32(e + REVERSE_RATIO),
            peak_torque: ram.i32(e + PEAK_TORQUE),
            torque_curve: std::array::from_fn(|k| ram.u8(e + TORQUE_CURVE + k as u32)),
        }
    }

    pub fn write(&self, ram: &mut Ram, e: u32) {
        use engine::*;
        ram.set_i32(e + RPM, self.rpm);
        ram.set_i32(e + WHEEL_RPM, self.wheel_rpm);
        ram.set_u8(e + REVERSE, self.reverse);
        ram.set_u8(e + GEAR, self.gear);
        ram.set_i32(e + RATIO, self.ratio);
        ram.set_i64(e + DRIVE, self.drive);
        ram.set_i32(e + REDLINE, self.redline);
        ram.set_i32(e + IDLE, self.idle);
        ram.set_i32(e + GEARS, self.gears);
        ram.set_i32(e + FINAL_DRIVE, self.final_drive);
        for (g, r) in self.gear_ratios.iter().enumerate() {
            ram.set_i32(e + GEAR_RATIOS + 4 * g as u32, *r);
        }
        ram.set_i32(e + REVERSE_RATIO, self.reverse_ratio);
        ram.set_i32(e + PEAK_TORQUE, self.peak_torque);
        for (k, b) in self.torque_curve.iter().enumerate() {
            ram.set_u8(e + TORQUE_CURVE + k as u32, *b);
        }
    }
}

impl Tuning {
    pub fn read(ram: &Ram) -> Tuning {
        use tuning::*;
        let set = |k: u32| {
            let at = TUNING + SETS + 3 * k;
            TuningSet { downforce_front: ram.u8(at), downforce_rear: ram.u8(at + 1), grip_front: ram.u8(at + 2) }
        };
        Tuning {
            downforce_mph: ram.u8(TUNING + DOWNFORCE_MPH),
            sets: [set(0), set(1), set(2)],
            surface_drag: ram.u8(TUNING + SURFACE_DRAG),
        }
    }
}

fn read_axle(ram: &Ram, at: u32, rear: bool) -> Axle {
    let pick = |front: u32, back: u32| at + if rear { back } else { front };
    Axle {
        wheels: ram.u8(pick(FRONT_WHEELS, REAR_WHEELS)),
        grip: ram.i32(pick(GRIP_FRONT, GRIP_REAR)),
        damp_in: ram.i32(pick(DAMP_FRONT_IN, DAMP_REAR_IN)),
        damp_out: ram.i32(pick(DAMP_FRONT_OUT, DAMP_REAR_OUT)),
        downforce: ram.i32(pick(DOWNFORCE_FRONT, DOWNFORCE_REAR)),
        downforce_scale: ram.i32(pick(DOWNFORCE_FRONT_SCALE, DOWNFORCE_REAR_SCALE)),
    }
}

fn write_axle(axle: &Axle, ram: &mut Ram, at: u32, rear: bool) {
    let pick = |front: u32, back: u32| at + if rear { back } else { front };
    ram.set_u8(pick(FRONT_WHEELS, REAR_WHEELS), axle.wheels);
    ram.set_i32(pick(GRIP_FRONT, GRIP_REAR), axle.grip);
    ram.set_i32(pick(DAMP_FRONT_IN, DAMP_REAR_IN), axle.damp_in);
    ram.set_i32(pick(DAMP_FRONT_OUT, DAMP_REAR_OUT), axle.damp_out);
    ram.set_i32(pick(DOWNFORCE_FRONT, DOWNFORCE_REAR), axle.downforce);
    ram.set_i32(pick(DOWNFORCE_FRONT_SCALE, DOWNFORCE_REAR_SCALE), axle.downforce_scale);
}

impl Car {
    /// The car record at `at`.
    pub fn read(ram: &Ram, at: u32) -> Car {
        let wheels = (0..ram.u8(at + WHEEL_COUNT) as u32).map(|i| Wheel::read(ram, at + WHEELS + i * WHEEL_SIZE));
        Car {
            flags: ram.i32(at + FLAGS),
            steer: ram.i32(at + STEER),
            accel: ram.i32(at + ACCEL),
            brake: ram.i32(at + BRAKE),
            handbrake: ram.u8(at + HANDBRAKE),
            body: Body::read(ram, at + BODY),
            drag_point: ram.vec3(at + DRAG_POINT),
            wheels: wheels.collect(),
            grounded: ram.u8(at + GROUNDED),
            grounded_level: ram.u8(at + GROUNDED_LEVEL),
            engine: Engine::read(ram, at + ENGINE),
            mass: ram.i32(at + MASS),
            cg_along: ram.i32(at + CG_ALONG),
            cg_up: ram.i32(at + CG_UP),
            brake_grip: ram.i32(at + BRAKE_GRIP),
            brake_bias: ram.i32(at + BRAKE_BIAS),
            drag: ram.i32(at + DRAG),
            front: read_axle(ram, at, false),
            rear: read_axle(ram, at, true),
            origin: ram.vec3(at + ORIGIN),
            width: ram.i32(at + WIDTH),
            length: ram.i32(at + LENGTH),
            height: ram.i32(at + HEIGHT),
            unknown_6b4: ram.i32(at + UNKNOWN_6B4),
            unknown_865: ram.u8(at + UNKNOWN_865),
            unknown_86a: ram.u8(at + UNKNOWN_86A),
            unknown_86b: ram.u8(at + UNKNOWN_86B),
        }
    }

    /// Writes the car back over the record at `at`.
    pub fn write(&self, ram: &mut Ram, at: u32) {
        ram.set_i32(at + FLAGS, self.flags);
        ram.set_i32(at + STEER, self.steer);
        ram.set_i32(at + ACCEL, self.accel);
        ram.set_i32(at + BRAKE, self.brake);
        ram.set_u8(at + HANDBRAKE, self.handbrake);
        self.body.write(ram, at + BODY);
        ram.set_vec3(at + DRAG_POINT, self.drag_point);
        ram.set_u8(at + WHEEL_COUNT, self.wheels.len() as u8);
        for (i, wheel) in self.wheels.iter().enumerate() {
            wheel.write(ram, at + WHEELS + i as u32 * WHEEL_SIZE);
        }
        ram.set_u8(at + GROUNDED, self.grounded);
        ram.set_u8(at + GROUNDED_LEVEL, self.grounded_level);
        self.engine.write(ram, at + ENGINE);
        ram.set_i32(at + MASS, self.mass);
        ram.set_i32(at + CG_ALONG, self.cg_along);
        ram.set_i32(at + CG_UP, self.cg_up);
        ram.set_i32(at + BRAKE_GRIP, self.brake_grip);
        ram.set_i32(at + BRAKE_BIAS, self.brake_bias);
        ram.set_i32(at + DRAG, self.drag);
        write_axle(&self.front, ram, at, false);
        write_axle(&self.rear, ram, at, true);
        ram.set_vec3(at + ORIGIN, self.origin);
        ram.set_i32(at + WIDTH, self.width);
        ram.set_i32(at + LENGTH, self.length);
        ram.set_i32(at + HEIGHT, self.height);
        ram.set_i32(at + UNKNOWN_6B4, self.unknown_6b4);
        ram.set_u8(at + UNKNOWN_865, self.unknown_865);
        ram.set_u8(at + UNKNOWN_86A, self.unknown_86a);
        ram.set_u8(at + UNKNOWN_86B, self.unknown_86b);
    }
}
