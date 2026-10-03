//! The car object: one 0x930-byte record per car, in an array at
//! 0x80128fcc, with the count at 0x800d263c. `cars_update` (0x8004064c) walks
//! it each frame and dispatches on the state byte at +0x891.
//!
//! Offsets are named as their use is understood; see
//! `docs/engine/car-object.md`.

use crate::math::{Tables, apply_matrix_lv, div_fx, dot, fx};
use crate::ram::Ram;

pub const CARS: u32 = 0x8012_8fcc;
pub const CAR_SIZE: u32 = 0x930;
pub const CAR_COUNT: u32 = 0x800d_263c;

/// Steering angle, radians 4.12 (negative steers right).
pub const STEER: u32 = 0x10;
/// The body's up axis in world space (inferred from its use below).
pub const UP: u32 = 0xec;
/// World position.
pub const POS: u32 = 0x10c;
/// Linear velocity.
pub const VEL: u32 = 0x12c;
/// Body rotation, a libgte MATRIX (only its 3x3 part is read here).
pub const ROT: u32 = 0x140;
/// Angular velocity.
pub const SPIN: u32 = 0x1d0;
/// The wheels, `WHEEL_SIZE` bytes each.
pub const WHEELS: u32 = 0x218;
pub const WHEEL_SIZE: u32 = 0x88;
/// How many wheels the car has.
pub const WHEEL_COUNT: u32 = 0x548;
/// Wheels touching the ground, counted by `update_wheels`.
pub const GROUNDED: u32 = 0x54b;
/// Grounded wheels whose ground faces against the body's up axis.
pub const GROUNDED_AGAINST: u32 = 0x54c;
/// The point the wheel mounts are measured from (a centre of mass?).
pub const ORIGIN: u32 = 0x770;

/// Offsets within a wheel.
pub mod wheel {
    /// Mount point, body space.
    pub const MOUNT: u32 = 0x00;
    /// Bit 1: the wheel steers.
    pub const FLAGS: u32 = 0x18;
    /// Rolling direction, world space, unit 4.12.
    pub const HEADING: u32 = 0x1c;
    /// Mount point, world space.
    pub const WORLD: u32 = 0x2c;
    /// Non-zero when the wheel touches the ground.
    pub const ON_GROUND: u32 = 0x3c;
    /// Ground normal under the wheel.
    pub const NORMAL: u32 = 0x40;
    /// Contact point, world space.
    pub const CONTACT: u32 = 0x50;
    /// Velocity of the body at the contact point.
    pub const CONTACT_VEL: u32 = 0x60;
}

fn sub(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[0].wrapping_sub(b[0]), a[1].wrapping_sub(b[1]), a[2].wrapping_sub(b[2])]
}

fn add(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [a[0].wrapping_add(b[0]), a[1].wrapping_add(b[1]), a[2].wrapping_add(b[2])]
}

/// 0x80040a90: places the wheels for this frame. Each wheel's heading is the
/// body's forward axis, turned by the steering angle for wheels that steer
/// (the front pair one way, the rear the other) and, on the ground, laid
/// into the ground plane. Then its world position, and for a grounded wheel
/// the body's velocity at the contact point (v + ω × r), counting grounded
/// wheels as it goes.
///
/// The original also copies four bytes of uninitialised stack into each
/// heading's padding word (+0x28); the port leaves that word alone.
pub fn update_wheels(t: &Tables, ram: &mut Ram, car: u32) {
    let rot = ram.matrix(car + ROT);
    let pos = ram.vec3(car + POS);
    let column = |j: usize| rot.map(|row| row[j] as i32);
    let (side, forward) = (column(0), column(1));
    let angle = ram.i32(car + STEER).wrapping_neg();
    let (c, s) = (t.cos(angle), t.sin(angle));
    let a = forward.map(|x| fx(x, c));
    let b = side.map(|x| fx(x, s));
    let front = sub(a, b);
    let rear = add(a, b);
    ram.set_u8(car + GROUNDED, 0);
    ram.set_u8(car + GROUNDED_AGAINST, 0);
    for i in 0..ram.u8(car + WHEEL_COUNT) as u32 {
        let w = car + WHEELS + i * WHEEL_SIZE;
        let heading = match ram.u8(w + wheel::FLAGS) & 2 {
            0 => forward,
            _ if i < 2 => front,
            _ => rear,
        };
        ram.set_vec3(w + wheel::HEADING, heading);
        if ram.u8(w + wheel::ON_GROUND) != 0 {
            let n = ram.vec3(w + wheel::NORMAL);
            let d = dot(heading, n);
            let flat = sub(heading, n.map(|x| fx(x, d)));
            let len = t.length(flat);
            ram.set_vec3(w + wheel::HEADING, flat.map(|x| div_fx(x, len)));
        }
        let mount = sub(ram.vec3(w + wheel::MOUNT), ram.vec3(car + ORIGIN));
        ram.set_vec3(w + wheel::WORLD, add(apply_matrix_lv(&rot, mount), pos));
        if ram.u8(w + wheel::ON_GROUND) != 0 {
            let r = sub(ram.vec3(w + wheel::CONTACT), pos);
            let spin = ram.vec3(car + SPIN);
            let cross = [
                fx(spin[1], r[2]).wrapping_sub(fx(r[1], spin[2])),
                fx(r[0], spin[2]).wrapping_sub(fx(spin[0], r[2])),
                fx(spin[0], r[1]).wrapping_sub(fx(r[0], spin[1])),
            ];
            ram.set_vec3(w + wheel::CONTACT_VEL, add(ram.vec3(car + VEL), cross));
            ram.set_u8(car + GROUNDED, ram.u8(car + GROUNDED).wrapping_add(1));
            let n = ram.vec3(w + wheel::NORMAL);
            if dot(n, ram.vec3(car + UP)).wrapping_neg() >= 2049 {
                ram.set_u8(car + GROUNDED_AGAINST, ram.u8(car + GROUNDED_AGAINST).wrapping_add(1));
            }
        }
    }
}
