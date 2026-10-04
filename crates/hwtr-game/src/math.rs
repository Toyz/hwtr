//! The game's fixed-point trigonometry.
//!
//! Angles are radians in 4.12 fixed point (a full turn is 25736, 2π·4096);
//! results are 4.12 (4096 = 1). The values come from a quarter-wave table in
//! the executable, read once by [`Tables::from_exe`].

use hwtr_psx::Exe;

/// The quarter-wave cosine table at 0x800b5e04: cos(i / 4096) · 4096 for
/// i = 0..=6434.
pub const COS_TABLE: u32 = 0x800b_5e04;
pub const COS_ENTRIES: usize = 6435;

/// A full turn, π/2·4096·4 rounded as the game rounds it.
pub const TURN: i32 = 25736;

/// The surface friction table at 0x800beac0, one byte per surface kind.
pub const FRICTION_TABLE: u32 = 0x800b_eac0;
pub const FRICTION_ENTRIES: usize = 256;

/// The square-root table at 0x800b904c: 1024·√i for i = 0..4096.
pub const SQRT_TABLE: u32 = 0x800b_904c;
pub const SQRT_ENTRIES: usize = 4096;

pub struct Tables {
    pub cos: Vec<u16>,
    pub sqrt: Vec<u16>,
    /// Each ground surface's tyre friction, in percent (0x800beac0).
    pub surface_friction: Vec<u8>,
    /// What stunts are worth.
    pub stunts: crate::car::stunt::StuntTable,
    /// The race camera's views.
    pub views: crate::camera::Views,
    /// Each track's checkpoints a lap, by world (in [`crate::race::WORLDS`]
    /// order) and number (0x800c5c64, which the front end copies into the
    /// race it sets up).
    pub checkpoints: [[u8; 3]; 4],
    /// The HUD's turbo meter.
    pub meter: crate::hud::MeterTables,
    /// acos for cosines 0 to 1 (0x800bb04c), and libgte's sine and cosine
    /// of 4096ths of a turn (rcossin_tbl, 0x800c90f4).
    pub acos: Vec<u16>,
    pub rcossin: Vec<(i16, i16)>,
}

const CHECKPOINT_TABLE: u32 = 0x800c_5c64;
const ACOS_TABLE: u32 = 0x800b_b04c;
const RCOSSIN_TABLE: u32 = 0x800c_90f4;

impl Tables {
    pub fn from_exe(exe: &Exe) -> Tables {
        let m = exe.view();
        Tables::from_bytes(|a| m.u8(a).unwrap_or(0))
    }

    /// The tables from `byte`, which reads the executable at an address.
    pub fn from_bytes(byte: impl Fn(u32) -> u8) -> Tables {
        let u16_at = |a: u32| u16::from_le_bytes([byte(a), byte(a + 1)]);
        let cos = (0..COS_ENTRIES as u32).map(|i| u16_at(COS_TABLE + 2 * i)).collect();
        let sqrt = (0..SQRT_ENTRIES as u32).map(|i| u16_at(SQRT_TABLE + 2 * i)).collect();
        let surface_friction = (0..FRICTION_ENTRIES as u32).map(|i| byte(FRICTION_TABLE + i)).collect();
        let stunts = crate::car::stunt::StuntTable::read(&byte);
        let views = crate::camera::Views::read(&byte);
        let checkpoints = std::array::from_fn(|w| std::array::from_fn(|n| byte(CHECKPOINT_TABLE + 3 * w as u32 + n as u32)));
        let meter = crate::hud::MeterTables::read(&byte);
        let acos = (0..4097).map(|i| u16_at(ACOS_TABLE + 2 * i)).collect();
        let rcossin = (0..4096).map(|i| (u16_at(RCOSSIN_TABLE + 4 * i) as i16, u16_at(RCOSSIN_TABLE + 4 * i + 2) as i16)).collect();
        Tables { cos, sqrt, surface_friction, stunts, views, checkpoints, meter, acos, rcossin }
    }

