//! The sound library's key-on (libsnd's `SsUtKeyOnV`, 0x800a67c4): from a
//! bank's program and tone and a note, the SPU voice's settings: its volume
//! left and right, its pitch, where its sample starts, its envelope.
//!
//! The volume is the voice's (the louder of its left and right), times the
//! bank's master volume, the program's and the tone's, each over 127; then
//! panned three times, by the tone, the program and the voice's own balance
//! (0x800ae7e0). The pitch is the note against the tone's centre note and
//! fine tuning, by table (0x800ae6b4). The envelope is the tone's, its
//! release made slower by a global (0x800adf0c).

/// A sound bank as libsnd keeps it: each program's tones in their 16-tone
/// block, the bank's master volume, and where each sample sits in SPU RAM.
#[derive(Clone, Debug, Default)]
pub struct Bank {
    pub master_volume: u8,
    /// By program number: its volume, pan, and the block its tones are in
    /// (libsnd keeps that in the record's byte 8).
    pub programs: Vec<Option<Program>>,
    /// 16 tones a block.
    pub tones: Vec<Option<Tone>>,
    /// Each sample's start in SPU RAM, in 8-byte units, by sample number.
    pub samples: Vec<u16>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Program {
    pub volume: u8,
    pub pan: u8,
    pub block: u8,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Tone {
    pub mode: u8,
    pub volume: u8,
    pub pan: u8,
    pub centre: u8,
    pub shift: u8,
    pub adsr1: u16,
    pub adsr2: u16,
    pub sample: u16,
    /// How far a full pitch bend goes down and up, in semitones.
    pub bend_down: u8,
    pub bend_up: u8,
}

/// A voice's settings, as libsnd writes them for the SPU.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Voice {
    pub volume: [u16; 2],
    pub pitch: u16,
    pub start: u16,
    pub adsr1: u16,
    pub adsr2: u16,
    /// The tone's mode bit 2: the voice goes to the reverb.
    pub reverb: bool,
}

impl Bank {
    /// From a VH: the programs (each with its tone block, in the order they
    /// are present), their tones, and the samples placed one after another
    /// from `base` (bytes into SPU RAM).
    pub fn from_vh(vh: &[u8], base: u32) -> Option<Bank> {
        let u16_at = |a: usize| Some(u16::from_le_bytes(vh.get(a..a + 2)?.try_into().ok()?));
        if vh.get(..4)? != b"pBAV" {
            return None;
        }
        let used = u16_at(18)? as usize;
        let vags = u16_at(22)? as usize;
        let tones_at = 32 + 128 * 16;
        let sizes_at = tones_at + used * 16 * 32;
        let mut programs = vec![None; 128];
        let mut block = 0u8;
        for (p, slot) in programs.iter_mut().enumerate() {
            let r = vh.get(32 + p * 16..32 + p * 16 + 16)?;
            if r[0] == 0 {
                continue;
            }
            *slot = Some(Program { volume: r[1], pan: r[4], block });
            block += 1;
        }
        let tones = (0..used * 16)
            .map(|k| {
                let r = vh.get(tones_at + 32 * k..tones_at + 32 * k + 32)?;
                Some(Tone {
                    mode: r[1],
                    volume: r[2],
                    pan: r[3],
                    centre: r[4],
                    shift: r[5],
                    adsr1: u16::from_le_bytes([r[16], r[17]]),
                    adsr2: u16::from_le_bytes([r[18], r[19]]),
                    sample: u16::from_le_bytes([r[22], r[23]]),
                    bend_down: r[12],
                    bend_up: r[13],
                })
            })
            .collect();
        let mut samples = vec![0u16];
        let mut at = base;
        for n in 1..=vags {
            samples.push((at / 8) as u16);
            at += u16_at(sizes_at + 2 * n)? as u32 * 8;
        }
        Some(Bank { master_volume: *vh.get(24)?, programs, tones, samples })
    }
}

/// The note tables at 0x800c8f54 (12 semitones) and 0x800c8f6c (128 steps
/// of a semitone).
#[derive(Clone, Debug)]
pub struct Tables {
    pub semitones: [u16; 12],
    pub fine: [u16; 128],
}

impl Tables {
    pub fn read(byte: &dyn Fn(u32) -> u8) -> Tables {
        let half = |a: u32| u16::from_le_bytes([byte(a), byte(a + 1)]);
        Tables {
            semitones: std::array::from_fn(|k| half(0x800c_8f54 + 2 * k as u32)),
            fine: std::array::from_fn(|k| half(0x800c_8f6c + 2 * k as u32)),
        }
    }

