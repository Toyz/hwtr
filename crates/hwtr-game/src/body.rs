//! The rigid body: mass, inertia, position, rotation, momentum and the
//! force and torque summed over a step. Cars carry one (see
//! [`crate::car::Car::body`]); 0x8006c504 integrates it, for cars under full
//! physics (`car_update`), cars at state 1 (0x80040494) and two other
//! callers (0x8006b754, 0x8007c894).

use crate::math::{
    Matrix, Matrix64, Tables, Vec3, add, column, cross, div_fx, divdi3, dot, fx, mul_16_64, mul_64_16, sub, transpose,
};

/// The fastest a body moves, inches a second (about 131 mph).
pub const MAX_SPEED: i32 = 0x90_0000;

/// What [`Body::damp_spin`] keeps of the angular momentum: `1 - 25 × 8/4096`,
/// about 0.95.
const SPIN_KEPT: i32 = 0x1000 - fx(8, 0x1_9000);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Body {
    /// The inertia tensor in the body's axes, and its inverse.
    pub inertia: Matrix64,
    pub inv_inertia: Matrix64,
    pub mass: i32,
    pub inv_mass: i32,
    /// Gravity's strength (386, inches a second squared) and direction.
    pub gravity: i32,
    pub gravity_dir: Vec3,
    /// The centre of mass from the position, world axes (where drag acts on
    /// a car), and the word after it.
    pub centre: Vec3,
    pub centre_pad: i32,
    pub pos: Vec3,
    pub momentum: Vec3,
    pub vel: Vec3,
    /// The velocity's length.
    pub speed: i32,
    /// Body to world.
    pub rot: Matrix,
    pub ang_momentum: [i64; 3],
    /// The inverse inertia in world axes, `R I⁻¹ Rᵀ`.
    pub inv_inertia_world: Matrix64,
    /// Angular velocity, radians a second, and its length.
    pub spin: Vec3,
    pub spin_rate: i32,
    /// Force and torque summed over the step.
    pub force: Vec3,
    pub torque: [i64; 3],
    /// Non-zero: at rest and not integrated.
    pub asleep: u8,
    /// Steps the body has been still for; it falls asleep after enough.
    pub sleep_count: i32,
}

/// The 64-bit product the torque uses: both factors shifted up 8 as 64-bit
/// values, the product shifted down 20, so 4.12 by 4.12 to 4.12 without
/// losing the top bits.
fn wide(a: i32, b: i32) -> i64 {
    ((a as i64) << 8).wrapping_mul((b as i64) << 8) >> 20
}

/// A box's inertia about its centre, and the inverse, 64-bit as the game
/// keeps them (0x80071940): each moment `m (a² + b²) / 12`, the tensor
/// scaled up 8 bits and the inverse `12 · 2³² / (m (a² + b²))`.
fn box_inertia(mass: i32, [w, l, h]: Vec3) -> (Matrix64, Matrix64) {
    let sq = |x: i32| fx(x, x);
    let moments =
        [fx(mass, sq(l).wrapping_add(sq(h))), fx(mass, sq(w).wrapping_add(sq(h))), fx(mass, sq(w).wrapping_add(sq(l)))];
    let (mut inertia, mut inverse) = ([[0i64; 3]; 3], [[0i64; 3]; 3]);
    for (k, m) in moments.into_iter().enumerate() {
        inertia[k][k] = divdi3((m as i64) << 28, 12 << 20);
        inverse[k][k] = divdi3(3072 << 32, (m as i64) << 8);
    }
    (inertia, inverse)
}

impl Body {
    /// 0x8006bdd8: a box-shaped body of `mass` and `size` (width, length,
    /// height) at rest at the origin, unrotated, with its centre of mass at
    /// `centre` and gravity pulling down at 386 in/s².
    pub fn new(mass: i32, size: Vec3, centre: Vec3) -> Body {
        let (inertia, inv_inertia) = box_inertia(mass, size);
        let rot = [[0x1000, 0, 0], [0, 0x1000, 0], [0, 0, 0x1000]];
        Body {
            inertia,
            inv_inertia_world: mul_64_16(&mul_16_64(&rot, &inv_inertia), &transpose(&rot)),
            inv_inertia,
            mass,
            inv_mass: div_fx(0x1000, mass),
            gravity: 0x18_2000,
            gravity_dir: [0, 0, -0x1000],
            centre,
            rot,
            ..Body::default()
        }
    }