    /// The checkpoints a lap of `world` (a name from [`crate::race::WORLDS`])
    /// track `number` (1 to 3) has; 0 for a track not in the table.
    pub fn checkpoints(&self, world: &str, number: u8) -> u8 {
        let w = crate::race::WORLDS.iter().position(|&n| n.eq_ignore_ascii_case(world));
        w.zip((number as usize).checked_sub(1)).and_then(|(w, n)| self.checkpoints[w].get(n).copied()).unwrap_or(0)
    }

    /// 0x80010bb4: acos of a cosine (4.12, -1 to 1), radians.
    pub fn acos(&self, c: i32) -> i32 {
        let v = self.acos.get(c.unsigned_abs() as usize).copied().unwrap_or(0) as i32;
        if c < 0 { 0x3244 - v } else { v }
    }

    /// libgte's RotMatrixZ-like turn (0x800a8308): the first two rows of
    /// `m` turned by `angle` 4096ths of a turn, as rcossin_tbl gives them.
    pub fn turn_rows(&self, angle: i32, m: &mut Matrix) {
        let (s, c) = self.rcossin[(angle.unsigned_abs() & 0xfff) as usize];
        let (s, c) = (if angle < 0 { -(s as i32) } else { s as i32 }, c as i32);
        let (r0, r1) = (m[0].map(|v| v as i32), m[1].map(|v| v as i32));
        for j in 0..3 {
            m[0][j] = (c.wrapping_mul(r0[j]).wrapping_sub(s.wrapping_mul(r1[j])) >> 12) as i16;
            m[1][j] = (s.wrapping_mul(r0[j]).wrapping_add(c.wrapping_mul(r1[j])) >> 12) as i16;
        }
    }

    /// cos(x), 0x80010afc.
    pub fn cos(&self, x: i32) -> i32 {
        // |x| mod a turn, the remainder as the original computes it (by a
        // reciprocal multiply, which agrees with % for every i32).
        let mut a = x.unsigned_abs() as i32;
        if a >= 25737 {
            a %= TURN;
        }
        // Fold into the first quadrant: cos is even about π and odd about π/2.
        if a >= 12869 {
            a = 25736 - a;
        }
        let mut negate = false;
        if a >= 6435 {
            negate = true;
            a = 12868 - a;
        }
        let v = self.cos[a as usize] as i32;
        if negate { -v } else { v }
    }

    /// The length of a vector, 0x80026650: √(x² + y² + z²) by the square-root
    /// table, with the sum of squares in 64 bits. A sum of 2⁶³ or more (as
    /// the wrapping sum's sign shows) gives 0x7fffffff.
    pub fn length(&self, v: [i32; 3]) -> i32 {
        let sq = |c: i32| (c as i64 * c as i64) as u64;
        let s = sq(v[0]).wrapping_add(sq(v[1])).wrapping_add(sq(v[2]));
        if s & (1 << 63) != 0 {
            return 0x7fff_ffff;
        }
        // The highest even n with 2^n <= s, found 32, 16, 8, 4, 2 at a time.
        let mut n = 0u32;
        let mut step = 32;
        while step >= 2 {
            if s >= 1u64 << (n + step) {
                n += step;
            }
            step >>= 1;
        }
        // s scaled to 2^10..2^12, then √ by the table and scaled back.
        let shift = n as i32 - 10;
        let index = if shift < 0 { (s << -shift) as u32 } else { (s >> shift) as u32 };
        let root = self.sqrt[index as usize] as u64;
        let back = (30 - n as i32) >> 1;
        (if back >= 0 { root >> back } else { root << -back }) as i32
    }

    /// sin(x) = cos(x - 6434), 0x80010adc.
    pub fn sin(&self, x: i32) -> i32 {
        self.cos(x.wrapping_sub(6434))
    }
}

/// A rotation (or any) matrix, 4.12, rows as the GTE takes them.
pub type Matrix = [[i16; 3]; 3];

/// A matrix of 64-bit entries, as the rigid-body code keeps inertia.
pub type Matrix64 = [[i64; 3]; 3];

