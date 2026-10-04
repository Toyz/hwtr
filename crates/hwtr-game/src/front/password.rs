//! The players' passwords (0x80069ae4 makes one, 0x80069d78 reads one):
//! twenty letters of the alphabet at 0x800bed14 carrying a player's cars,
//! tracks, cup progress and records, or one of the cheats.
//!
//! Eleven bytes are packed: 24 of the cars (a bit each, in the order at
//! 0x800becfc), the tracks' low 16 bits, and the six records' low seven
//! bits with the two progress bytes' bits on top (a fixed six bytes when
//! there is no progress). They go out five bits a letter, each letter the
//! running sum of the bits so far (mod 32), 18 letters; two more check the
//! sum of those, and six pairs are swapped (0x800bee54).

use super::Profile;

/// Letters.
pub const LENGTH: usize = 20;
const PACKED: usize = 11;
const CARRIED: usize = 18;

/// What a password reads into: a cheat sets these cars (as packed bits)
/// and tracks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cheat {
    pub word: String,
    pub cars: u32,
    pub tracks: u32,
}

/// The tables the passwords are made with, from the executable.
#[derive(Clone, Debug)]
pub struct Passwords {
    /// The car for each packed bit (0x800becfc; -1 ends it).
    order: [i8; 24],
    alphabet: [u8; 32],
    /// The pairs swapped, in order (0x800bee54).
    swaps: [(u8, u8); 6],
    /// The cheats (0x800bed34, 24 of 12 bytes: the word, cars, tracks).
    cheats: Vec<Cheat>,
}

/// The fixed bytes in place of the records before any cup progress.
const NO_PROGRESS: [u8; 6] = [19, 69, 37, 42, 11, 18];

/// The upper bits of a letter split across two bytes (0x800d0fb8), by
/// where it starts.
const SPILL: [u8; 8] = [0, 0, 0, 0, 1, 3, 7, 15];

impl Passwords {
    pub fn new(byte: &dyn Fn(u32) -> u8) -> Passwords {
        let word = |a: u32| u32::from_le_bytes(std::array::from_fn(|k| byte(a + k as u32)));
        let text = |mut a: u32| {
            let mut s = String::new();
            while byte(a) != 0 && s.len() < 32 {
                s.push(byte(a) as char);
                a += 1;
            }
            s
        };
        Passwords {
            order: std::array::from_fn(|k| byte(0x800b_ecfc + k as u32) as i8),
            alphabet: std::array::from_fn(|k| byte(0x800b_ed14 + k as u32)),
            swaps: std::array::from_fn(|k| (byte(0x800b_ee54 + 2 * k as u32), byte(0x800b_ee55 + 2 * k as u32))),
            cheats: (0..24)
                .map(|k| {
                    let e = 0x800b_ed34 + 12 * k;
                    Cheat { word: text(word(e)), cars: word(e + 4), tracks: word(e + 8) }
                })
                .collect(),
        }
    }

    /// 0x80069860: the cars `p` has, packed a bit each in the order's
    /// order.
    fn pack_cars(&self, p: &Profile) -> u32 {
        let mut bits = 0;
        for (k, &car) in self.order.iter().enumerate() {
            if car == -1 {
                break;
            }
            let has = if car < 32 { p.cars[0] >> car & 1 } else { p.cars[1] >> (car - 32) & 1 };
            if has != 0 {
                bits |= 1 << k;
            }
        }
        bits
    }

    /// 0x800698e8: the packed cars into `p`, added to those it has.
    fn unpack_cars(&self, p: &mut Profile, mut bits: u32) {
        for &car in &self.order {
            if bits == 0 {
                break;
            }
            if bits & 1 != 0 {
                if car < 32 {
                    p.cars[0] |= 1u32.wrapping_shl(car as u32);
                } else {
                    p.cars[1] |= 1 << (car - 32);
                }
            }
            bits >>= 1;
        }
    }

    /// The two letters checking the first eighteen.
    fn check(&self, letters: &[u8]) -> [u8; 2] {
        let sum = letters[..CARRIED].iter().fold(0u32, |s, &c| s + c as u32) as usize & 31;
        [self.alphabet[sum], self.alphabet[31 - sum]]
    }

