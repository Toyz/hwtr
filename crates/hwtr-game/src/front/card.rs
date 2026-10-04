//! The game's memory card file, `BASLUS-00964HTWHEELS`, byte for byte: one
//! 8 KiB block, a 512-byte title frame (0x80024aec) and the save
//! (0x8006917c writes it, 0x80068ea0 reads it), so a file from a real card
//! or an emulator's works here and the other way round.
//!
//! The save is 2912 bytes:
//!
//! | offset | what |
//! |---|---|
//! | 0x000 | the card's id, a random number (0x8006981c) |
//! | 0x004 | 60 high scores, 12 tracks of 5 (16 bytes: a name of 12, a value) |
//! | 0x3c4 | 60 best times, the same |
//! | 0x784 | 15 cup winners, 3 cups of 5 |
//! | 0x874 | the settings, 8 bytes |
//! | 0x87c | the ids of the players last playing, one and two |
//! | 0x884 | how many players' records follow (up to 4) |
//! | 0x888 | the players' records, 180 bytes each |
//! | 0xb58 | the sum of every byte before it |
//!
//! A player's record: two words (the second its id), the name (12 bytes),
//! the unlocked cars and tracks, the car and track last chosen, progress,
//! six records, cheats, and the 29 words of its button mapping.

use super::{Profile, Settings};
use crate::pad::Mapping;

/// The file's name on the card.
pub const FILE_NAME: &str = "BASLUS-00964HTWHEELS";
/// The file: one block.
pub const FILE_SIZE: usize = 0x2000;
/// The title frame before the save.
pub const HEADER_SIZE: usize = 512;
/// The save, and what its checksum covers.
pub const SAVE_SIZE: usize = 2912;
const SUMMED: usize = 2904;

const SCORES: usize = 0x004;
const TIMES: usize = 0x3c4;
const CUPS: usize = 0x784;
const SETTINGS: usize = 0x874;
const PLAYER_IDS: usize = 0x87c;
const PLAYER_COUNT: usize = 0x884;
const PLAYERS: usize = 0x888;
const PLAYER_SIZE: usize = 180;
pub const MOST_PLAYERS: usize = 4;

/// A line of a score table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Score {
    pub name: String,
    pub value: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Save {
    pub card_id: u32,
    pub high_scores: Vec<Score>,
    pub best_times: Vec<Score>,
    pub cup_winners: Vec<Score>,
    pub settings: Settings,
    /// The ids of the players last playing.
    pub player_ids: [u32; 2],
    pub players: Vec<Profile>,
}

