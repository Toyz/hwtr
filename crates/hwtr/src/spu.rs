//! The PlayStation's sound chip's voices, as the port needs them: 24
//! voices, each playing PS-ADPCM at a pitch (0x1000 is 44.1 kHz), through
//! an ADSR envelope, at a volume left and right, honouring the blocks' loop
//! flags. Unlike the console, there is no 512 KiB of sample memory to share:
//! each bank's samples are their own buffer, every bank stays loaded, and a
//! voice plays from a bank at an offset. It plays through rrt's audio
//! output; the game sets voices and keys them on between ticks.
//!
//! The sample format and the chip's 4-point Gaussian interpolation are
//! rrt's (`rrt::emu::spu`); the voices, envelopes and mixing are here.
//! There is no reverb.

use std::sync::Arc;

use rrt::audio::Source;
use rrt::emu::spu::{self as adpcm, History, PER_FRAME};

const VOICES: usize = 24;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Off,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Debug, Default)]
struct Voice {
    /// The bank's samples it plays from.
    data: Arc<[u8]>,
    // Settings.
    volume: [u16; 2],
    pitch: u16,
    start: u32,
    adsr1: u16,
    adsr2: u16,
    // Playback.
    addr: u32,
    repeat: u32,
    counter: u32,
    /// The block's samples after the previous block's last three, which
    /// the interpolation reaches back to.
    samples: [i16; 3 + PER_FRAME],
    history: History,
    flags: u8,
    phase: Phase,
    level: i32,
    wait: u32,
}

#[derive(Default)]
pub struct Spu {
    voices: [Voice; VOICES],
    stream: Stream,
    cd: Cd,
}

/// The CD's music: a track's PCM (interleaved stereo, 44.1 kHz), where in
/// it the drive is, whether it plays, and its volume (CdMix, 128 full).
#[derive(Default)]
struct Cd {
    pcm: Arc<[i16]>,
    at: usize,
    playing: bool,
    volume: i32,
}

/// A stream of sound played beside the voices (a movie's), interleaved
/// stereo at its own rate, and how far in it is (in 44.1 kHz frames).
#[derive(Default)]
struct Stream {
    pcm: Vec<i16>,
    rate: u32,
    at: u64,
}

/// One envelope step's settings: how often (samples) and by how much.
fn envelope(level: i32, increase: bool, exponential: bool, shift: u32, step: u32) -> (u32, i32) {
    let step = if increase { 7 - step as i32 } else { -8 + step as i32 };
    let mut cycles = 1u32 << shift.saturating_sub(11);
    let mut adjust = step << 11u32.saturating_sub(shift);
    if exponential && increase && level > 0x6000 {
        cycles *= 4;
    }
    if exponential && !increase {
        adjust = (adjust * level) >> 15;
    }
    (cycles, adjust)
}

impl Voice {
    /// Decodes the block at `addr` into `samples`; past the bank's end the
    /// voice stops.
    fn decode(&mut self) {
        let at = self.addr as usize;
        let Some(b) = self.data.get(at..at + 16) else {
            self.phase = Phase::Off;
            return;
        };
        self.flags = b[1];
        if self.flags & adpcm::LOOP_START != 0 {
            self.repeat = self.addr;
        }
        self.samples.copy_within(PER_FRAME.., 0);
        let mut out = [0; PER_FRAME];
        adpcm::decode_frame(b, &mut self.history, &mut out);
        self.samples[3..].copy_from_slice(&out);
    }

    /// Past the block: on to the next, or round its loop, or done.
    fn advance(&mut self) {
        if self.flags & adpcm::END != 0 {
            self.addr = self.repeat;
            if self.flags & adpcm::REPEAT == 0 {
                self.phase = Phase::Off;
                self.level = 0;
                return;
            }
        } else {
            self.addr += 16;
        }
        self.decode();
    }

