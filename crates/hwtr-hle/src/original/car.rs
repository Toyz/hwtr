//! Where the original keeps a car: one 0x930-byte record each, in an array
//! at 0x80128fcc, with the count at 0x800d263c. `cars_update` (0x8004064c)
//! walks it each frame and dispatches on the state byte at +0x891.
//!
//! The codec here converts between that layout and [`Car`]; the port's logic
//! never sees it. Tests and the reference's shadow checks use it to hold the
//! port to the original byte for byte.

use super::body;
use super::{InMemory, Ram};
use hwtr_game::body::Body;
use hwtr_game::car::handling::{BLOCK_A, Handling};
use hwtr_game::car::{Armed, AxisLock, Car, Engine, Ground, GroundPlane, Respawn, Tuning, Wheel};
use hwtr_game::laps::Laps;

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
pub const GRAVITY_DIR: u32 = BODY + body::GRAVITY_DIR;
/// The body's centre of mass offset, where drag acts.
pub const DRAG_POINT: u32 = BODY + body::CENTRE;
pub const POS: u32 = BODY + body::POS;
pub const VEL: u32 = BODY + body::VEL;
pub const SPEED: u32 = BODY + body::SPEED;
pub const ROT: u32 = BODY + body::ROT;
pub const SPIN: u32 = BODY + body::SPIN;
pub const FORCE: u32 = BODY + body::FORCE;
pub const TORQUE: u32 = BODY + body::TORQUE;
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
pub const ALL_TERRAIN: u32 = 0x865;
pub const STATE: u32 = 0x891;
pub const WRECKED: u32 = 0x62c;
pub const WRECK_VIEW: u32 = 0x62d;
pub const RESET_HELD: u32 = 0x25;
pub const TURBO_HELD: u32 = 0x27;
pub const FINISHED: u32 = 0x5e3;
pub const LAP_START: u32 = 0x5a8;
pub const PASSED: u32 = 0x5ac;
pub const LAP_ENDS: u32 = 0x5b8;
pub const BEST_LAP: u32 = 0x5d8;
pub const LAPS_DONE: u32 = 0x5e0;
pub const PASSED_COUNT: u32 = 0x5e1;
pub const PLACE: u32 = 0x5e2;
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
pub const BOOSTING: u32 = 0x634;
pub const BOOST_SPEED: u32 = 0x638;
pub const STUNT_POINTS: u32 = 0x624;
pub const AIRBORNE: u32 = 0x628;
pub const CONTACT_CLOCK: u32 = 0x61c;
pub const CONTACT_MS: u32 = 0x620;
pub const CONTACT_TIME: u32 = 0x920;
pub const RESPAWN_POS: u32 = 0x79c;
pub const RESPAWN_ROT: u32 = 0x7ac;
pub const RESET_REQUESTED: u32 = 0x869;
pub const RESET_GRACE: u32 = 0x924;
pub const WRECK_MS: u32 = 0x630;
pub const TURBO_BEFORE: u32 = 0x29;
pub const STRONG_BRAKES: u32 = 0x864;
pub const STEEL: u32 = 0x866;
pub const RUBBER: u32 = 0x867;
pub const GYRO: u32 = 0x868;
pub const POWER_UP: u32 = 0x892;
/// The race setup's options (0x80138c94 +0x1c), which size the cars.
pub const RACE_OPTIONS: u32 = 0x8013_8cb0;
pub const STUCK_MS: u32 = 0x91c;
pub const WRONG_WAY: u32 = 0x86c;
pub const WRONG_WAY_MS: u32 = 0x870;
pub const JUST_RESET: u32 = 0x928;
pub const RESPAWN_ZONE: u32 = 0x7cc;
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
    pub const WIDTH: u32 = 0x10;
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

impl InMemory for Wheel {
    fn read(ram: &Ram, w: u32) -> Wheel {
        use wheel::*;
        Wheel {
            mount: ram.vec3(w + MOUNT),
            width: ram.i32(w + wheel::WIDTH),
            diameter: ram.i32(w + DIAMETER),
            rear: ram.u8(w + FLAGS) & 1 != 0,
            steers: ram.u8(w + FLAGS) & 2 != 0,
            driven: ram.u8(w + FLAGS) & 4 != 0,
            heading: ram.vec3(w + HEADING),
            world: ram.vec3(w + WORLD),
            on_ground: ram.flag(w + GROUND),
            surface: ram.u8(w + SURFACE),
            normal: ram.vec3(w + NORMAL),
            contact: ram.vec3(w + CONTACT),
            contact_vel: ram.vec3(w + CONTACT_VEL),
            friction: ram.i32(w + FRICTION),
            spring: ram.i32(w + SPRING),
            slipping: ram.flag(w + SLIP),
            compression: ram.i32(w + COMPRESSION),
            spin_rate: ram.i32(w + SPIN_RATE),
        }
    }

