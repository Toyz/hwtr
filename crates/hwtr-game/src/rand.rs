//! The game's random numbers: PsyQ's `rand` (0x800a60c8), one generator for
//! everything, and `random(n)` (0x800145f0, interface slot 0x8012fd34).

/// The generator's state (0x80142388).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rand {
    pub seed: u32,
}

impl Rand {
    /// 0x800a60c8: the next number, 0 to 0x7fff.
    pub fn next(&mut self) -> u32 {
        self.seed = self.seed.wrapping_mul(0x41c6_4e6d).wrapping_add(0x3039);
        (self.seed >> 16) & 0x7fff
    }

    /// 0x800145f0: the next number below `n` (the remainder; for `n` of 0,
    /// the R3000A's `divu` leaves the number itself).
    pub fn below(&mut self, n: u32) -> u32 {
        let r = self.next();
        r.checked_rem(n).unwrap_or(r)
    }
}

/// Where the original keeps it.
pub mod layout {
    pub const SEED: u32 = 0x8014_2388;
}
