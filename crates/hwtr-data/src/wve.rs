//! EA's PlayStation movies (`EA_LOGO.WVE`, `ONLINE.WVE`), which the boot
//! executable (`SLUS_009.64`, its `playVAGmovie`) plays before the game.
//!
//! The file is chunks, each a 4-byte tag and a big-endian size counting
//! the 8-byte header:
//!
//! ```text
//! VLC0   the MDEC word each of the decoder's codes stands for (224 u16)
//! au00   sound: u32 BE first sample, u16 8, u16 2 (channels), then frames
//! au01   the last sound chunk, the same
//! MDEC   a picture: u16 BE width, u16 BE height, u32 BE number, then a
//!        PlayStation bitstream (v2) frame
//! ```
//!
//! The sound is PS-ADPCM without its flags byte: each 15-byte frame is the
//! shift and filter byte and 14 bytes of nibbles, 28 samples; a chunk's
//! first half of frames is the left channel and its second half the right
//! (left and right then correlate 0.78 on ONLINE.WVE's music, against 0.17
//! taken frame by frame, which also clips), 22050 Hz (the chunk's
//! first-sample counter steps by 28 for every 30 bytes). The pictures are 320 x 224 at 15 a
//! second (601 pictures to 40 s of sound in `ONLINE.WVE`).
//!
//! The bitstream: halfwords, little-endian, read high bit first; after a
//! header (halfword count, 0x3800, quantiser scale, version 2), 16 x 16
//! macroblocks top to bottom and then left to right, each six 8 x 8 blocks
//! (Cr, Cb, then the four Y), each a 10-bit DC and then codes of EA's own
//! meaning (see [`Codes`]; not the PlayStation's standard bitstream, which
//! ffmpeg reads and these frames are not), to an end word.

pub const RATE: u32 = 22050;
pub const FPS: u32 = 15;

/// A movie's chunks.
#[derive(Clone, Debug, Default)]
pub struct Wve<'a> {
    /// The `VLC0` chunk's 224 code values.
    pub values: Vec<u16>,
    pub pictures: Vec<Picture<'a>>,
    /// Each sound chunk's frames, after its 8-byte header.
    pub sound: Vec<&'a [u8]>,
}

#[derive(Clone, Copy, Debug)]
pub struct Picture<'a> {
    pub width: u16,
    pub height: u16,
    pub number: u32,
    /// The bitstream frame, from its header.
    pub data: &'a [u8],
}

impl<'a> Wve<'a> {
    pub fn parse(b: &'a [u8]) -> Option<Wve<'a>> {
        let mut out = Wve::default();
        let mut at = 0;
        while at + 8 <= b.len() {
            let tag = &b[at..at + 4];
            let size = u32::from_be_bytes(b[at + 4..at + 8].try_into().ok()?) as usize;
            let body = b.get(at + 8..at + size)?;
            match tag {
                b"MDEC" if body.len() >= 8 => out.pictures.push(Picture {
                    width: u16::from_be_bytes([body[0], body[1]]),
                    height: u16::from_be_bytes([body[2], body[3]]),
                    number: u32::from_be_bytes(body[4..8].try_into().ok()?),
                    data: &body[8..],
                }),
                b"au00" | b"au01" if body.len() >= 8 => out.sound.push(&body[8..]),
                b"VLC0" => out.values = body.chunks_exact(2).map(|h| u16::from_le_bytes([h[0], h[1]])).collect(),
                _ => {}
            }
            if size < 8 {
                return None;
            }
            at += size;
        }
        Some(out)
    }

    /// The sound, decoded: interleaved stereo at [`RATE`].
    pub fn sound(&self) -> Vec<i16> {
        const FILTERS: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];
        let mut out = Vec::new();
        let mut hist = [(0i32, 0i32); 2];
        for chunk in &self.sound {
            let frames: Vec<&[u8]> = chunk.chunks_exact(15).collect();
            let half = frames.len() / 2;
            let mut both = [Vec::with_capacity(half * 28), Vec::with_capacity(half * 28)];
            for (ch, side) in [&frames[..half], &frames[half..2 * half]].into_iter().enumerate() {
                for frame in side {
                    let shift = (frame[0] & 15).min(12) as u32;
                    let (f0, f1) = FILTERS[((frame[0] >> 4) as usize).min(4)];
                    let (mut s1, mut s2) = hist[ch];
                    for k in 0..28 {
                        let nibble = (frame[1 + k / 2] >> ((k & 1) * 4)) & 15;
                        let raw = (((nibble as i16) << 12) >> shift) as i32;
                        let s = (raw + ((s1 * f0 + s2 * f1 + 32) >> 6)).clamp(-0x8000, 0x7fff);
                        both[ch].push(s as i16);
                        s2 = s1;
                        s1 = s;
                    }
                    hist[ch] = (s1, s2);
                }
            }
            for (l, r) in both[0].iter().zip(&both[1]) {
                out.push(*l);
                out.push(*r);
            }
        }
        out
    }
}

/// The scan order of a block's coefficients.
const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57,
    50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// The MDEC's quantiser table (MPEG-1's intra matrix), in raster order.
