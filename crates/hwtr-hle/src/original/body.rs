//! The original's memory as body: where it keeps it, read into and written
//! from the port's types.

use hwtr_game::body::Body;
use super::{InMemory, Ram};

pub const INERTIA: u32 = 0x00;
pub const INV_INERTIA: u32 = 0x58;
pub const MASS: u32 = 0xb0;
pub const INV_MASS: u32 = 0xb4;
pub const GRAVITY: u32 = 0xb8;
pub const GRAVITY_DIR: u32 = 0xbc;
pub const CENTRE: u32 = 0xcc;
pub const POS: u32 = 0xdc;
pub const MOMENTUM: u32 = 0xec;
pub const VEL: u32 = 0xfc;
pub const SPEED: u32 = 0x10c;
/// A libgte MATRIX; its translation part is not used.
pub const ROT: u32 = 0x110;
pub const ANG_MOMENTUM: u32 = 0x130;
pub const INV_INERTIA_WORLD: u32 = 0x148;
pub const SPIN: u32 = 0x1a0;
pub const SPIN_RATE: u32 = 0x1b0;
pub const FORCE: u32 = 0x1b4;
pub const TORQUE: u32 = 0x1c8;
pub const ASLEEP: u32 = 0x1e0;
pub const SLEEP_COUNT: u32 = 0x1e4;

impl InMemory for Body {
    fn read(ram: &Ram, b: u32) -> Body {
        let wide3 = |a: u32| [0, 1, 2].map(|k| ram.i64(a + 8 * k));
        Body {
            inertia: ram.matrix64(b + INERTIA),
            inv_inertia: ram.matrix64(b + INV_INERTIA),
            mass: ram.i32(b + MASS),
            inv_mass: ram.i32(b + INV_MASS),
            gravity: ram.i32(b + GRAVITY),
            gravity_dir: ram.vec3(b + GRAVITY_DIR),
            centre: ram.vec3(b + CENTRE),
            pos: ram.vec3(b + POS),
            momentum: ram.vec3(b + MOMENTUM),
            vel: ram.vec3(b + VEL),
            speed: ram.i32(b + SPEED),
            rot: ram.matrix(b + ROT),
            ang_momentum: wide3(b + ANG_MOMENTUM),
            inv_inertia_world: ram.matrix64(b + INV_INERTIA_WORLD),
            spin: ram.vec3(b + SPIN),
            spin_rate: ram.i32(b + SPIN_RATE),
            force: ram.vec3(b + FORCE),
            torque: wide3(b + TORQUE),
            asleep: ram.flag(b + ASLEEP),
            sleep_count: ram.i32(b + SLEEP_COUNT),
        }
    }

    fn write(&self, ram: &mut Ram, b: u32) {
        ram.set_matrix64(b + INERTIA, &self.inertia);
        ram.set_matrix64(b + INV_INERTIA, &self.inv_inertia);
        ram.set_i32(b + MASS, self.mass);
        ram.set_i32(b + INV_MASS, self.inv_mass);
        ram.set_i32(b + GRAVITY, self.gravity);
        ram.set_vec3(b + GRAVITY_DIR, self.gravity_dir);
        ram.set_vec3(b + CENTRE, self.centre);
        ram.set_vec3(b + POS, self.pos);
        ram.set_vec3(b + MOMENTUM, self.momentum);
        ram.set_vec3(b + VEL, self.vel);
        ram.set_i32(b + SPEED, self.speed);
        ram.set_matrix(b + ROT, &self.rot);
        for k in 0..3 {
            ram.set_i64(b + ANG_MOMENTUM + 8 * k, self.ang_momentum[k as usize]);
            ram.set_i64(b + TORQUE + 8 * k, self.torque[k as usize]);
        }
        ram.set_matrix64(b + INV_INERTIA_WORLD, &self.inv_inertia_world);
        ram.set_vec3(b + SPIN, self.spin);
        ram.set_i32(b + SPIN_RATE, self.spin_rate);
        ram.set_vec3(b + FORCE, self.force);
        ram.set_flag(b + ASLEEP, self.asleep);
        ram.set_i32(b + SLEEP_COUNT, self.sleep_count);
    }
}