    /// Adds `force`, acting at the world point `at`, to the force sum, and
    /// its torque about the position to the torque sum.
    pub fn apply_force(&mut self, at: Vec3, force: Vec3) {
        let r = sub(at, self.pos);
        let torque = [
            wide(r[1], force[2]).wrapping_sub(wide(force[1], r[2])),
            wide(force[0], r[2]).wrapping_sub(wide(r[0], force[2])),
            wide(r[0], force[1]).wrapping_sub(wide(force[0], r[1])),
        ];
        for (sum, t) in self.torque.iter_mut().zip(torque) {
            *sum = sum.wrapping_add(t);
        }
        self.force = add(self.force, force);
    }

    /// 0x8003d5cc: slows the spin, a step's worth of damping in the air.
    pub fn damp_spin(&mut self) {
        self.ang_momentum = self.ang_momentum.map(|l| l.wrapping_mul(SPIN_KEPT as i64) >> 12);
    }

    /// 0x80071bc0: holds body axis `axis` (0, 1, anything else 2) to the
    /// unit direction `dir`. The angular momentum keeps only its part about
    /// `dir`; then, if the axis is already within 60 degrees of `dir`, the
    /// axis becomes `dir` and the other two lose their parts along it.
    pub fn align(&mut self, axis: u8, dir: Vec3) {
        let wide = |c: i32, x: i64| ((c as i64) << 8).wrapping_mul(x) >> 20;
        let along = (0..3).fold(0i64, |s, k| s.wrapping_add(wide(dir[k], self.ang_momentum[k])));
        self.ang_momentum = dir.map(|c| wide(c, along));
        let k = match axis {
            0 => 0,
            1 => 1,
            _ => 2,
        };
        let columns = [0, 1, 2].map(|j| column(&self.rot, j));
        if dot(dir, columns[k]) < 0x800 {
            return;
        }
        let mut columns = columns.map(|c| sub(c, dir.map(|x| fx(x, dot(dir, c)))));
        columns[k] = dir;
        for (j, c) in columns.iter().enumerate() {
            for (row, x) in self.rot.iter_mut().zip(c) {
                row[j] = *x as i16;
            }
        }
    }

    /// The angular velocity from the angular momentum, `I⁻¹ L` in world
    /// axes.
    fn spin_of_momentum(&self) -> Vec3 {
        let l = self.ang_momentum;
        self.inv_inertia_world.map(|row| {
            let s = (0..3).fold(0i64, |s, k| s.wrapping_add(row[k].wrapping_mul(l[k]) >> 20));
            (s >> 8) as i32
        })
    }

    /// 0x8006dc08: a collision impulse at the world point `at` against a
    /// surface with unit normal `n` (out of it), returning its size along
    /// the normal. Only a point moving into the surface is pushed; `bounce`
    /// is the share of its inward speed taken away (one and the restitution),
    /// and the impulse along the surface opposes its sliding, `friction`
    /// times the normal impulse.
    pub fn impulse(&mut self, t: &Tables, at: Vec3, n: Vec3, bounce: i32, friction: i32) -> i32 {
        let r = sub(at, self.pos);
        let vel = add(self.vel, cross(self.spin, r));
        let inward = dot(n, vel);
        if inward > 0 {
            return 0;
        }
        // How the point gives along n per unit impulse: 1/m + n·((I⁻¹(r×n))×r).
        let a = cross(r, n);
        let w = self.inv_inertia_world.map(|row| {
            (0..3).fold(0i64, |s, k| s.wrapping_add(row[k].wrapping_mul(a[k] as i64) >> 12))
        });
        let by = |x: i64, y: i32| x.wrapping_mul(y as i64) >> 12;
        let turn = [
            (by(w[1], r[2]).wrapping_sub(by(w[2], r[1])) >> 8) as i32,
            (by(w[2], r[0]).wrapping_sub(by(w[0], r[2])) >> 8) as i32,
            (by(w[0], r[1]).wrapping_sub(by(w[1], r[0])) >> 8) as i32,
        ];
        let give = self.inv_mass.wrapping_add(dot(n, turn));
        let size = div_fx(fx(bounce.wrapping_neg(), inward), give);
        let normal = n.map(|c| fx(c, size));
        let sliding = sub(vel, n.map(|c| fx(c, inward)));
        let along = if sliding.iter().all(|c| c.wrapping_abs() <= 0x1000) {
            [0; 3]
        } else {
            let k = fx(size.wrapping_neg(), friction);
            t.normalize(sliding).map(|c| fx(c, k))
        };
        let p = add(normal, along);
        self.momentum = add(p, self.momentum);
        let torque = cross(r, p);
        for (l, c) in self.ang_momentum.iter_mut().zip(torque) {
            *l = l.wrapping_add((c as i64) << 8);
        }
        self.vel = self.momentum.map(|c| fx(c, self.inv_mass));
        self.spin = self.spin_of_momentum();
        size
    }