const QUANT: [i32; 64] = [
    8, 16, 19, 22, 26, 27, 29, 34, 16, 16, 22, 24, 27, 29, 34, 37, 19, 22, 26, 27, 29, 34, 34, 38, 22, 22, 26, 27, 29, 34, 37, 40, 22, 26,
    27, 29, 32, 35, 40, 48, 26, 27, 29, 32, 35, 40, 48, 58, 26, 27, 29, 34, 38, 46, 56, 69, 27, 29, 35, 38, 46, 56, 69, 83,
];

/// The bitstream's bits: halfwords little-endian, each high bit first.
struct Bits<'a> {
    data: &'a [u8],
    at: usize,
}

impl Bits<'_> {
    fn half(&self, k: usize) -> u64 {
        self.data.get(2 * k..2 * k + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]) as u64)
    }

    /// The next `n` bits (up to 32).
    fn peek(&self, n: u32) -> u32 {
        let k = self.at / 16;
        let window = self.half(k) << 32 | self.half(k + 1) << 16 | self.half(k + 2);
        let skip = (self.at % 16) as u32;
        ((window << (16 + skip)) >> (64 - n)) as u32
    }

    fn take(&mut self, n: u32) -> u32 {
        let v = self.peek(n);
        self.at += n as usize;
        v
    }

    fn done(&self) -> bool {
        self.at >= self.data.len() * 8
    }
}

/// The movie's codes (0x80108cfc): the code shapes are fixed in the boot
/// executable (96 short ones at 0x8011b348, each a length and the code
/// left-aligned in 16 bits; 128 long ones at 0x8011b4c8, which follow eight
/// zero bits), and each movie's `VLC0` chunk gives what each stands for, an
/// MDEC word (run in the top 6 bits, a signed 10-bit level). Built as the
/// original does: a 13-bit lookup and a 9-bit one, each entry the code's
/// length and word.
#[derive(Clone)]
pub struct Codes {
    short: Vec<(u8, u16)>,
    long: Vec<(u8, u16)>,
}

/// Where the code shapes are in `SLUS_009.64` (loaded at 0x80100000 after
/// its 0x800-byte header).
pub const SHORT_CODES: u32 = 0x8011_b348;
pub const LONG_CODES: u32 = 0x8011_b4c8;

impl Codes {
    /// From the boot executable's bytes (`byte` by address) and a movie's
    /// `VLC0` values.
    pub fn new(byte: &dyn Fn(u32) -> u8, values: &[u16]) -> Option<Codes> {
        if values.len() < 96 + 128 {
            return None;
        }
        let shape = |at: u32| (byte(at), u16::from_le_bytes([byte(at + 2), byte(at + 3)]));
        let mut short = vec![(0u8, 0u16); 1 << 13];
        for k in 0..96 {
            let (len, code) = shape(SHORT_CODES + 4 * k as u32);
            if !(1..=13).contains(&len) {
                return None;
            }
            let first = ((code & 0xfff8) >> 3) as usize;
            for e in short.iter_mut().skip(first).take(1 << (13 - len)) {
                *e = (len, values[k]);
            }
        }
        let mut long = vec![(0u8, 0u16); 1 << 9];
        for k in 0..128 {
            let (len, code) = shape(LONG_CODES + 4 * k as u32);
            if !(1..=9).contains(&len) {
                return None;
            }
            let first = ((code & 0xfff8) >> 7) as usize;
            for e in long.iter_mut().skip(first).take(1 << (9 - len)) {
                *e = (len, values[96 + k]);
            }
        }
        Some(Codes { short, long })
    }
}

/// An MDEC word that escapes: the next 16 bits are the word itself.
const ESCAPE: u16 = 0x7c1f;
/// The end of a block.
const END: u16 = 0xfe00;

/// One block (0x801090ac's words, then the MDEC's dequantising as psx-spx
/// gives it): its coefficients in raster order, or None at the frame's end
/// (a DC of 0x1ff) or if the stream breaks.
fn block(bits: &mut Bits, codes: &Codes, qscale: i32) -> Option<[i32; 64]> {
    let mut c = [0i32; 64];
    let dc = bits.take(10);
    if dc == 0x1ff || bits.done() {
        return None;
    }
    let signed = |v: u32| ((v as i32) << 22) >> 22;
    c[0] = (signed(dc) * 2).clamp(-0x400, 0x3ff);
    let mut k = 0usize;
    loop {
        if bits.done() {
            return None;
        }
        let top = bits.peek(13) as usize;
        let (len, word) = if top < 32 {
            bits.take(8);
            codes.long[bits.peek(9) as usize]
        } else {
            codes.short[top]
        };
        if len == 0 {
            return None;
        }
        bits.take(len as u32);
        let word = if word == ESCAPE { bits.take(16) as u16 } else { word };
        if word == END {
            return Some(c);
        }
        k += (word >> 10) as usize + 1;
        let j = *ZIGZAG.get(k)?;
        let level = signed((word & 0x3ff) as u32);
        c[j] = ((level * QUANT[j] * qscale + 4) / 8).clamp(-0x400, 0x3ff);
    }
}