    fn write(&self, ram: &mut Ram, w: u32) {
        use wheel::*;
        ram.set_vec3(w + MOUNT, self.mount);
        ram.set_i32(w + wheel::WIDTH, self.width);
        ram.set_i32(w + DIAMETER, self.diameter);
        // Bits the port does not model are left as they are.
        let others = ram.u8(w + FLAGS) & !7;
        ram.set_u8(w + FLAGS, others | self.rear as u8 | (self.steers as u8) << 1 | (self.driven as u8) << 2);
        ram.set_vec3(w + HEADING, self.heading);
        ram.set_vec3(w + WORLD, self.world);
        ram.set_flag(w + GROUND, self.on_ground);
        ram.set_u8(w + SURFACE, self.surface);
        ram.set_vec3(w + NORMAL, self.normal);
        ram.set_vec3(w + CONTACT, self.contact);
        ram.set_vec3(w + CONTACT_VEL, self.contact_vel);
        ram.set_i32(w + FRICTION, self.friction);
        ram.set_i32(w + SPRING, self.spring);
        ram.set_flag(w + SLIP, self.slipping);
        ram.set_i32(w + COMPRESSION, self.compression);
        ram.set_i32(w + SPIN_RATE, self.spin_rate);
    }
}

impl InMemory for Engine {
    fn read(ram: &Ram, e: u32) -> Engine {
        use engine::*;
        Engine {
            rpm: ram.i32(e + RPM),
            wheel_rpm: ram.i32(e + WHEEL_RPM),
            reverse: ram.flag(e + REVERSE),
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

    fn write(&self, ram: &mut Ram, e: u32) {
        use engine::*;
        ram.set_i32(e + RPM, self.rpm);
        ram.set_i32(e + WHEEL_RPM, self.wheel_rpm);
        ram.set_flag(e + REVERSE, self.reverse);
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

/// The settings the original loaded (TUNING.PRM, at [`TUNING`]).
pub fn tuning(ram: &Ram) -> Tuning {
    let bytes: Vec<u8> = (0..256).map(|k| ram.u8(TUNING + k)).collect();
    Tuning::from_prm(&bytes)
}

impl InMemory for Car {
    /// The car record at `at`.
    fn read(ram: &Ram, at: u32) -> Car {
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
            handbrake: ram.flag(at + HANDBRAKE),
            body: Body::read(ram, at + BODY),
            wheels: wheels.collect(),
            grounded: ram.u8(at + GROUNDED),
            grounded_level: ram.u8(at + GROUNDED_LEVEL),
            engine: Engine::read(ram, at + ENGINE),
            front_wheels: ram.u8(at + FRONT_WHEELS),
            rear_wheels: ram.u8(at + REAR_WHEELS),
            handling: Handling::from_bytes(&handling),
            origin: ram.vec3(at + ORIGIN),
            width: ram.i32(at + WIDTH),
            length: ram.i32(at + LENGTH),
            height: ram.i32(at + HEIGHT),
            spring_preload: ram.i32(at + SPRING_PRELOAD),
            extension: [ram.i32(at + EXTENSION), ram.i32(at + EXTENSION + 4)],
            all_terrain: ram.flag(at + ALL_TERRAIN),
            state: ram.u8(at + STATE),
            wrecked: ram.flag(at + WRECKED),
            wreck_line: ram.flag(at + WRECK_VIEW),
            // An event the port passes on at once; never left set.
            jolted: false,
            wreck_ms: ram.i32(at + WRECK_MS) as u32,
            reset_requested: ram.flag(at + RESET_REQUESTED),
            reset_grace_ms: ram.i32(at + RESET_GRACE) as u32,
            reset_held: ram.flag(at + RESET_HELD),
            turbo_held: ram.flag(at + TURBO_HELD),
            turbo_before: ram.flag(at + TURBO_BEFORE),
            strong_brakes: ram.flag(at + STRONG_BRAKES),
            steel: ram.flag(at + STEEL),
            rubber: ram.flag(at + RUBBER),
            gyro: ram.flag(at + GYRO),
            power_up: ram.u8(at + POWER_UP),
            turbo_given: false,
            options: ram.i32(RACE_OPTIONS) as u32,
            stuck_ms: ram.i32(at + STUCK_MS) as u32,
            wrong_way_ms: ram.i32(at + WRONG_WAY_MS) as u32,
            wrong_way: ram.flag(at + WRONG_WAY),
            just_reset: ram.flag(at + JUST_RESET),
            model_faces: {
                let model = super::effects::model(ram, ram.u8(at + SLOT) as u32);
                if model == 0 { 0 } else { ram.i32(ram.i32(model + 4) as u32 + 0x34).clamp(0, 0xffff) as u16 }
            },
            wreck_draws: Default::default(),
            turbo_fired: Default::default(),
            sounds: Default::default(),
            lines: Default::default(),
            laps: laps(ram, at),
            lap_distance: ram.i32(at + LAP_DISTANCE),
            ground: Ground {
                floor: GroundPlane {
                    found: ram.flag(at + FLOOR[0]),
                    normal: ram.vec3(at + FLOOR[2]),
                    d: ram.i32(at + FLOOR[3]),
                },
                origin: ram.vec3(at + FLOOR[1]),
                nearest: GroundPlane {
                    found: ram.flag(at + NEAREST[0]),
                    normal: ram.vec3(at + NEAREST[1]),
                    d: ram.i32(at + NEAREST[2]),
                },
            },
            air_total_ms: ram.i32(at + AIR_TOTAL_MS) as u32,
            stunt_spin: ram.vec3(at + STUNT_SPIN),
            stunt_turn: ram.vec3(at + STUNT_TURN),
            stunt_peak: ram.vec3(at + STUNT_PEAK),
            air_ms: ram.i32(at + AIR_MS) as u32,
            turbo_hint: ram.flag(at + TURBO_HINT),
            boost: ram.flag(at + BOOSTING).then(|| ram.i32(at + BOOST_SPEED)),
            stunt_points: ram.i32(at + STUNT_POINTS),
            airborne: ram.flag(at + AIRBORNE),
            contact_clock: ram.i32(at + CONTACT_CLOCK) as u32,
            contact_ms: ram.i32(at + CONTACT_MS) as u32,
            contact_time: ram.i32(at + CONTACT_TIME) as u32,
            respawn: Respawn {
                pos: ram.vec3(at + RESPAWN_POS),
                rot: ram.matrix(at + RESPAWN_ROT),
                zone: u16::try_from(ram.i32(at + RESPAWN_ZONE)).ok(),
            },
            turbos: ram.u8(at + TURBOS),
            air_control: ram.flag(at + AIR_CONTROL),
            air_armed: Armed { along: ram.u8(at + AIR_ARMED) & 1 != 0, across: ram.u8(at + AIR_ARMED) & 2 != 0 },
            air_lock: AxisLock {
                active: ram.flag(at + AIR_LOCK),
                axis: ram.i32(at + AIR_LOCK_AXIS),
                dir: ram.vec3(at + AIR_LOCK_DIR),
            },
            righting: [0, 1, 2].map(|k| ram.i32(at + RIGHTING + 4 * k) as u32),
            rights_itself: ram.flag(at + RIGHTS_ITSELF),
            roll_way: ram.flag(at + ROLL_WAY),
        }
    }

    /// Writes the car back over the record at `at`.
    fn write(&self, ram: &mut Ram, at: u32) {
        ram.set_u8(at + SLOT, self.slot);
        ram.set_i32(at + FLAGS, self.flags);
        ram.set_i32(at + FLAGS_8, self.flags_8);
        ram.set_u8(at + PLAYER, self.player);
        ram.set_i32(at + STEER, self.steer);
        ram.set_i32(at + ACCEL, self.accel);
        ram.set_i32(at + BRAKE, self.brake);
        ram.set_i32(at + STICK, self.stick[0]);
        ram.set_i32(at + STICK + 4, self.stick[1]);
        ram.set_flag(at + HANDBRAKE, self.handbrake);
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
        write_handling(ram, at + HANDLING, &self.handling);
        ram.set_vec3(at + ORIGIN, self.origin);
        ram.set_i32(at + WIDTH, self.width);
        ram.set_i32(at + LENGTH, self.length);
        ram.set_i32(at + HEIGHT, self.height);
        ram.set_i32(at + SPRING_PRELOAD, self.spring_preload);
        ram.set_i32(at + EXTENSION, self.extension[0]);
        ram.set_i32(at + EXTENSION + 4, self.extension[1]);
        ram.set_flag(at + ALL_TERRAIN, self.all_terrain);
        ram.set_u8(at + STATE, self.state);
        ram.set_flag(at + WRECKED, self.wrecked);
        ram.set_flag(at + WRECK_VIEW, self.wreck_line);
        ram.set_i32(at + WRECK_MS, self.wreck_ms as i32);
        ram.set_flag(at + RESET_REQUESTED, self.reset_requested);
        ram.set_i32(at + RESET_GRACE, self.reset_grace_ms as i32);
        ram.set_flag(at + RESET_HELD, self.reset_held);
        ram.set_flag(at + TURBO_HELD, self.turbo_held);
        ram.set_flag(at + TURBO_BEFORE, self.turbo_before);
        ram.set_flag(at + STRONG_BRAKES, self.strong_brakes);
        ram.set_flag(at + STEEL, self.steel);
        ram.set_flag(at + RUBBER, self.rubber);
        ram.set_flag(at + GYRO, self.gyro);
        ram.set_u8(at + POWER_UP, self.power_up);
        ram.set_i32(at + STUCK_MS, self.stuck_ms as i32);
        ram.set_i32(at + WRONG_WAY_MS, self.wrong_way_ms as i32);
        ram.set_flag(at + WRONG_WAY, self.wrong_way);
        ram.set_flag(at + JUST_RESET, self.just_reset);
        write_laps(ram, at, &self.laps);
        ram.set_i32(at + LAP_DISTANCE, self.lap_distance);
        let g = &self.ground;
        ram.set_flag(at + FLOOR[0], g.floor.found);
        ram.set_vec3(at + FLOOR[1], g.origin);
        ram.set_vec3(at + FLOOR[2], g.floor.normal);
        ram.set_i32(at + FLOOR[3], g.floor.d);
        ram.set_flag(at + NEAREST[0], g.nearest.found);
        ram.set_vec3(at + NEAREST[1], g.nearest.normal);
        ram.set_i32(at + NEAREST[2], g.nearest.d);
        ram.set_i32(at + AIR_TOTAL_MS, self.air_total_ms as i32);
        ram.set_vec3(at + STUNT_SPIN, self.stunt_spin);
        ram.set_vec3(at + STUNT_TURN, self.stunt_turn);
        ram.set_vec3(at + STUNT_PEAK, self.stunt_peak);
        ram.set_i32(at + AIR_MS, self.air_ms as i32);
        ram.set_flag(at + TURBO_HINT, self.turbo_hint);
        ram.set_flag(at + BOOSTING, self.boost.is_some());
        if let Some(speed) = self.boost {
            ram.set_i32(at + BOOST_SPEED, speed);
        }
        ram.set_i32(at + STUNT_POINTS, self.stunt_points);
        ram.set_flag(at + AIRBORNE, self.airborne);
        ram.set_i32(at + CONTACT_CLOCK, self.contact_clock as i32);
        ram.set_i32(at + CONTACT_MS, self.contact_ms as i32);
        ram.set_i32(at + CONTACT_TIME, self.contact_time as i32);
        ram.set_vec3(at + RESPAWN_POS, self.respawn.pos);
        ram.set_matrix(at + RESPAWN_ROT, &self.respawn.rot);
        ram.set_i32(at + RESPAWN_ZONE, self.respawn.zone.map_or(-1, i32::from));
        ram.set_u8(at + TURBOS, self.turbos);
        ram.set_flag(at + AIR_CONTROL, self.air_control);
        let others = ram.u8(at + AIR_ARMED) & !3;
        ram.set_u8(at + AIR_ARMED, others | self.air_armed.along as u8 | (self.air_armed.across as u8) << 1);
        ram.set_flag(at + AIR_LOCK, self.air_lock.active);
        ram.set_i32(at + AIR_LOCK_AXIS, self.air_lock.axis);
        ram.set_vec3(at + AIR_LOCK_DIR, self.air_lock.dir);
        for (k, t) in self.righting.iter().enumerate() {
            ram.set_i32(at + RIGHTING + 4 * k as u32, *t as i32);
        }
        ram.set_flag(at + RIGHTS_ITSELF, self.rights_itself);
        ram.set_flag(at + ROLL_WAY, self.roll_way);
    }
}

/// Where block A keeps each field the port models (the rest it leaves as
/// the original has it).
mod block_a {
    pub const DRIVE: u32 = 0x00;
    pub const STEER_LOCK: u32 = 0x04;
    pub const MASS: u32 = 0x08;
    pub const DOWNFORCE: u32 = 0x20;
    pub const AXLES: u32 = 0x30;
    pub const AIR_POWER: u32 = 0x60;
    pub const AI_PACE: u32 = 0x6c;
    pub const AI_RUBBER: u32 = 0x70;
    pub const SKILL: u32 = 0x74;
    pub const FLAGS: u32 = 0x78;
    pub const WHEEL_COUNT: u32 = 0x7c;
    pub const ORIGIN: u32 = 0x80;
    pub const SIZE: u32 = 0x90;
    pub const MOUNTS: u32 = 0xa0;
    pub const DIAMETERS: u32 = 0x118;
}

/// Writes the handling's fields over block A at `at`.
fn write_handling(ram: &mut Ram, at: u32, h: &Handling) {
    use block_a::*;
    for (k, on) in [h.front_driven, h.front_steers, h.rear_driven, h.rear_steers].into_iter().enumerate() {
        ram.set_flag(at + DRIVE + k as u32, on);
    }
    ram.set_i32(at + STEER_LOCK, h.steer_lock);
    for (k, v) in [h.mass, h.cg_along, h.cg_up, h.brake_grip, h.brake_bias, h.drag].into_iter().enumerate() {
        ram.set_i32(at + MASS + 4 * k as u32, v);
    }
    let down = [h.front.downforce, h.rear.downforce, h.front.downforce_scale, h.rear.downforce_scale];
    for (k, v) in down.into_iter().enumerate() {
        ram.set_i32(at + DOWNFORCE + 4 * k as u32, v);
    }
    for (a, axle) in [h.front, h.rear].into_iter().enumerate() {
        let fields = [axle.stiffness, axle.grip, axle.travel, axle.damp_in, axle.damp_out, axle.ride_height];
        for (k, v) in fields.into_iter().enumerate() {
            ram.set_i32(at + AXLES + 24 * a as u32 + 4 * k as u32, v);
        }
    }
    for (k, v) in [h.air_power.pitch, h.air_power.roll, h.air_power.yaw].into_iter().enumerate() {
        ram.set_i32(at + AIR_POWER + 4 * k as u32, v);
    }
    ram.set_i32(at + AI_PACE, h.ai_pace);
    ram.set_i32(at + AI_RUBBER, h.ai_rubber);
    ram.set_i32(at + SKILL, h.skill);
    ram.set_i32(at + FLAGS, ((h.flags & !1) | h.all_terrain as u32) as i32);
    ram.set_u8(at + WHEEL_COUNT, h.wheel_count);
    ram.set_vec3(at + ORIGIN, h.origin);
    ram.set_vec3(at + SIZE, h.size);
    for (k, m) in h.mounts.iter().enumerate() {
        ram.set_vec3(at + MOUNTS + 16 * k as u32, *m);
    }
    for (k, w) in h.widths.iter().enumerate() {
        ram.set_i32(at + DIAMETERS - 24 + 4 * k as u32, *w);
    }
    for (k, d) in h.diameters.iter().enumerate() {
        ram.set_i32(at + DIAMETERS + 4 * k as u32, *d);
    }
}

/// A car's laps. The lap ends are eight words; the laps done tell how many
/// are in use (one in a flying-lap race, which keeps the count at one).
fn laps(ram: &Ram, at: u32) -> Laps {
    let done = ram.u8(at + LAPS_DONE);
    let used = (done as u32).clamp(1, 8);
    let ends: Vec<u32> = (0..used).map(|k| ram.i32(at + LAP_ENDS + 4 * k) as u32).collect();
    Laps {
        start: ram.i32(at + LAP_START) as u32,
        done,
        passed: std::array::from_fn(|k| ram.u8(at + PASSED + k as u32) != 0),
        passed_count: ram.u8(at + PASSED_COUNT),
        ends: if done == 0 && ends == [0] { Vec::new() } else { ends },
        best: ram.i32(at + BEST_LAP) as u32,
        finished: ram.flag(at + FINISHED),
        place: ram.u8(at + PLACE),
    }
}

fn write_laps(ram: &mut Ram, at: u32, laps: &Laps) {
    ram.set_i32(at + LAP_START, laps.start as i32);
    ram.set_u8(at + LAPS_DONE, laps.done);
    for (k, &p) in laps.passed.iter().enumerate() {
        ram.set_u8(at + PASSED + k as u32, p as u8);
    }
    ram.set_u8(at + PASSED_COUNT, laps.passed_count);
    for k in 0..8 {
        ram.set_i32(at + LAP_ENDS + 4 * k, laps.ends.get(k as usize).copied().unwrap_or(0) as i32);
    }
    ram.set_i32(at + BEST_LAP, laps.best as i32);
    ram.set_flag(at + FINISHED, laps.finished);
    ram.set_u8(at + PLACE, laps.place);
}