    fn envelope_tick(&mut self) {
        let (increase, exponential, shift, step) = match self.phase {
            Phase::Off => return,
            Phase::Attack => (true, self.adsr1 & 0x8000 != 0, ((self.adsr1 >> 10) & 31) as u32, ((self.adsr1 >> 8) & 3) as u32),
            Phase::Decay => (false, true, ((self.adsr1 >> 4) & 15) as u32, 0),
            Phase::Sustain => {
                (self.adsr2 & 0x4000 == 0, self.adsr2 & 0x8000 != 0, ((self.adsr2 >> 8) & 31) as u32, ((self.adsr2 >> 6) & 3) as u32)
            }
            Phase::Release => (false, self.adsr2 & 0x20 != 0, (self.adsr2 & 31) as u32, 0),
        };
        if self.wait > 0 {
            self.wait -= 1;
            return;
        }
        let (cycles, adjust) = envelope(self.level, increase, exponential, shift, step);
        self.wait = cycles - 1;
        self.level = (self.level + adjust).clamp(0, 0x7fff);
        match self.phase {
            Phase::Attack if self.level >= 0x7fff => self.phase = Phase::Decay,
            Phase::Decay => {
                let sustain = (((self.adsr1 & 15) as i32) + 1) * 0x800;
                if self.level <= sustain {
                    self.phase = Phase::Sustain;
                }
            }
            Phase::Release if self.level == 0 => self.phase = Phase::Off,
            _ => {}
        }
    }

    /// One output sample, left and right.
    fn sample(&mut self) -> (i32, i32) {
        if self.phase == Phase::Off {
            return (0, 0);
        }
        let k = ((self.counter >> 12) as usize).min(PER_FRAME - 1);
        let w = &self.samples[k..k + 4];
        let s = adpcm::interpolate([w[0], w[1], w[2], w[3]], (self.counter >> 4) as u8);
        let s = (s * self.level) >> 15;
        self.envelope_tick();
        self.counter += self.pitch.min(0x3fff) as u32;
        while self.counter >> 12 >= PER_FRAME as u32 {
            self.counter -= (PER_FRAME as u32) << 12;
            self.advance();
            if self.phase == Phase::Off {
                break;
            }
        }
        let vol = |v: u16| if v & 0x8000 == 0 { ((v & 0x7fff) as i32) * 2 } else { 0x7fff };
        ((s * vol(self.volume[0])) >> 15, (s * vol(self.volume[1])) >> 15)
    }
}

impl Spu {
    /// Voice `k`'s settings, then its key on, playing from `bank`'s samples
    /// (the start counted from the bank's own beginning).
    pub fn key_on(&mut self, k: usize, bank: &Arc<[u8]>, v: &hwtr_game::snd::Voice) {
        let Some(voice) = self.voices.get_mut(k) else { return };
        *voice = Voice {
            data: Arc::clone(bank),
            volume: v.volume,
            pitch: v.pitch,
            start: v.start as u32 * 8,
            adsr1: v.adsr1,
            adsr2: v.adsr2,
            ..Voice::default()
        };
        voice.addr = voice.start;
        voice.repeat = voice.start;
        voice.phase = Phase::Attack;
        voice.decode();
    }

    /// Voice `k` let go: into its release.
    pub fn key_off(&mut self, k: usize) {
        if let Some(v) = self.voices.get_mut(k)
            && v.phase != Phase::Off
        {
            v.phase = Phase::Release;
            v.wait = 0;
        }
    }
}

impl Spu {
    /// `pcm` (interleaved stereo at `rate`) played from the start, over any
    /// stream before it.
    pub fn play_stream(&mut self, pcm: Vec<i16>, rate: u32) {
        self.stream = Stream { pcm, rate, at: 0 };
    }

    pub fn stop_stream(&mut self) {
        self.stream = Stream::default();
    }

    /// A CD track from its start, round and round.
    pub fn cd_play(&mut self, pcm: Arc<[i16]>) {
        self.cd.pcm = pcm;
        self.cd.at = 0;
        self.cd.playing = true;
    }

    /// Stopped and back to the start, or held where it is.
    pub fn cd_stop(&mut self) {
        self.cd.playing = false;
        self.cd.at = 0;
    }

    pub fn cd_pause(&mut self) {
        self.cd.playing = false;
    }

    pub fn cd_resume(&mut self) {
        self.cd.playing = !self.cd.pcm.is_empty();
    }