/// `a · m`, 0x80026884: each product of a 64-bit entry and a 4.12 one taken
/// in 64 bits and shifted down 12, then summed.
pub fn mul_64_16(a: &Matrix64, m: &Matrix) -> Matrix64 {
    let mut out = [[0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, x) in row.iter_mut().enumerate() {
            *x = (0..3).fold(0i64, |s, k| s.wrapping_add(a[i][k].wrapping_mul(m[k][j] as i64) >> 12));
        }
    }
    out
}

/// `m · a`, 0x800273ec, the same way.
pub fn mul_16_64(m: &Matrix, a: &Matrix64) -> Matrix64 {
    let mut out = [[0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, x) in row.iter_mut().enumerate() {
            *x = (0..3).fold(0i64, |s, k| s.wrapping_add((m[i][k] as i64).wrapping_mul(a[k][j]) >> 12));
        }
    }
    out
}

/// Column `j` of `m`: for a rotation, 0 is the body's sideways axis, 1 its
/// forward axis and 2 its up axis, in world space.
pub fn column(m: &Matrix, j: usize) -> Vec3 {
    m.map(|row| row[j] as i32)
}

pub fn transpose(m: &Matrix) -> Matrix {
    let mut t = [[0; 3]; 3];
    for (i, row) in m.iter().enumerate() {
        for (j, x) in row.iter().enumerate() {
            t[j][i] = *x;
        }
    }
    t
}

/// MVMVA's sum for one row with no translation: Σ m·v in 64 bits, shifted by
/// `sf` and kept as the 32-bit MAC register keeps it.
fn mac_row(row: &[i16; 3], v: [i16; 3], sf: u32) -> i32 {
    let s: i64 = row.iter().zip(v).map(|(&m, x)| m as i64 * x as i64).sum();
    (s >> sf) as i32
}

/// libgte's ApplyMatrixLV, 0x800a2ba8: `m · v` for a 32-bit vector, done on
/// the GTE as `m · lo >> 12 + (m · hi) · 8` with each component split into
/// its magnitude's top bits (`|x| >> 15`) and bottom 15 bits, signs kept.
/// The top part passes through a 16-bit IR register, so for |x| >= 2^30 it
/// wraps, as on the console.
pub fn apply_matrix_lv(m: &Matrix, v: [i32; 3]) -> [i32; 3] {
    let split = |x: i32| -> (i16, i16) {
        let mag = x.unsigned_abs();
        let (hi, lo) = ((mag >> 15) as i32, (mag & 0x7fff) as i32);
        if x < 0 { ((-hi) as i16, (-lo) as i16) } else { (hi as i16, lo as i16) }
    };
    let parts = v.map(split);
    let hi = [parts[0].0, parts[1].0, parts[2].0];
    let lo = [parts[0].1, parts[1].1, parts[2].1];
    let mut out = [0i32; 3];
    for i in 0..3 {
        let h = mac_row(&m[i], hi, 0);
        // The high product, scaled by 8 through its magnitude.
        let h8 = if h < 0 { (h.wrapping_neg() << 3).wrapping_neg() } else { h << 3 };
        out[i] = mac_row(&m[i], lo, 12).wrapping_add(h8);
    }
    out
}

/// A vector, 4.12 or world units.
pub type Vec3 = [i32; 3];

pub fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0].wrapping_add(b[0]), a[1].wrapping_add(b[1]), a[2].wrapping_add(b[2])]
}

pub fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0].wrapping_sub(b[0]), a[1].wrapping_sub(b[1]), a[2].wrapping_sub(b[2])]
}

/// `a × b`, each product by `fx`.
pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        fx(a[1], b[2]).wrapping_sub(fx(a[2], b[1])),
        fx(a[2], b[0]).wrapping_sub(fx(a[0], b[2])),
        fx(a[0], b[1]).wrapping_sub(fx(a[1], b[0])),
    ]
}

/// GCC's fixed-point multiply as the game compiles it: the 64-bit product
/// shifted down 12 and kept to 32 bits (`mflo >> 12 | mfhi << 20`).
pub const fn fx(a: i32, b: i32) -> i32 {
    ((a as i64 * b as i64) >> 12) as i32
}

