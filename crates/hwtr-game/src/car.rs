//! The car object: one 0x930-byte record per car, in an array at
//! 0x80128fcc, with the count at 0x800d263c. `cars_update` (0x8004064c) walks
//! it each frame and dispatches on the state byte at +0x891.
//!
//! Offsets are named as their use is understood; see
//! `docs/engine/car-object.md`.

use crate::math::{Tables, apply_matrix_lv, div, div_fx, dot, fx};
use crate::ram::Ram;

pub const CARS: u32 = 0x8012_8fcc;
pub const CAR_SIZE: u32 = 0x930;
pub const CAR_COUNT: u32 = 0x800d_263c;

/// Flags; bits 7, 8 and 9 pick which pair of tuning percentages scales the
/// downforce.
pub const FLAGS: u32 = 0x4;
/// Steering angle, radians 4.12 (negative steers right).
pub const STEER: u32 = 0x10;
/// Accelerator, 0 to 1 (4.12).
pub const ACCEL: u32 = 0x14;
/// Brake, 0 to 1; also the throttle in reverse.
pub const BRAKE: u32 = 0x18;
/// The body's up axis in world space (inferred from its use below).
pub const UP: u32 = 0xec;
/// Where drag acts, relative to the position, world axes.
pub const DRAG_POINT: u32 = 0xfc;
/// World position, inches.
pub const POS: u32 = 0x10c;
/// Linear velocity, inches per second.
pub const VEL: u32 = 0x12c;
/// Speed, the length of the velocity.
pub const SPEED: u32 = 0x13c;
/// Body rotation, a libgte MATRIX (only its 3x3 part is read here).
pub const ROT: u32 = 0x140;
/// Angular velocity.
pub const SPIN: u32 = 0x1d0;
/// Force summed over the step, world axes.
pub const FORCE: u32 = 0x1e4;
/// Torque summed over the step, three 64-bit words.
pub const TORQUE: u32 = 0x1f8;
/// The wheels, `WHEEL_SIZE` bytes each.
pub const WHEELS: u32 = 0x218;
pub const WHEEL_SIZE: u32 = 0x88;
/// How many wheels the car has.
pub const WHEEL_COUNT: u32 = 0x548;
/// Wheels touching the ground, counted by `update_wheels`.
pub const GROUNDED: u32 = 0x54b;
/// Grounded wheels whose ground faces against the body's up axis.
pub const GROUNDED_AGAINST: u32 = 0x54c;
/// The engine and gearbox, see [`engine`].
pub const ENGINE: u32 = 0x550;
/// Drag coefficient.
pub const DRAG: u32 = 0x658;
/// Downforce: two coefficients and two factors, front and rear (by
/// inference: the two results go to the two axles?).
pub const DOWNFORCE_A: u32 = 0x65c;
pub const DOWNFORCE_B: u32 = 0x660;
pub const DOWNFORCE_A_SCALE: u32 = 0x664;
pub const DOWNFORCE_B_SCALE: u32 = 0x668;
/// The point the wheel mounts are measured from (a centre of mass?).
pub const ORIGIN: u32 = 0x770;
/// Frontal width and height, inches.
pub const WIDTH: u32 = 0x780;
pub const HEIGHT: u32 = 0x788;

/// A table of byte settings: +10 is a speed in mph added to the car's for
/// downforce while it is on the ground, and +33 to +40 are percentages that
/// scale the downforce (see `aero`).
pub const TUNING: u32 = 0x8013_6a18;

/// Offsets within a wheel.
pub mod wheel {
    /// Mount point, body space.
    pub const MOUNT: u32 = 0x00;
    /// Diameter, inches.
    pub const DIAMETER: u32 = 0x14;
    /// Bit 1: the wheel steers. Bit 2: the engine drives it.
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
    /// Non-zero when the wheel has lost grip (by inference: when no driven
    /// wheel has it clear, the engine revs freely).
    pub const SLIPPING: u32 = 0x78;
}