    /// The CD's volume, 0 to 128.
    pub fn cd_volume(&mut self, v: i32) {
        self.cd.volume = v;
    }

    /// How far the stream has played, in milliseconds.
    pub fn stream_ms(&self) -> u64 {
        self.stream.at * 1000 / 44100
    }

    /// Voice `k`'s volume left and right, as libsnd's `SsUtSetVVol` leaves
    /// it.
    pub fn set_volume(&mut self, k: usize, volume: [u16; 2]) {
        if let Some(v) = self.voices.get_mut(k) {
            v.volume = volume;
        }
    }

    /// Voice `k`'s pitch (0x1000 the sample's own rate).
    pub fn set_pitch(&mut self, k: usize, pitch: u16) {
        if let Some(v) = self.voices.get_mut(k) {
            v.pitch = pitch;
        }
    }

    /// Voice `k`'s pitch.
    #[cfg(test)]
    pub fn pitch(&self, k: usize) -> u16 {
        self.voices.get(k).map_or(0, |v| v.pitch)
    }

    /// Whether voice `k` is sounding.
    pub fn active(&self, k: usize) -> bool {
        self.voices.get(k).is_some_and(|v| v.phase != Phase::Off)
    }
}

/// A bank of effects: the bank, the note tables, and each effect's tone,
/// program and note.
pub struct Effects {
    pub bank: hwtr_game::snd::Bank,
    pub samples: Arc<[u8]>,
    pub notes: hwtr_game::snd::Tables,
    table: Vec<[u32; 3]>,
    /// libsnd's mono switch (the Audio Mode option, 0x80014c40).
    pub mono: bool,
}

impl Effects {
    /// 0x8001a73c: the effects table from the executable's (0x800bd5f8, 61
    /// records of tone, program, note, id), by id, for `bank`.
    pub fn new(byte: &dyn Fn(u32) -> u8, bank: hwtr_game::snd::Bank, samples: &[u8]) -> Effects {
        let word = |a: u32| u32::from_le_bytes([byte(a), byte(a + 1), byte(a + 2), byte(a + 3)]);
        let table = (0..61u32)
            .map(|id| {
                (0..61u32)
                    .map(|k| 0x800b_d5f8 + 16 * k)
                    .find(|&r| word(r + 12) == id)
                    .map_or([0; 3], |r| [word(r), word(r + 4), word(r + 8)])
            })
            .collect();
        Effects { bank, samples: samples.into(), notes: hwtr_game::snd::Tables::read(byte), table, mono: false }
    }

    /// Effect `id`'s tone, program and note.
    pub fn record(&self, id: u8) -> [u32; 3] {
        self.table.get(id as usize).copied().unwrap_or_default()
    }

    /// Effect `id` at `volume` (0 to 127) as a voice's settings.
    pub fn voice(&self, id: u8, volume: i32) -> Option<hwtr_game::snd::Voice> {
        let [tone, program, note] = self.table.get(id as usize).copied().unwrap_or_default();
        hwtr_game::snd::key_on(&self.bank, &self.notes, program as usize, tone as usize, note as i16 as i32, 0, volume, volume, self.mono, 0)
    }
}

impl Source for Spu {
    fn rate(&self) -> u32 {
        44100
    }