    /// 0x800ae6b4: the SPU pitch (0x1000 the sample's own rate) of `note`
    /// and `fine` (128ths of a semitone) for a tone recorded at `centre`
    /// and `shift`.
    pub fn pitch(&self, note: i32, fine: i32, centre: u8, shift: u8) -> u16 {
        let i16_ = |v: i32| v as i16 as i32;
        let half = |v: i32| if v < 0 { v + 127 } else { v } >> 7;
        let f = i16_(shift as i32 + fine);
        let up = half(f);
        let mut n = note + up - centre as i32;
        let mut f = f - (up << 7);
        if i16_(f) < 0 {
            f += 128;
            n = n - 1 + half(i16_(f));
        }
        let n = i16_(n);
        let octave = n / 12;
        let (semitone, octave) = match n - octave * 12 {
            s if i16_(s) < 0 => (s + 12, octave - 3),
            s => (s, octave - 2),
        };
        let at = |t: &[u16], k: i32| t.get(i16_(k) as usize).copied().unwrap_or(0) as i32;
        let p = (at(&self.semitones, semitone) * at(&self.fine, f)) >> 16;
        let octave = i16_(octave);
        if octave >= 0 {
            return 0x3fff;
        }
        let k = (-octave) as u32;
        ((p as u32).wrapping_add(1 << (k - 1)) >> k) as u16
    }
}

/// libsnd's pitch bend (0x800ac6a8 with 0x800ae64c): the pitch of a voice
/// keyed at `note` from `program`'s `tone`, bent by `bend` (64 none, each
/// step a 63rd of the tone's bend range up, a 64th of it down). None if
/// the program or tone is not there.
pub fn bend_pitch(bank: &Bank, tables: &Tables, program: usize, tone: usize, note: i32, bend: i32) -> Option<u16> {
    let p = (*bank.programs.get(program)?)?;
    let t = (*bank.tones.get(((p.block as usize) << 4) + tone)?)?;
    let step = (bend - 64) as i16 as i32;
    let (note, fine) = if step > 0 {
        let x = step * t.bend_up as i32;
        (note + x / 63, (x % 63) << 1)
    } else if step < 0 {
        let x = step * t.bend_down as i32;
        let q = (if x < 0 { x + 63 } else { x }) >> 6;
        (note + q - 1, ((x - (q << 6)) << 1) + 127)
    } else {
        (note, 0)
    };
    Some(tables.pitch(note as u16 as i16 as i32, fine as u16 as i16 as i32, t.centre, t.shift))
}

/// `SsUtKeyOnV` (0x800a67c4) with what it calls: voice `voice`'s settings
/// for program `program`, tone `tone` of `bank` at `note` and `fine`, at
/// `left` and `right` (0 to 127). `mono` is libsnd's mono switch
/// (0x801427bc); `release` slows every release (0x80142800). None if the
/// program or tone is not there.
#[allow(clippy::too_many_arguments)]
pub fn key_on(
    bank: &Bank,
    tables: &Tables,
    program: usize,
    tone: usize,
    note: i32,
    fine: i32,
    left: i32,
    right: i32,
    mono: bool,
    release: u16,
) -> Option<Voice> {
    let p = (*bank.programs.get(program)?)?;
    let t = (*bank.tones.get(((p.block as usize) << 4) + tone)?)?;
    // The voice's own volume and balance from left and right.
    let (volume, balance) = if left == right {
        (left, 64)
    } else if right < left {
        (left, (right << 6) / left)
    } else {
        (right, 127 - (left << 6) / right)
    };
    let master = bank.master_volume as i32;
    let v = volume * ((master << 14) - master) / 16129;
    let v = ((v * p.volume as i8 as i32 * t.volume as i8 as i32) as u32 / 16129) as i32;
    let (mut l, mut r) = (v as u32, v as u32);
    for pan in [t.pan, p.pan, balance as u8] {
        if pan < 64 {
            r = r * pan as u32 / 63;
        } else {
            l = l * (127 - pan as u32) / 63;
        }
    }
    if mono {
        let m = l.max(r);
        l = m;
        r = m;
    }
    let release_rate = ((release as u32 + (t.adsr2 & 31) as u32) as u16 as i16).min(31) as u16;
    let start = bank.samples.get(t.sample as usize).copied()?;
    Some(Voice {
        volume: [l as u16, r as u16],
        pitch: tables.pitch(note, fine, t.centre, t.shift),
        start,
        adsr1: t.adsr1,
        adsr2: (t.adsr2 & 0xffe0) | release_rate,
        reverb: t.mode & 4 != 0,
    })
}
