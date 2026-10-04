//! The front end's font, `SCRNFNT.OVL`, as the screens space it: each
//! glyph's width (0x8001dd4c) and the kerning between pairs of letters
//! (0x8007f8d4), whose adjustments the executable stores as percentages of
//! the first letter's width and scales once, when the font loads
//! (0x80087628).

use crate::hud::Font;

/// The letters the front end knows (0x800bfe64), in the order its width
/// and kerning tables use.
pub const ALPHABET: &[u8; 75] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789?!_.)( :/-'&=";

/// The kerning table (0x800befe0): one list of pairs per first letter.
const KERNING_TABLE: u32 = 0x800b_efe0;
/// Where the pair lists live: one block, 0x800bee94 to the table itself.
const PAIRS_FROM: u32 = 0x800b_ee94;
const PAIRS_TO: u32 = 0x800b_ef20;

/// One first letter's pairs: a run of `count` pairs from `at` in the
/// block, and whether its percentages have been turned into pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Kern {
    at: usize,
    count: u8,
    first: u8,
    scaled: bool,
}

#[derive(Clone, Debug)]
pub struct ScreenFont {
    /// Each glyph's width, by character code (upper case).
    widths: Vec<u16>,
    /// The first letters, in order. The binary search reads one entry past
    /// the stored count, as the original does: the table's own header,
    /// whose letter is 0.
    kerns: Vec<Kern>,
    /// Every pair as (second letter, adjustment).
    pairs: Vec<(u8, i8)>,
}

impl ScreenFont {
    /// The font's glyph widths and the executable's kerning, not yet scaled.
    pub fn new(font: &Font, byte: impl Fn(u32) -> u8) -> ScreenFont {
        let word = |a: u32| u32::from_le_bytes([byte(a), byte(a + 1), byte(a + 2), byte(a + 3)]);
        let list = word(KERNING_TABLE);
        let count = u16::from_le_bytes([byte(KERNING_TABLE + 4), byte(KERNING_TABLE + 5)]) as u32;
        let kerns = (0..=count)
            .map(|k| {
                let e = list + 8 * k;
                let at = word(e);
                Kern {
                    at: (at.wrapping_sub(PAIRS_FROM) / 2) as usize,
                    count: byte(e + 4),
                    first: byte(e + 5),
                    scaled: byte(e + 6) != 0,
                }
            })
            .collect();
        let pairs = (PAIRS_FROM..PAIRS_TO).step_by(2).map(|a| (byte(a), byte(a + 1) as i8)).collect();
        ScreenFont { widths: font.glyphs.iter().map(|g| g.w).collect(), kerns, pairs }
    }

    /// 0x80087628, run each time the font loads: every letter's list marked
    /// and scaled by its width, again if it was already (so a second load
    /// shrinks the kerning further, as on the console).
    pub fn prepare(&mut self) {
        for &c in ALPHABET {
            let w = self.width(c.to_ascii_uppercase());
            self.scale(c, w);
        }
    }

    /// 0x8001dd4c: a glyph's width; letters are drawn upper case.
    pub fn glyph_width(&self, c: u8) -> u16 {
        self.widths.get(c.to_ascii_uppercase() as usize).copied().unwrap_or(0)
    }

    /// 0x800875c0 by character: the width of the alphabet's letter at
    /// `c`'s place; a space is 5 wide. A character outside the alphabet
    /// reads the alphabet table's 0, so glyph 0's width.
    pub fn width(&self, c: u8) -> u8 {
        let c = index(c).map_or(0, |i| ALPHABET[i]);
        if c == b' ' { 5 } else { self.glyph_width(c) as u8 }
    }

    /// 0x800876ac: how far the pen moves from `a` to `b`: `a`'s width and
    /// the pair's kerning, or nothing if either is outside the alphabet.
    /// (The width is looked up by `a` upper-cased, the kerning by the
    /// letters as they are.)
    pub fn advance(&self, a: u8, b: u8) -> i32 {
        let (ia, ib) = (index(a.to_ascii_uppercase()), index(b.to_ascii_uppercase()));
        if ia.is_none() || ib.is_none() {
            return 0;
        }
        self.kerning(a, b) as i32 + self.width(a.to_ascii_uppercase()) as i32
    }

    /// The last letter's width (0x80087764), which ends a line.
    pub fn last(&self, c: u8) -> i32 {
        self.width(c) as i32
    }

    /// The first letter's entry: a binary search over the stored count and
    /// one more (0x8007f8d4's first loop).
    fn find(&self, first: u8) -> Option<usize> {
        let (mut lo, mut hi) = (0i32, self.kerns.len() as i32 - 1);
        while lo <= hi {
            let mid = (lo + hi) / 2;
            let key = self.kerns[mid as usize].first;
            if key == first {
                return Some(mid as usize);
            }
            if key < first { lo = mid + 1 } else { hi = mid - 1 }
        }
        None
    }

    /// 0x8007f8d4: the adjustment between `a` and `b`; nothing for a letter
    /// whose list was never scaled.
    pub fn kerning(&self, a: u8, b: u8) -> i8 {
        let Some(k) = self.find(a).map(|i| self.kerns[i]) else { return 0 };
        if !k.scaled {
            return 0;
        }
        let (mut lo, mut hi) = (0i32, k.count as i32);
        while lo <= hi {
            let mid = (lo + hi) / 2;
            let (key, adjust) = self.pairs.get(k.at + mid as usize).copied().unwrap_or((0, 0));
            if key == b {
                return adjust;
            }
            if key < b { lo = mid + 1 } else { hi = mid - 1 }
        }
        0
    }

    /// 0x8007fa84: marks `first`'s list scaled and turns its percentages of
    /// `width` into pixels, rounding toward zero.
    fn scale(&mut self, first: u8, width: u8) {
        let Some(i) = self.find(first) else { return };
        self.kerns[i].scaled = true;
        let k = self.kerns[i];
        for p in self.pairs.iter_mut().skip(k.at).take(k.count as usize) {
            p.1 = (p.1 as i32 * width as i32 / 100) as i8;
        }
    }
}

/// 0x8008754c: where `c` is in the alphabet.
pub fn index(c: u8) -> Option<usize> {
    ALPHABET.iter().position(|&a| a == c)
}