/// The inverse DCT of a block, signed (a DC of 8 is 1).
fn idct(c: &[i32; 64]) -> [f32; 64] {
    static COS: std::sync::OnceLock<[[f32; 8]; 8]> = std::sync::OnceLock::new();
    let cos = COS.get_or_init(|| {
        let mut cos = [[0f32; 8]; 8];
        for (x, row) in cos.iter_mut().enumerate() {
            for (u, v) in row.iter_mut().enumerate() {
                let k = if u == 0 { std::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
                *v = k * (((2 * x + 1) as f32 * u as f32 * std::f32::consts::PI) / 16.0).cos();
            }
        }
        cos
    });
    let mut tmp = [0f32; 64];
    for v in 0..8 {
        for x in 0..8 {
            tmp[v * 8 + x] = (0..8).map(|u| cos[x][u] * c[v * 8 + u] as f32).sum::<f32>() / 2.0;
        }
    }
    let mut out = [0f32; 64];
    for y in 0..8 {
        for x in 0..8 {
            out[y * 8 + x] = (0..8).map(|v| cos[y][v] * tmp[v * 8 + x]).sum::<f32>() / 2.0;
        }
    }
    out
}

/// A picture's pixels, RGB, `width` x `height` (full-range YCbCr as the
/// MDEC converts it). None if the bitstream breaks off.
pub fn decode(p: &Picture, codes: &Codes) -> Option<Vec<u8>> {
    let (w, h) = (p.width as usize, p.height as usize);
    if p.data.len() < 8 {
        return None;
    }
    let qscale = u16::from_le_bytes([p.data[4], p.data[5]]) as i32;
    let version = u16::from_le_bytes([p.data[6], p.data[7]]);
    if version != 2 {
        return None;
    }
    let mut bits = Bits { data: &p.data[8..], at: 0 };
    let mut rgb = vec![0u8; w * h * 3];
    for mx in 0..w.div_ceil(16) {
        for my in 0..h.div_ceil(16) {
            let cr = idct(&block(&mut bits, codes, qscale)?);
            let cb = idct(&block(&mut bits, codes, qscale)?);
            let ys: Vec<[f32; 64]> = (0..4).map(|_| block(&mut bits, codes, qscale).map(|b| idct(&b))).collect::<Option<_>>()?;
            for y in 0..16 {
                for x in 0..16 {
                    let (px, py) = (mx * 16 + x, my * 16 + y);
                    if px >= w || py >= h {
                        continue;
                    }
                    let luma = ys[(y / 8) * 2 + x / 8][(y % 8) * 8 + x % 8];
                    let c = (y / 2) * 8 + x / 2;
                    let (r_, b_) = (cr[c], cb[c]);
                    // The MDEC's: signed, clamped to -128..127, then 128 up.
                    let out = |v: f32| (v.round().clamp(-128.0, 127.0) + 128.0) as u8;
                    let o = (py * w + px) * 3;
                    rgb[o] = out(luma + 1.402 * r_);
                    rgb[o + 1] = out(luma - 0.3437 * b_ - 0.7143 * r_);
                    rgb[o + 2] = out(luma + 1.772 * b_);
                }
            }
        }
    }
    Some(rgb)
}

/// How far a picture's bitstream decodes: blocks done, the bit reached,
/// and the bits there are (for finding where a stream goes wrong).
pub fn decode_reach(p: &Picture, codes: &Codes) -> (usize, usize, usize) {
    let qscale = u16::from_le_bytes([p.data[4], p.data[5]]) as i32;
    let mut bits = Bits { data: &p.data[8..], at: 0 };
    let total = (p.width as usize).div_ceil(16) * (p.height as usize).div_ceil(16) * 6;
    for n in 0..total {
        let at = bits.at;
        if block(&mut bits, codes, qscale).is_none() {
            return (n, at, (p.data.len() - 8) * 8);
        }
    }
    (total, bits.at, (p.data.len() - 8) * 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movie(name: &str) -> Option<Vec<u8>> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work/fs").join(name);
        std::fs::read(path).ok()
    }

    /// Every picture of both movies decodes whole, and the sound is about
    /// as long as the pictures at 15 a second.
    #[test]
    fn the_movies_decode() {
        for name in ["EA_LOGO.WVE", "ONLINE.WVE"] {
            let Some(b) = movie(name) else {
                eprintln!("skipped: {name} not extracted");
                return;
            };
            let Some(slus) = movie("SLUS_009.64") else { return };
            let byte = |a: u32| a.checked_sub(0x8010_0000).and_then(|o| slus.get(o as usize + 0x800)).copied().unwrap_or(0);
            let w = Wve::parse(&b).expect("chunks");
            let codes = Codes::new(&byte, &w.values).expect("the codes");
            for p in &w.pictures {
                assert!(decode(p, &codes).is_some(), "{name} picture {} does not decode: {:?}", p.number, decode_reach(p, &codes));
            }
            let seconds = w.sound().len() as f32 / 2.0 / RATE as f32;
            let video = w.pictures.len() as f32 / FPS as f32;
            assert!((seconds - video).abs() < 1.0, "{name}: {seconds} s of sound to {video} s of pictures");
        }
    }
}