    /// 0x80069ae4: the password for `p`.
    pub fn make(&self, p: &Profile) -> String {
        let cars = self.pack_cars(p);
        let mut b = [0u8; PACKED + 1];
        b[..3].copy_from_slice(&cars.to_le_bytes()[..3]);
        b[3..5].copy_from_slice(&p.tracks.to_le_bytes()[..2]);
        if p.progress[1] == 0 {
            b[5..11].copy_from_slice(&NO_PROGRESS);
        } else {
            let (p0, p1) = (p.progress[0], p.progress[1]);
            let top = [p0 & 1, p0 >> 1 & 1, p1 & 1, p1 >> 1 & 1, p1 >> 2 & 1, p1 >> 3 & 1];
            for k in 0..6 {
                b[5 + k] = (p.records[k] as u8 & 127) | top[k] << 7;
            }
        }
        let mut letters = [0u8; LENGTH];
        let (mut sum, mut at, mut bit) = (0u8, 0, 0);
        for letter in letters.iter_mut().take(CARRIED) {
            let v = if bit < 4 { b[at] >> bit & 31 } else { b[at] >> bit | (b[at + 1] & SPILL[bit]) << (8 - bit) };
            // 0x80069a60: the running sum.
            sum = ((v & 31) + sum) % 32;
            *letter = self.alphabet[sum as usize];
            bit += 5;
            if bit >= 8 {
                bit -= 8;
                at += 1;
            }
        }
        let [c0, c1] = self.check(&letters);
        letters[18] = c0;
        letters[19] = c1;
        // 0x80069960.
        for &(a, b) in &self.swaps {
            letters.swap(a as usize, b as usize);
        }
        letters.iter().map(|&c| c as char).collect()
    }

    /// 0x80069d78: `text` read into `p`, true if it is a password: a cheat
    /// adds its cars and tracks ("TWJM" a car of its own); otherwise its
    /// check must hold, and then its cars and tracks are added and its
    /// progress and records taken.
    pub fn read(&self, text: &str, p: &mut Profile) -> bool {
        let text: Vec<u8> = text.bytes().take(LENGTH).collect();
        if text == b"TWJM" {
            p.cars[1] |= 32;
            return true;
        }
        if let Some(c) = self.cheats.iter().find(|c| c.word.as_bytes() == text) {
            self.unpack_cars(p, c.cars);
            p.tracks |= c.tracks;
            return true;
        }
        let mut letters = [0u8; LENGTH];
        letters[..text.len()].copy_from_slice(&text);
        // 0x800699c0.
        for &(a, b) in self.swaps.iter().rev() {
            letters.swap(a as usize, b as usize);
        }
        if self.check(&letters) != [letters[18], letters[19]] {
            return false;
        }
        let mut b = [0u8; PACKED + 1];
        let (mut sum, mut at, mut bit) = (0u8, 0, 0);
        for &c in &letters[..CARRIED] {
            // 0x80069aa0: back from the running sum (a letter not in the
            // alphabet counts as 255).
            let k = self.alphabet.iter().position(|&a| a == c).map_or(255, |k| k as u8);
            let d = k as i32 - sum as i32;
            sum = k;
            let v = (if d < 0 { d + 32 } else { d }) as u8;
            if bit < 4 {
                b[at] |= (v & 31) << bit;
            } else {
                b[at] |= v << bit;
                b[at + 1] |= SPILL[bit] & v >> (8 - bit);
            }
            bit += 5;
            if bit >= 8 {
                bit -= 8;
                at += 1;
            }
        }
        self.unpack_cars(p, u32::from_le_bytes([b[0], b[1], b[2], 0]));
        p.tracks |= u16::from_le_bytes([b[3], b[4]]) as u32;
        for k in 0..6 {
            p.records[k] = (b[5 + k] & 127) as u32;
        }
        p.progress[0] = b[5] >> 7 | (b[6] >> 6 & 2);
        p.progress[1] = b[7] >> 7 | (b[8] >> 6 & 2) | (b[9] >> 5 & 4) | (b[10] >> 4 & 8);
        if p.progress[1] == 0 {
            p.records = [0; 6];
        }
        true
    }
}
