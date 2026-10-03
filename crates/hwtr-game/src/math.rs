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

/// The square-root table at 0x800b904c: 1024·√i for i = 0..4096.
pub const SQRT_TABLE: u32 = 0x800b_904c;
pub const SQRT_ENTRIES: usize = 4096;

pub struct Tables {
    pub cos: Vec<u16>,
    pub sqrt: Vec<u16>,
}

impl Tables {
    pub fn from_exe(exe: &Exe) -> Tables {
        let m = exe.view();
        let cos = (0..COS_ENTRIES as u32).map(|i| m.u16(COS_TABLE + 2 * i).unwrap_or(0)).collect();
        let sqrt = (0..SQRT_ENTRIES as u32).map(|i| m.u16(SQRT_TABLE + 2 * i).unwrap_or(0)).collect();
        Tables { cos, sqrt }
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
