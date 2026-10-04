//! Where the original keeps a car: one 0x930-byte record each, in an array
//! at 0x80128fcc, with the count at 0x800d263c. `cars_update` (0x8004064c)
//! walks it each frame and dispatches on the state byte at +0x891.
//!
//! The codec here converts between that layout and [`Car`]; the port's logic
//! never sees it. Tests and the reference's shadow checks use it to hold the
//! port to the original byte for byte.

use super::handling::{BLOCK_A, Handling};
use super::{AxisLock, Car, Engine, Ground, GroundPlane, Tuning, TuningSet, Wheel};
use crate::body::{self, Body};
use crate::ram::Ram;

pub const CARS: u32 = 0x8012_8fcc;
pub const CAR_SIZE: u32 = 0x930;
pub const CAR_COUNT: u32 = 0x800d_263c;

pub const SLOT: u32 = 0x0;
pub const FLAGS: u32 = 0x4;
pub const FLAGS_8: u32 = 0x8;
pub const PLAYER: u32 = 0xc;
pub const STEER: u32 = 0x10;
pub const ACCEL: u32 = 0x14;
pub const BRAKE: u32 = 0x18;
pub const STICK: u32 = 0x1c;
pub const HANDBRAKE: u32 = 0x24;
/// The rigid body; the car's position, velocity and the rest are its.
pub const BODY: u32 = 0x30;
pub const GRAVITY_DIR: u32 = BODY + body::layout::GRAVITY_DIR;
/// The body's centre of mass offset, where drag acts.
pub const DRAG_POINT: u32 = BODY + body::layout::CENTRE;
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
/// CWH block A, the handling, from here to +0x770.
pub const HANDLING: u32 = 0x63c;
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
pub const AIR_PITCH: u32 = 0x69c;
pub const AIR_ROLL: u32 = 0x6a0;
pub const AIR_YAW: u32 = 0x6a4;
pub const UNKNOWN_6B4: u32 = 0x6b4;
pub const ORIGIN: u32 = 0x770;
pub const WIDTH: u32 = 0x780;
pub const LENGTH: u32 = 0x784;
pub const HEIGHT: u32 = 0x788;
pub const SPRING_PRELOAD: u32 = 0x790;
pub const EXTENSION: u32 = 0x794;
pub const UNKNOWN_865: u32 = 0x865;
pub const STATE: u32 = 0x891;
pub const WRECKED: u32 = 0x62c;
pub const WRECK_VIEW: u32 = 0x62d;
pub const UNKNOWN_25: u32 = 0x25;
pub const UNKNOWN_26: u32 = 0x26;
pub const UNKNOWN_27: u32 = 0x27;
pub const UNKNOWN_5E3: u32 = 0x5e3;
pub const LAP_DISTANCE: u32 = 0x5dc;
/// The ground: the floor (found +0x8b2, origin +0x8b4, normal +0x8c4, d
/// +0x8d4) and the nearest surface (found +0x8f0, normal +0x8f4, d +0x904).
pub const FLOOR: [u32; 4] = [0x8b2, 0x8b4, 0x8c4, 0x8d4];
pub const NEAREST: [u32; 3] = [0x8f0, 0x8f4, 0x904];
pub const AIR_TOTAL_MS: u32 = 0x618;
pub const STUNT_SPIN: u32 = 0x5e4;
pub const STUNT_TURN: u32 = 0x5f4;
pub const STUNT_PEAK: u32 = 0x604;
pub const AIR_MS: u32 = 0x614;
pub const TURBO_HINT: u32 = 0x875;
pub const STUNT_POINTS: u32 = 0x624;
pub const AIRBORNE: u32 = 0x628;
pub const CONTACT_CLOCK: u32 = 0x61c;
pub const CONTACT_MS: u32 = 0x620;
pub const CONTACT_TIME: u32 = 0x920;
pub const UNKNOWN_7CC: u32 = 0x7cc;
pub const TURBOS: u32 = 0x874;
pub const AIR_CONTROL: u32 = 0x86a;
pub const AIR_ARMED: u32 = 0x86b;
/// The axis lock: active (byte), axis (word), direction (VECTOR).
pub const AIR_LOCK: u32 = 0x876;
pub const AIR_LOCK_AXIS: u32 = 0x878;
pub const AIR_LOCK_DIR: u32 = 0x87c;
pub const RIGHTING: u32 = 0x90c;
pub const RIGHTS_ITSELF: u32 = 0x918;
pub const ROLL_WAY: u32 = 0x919;