    fn render(&mut self, out: &mut [i16]) {
        for frame in out.chunks_exact_mut(2) {
            let (mut l, mut r) = (0i32, 0i32);
            for v in self.voices.iter_mut() {
                let (a, b) = v.sample();
                l += a;
                r += b;
            }
            let cd = &mut self.cd;
            if cd.playing && cd.pcm.len() >= 2 {
                if 2 * cd.at + 1 >= cd.pcm.len() {
                    cd.at = 0;
                }
                l += cd.pcm[2 * cd.at] as i32 * cd.volume / 128;
                r += cd.pcm[2 * cd.at + 1] as i32 * cd.volume / 128;
                cd.at += 1;
            }
            let st = &mut self.stream;
            if st.rate > 0 {
                // Linear between the stream's frames.
                let pos = st.at * st.rate as u64;
                let (k, frac) = ((pos / 44100) as usize, (pos % 44100) as i32);
                let at = |i: usize, c: usize| st.pcm.get(2 * i + c).copied().unwrap_or(0) as i32;
                if 2 * k < st.pcm.len() {
                    l += at(k, 0) + (at(k + 1, 0) - at(k, 0)) * frac / 44100;
                    r += at(k, 1) + (at(k + 1, 1) - at(k, 1)) * frac / 44100;
                    st.at += 1;
                }
            }
            frame[0] = l.clamp(-0x8000, 0x7fff) as i16;
            frame[1] = r.clamp(-0x8000, 0x7fff) as i16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The menu's Cross sound (effect 50) out of `HWMENU`: it sounds, and
    /// it ends. Written to `target/test-tmp/cross.wav` to listen to.
    #[test]
    fn a_menu_sound_plays() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work");
        let (Ok(vh), Ok(vb), Ok(exe)) = (
            std::fs::read(dir.join("big/SCREENSBIG/HWMENUVH")),
            std::fs::read(dir.join("big/SCREENSBIG/HWMENUVB")),
            std::fs::read(dir.join("fs/CCCPSX.EXE")),
        ) else {
            eprintln!("skipped: the disc is not extracted");
            return;
        };
        let byte = |a: u32| a.checked_sub(0x8001_0000).and_then(|o| exe.get(o as usize + 0x800)).copied().unwrap_or(0);
        let word = |a: u32| u32::from_le_bytes([byte(a), byte(a + 1), byte(a + 2), byte(a + 3)]);
        let record = (0..61u32).map(|k| 0x800b_d5f8 + 16 * k).find(|&r| word(r + 12) == 50).expect("effect 50");
        let bank = hwtr_game::snd::Bank::from_vh(&vh, 0).unwrap();
        let notes = hwtr_game::snd::Tables::read(&byte);
        let v = hwtr_game::snd::key_on(&bank, &notes, word(record + 4) as usize, word(record) as usize, word(record + 8) as i32, 0, 127, 127, false, 0)
            .expect("a voice");
        let mut spu = Spu::default();
        spu.key_on(1, &vb.into(), &v);
        let mut out = vec![0i16; 2 * 44100 * 2];
        spu.render(&mut out);
        let loud = out.iter().filter(|s| s.unsigned_abs() > 256).count();
        assert!(loud > 1000, "{loud} loud samples");
        let tail = &out[out.len() - 4410..];
        assert!(tail.iter().all(|&s| s == 0), "still sounding after two seconds");
        let tmp = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-tmp");
        std::fs::create_dir_all(&tmp).unwrap();
        let mono: Vec<i16> = out.chunks(2).map(|f| f[0]).collect();
        std::fs::write(tmp.join("cross.wav"), hwtr_data::vab::wav(&mono, 44100)).unwrap();
    }

    /// A music bank's one long sample (`ELECTRIC`, program 0, tone 0, note
    /// 60): it loops, so it is still playing after half a minute.
    #[test]
    fn music_loops() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work");
        let (Ok(vh), Ok(vb), Ok(exe)) = (
            std::fs::read(dir.join("big/SCREENSBIG/ELECTRICVH")),
            std::fs::read(dir.join("big/SCREENSBIG/ELECTRICVB")),
            std::fs::read(dir.join("fs/CCCPSX.EXE")),
        ) else {
            eprintln!("skipped: the disc is not extracted");
            return;
        };
        let byte = |a: u32| a.checked_sub(0x8001_0000).and_then(|o| exe.get(o as usize + 0x800)).copied().unwrap_or(0);
        let bank = hwtr_game::snd::Bank::from_vh(&vh, 0).unwrap();
        let notes = hwtr_game::snd::Tables::read(&byte);
        let v = hwtr_game::snd::key_on(&bank, &notes, 0, 0, 60, 0, 50, 50, false, 0).expect("a voice");
        let mut spu = Spu::default();
        spu.key_on(0, &vb.into(), &v);
        let mut out = vec![0i16; 2 * 44100];
        for _ in 0..30 {
            spu.render(&mut out);
        }
        let loud = out.iter().filter(|s| s.unsigned_abs() > 64).count();
        assert!(loud > 10_000, "{loud} loud samples in the 30th second");
    }
}