/// A 4.12 dot product, each term rounded down by `fx` before the sum.
pub fn dot(a: [i32; 3], b: [i32; 3]) -> i32 {
    fx(a[0], b[0]).wrapping_add(fx(a[1], b[1])).wrapping_add(fx(a[2], b[2]))
}

/// The R3000A's `div`: quotient and remainder, with what the hardware gives
/// for a zero divisor (quotient -1 or 1 by the dividend's sign, remainder the
/// dividend) and for `i32::MIN / -1`.
pub fn div(a: i32, b: i32) -> (i32, i32) {
    if b == 0 { (if a >= 0 { -1 } else { 1 }, a) } else { (a.wrapping_div(b), a.wrapping_rem(b)) }
}

/// `(a << 12) / b` without losing the top bits of `a`, as GCC inlines it at
/// many call sites: for a divisor within ±0x80000, the whole part and the
/// remainder are divided separately; past that, `a` is divided by `b >> 12`.
pub fn div_fx(a: i32, b: i32) -> i32 {
    if (b.wrapping_add(0x8_0000) as u32) <= 0x10_0000 {
        let (q, r) = div(a, b);
        (q << 12).wrapping_add(div(r << 12, b).0)
    } else {
        div(a, b >> 12).0
    }
}

impl Tables {
    /// `v` scaled to length 1, each component divided by the length with
    /// `div_fx`, as the game normalises inline.
    pub fn normalize(&self, v: [i32; 3]) -> [i32; 3] {
        let len = self.length(v);
        v.map(|c| div_fx(c, len))
    }

    /// 0x80025be4: the rotation made orthonormal again after a physics step,
    /// by Gram-Schmidt on its columns in order: the first normalised, the
    /// second less its projection on the first, the third less its
    /// projections on both (each taken from the third as it was), each
    /// normalised; entries kept to 16 bits.
    pub fn orthonormalize(&self, m: &Matrix) -> Matrix {
        let column = |j: usize| m.map(|row| row[j] as i32);
        let scale = |v: [i32; 3], k: i32| v.map(|c| fx(c, k));
        let sub = |a: [i32; 3], b: [i32; 3]| [0, 1, 2].map(|i| a[i].wrapping_sub(b[i]));
        let a = self.normalize(column(0));
        let c1 = column(1);
        let b = self.normalize(sub(c1, scale(a, dot(c1, a))));
        let c2 = column(2);
        let c = self.normalize(sub(sub(c2, scale(a, dot(c2, a))), scale(b, dot(c2, b))));
        let mut out = [[0; 3]; 3];
        for i in 0..3 {
            out[i] = [a[i] as i16, b[i] as i16, c[i] as i16];
        }
        out
    }
}

/// libgcc's `__divdi3` (0x800a9f78), 64-bit division truncated toward zero.
/// The game never divides by zero with it; that gives 0 here.
pub fn divdi3(a: i64, b: i64) -> i64 {
    a.checked_div(b).unwrap_or(0)
}

/// 0x8006abe4: the rotation of the unit quaternion (x, y, z, w), 4.12, each
/// doubled product taken as `fx(fx(2, a), b)`; entries kept to 16 bits.
pub fn quat_to_matrix([x, y, z, w]: [i32; 4]) -> Matrix {
    let two = |a: i32, b: i32| fx(fx(0x2000, a), b);
    let (zw, yw, xw) = (two(z, w), two(y, w), two(x, w));
    let (yx, zx, xx) = (two(y, x), two(z, x), two(x, x));
    let (zy, yy, zz) = (two(z, y), two(y, y), two(z, z));
    let one = 0x1000i32;
    let m = [
        [one.wrapping_sub(yy.wrapping_add(zz)), yx.wrapping_sub(zw), zx.wrapping_add(yw)],
        [yx.wrapping_add(zw), one.wrapping_sub(xx.wrapping_add(zz)), zy.wrapping_sub(xw)],
        [zx.wrapping_sub(yw), zy.wrapping_add(xw), one.wrapping_sub(xx.wrapping_add(yy))],
    ];
    m.map(|row| row.map(|v| v as i16))
}