/// The shared tuning bytes.
pub const TUNING: u32 = 0x8013_6a18;

/// Offsets within a wheel.
pub mod wheel {
    pub const MOUNT: u32 = 0x00;
    pub const MOUNT_PAD: u32 = 0x0c;
    pub const UNKNOWN_10: u32 = 0x10;
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
    pub const COMPRESSION: u32 = 0x7c;
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
    pub const COMPUTER_SKILL: u32 = 14;
    pub const STEER_MPH: u32 = 8;
    pub const STEER_PERCENT: [u32; 2] = [0x3a, 9];
    pub const PLAYER_SKILL: u32 = 16;
    pub const RIGHT_SIDE_MS: u32 = 0x35;
    pub const RIGHT_END_MS: u32 = 0x36;
    pub const RIGHT_ROOF_MS: u32 = 0x37;
    pub const STAY_ROOF_PERCENT: u32 = 0x38;
    pub const WRECK_ROOF_TENS: u32 = 0x39;
}

impl Wheel {
    pub fn read(ram: &Ram, w: u32) -> Wheel {
        use wheel::*;
        Wheel {
            mount: ram.vec3(w + MOUNT),
            mount_pad: ram.i32(w + MOUNT_PAD),
            unknown_10: ram.i32(w + UNKNOWN_10),
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
            compression: ram.i32(w + COMPRESSION),
            spin_rate: ram.i32(w + SPIN_RATE),
        }
    }