/// Offsets within the engine record at [`ENGINE`]. Speeds of rotation are
/// revolutions per minute, 4.12.
pub mod engine {
    pub const RPM: u32 = 0x00;
    /// The rpm over the overall ratio: the driven wheels' speed.
    pub const WHEEL_RPM: u32 = 0x04;
    /// Non-zero when the car rolls backwards along its forward axis.
    pub const REVERSE: u32 = 0x08;
    /// The gear in use, from 0.
    pub const GEAR: u32 = 0x09;
    /// The overall ratio in use (gear times final drive), negative in
    /// reverse.
    pub const RATIO: u32 = 0x0c;
    /// Force at the driven wheels, 64-bit.
    pub const DRIVE: u32 = 0x10;
    pub const REDLINE: u32 = 0x18;
    pub const IDLE: u32 = 0x1c;
    /// How many forward gears.
    pub const GEARS: u32 = 0x20;
    pub const FINAL_DRIVE: u32 = 0x24;
    /// The forward gears' ratios, one word each.
    pub const GEAR_RATIOS: u32 = 0x28;
    pub const REVERSE_RATIO: u32 = 0x40;
    /// Peak torque, foot-pounds.
    pub const PEAK_TORQUE: u32 = 0x44;
    /// The torque curve: 17 bytes, 255 for the peak, from idle to redline.
    pub const TORQUE_CURVE: u32 = 0x48;
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

/// Sea-level air density in slugs per cubic foot, times 1000, 4.12: the game
/// computes it as `0x949000 / 1000` and divides the product by 1000 again.
const AIR: i32 = 0x94_9000 / 1000;

/// The 64-bit product the torque uses: both factors shifted up 8 as 64-bit
/// values, the product shifted down 20, so 4.12 by 4.12 to 4.12 without
/// losing the top bits.
fn wide(a: i32, b: i32) -> i64 {
    ((a as i64) << 8).wrapping_mul((b as i64) << 8) >> 20
}

/// 0x80041af0: aerodynamics. Above a speed of 1, adds the drag force
/// `0.5 ρ v² Cd w h` (feet and seconds, from inches) against the velocity to
/// the force sum, and its torque about the position, acting at
/// `DRAG_POINT`, to the torque sum. Returns two downforces from the same
/// dynamic pressure, computed with a bonus speed while the car is on the
/// ground and scaled by tuning percentages chosen by the car's flags; at
/// speed 1 or less both are 0.
pub fn aero(ram: &mut Ram, car: u32) -> (i32, i32) {
    let speed = ram.i32(car + SPEED);
    if speed <= 4096 {
        return (0, 0);
    }
    let dir = ram.vec3(car + VEL).map(|v| div_fx(v, speed));
    let twelfth = div_fx(4096, 0xc000);
    let feet = fx(speed, twelfth);
    let pressure = |v: i32| div(fx(2048, fx(AIR, fx(v, v))), 1000).0;
    let q = pressure(feet);
    let area = fx(fx(ram.i32(car + WIDTH), twelfth), fx(ram.i32(car + HEIGHT), twelfth));
    let drag = fx(q, fx(area, ram.i32(car + DRAG)));
    let q = if ram.u8(car + GROUNDED) != 0 {
        let bonus = fx((ram.u8(TUNING + 10) as i32) << 12, div_fx(0xb_0000, 0xa000));
        pressure(feet.wrapping_add(fx(bonus, twelfth)))
    } else {
        q
    };
    let mut down_a = fx(q, fx(ram.i32(car + DOWNFORCE_A_SCALE), ram.i32(car + DOWNFORCE_A)));
    let mut down_b = fx(q, fx(ram.i32(car + DOWNFORCE_B_SCALE), ram.i32(car + DOWNFORCE_B)));
    let flags = ram.i32(car + FLAGS);
    if flags & 0x380 != 0 {
        let (a, b) = if flags & 0x80 != 0 {
            (33, 34)
        } else if flags & 0x100 != 0 {
            (36, 37)
        } else {
            (39, 40)
        };
        let percent = |k: u32| div_fx((ram.u8(TUNING + k) as i32) << 12, 0x6_4000);
        down_a = fx(down_a, percent(a));
        down_b = fx(down_b, percent(b));
    }
    let force = dir.map(|d| fx(d, drag.wrapping_neg()));
    let r = ram.vec3(car + DRAG_POINT);
    let torque = [
        wide(r[1], force[2]).wrapping_sub(wide(force[1], r[2])),
        wide(force[0], r[2]).wrapping_sub(wide(r[0], force[2])),
        wide(r[0], force[1]).wrapping_sub(wide(force[0], r[1])),
    ];
    for (k, t) in torque.into_iter().enumerate() {
        let at = car + TORQUE + 8 * k as u32;
        ram.set_i64(at, ram.i64(at).wrapping_add(t));
    }
    ram.set_vec3(car + FORCE, add(ram.vec3(car + FORCE), force));
    (down_a, down_b)
}

/// libgcc's `__divdi3` (0x800a9f78), 64-bit division truncated toward zero.
/// A zero divisor never reaches it from the car code (see `drivetrain`); it
/// gives 0 here.
fn divdi3(a: i64, b: i64) -> i64 {
    if b == 0 { 0 } else { a.wrapping_div(b) }
}

/// 0x80060138: the engine and gearbox. With driven wheels on the ground, the
/// road speed under them (their mean contact point's velocity, in the mean
/// ground plane, less its sideways part) turns them at `speed / (π d)`
/// revolutions a second; that through the overall ratio is the engine's rpm,
/// in the lowest forward gear that keeps it under the redline (or in
/// reverse when the car rolls backwards). When no driven wheel grips, the
/// engine revs to at least the throttle's share of the redline. The torque
/// curve at that rpm, through the ratio and the wheel radius, times the
/// throttle, is the drive force. Above the redline the rpm is held there and
/// there is no drive; in the air the engine revs with the throttle and
/// there is none either. The rpm never falls below idle.
pub fn drivetrain(t: &Tables, ram: &mut Ram, car: u32) {
    let e = car + ENGINE;
    let (mut contact, mut normal) = ([0i32; 3], [0i32; 3]);
    let (mut driven, mut gripping, mut diameter) = (0i32, 0, 0i32);
    for i in 0..ram.u8(car + WHEEL_COUNT) as u32 {
        let w = car + WHEELS + i * WHEEL_SIZE;
        if ram.u8(w + wheel::FLAGS) & 4 == 0 || ram.u8(w + wheel::ON_GROUND) == 0 {
            continue;
        }
        contact = add(contact, ram.vec3(w + wheel::CONTACT));
        normal = add(normal, ram.vec3(w + wheel::NORMAL));
        if driven == 0 {
            diameter = ram.i32(w + wheel::DIAMETER);
        }
        if ram.u8(w + wheel::SLIPPING) == 0 {
            gripping += 1;
        }
        driven += 1;
    }
    let throttle = |ram: &Ram| ram.i32(car + if ram.u8(e + engine::REVERSE) != 0 { BRAKE } else { ACCEL });
    if driven == 0 || diameter <= 0 {
        // In the air: the gear stays, and the engine revs with the throttle.
        let ratio = if ram.u8(e + engine::REVERSE) != 0 {
            ram.i32(e + engine::REVERSE_RATIO)
        } else {
            ram.i32(e + engine::GEAR_RATIOS + 4 * ram.u8(e + engine::GEAR) as u32)
        };
        ram.set_i32(e + engine::RATIO, fx(ram.i32(e + engine::FINAL_DRIVE), ratio));
        ram.set_i32(e + engine::RPM, fx(ram.i32(e + engine::REDLINE), throttle(ram)));
        ram.set_i64(e + engine::DRIVE, 0);
    } else {
        let mean = div_fx(4096, driven << 12);
        let contact = contact.map(|c| fx(c, mean));
        let normal = normal.map(|c| fx(c, mean));
        let len = t.length(normal);
        let normal = normal.map(|c| div_fx(c, len));
        let r = sub(contact, ram.vec3(car + POS));
        let spin = ram.vec3(car + SPIN);
        let cross = [
            fx(spin[1], r[2]).wrapping_sub(fx(r[1], spin[2])),
            fx(r[0], spin[2]).wrapping_sub(fx(spin[0], r[2])),
            fx(spin[0], r[1]).wrapping_sub(fx(r[0], spin[1])),
        ];
        let v = add(ram.vec3(car + VEL), cross);
        let v = sub(v, normal.map(|c| fx(c, dot(v, normal))));
        let rot = ram.matrix(car + ROT);
        let column = |j: usize| rot.map(|row| row[j] as i32);
        let side = column(0);
        let v = sub(v, side.map(|c| fx(c, dot(v, side))));
        let reverse = (dot(v, column(1)) as u32 >> 31) as u8;
        ram.set_u8(e + engine::REVERSE, reverse);
        let speed = t.length(v);
        // Revolutions a minute: speed over circumference (π is 0x3244), times 60.
        let wheel_rpm = fx(0x3_c000, div_fx(speed, fx(diameter, 0x3244)));
        let final_drive = ram.i32(e + engine::FINAL_DRIVE);
        if reverse != 0 {
            let ratio = fx(final_drive, ram.i32(e + engine::REVERSE_RATIO));
            ram.set_i32(e + engine::RPM, fx(ratio, wheel_rpm));
            ram.set_i32(e + engine::RATIO, ratio.wrapping_neg());
        } else {
            for g in 0..ram.i32(e + engine::GEARS) as u32 {
                ram.set_u8(e + engine::GEAR, g as u8);
                let ratio = fx(final_drive, ram.i32(e + engine::GEAR_RATIOS + 4 * g));
                ram.set_i32(e + engine::RATIO, ratio);
                let rpm = fx(ratio, wheel_rpm);
                ram.set_i32(e + engine::RPM, rpm);
                if rpm < ram.i32(e + engine::REDLINE) {
                    break;
                }
            }
        }
        if gripping == 0 {
            let free = fx(ram.i32(e + engine::REDLINE), throttle(ram));
            ram.set_i32(e + engine::RPM, ram.i32(e + engine::RPM).max(free));
        }
        let (rpm, redline, idle) = (ram.i32(e + engine::RPM), ram.i32(e + engine::REDLINE), ram.i32(e + engine::IDLE));
        if redline < rpm {
            ram.set_i64(e + engine::DRIVE, 0);
            ram.set_i32(e + engine::RPM, redline);
        } else {
            let along = div_fx(rpm.wrapping_sub(idle), redline.wrapping_sub(idle));
            let k = (fx(along, 0x1_0000) >> 12).clamp(0, 16) as u32;
            let curve = div((ram.u8(e + engine::TORQUE_CURVE + k) as i32) << 12, 255).0;
            // Foot-pounds to inch-pounds.
            let torque = fx(fx(curve, ram.i32(e + engine::PEAK_TORQUE)), 0xc000);
            let at_axle = ((torque as i64) << 8).wrapping_mul(ram.i32(e + engine::RATIO) as i64) >> 12;
            let force = divdi3(at_axle.wrapping_shl(20), (fx(diameter, 2048) as i64) << 8);
            ram.set_i64(e + engine::DRIVE, force.wrapping_mul(throttle(ram) as i64) >> 12);
        }
    }
    let rpm = ram.i32(e + engine::RPM);
    ram.set_i32(e + engine::WHEEL_RPM, div_fx(rpm, ram.i32(e + engine::RATIO)));
    ram.set_i32(e + engine::RPM, rpm.max(ram.i32(e + engine::IDLE)));
}