    /// 0x8006c504: one step of `dt` seconds. Gravity joins the force sum;
    /// the momentum takes the force, and the velocity follows through the
    /// inverse mass, its length held to [`MAX_SPEED`]; the position moves.
    /// The angular momentum takes the torque, the world inverse inertia is
    /// turned to the current rotation, the angular velocity follows (held to
    /// 4π a second), and the rotation turns by it: `R += [ω dt]ₓ R`, entry by
    /// entry in 16 bits, with no renormalising here. Both sums are cleared.
    /// An asleep body is left alone.
    pub fn integrate(&mut self, t: &Tables, dt: i32) {
        if self.asleep != 0 {
            return;
        }
        let weight = fx(self.gravity, self.mass);
        self.force = add(self.force, self.gravity_dir.map(|c| fx(c, weight)));
        self.momentum = add(self.force.map(|c| fx(c, dt)), self.momentum);
        self.vel = self.momentum.map(|c| fx(c, self.inv_mass));
        self.speed = t.length(self.vel);
        if self.speed > MAX_SPEED {
            let k = div_fx(MAX_SPEED, self.speed);
            self.momentum = self.momentum.map(|c| fx(c, k));
            self.vel = self.vel.map(|c| fx(c, k));
            self.speed = MAX_SPEED;
        }
        self.pos = add(self.vel.map(|c| fx(c, dt)), self.pos);
        self.force = [0; 3];
        for (l, torque) in self.ang_momentum.iter_mut().zip(self.torque) {
            *l = l.wrapping_add(torque.wrapping_mul(dt as i64) >> 12);
        }
        let rot = self.rot;
        self.inv_inertia_world = mul_64_16(&mul_16_64(&rot, &self.inv_inertia), &transpose(&rot));
        self.spin = self.spin_of_momentum();
        self.spin_rate = t.length(self.spin);
        let max = fx(0x4000, 0x3244);
        if self.spin_rate > max {
            let k = div_fx(max, self.spin_rate);
            self.ang_momentum = self.ang_momentum.map(|l| l.wrapping_mul(k as i64) >> 12);
            self.spin = self.spin.map(|c| fx(c, k));
            self.spin_rate = max;
        }
        let d = self.spin.map(|c| ((c as i64) << 8).wrapping_mul(dt as i64) >> 12);
        let skew: Matrix64 =
            [[0, d[2].wrapping_neg(), d[1]], [d[2], 0, d[0].wrapping_neg()], [d[1].wrapping_neg(), d[0], 0]];
        let turn = mul_64_16(&skew, &rot);
        for i in 0..3 {
            for j in 0..3 {
                self.rot[i][j] = rot[i][j].wrapping_add((turn[i][j] >> 8) as i16);
            }
        }
        self.torque = [0; 3];
    }
}

/// Where the original keeps a body: offsets from its start, and the codec.
pub mod layout {
    use super::Body;
    use crate::ram::Ram;

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

    impl Body {
        pub fn read(ram: &Ram, b: u32) -> Body {
            let wide3 = |a: u32| [0, 1, 2].map(|k| ram.i64(a + 8 * k));
            Body {
                inertia: ram.matrix64(b + INERTIA),
                inv_inertia: ram.matrix64(b + INV_INERTIA),
                mass: ram.i32(b + MASS),
                inv_mass: ram.i32(b + INV_MASS),
                gravity: ram.i32(b + GRAVITY),
                gravity_dir: ram.vec3(b + GRAVITY_DIR),
                centre: ram.vec3(b + CENTRE),
                centre_pad: ram.i32(b + CENTRE + 12),
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
                asleep: ram.u8(b + ASLEEP),
                sleep_count: ram.i32(b + SLEEP_COUNT),
            }
        }

        pub fn write(&self, ram: &mut Ram, b: u32) {
            ram.set_matrix64(b + INERTIA, &self.inertia);
            ram.set_matrix64(b + INV_INERTIA, &self.inv_inertia);
            ram.set_i32(b + MASS, self.mass);
            ram.set_i32(b + INV_MASS, self.inv_mass);
            ram.set_i32(b + GRAVITY, self.gravity);
            ram.set_vec3(b + GRAVITY_DIR, self.gravity_dir);
            ram.set_vec3(b + CENTRE, self.centre);
            ram.set_i32(b + CENTRE + 12, self.centre_pad);
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
            ram.set_u8(b + ASLEEP, self.asleep);
            ram.set_i32(b + SLEEP_COUNT, self.sleep_count);
        }
    }
}