    pub fn write(&self, ram: &mut Ram, w: u32) {
        use wheel::*;
        ram.set_vec3(w + MOUNT, self.mount);
        ram.set_i32(w + MOUNT_PAD, self.mount_pad);
        ram.set_i32(w + UNKNOWN_10, self.unknown_10);
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
        ram.set_i32(w + COMPRESSION, self.compression);
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
    /// TUNING.PRM, the 256 bytes the game loads to 0x80136a18.
    pub fn from_prm(b: &[u8]) -> Tuning {
        use tuning::*;
        let at = |k: u32| b.get(k as usize).copied().unwrap_or(0);
        let set = |k: u32| {
            let o = SETS + 3 * k;
            TuningSet { downforce_front: at(o), downforce_rear: at(o + 1), grip_front: at(o + 2) }
        };
        Tuning {
            downforce_mph: at(DOWNFORCE_MPH),
            sets: [set(0), set(1), set(2)],
            surface_drag: at(SURFACE_DRAG),
            computer_skill: [at(COMPUTER_SKILL), at(COMPUTER_SKILL + 1)],
            player_skill: [at(PLAYER_SKILL), at(PLAYER_SKILL + 1)],
            steer_mph: at(STEER_MPH),
            steer_percent: STEER_PERCENT.map(at),
            right_side_ms: at(RIGHT_SIDE_MS),
            right_end_ms: at(RIGHT_END_MS),
            stay_roof_percent: at(STAY_ROOF_PERCENT),
            right_roof_ms: at(RIGHT_ROOF_MS),
            wreck_roof_tens: at(WRECK_ROOF_TENS),
        }
    }

    pub fn read(ram: &Ram) -> Tuning {
        let bytes: Vec<u8> = (0..256).map(|k| ram.u8(TUNING + k)).collect();
        Tuning::from_prm(&bytes)
    }
}

impl Car {
    /// The car record at `at`.
    pub fn read(ram: &Ram, at: u32) -> Car {
        let wheels = (0..ram.u8(at + WHEEL_COUNT) as u32).map(|i| Wheel::read(ram, at + WHEELS + i * WHEEL_SIZE));
        let handling: [u8; BLOCK_A] = std::array::from_fn(|k| ram.u8(at + HANDLING + k as u32));
        Car {
            slot: ram.u8(at + SLOT),
            flags: ram.i32(at + FLAGS),
            flags_8: ram.i32(at + FLAGS_8),
            player: ram.u8(at + PLAYER),
            steer: ram.i32(at + STEER),
            accel: ram.i32(at + ACCEL),
            brake: ram.i32(at + BRAKE),
            stick: [ram.i32(at + STICK), ram.i32(at + STICK + 4)],
            handbrake: ram.u8(at + HANDBRAKE),
            body: Body::read(ram, at + BODY),
            wheels: wheels.collect(),
            grounded: ram.u8(at + GROUNDED),
            grounded_level: ram.u8(at + GROUNDED_LEVEL),
            engine: Engine::read(ram, at + ENGINE),
            front_wheels: ram.u8(at + FRONT_WHEELS),
            rear_wheels: ram.u8(at + REAR_WHEELS),
            handling: Handling::from_bytes(&handling),
            origin: ram.vec3(at + ORIGIN),
            origin_pad: ram.i32(at + ORIGIN + 12),
            width: ram.i32(at + WIDTH),
            length: ram.i32(at + LENGTH),
            height: ram.i32(at + HEIGHT),
            size_pad: ram.i32(at + HEIGHT + 4),
            spring_preload: ram.i32(at + SPRING_PRELOAD),
            extension: [ram.i32(at + EXTENSION), ram.i32(at + EXTENSION + 4)],
            unknown_865: ram.u8(at + UNKNOWN_865),
            state: ram.u8(at + STATE),
            wrecked: ram.u8(at + WRECKED),
            wreck_view: ram.u8(at + WRECK_VIEW),
            unknown_25: ram.u8(at + UNKNOWN_25),
            unknown_26: ram.u8(at + UNKNOWN_26),
            unknown_27: ram.u8(at + UNKNOWN_27),
            unknown_5e3: ram.u8(at + UNKNOWN_5E3),
            lap_distance: ram.i32(at + LAP_DISTANCE),
            ground: Ground {
                floor: GroundPlane {
                    found: ram.u8(at + FLOOR[0]),
                    normal: ram.vec3(at + FLOOR[2]),
                    d: ram.i32(at + FLOOR[3]),
                },
                origin: ram.vec3(at + FLOOR[1]),
                nearest: GroundPlane {
                    found: ram.u8(at + NEAREST[0]),
                    normal: ram.vec3(at + NEAREST[1]),
                    d: ram.i32(at + NEAREST[2]),
                },
            },
            air_total_ms: ram.i32(at + AIR_TOTAL_MS) as u32,
            stunt_spin: ram.vec3(at + STUNT_SPIN),
            stunt_turn: ram.vec3(at + STUNT_TURN),
            stunt_peak: ram.vec3(at + STUNT_PEAK),
            air_ms: ram.i32(at + AIR_MS) as u32,
            turbo_hint: ram.u8(at + TURBO_HINT),
            stunt_points: ram.i32(at + STUNT_POINTS),
            airborne: ram.u8(at + AIRBORNE),
            contact_clock: ram.i32(at + CONTACT_CLOCK) as u32,
            contact_ms: ram.i32(at + CONTACT_MS) as u32,
            contact_time: ram.i32(at + CONTACT_TIME) as u32,
            unknown_7cc: ram.i32(at + UNKNOWN_7CC),
            turbos: ram.u8(at + TURBOS),
            air_control: ram.u8(at + AIR_CONTROL),
            air_armed: ram.u8(at + AIR_ARMED),
            air_lock: AxisLock {
                active: ram.u8(at + AIR_LOCK),
                axis: ram.i32(at + AIR_LOCK_AXIS),
                dir: ram.vec3(at + AIR_LOCK_DIR),
            },
            righting: [0, 1, 2].map(|k| ram.i32(at + RIGHTING + 4 * k) as u32),
            rights_itself: ram.u8(at + RIGHTS_ITSELF),
            roll_way: ram.u8(at + ROLL_WAY),
        }
    }

