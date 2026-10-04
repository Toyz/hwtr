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

/// 0x80021888: the model's scale (cvs +0x14, 4.12): twice under cheat 2 or
/// 32, a third under 4 (the small cars), else one.
pub fn model_scale(options: u32) -> i32 {
    if options & (2 | 32) != 0 {
        0x2000
    } else if options & 4 != 0 {
        crate::math::div_fx(0x1000, 0x3000)
    } else {
        0x1000
    }
}

/// Under cheat 2 or 4 the car is drawn at its model's scale: its body and
/// the wheels on it (0x80022274), its shadow (0x80029728) and its glows'
/// places (0x80022cd0).
pub fn body_scaled(options: u32) -> bool {
    options & (2 | 4) != 0
}

/// Under cheat 32 each wheel's own rotation is drawn at the model's scale
/// (0x8002269c).
pub fn wheels_scaled(options: u32) -> bool {
    options & 32 != 0
}

/// `m` times the diagonal `s` as the draws make it (the GTE's mvmva, sf=1,
/// lm=0): each entry times `s`, kept to a half word.
pub fn scale_columns(m: &Matrix, s: i32) -> Matrix {
    m.map(|row| row.map(|v| fx(v as i32, s).clamp(-0x8000, 0x7fff) as i16))
}

/// How far each car's shadow reaches (0x800be00c, by car id): out past its
/// rear right wheel's tyre, in from its half length, and along.
pub const SHADOW_TABLE: u32 = 0x800b_e00c;
pub const SHADOW_CARS: usize = 42;

/// The shadows' table, from the executable.
pub fn shadow_table(byte: &dyn Fn(u32) -> u8) -> Vec<[i8; 3]> {
    (0..SHADOW_CARS as u32).map(|k| std::array::from_fn(|i| byte(SHADOW_TABLE + 3 * k + i as u32) as i8)).collect()
}

/// 0x80029478: car's shadow on the ground it is near (`normal`, `d`, as the
/// car pose kept it), the car placed at `at` turned by `rot` (its model's
/// root): four quads, each a quarter of a box as wide as the rear right
/// wheel's mount plus an eighth of its tyre and `reach[0]`, as long as half
/// the car less `reach[1]` and at half its height below, moved by the
/// handling's origin (and `reach[2]` along), each corner dropped straight
/// down onto the ground. None on ground that faces less than 60 degrees up.
/// Under cheat 2 or 4 `rot` comes scaled by the model's scale
/// ([`scale_columns`]), which shrinks or grows the box with the car.
pub fn shadow_quads(
    h: &super::handling::Handling,
    reach: [i8; 3],
    at: crate::math::Vec3,
    rot: &Matrix,
    normal: crate::math::Vec3,
    d: i32,
) -> Option<[[crate::math::Vec3; 4]; 4]> {
    use crate::math::{Vec3, add, apply_matrix_lv, div_fx};
    let below = normal[2].wrapping_neg();
    if below >= -2047 {
        return None;
    }
    let x = h.mounts[3][0].wrapping_add(h.widths[3] >> 3).wrapping_add((reach[0] as i32) << 12);
    let y = (h.size[1] >> 1).wrapping_sub((reach[1] as i32) << 12);
    let z = (h.size[2] >> 1).wrapping_neg();
    let shift = [h.origin[0], h.origin[1].wrapping_add((reach[2] as i32) << 12), h.origin[2]];
    let corners: [Vec3; 4] =
        [[x, y, z], [x.wrapping_neg(), y, z], [x.wrapping_neg(), y.wrapping_neg(), z], [x, y.wrapping_neg(), z]];
    let ground = corners.map(|c| {
        let mut p = add(apply_matrix_lv(rot, add(c, shift)), at);
        let dist =
            fx(p[0], normal[0]).wrapping_add(fx(p[1], normal[1])).wrapping_add(fx(p[2], normal[2])).wrapping_add(d);
        p[2] = p[2].wrapping_add(div_fx(dist, below));
        p
    });
    let mid = |a: Vec3, b: Vec3| [0, 1, 2].map(|i| a[i].wrapping_add(b[i]) >> 1);
    let [p0, p1, p2, p3] = ground;
    let centre = mid(p1, p3);
    let (m01, m12, m23, m30) = (mid(p1, p0), mid(p1, p2), mid(p2, p3), mid(p0, p3));
    Some([[p0, m01, centre, m30], [m01, p1, m12, centre], [centre, m12, p2, m23], [m30, centre, m23, p3]])
}

/// Race slot `slot`'s shadow texture (0x80021b88, 0x80028b34): its CLUT
/// (row 482 on, at x 384) and its page, 4-bit, drawn subtracted (0x80029478
/// sets the blend to 2), as the race puts it.
pub fn shadow_texture(t: &Tables, slot: usize) -> (u16, u16) {
    let (x, y) = t.shadow_places.get(slot).copied().unwrap_or((384, 256));
    let clut = ((482 + slot as u16) << 6) | (384 >> 4);
    (clut, (x / 64) | ((y / 256) << 4) | (2 << 5))
}

/// 0x80028b34's shadow quads' texture corners: a quarter of the 64x64
/// image each, from the slot's first column `u`, the first two mirrored.
pub fn shadow_uv(u: u8) -> [[[u8; 2]; 4]; 4] {
    let (a, b, c, d) = (u, u.wrapping_add(31), u.wrapping_add(32), u.wrapping_add(63));
    let quad = |left: u8, right: u8, top: u8, bottom: u8| [[right, top], [left, top], [left, bottom], [right, bottom]];
    [quad(c, d, 0, 31), quad(a, b, 0, 31), quad(a, b, 32, 63), quad(c, d, 32, 63)]
}
