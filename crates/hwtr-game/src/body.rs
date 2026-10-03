//! The rigid body: mass, inertia, position, rotation, momentum and the
//! force and torque summed over a step. Cars carry one at +0x30 (see
//! [`crate::car::BODY`]); 0x8006c504 integrates it, for cars under full
//! physics (`car_update`), cars at state 1 (0x80040494) and two other
//! callers (0x8006b754, 0x8007c894).
//!
//! Offsets are from the body's start.

use crate::math::{Matrix64, Tables, div_fx, fx, mul_16_64, mul_64_16, transpose};
use crate::ram::Ram;

/// The inverse of the inertia tensor in the body's axes, 64-bit.
pub const INV_INERTIA: u32 = 0x58;
pub const MASS: u32 = 0xb0;
pub const INV_MASS: u32 = 0xb4;
/// Gravity's strength (386, inches a second squared) and direction.
pub const GRAVITY: u32 = 0xb8;
pub const GRAVITY_DIR: u32 = 0xbc;
pub const POS: u32 = 0xdc;
pub const MOMENTUM: u32 = 0xec;
pub const VEL: u32 = 0xfc;
/// The velocity's length.
pub const SPEED: u32 = 0x10c;
/// A libgte MATRIX: the rotation, body to world.
pub const ROT: u32 = 0x110;
/// Angular momentum, 64-bit.
pub const ANG_MOMENTUM: u32 = 0x130;
/// The inverse inertia in world axes, 64-bit, `R I⁻¹ Rᵀ`.
pub const INV_INERTIA_WORLD: u32 = 0x148;
/// Angular velocity, and its length.
pub const SPIN: u32 = 0x1a0;
pub const SPIN_RATE: u32 = 0x1b0;
/// Force summed over the step.
pub const FORCE: u32 = 0x1b4;
/// Torque summed over the step, 64-bit.
pub const TORQUE: u32 = 0x1c8;
/// Non-zero: the body is at rest and not integrated.
pub const ASLEEP: u32 = 0x1e0;

/// The fastest a body moves, inches a second (about 520 mph).
pub const MAX_SPEED: i32 = 0x90_0000;

fn add(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[0].wrapping_add(b[0]), a[1].wrapping_add(b[1]), a[2].wrapping_add(b[2])]
}

/// 0x8006c504: one step of `dt` seconds. Gravity joins the force sum; the
/// momentum takes the force, and the velocity follows through the inverse
/// mass, its length held to [`MAX_SPEED`]; the position moves. The angular
/// momentum takes the torque, the world inverse inertia is turned to the
/// current rotation, the angular velocity follows (held to 4π a second), and
/// the rotation turns by it: `R += [ω dt]ₓ R`, entry by entry in 16 bits,
/// with no renormalising here. Both sums are cleared. An asleep body is left
/// alone.
pub fn integrate(t: &Tables, ram: &mut Ram, b: u32, dt: i32) {
    if ram.u8(b + ASLEEP) != 0 {
        return;
    }
    let weight = fx(ram.i32(b + GRAVITY), ram.i32(b + MASS));
    let gravity = ram.vec3(b + GRAVITY_DIR).map(|c| fx(c, weight));
    ram.set_vec3(b + FORCE, add(ram.vec3(b + FORCE), gravity));
    let force = ram.vec3(b + FORCE);
    ram.set_vec3(b + MOMENTUM, add(force.map(|c| fx(c, dt)), ram.vec3(b + MOMENTUM)));
    let inv_mass = ram.i32(b + INV_MASS);
    ram.set_vec3(b + VEL, ram.vec3(b + MOMENTUM).map(|c| fx(c, inv_mass)));
    let speed = t.length(ram.vec3(b + VEL));
    ram.set_i32(b + SPEED, speed);
    if speed > MAX_SPEED {
        let k = div_fx(MAX_SPEED, speed);
        ram.set_vec3(b + MOMENTUM, ram.vec3(b + MOMENTUM).map(|c| fx(c, k)));
        ram.set_vec3(b + VEL, ram.vec3(b + VEL).map(|c| fx(c, k)));
        ram.set_i32(b + SPEED, MAX_SPEED);
    }
    ram.set_vec3(b + POS, add(ram.vec3(b + VEL).map(|c| fx(c, dt)), ram.vec3(b + POS)));
    ram.set_vec3(b + FORCE, [0; 3]);
    for k in 0..3 {
        let at = b + ANG_MOMENTUM + 8 * k;
        let torque = ram.i64(b + TORQUE + 8 * k);
        ram.set_i64(at, ram.i64(at).wrapping_add(torque.wrapping_mul(dt as i64) >> 12));
    }
    let rot = ram.matrix(b + ROT);
    let world = mul_64_16(&mul_16_64(&rot, &ram.matrix64(b + INV_INERTIA)), &transpose(&rot));
    ram.set_matrix64(b + INV_INERTIA_WORLD, &world);
    let momentum = [0, 1, 2].map(|k| ram.i64(b + ANG_MOMENTUM + 8 * k));
    let spin = world.map(|row| {
        let s = (0..3).fold(0i64, |s, k| s.wrapping_add(row[k].wrapping_mul(momentum[k]) >> 20));
        (s >> 8) as i32
    });
    ram.set_vec3(b + SPIN, spin);
    let rate = t.length(spin);
    ram.set_i32(b + SPIN_RATE, rate);
    let max = fx(0x4000, 0x3244);
    if rate > max {
        let k = div_fx(max, rate);
        for i in 0..3 {
            let at = b + ANG_MOMENTUM + 8 * i;
            ram.set_i64(at, ram.i64(at).wrapping_mul(k as i64) >> 12);
        }
        ram.set_vec3(b + SPIN, ram.vec3(b + SPIN).map(|c| fx(c, k)));
        ram.set_i32(b + SPIN_RATE, max);
    }
    let d = ram.vec3(b + SPIN).map(|c| ((c as i64) << 8).wrapping_mul(dt as i64) >> 12);
    let cross: Matrix64 =
        [[0, d[2].wrapping_neg(), d[1]], [d[2], 0, d[0].wrapping_neg()], [d[1].wrapping_neg(), d[0], 0]];
    let turn = mul_64_16(&cross, &rot);
    let mut next = rot;
    for i in 0..3 {
        for j in 0..3 {
            next[i][j] = rot[i][j].wrapping_add((turn[i][j] >> 8) as i16);
        }
    }
    ram.set_matrix(b + ROT, &next);
    for k in 0..3 {
        ram.set_i64(b + TORQUE + 8 * k, 0);
    }
}