    /// Writes the car back over the record at `at`.
    pub fn write(&self, ram: &mut Ram, at: u32) {
        ram.set_u8(at + SLOT, self.slot);
        ram.set_i32(at + FLAGS, self.flags);
        ram.set_i32(at + FLAGS_8, self.flags_8);
        ram.set_u8(at + PLAYER, self.player);
        ram.set_i32(at + STEER, self.steer);
        ram.set_i32(at + ACCEL, self.accel);
        ram.set_i32(at + BRAKE, self.brake);
        ram.set_i32(at + STICK, self.stick[0]);
        ram.set_i32(at + STICK + 4, self.stick[1]);
        ram.set_u8(at + HANDBRAKE, self.handbrake);
        self.body.write(ram, at + BODY);
        ram.set_u8(at + WHEEL_COUNT, self.wheels.len() as u8);
        for (i, wheel) in self.wheels.iter().enumerate() {
            wheel.write(ram, at + WHEELS + i as u32 * WHEEL_SIZE);
        }
        ram.set_u8(at + GROUNDED, self.grounded);
        ram.set_u8(at + GROUNDED_LEVEL, self.grounded_level);
        self.engine.write(ram, at + ENGINE);
        ram.set_u8(at + FRONT_WHEELS, self.front_wheels);
        ram.set_u8(at + REAR_WHEELS, self.rear_wheels);
        for (k, b) in self.handling.to_bytes().iter().enumerate() {
            ram.set_u8(at + HANDLING + k as u32, *b);
        }
        ram.set_vec3(at + ORIGIN, self.origin);
        ram.set_i32(at + ORIGIN + 12, self.origin_pad);
        ram.set_i32(at + WIDTH, self.width);
        ram.set_i32(at + LENGTH, self.length);
        ram.set_i32(at + HEIGHT, self.height);
        ram.set_i32(at + HEIGHT + 4, self.size_pad);
        ram.set_i32(at + SPRING_PRELOAD, self.spring_preload);
        ram.set_i32(at + EXTENSION, self.extension[0]);
        ram.set_i32(at + EXTENSION + 4, self.extension[1]);
        ram.set_u8(at + UNKNOWN_865, self.unknown_865);
        ram.set_u8(at + STATE, self.state);
        ram.set_u8(at + WRECKED, self.wrecked);
        ram.set_u8(at + WRECK_VIEW, self.wreck_view);
        ram.set_u8(at + UNKNOWN_25, self.unknown_25);
        ram.set_u8(at + UNKNOWN_26, self.unknown_26);
        ram.set_u8(at + UNKNOWN_27, self.unknown_27);
        ram.set_u8(at + UNKNOWN_5E3, self.unknown_5e3);
        ram.set_i32(at + LAP_DISTANCE, self.lap_distance);
        let g = &self.ground;
        ram.set_u8(at + FLOOR[0], g.floor.found);
        ram.set_vec3(at + FLOOR[1], g.origin);
        ram.set_vec3(at + FLOOR[2], g.floor.normal);
        ram.set_i32(at + FLOOR[3], g.floor.d);
        ram.set_u8(at + NEAREST[0], g.nearest.found);
        ram.set_vec3(at + NEAREST[1], g.nearest.normal);
        ram.set_i32(at + NEAREST[2], g.nearest.d);
        ram.set_i32(at + AIR_TOTAL_MS, self.air_total_ms as i32);
        ram.set_vec3(at + STUNT_SPIN, self.stunt_spin);
        ram.set_vec3(at + STUNT_TURN, self.stunt_turn);
        ram.set_vec3(at + STUNT_PEAK, self.stunt_peak);
        ram.set_i32(at + AIR_MS, self.air_ms as i32);
        ram.set_u8(at + TURBO_HINT, self.turbo_hint);
        ram.set_i32(at + STUNT_POINTS, self.stunt_points);
        ram.set_u8(at + AIRBORNE, self.airborne);
        ram.set_i32(at + CONTACT_CLOCK, self.contact_clock as i32);
        ram.set_i32(at + CONTACT_MS, self.contact_ms as i32);
        ram.set_i32(at + CONTACT_TIME, self.contact_time as i32);
        ram.set_i32(at + UNKNOWN_7CC, self.unknown_7cc);
        ram.set_u8(at + TURBOS, self.turbos);
        ram.set_u8(at + AIR_CONTROL, self.air_control);
        ram.set_u8(at + AIR_ARMED, self.air_armed);
        ram.set_u8(at + AIR_LOCK, self.air_lock.active);
        ram.set_i32(at + AIR_LOCK_AXIS, self.air_lock.axis);
        ram.set_vec3(at + AIR_LOCK_DIR, self.air_lock.dir);
        for (k, t) in self.righting.iter().enumerate() {
            ram.set_i32(at + RIGHTING + 4 * k as u32, *t as i32);
        }
        ram.set_u8(at + RIGHTS_ITSELF, self.rights_itself);
        ram.set_u8(at + ROLL_WAY, self.roll_way);
    }
}