fn word(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn put(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn text(b: &[u8]) -> String {
    b.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect()
}

fn put_text(b: &mut [u8], s: &str) {
    for (k, c) in s.bytes().take(b.len().saturating_sub(1)).enumerate() {
        b[k] = c;
    }
}

impl Save {
    /// What 0x80088544 clears the tables to (the name "EMPTY", string 84),
    /// with these settings and no players.
    pub fn new(card_id: u32, empty: &str, settings: Settings) -> Save {
        let table = |n: usize| vec![Score { name: empty.to_string(), value: 0 }; n];
        Save {
            card_id,
            high_scores: table(60),
            best_times: table(60),
            cup_winners: table(15),
            settings,
            player_ids: [0; 2],
            players: Vec::new(),
        }
    }

    /// The 2912 bytes, checksum included.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = vec![0u8; SAVE_SIZE];
        put(&mut b, 0, self.card_id);
        for (at, table) in [(SCORES, &self.high_scores), (TIMES, &self.best_times), (CUPS, &self.cup_winners)] {
            for (k, s) in table.iter().enumerate() {
                let e = at + 16 * k;
                put_text(&mut b[e..e + 12], &s.name);
                put(&mut b, e + 12, s.value);
            }
        }
        let s = &self.settings;
        b[SETTINGS..SETTINGS + 8]
            .copy_from_slice(&[s.other[0], s.mode, s.volume, s.music, s.difficulty, s.other[1], s.effects, s.spare]);
        put(&mut b, PLAYER_IDS, self.player_ids[0]);
        put(&mut b, PLAYER_IDS + 4, self.player_ids[1]);
        let n = self.players.len().min(MOST_PLAYERS);
        b[PLAYER_COUNT] = n as u8;
        for (k, p) in self.players.iter().take(n).enumerate() {
            let r = PLAYERS + PLAYER_SIZE * k;
            put(&mut b, r, p.tag);
            put(&mut b, r + 4, p.id);
            put_text(&mut b[r + 8..r + 0x14], &p.name);
            put(&mut b, r + 0x14, p.cars[0]);
            put(&mut b, r + 0x18, p.cars[1]);
            put(&mut b, r + 0x1c, p.tracks);
            b[r + 0x20] = p.car;
            b[r + 0x21] = p.track;
            b[r + 0x22] = p.progress[0];
            b[r + 0x23] = p.progress[1];
            for (i, &v) in p.records.iter().enumerate() {
                put(&mut b, r + 0x24 + 4 * i, v);
            }
            put(&mut b, r + 0x3c, p.cheats);
            for (i, &m) in p.mapping.0.iter().enumerate() {
                put(&mut b, r + 0x40 + 4 * i, m);
            }
        }
        let sum = b[..SUMMED].iter().fold(0u32, |s, &c| s.wrapping_add(c as u32));
        put(&mut b, SUMMED, sum);
        b
    }

    /// The save in `b`, if its checksum holds (0x80068c64).
    pub fn from_bytes(b: &[u8]) -> Option<Save> {
        if b.len() < SAVE_SIZE {
            return None;
        }
        let sum = b[..SUMMED].iter().fold(0u32, |s, &c| s.wrapping_add(c as u32));
        if sum != word(b, SUMMED) {
            return None;
        }
        let table = |at: usize, n: usize| {
            (0..n).map(|k| Score { name: text(&b[at + 16 * k..at + 16 * k + 12]), value: word(b, at + 16 * k + 12) }).collect()
        };
        let s = &b[SETTINGS..SETTINGS + 8];
        let n = (b[PLAYER_COUNT] as usize).min(MOST_PLAYERS);
        let players = (0..n)
            .map(|k| {
                let r = PLAYERS + PLAYER_SIZE * k;
                Profile {
                    tag: word(b, r),
                    id: word(b, r + 4),
                    name: text(&b[r + 8..r + 0x14]),
                    cars: [word(b, r + 0x14), word(b, r + 0x18)],
                    tracks: word(b, r + 0x1c),
                    car: b[r + 0x20],
                    track: b[r + 0x21],
                    progress: [b[r + 0x22], b[r + 0x23]],
                    records: std::array::from_fn(|i| word(b, r + 0x24 + 4 * i)),
                    cheats: word(b, r + 0x3c),
                    mapping: Mapping(std::array::from_fn(|i| word(b, r + 0x40 + 4 * i))),
                }
            })
            .collect();
        Some(Save {
            card_id: word(b, 0),
            high_scores: table(SCORES, 60),
            best_times: table(TIMES, 60),
            cup_winners: table(CUPS, 15),
            settings: Settings {
                other: [s[0], s[5]],
                mode: s[1],
                volume: s[2],
                music: s[3],
                difficulty: s[4],
                effects: s[6],
                spare: s[7],
            },
            player_ids: [word(b, PLAYER_IDS), word(b, PLAYER_IDS + 4)],
            players,
        })
    }

    /// 0x800694f8: the record of the player last playing as `k`.
    pub fn player(&self, k: usize) -> Option<&Profile> {
        let id = *self.player_ids.get(k)?;
        self.players.iter().find(|p| p.id == id)
    }
}

/// The title frame (0x80024aec): "SC", three icon frames, one block, the
/// title in Shift-JIS, then the icons' palette and pictures (0x80025a0c,
/// from `MEM1.TIM` to `MEM3.TIM`, 16 by 16 at 4 bits; the palette is the
/// last one's).
pub fn header(byte: &dyn Fn(u32) -> u8, icons: [&[u8]; 3]) -> [u8; HEADER_SIZE] {
    let mut h = [0u8; HEADER_SIZE];
    h[..4].copy_from_slice(&[b'S', b'C', 0x13, 1]);
    let title = b"HOT WHEELS TURBO RACING         ";
    for (k, &c) in title.iter().enumerate() {
        h[4 + 2 * k] = if c == b' ' { 0x81 } else { 0x82 };
        h[5 + 2 * k] = sjis(byte, c) as u8;
    }
    for (k, tim) in icons.iter().enumerate() {
        // A 4-bit TIM: an 8-byte header, the palette block (12 bytes, then
        // 16 colours), the picture block (12 bytes, then 128).
        if let Some(px) = tim.get(64..192) {
            h[128 + 128 * k..256 + 128 * k].copy_from_slice(px);
        }
    }
    if let Some(clut) = icons[2].get(20..52) {
        h[96..128].copy_from_slice(clut);
    }
    h
}

/// 0x800249b8: an ASCII character's Shift-JIS code (of which the header
/// keeps the low byte): punctuation from the table at 0x800bdd78, digits and
/// letters from the ranges at 0x800bdd6c.
fn sjis(byte: &dyn Fn(u32) -> u8, c: u8) -> u16 {
    let half = |a: u32| u16::from_le_bytes([byte(a), byte(a + 1)]);
    let (table, range) = match c {
        32..=47 => (Some(1), 0),
        48..=57 => (None, 0),
        58..=64 => (Some(11), 0),
        65..=90 => (None, 1),
        91..=96 => (Some(37), 0),
        97..=122 => (None, 2),
        123..=126 => (Some(63), 0),
        _ => return 0,
    };
    match table {
        Some(t) => half(0x800b_dd78 + 2 * (c as u32 - (t + 31))),
        None => {
            let at = 0x800b_dd6c + 4 * range;
            half(at).wrapping_add(c as u16).wrapping_sub(half(at + 2))
        }
    }
}

/// The whole file: the title frame, the save, and the rest of the block
/// (the original copies on past the save from its buffer; zeros here).
pub fn file(header: &[u8; HEADER_SIZE], save: &Save) -> Vec<u8> {
    let mut f = vec![0u8; FILE_SIZE];
    f[..HEADER_SIZE].copy_from_slice(header);
    let s = save.to_bytes();
    f[HEADER_SIZE..HEADER_SIZE + s.len()].copy_from_slice(&s);
    f
}

/// The save in a card file, as 0x80068ea0 takes it from 512 bytes in.
pub fn read_file(f: &[u8]) -> Option<Save> {
    Save::from_bytes(f.get(HEADER_SIZE..)?)
}
