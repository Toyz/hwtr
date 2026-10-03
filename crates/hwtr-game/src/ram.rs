//! Main RAM as the original lays it out.
//!
//! While the race is being ported, the port works on the original's data
//! structures in place, so a ported function can stand in for the original
//! one inside the running game (and be checked there, frame by frame). The
//! layouts become Rust types once every function touching them is ported.

use crate::math::Matrix;

/// The 2 MB of main RAM, addressed as the game addresses it (KSEG0 and its
/// mirrors all land in the same bytes).
pub struct Ram<'a>(pub &'a mut [u8]);

fn at(a: u32) -> usize {
    (a & 0x1f_ffff) as usize
}

impl Ram<'_> {
    pub fn u8(&self, a: u32) -> u8 {
        self.0[at(a)]
    }

    pub fn i16(&self, a: u32) -> i16 {
        i16::from_le_bytes([self.0[at(a)], self.0[at(a + 1)]])
    }

    pub fn i32(&self, a: u32) -> i32 {
        let i = at(a);
        i32::from_le_bytes(self.0[i..i + 4].try_into().unwrap())
    }

    /// A 64-bit word, stored low word first.
    pub fn i64(&self, a: u32) -> i64 {
        (self.i32(a) as u32 as i64) | ((self.i32(a + 4) as i64) << 32)
    }

    pub fn set_i64(&mut self, a: u32, v: i64) {
        self.set_i32(a, v as i32);
        self.set_i32(a + 4, (v >> 32) as i32);
    }

    pub fn set_u8(&mut self, a: u32, v: u8) {
        self.0[at(a)] = v;
    }

    pub fn set_i32(&mut self, a: u32, v: i32) {
        let i = at(a);
        self.0[i..i + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// The first three words of a libgte `VECTOR` (the fourth is padding).
    pub fn vec3(&self, a: u32) -> [i32; 3] {
        [self.i32(a), self.i32(a + 4), self.i32(a + 8)]
    }

    pub fn set_vec3(&mut self, a: u32, v: [i32; 3]) {
        for (k, x) in v.into_iter().enumerate() {
            self.set_i32(a + 4 * k as u32, x);
        }
    }

    /// The 3x3 part of a libgte `MATRIX`, row-major.
    pub fn matrix(&self, a: u32) -> Matrix {
        let mut m = [[0; 3]; 3];
        for (i, row) in m.iter_mut().enumerate() {
            for (j, x) in row.iter_mut().enumerate() {
                *x = self.i16(a + 2 * (3 * i + j) as u32);
            }
        }
        m
    }
}
