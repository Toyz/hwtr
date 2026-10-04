//! The PlayStation's sound chip's voices, as the port needs them: 24
//! voices, each playing PS-ADPCM at a pitch (0x1000 is 44.1 kHz), through
//! an ADSR envelope, at a volume left and right, honouring the blocks' loop
//! flags. Unlike the console, there is no 512 KiB of sample memory to share:
//! each bank's samples are their own buffer, every bank stays loaded, and a
//! voice plays from a bank at an offset. It plays through rrt's audio
//! output; the game sets voices and keys them on between ticks.
//!
//! The sample format and the chip's 4-point Gaussian interpolation are
//! rrt's (`rrt::kit::adpcm::spu`); the voices, envelopes and mixing are here,
//! and the chip's reverb ([`Reverb`]) with the race's "studio large".

use std::sync::Arc;

use rrt::audio::Source;
use rrt::kit::adpcm::spu::{self as adpcm, History, PER_FRAME};

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
    reverb: Reverb,
}

/// The chip's reverb, as the hardware runs it (psx-spx's account of it): at
/// half the output rate, in a ring of sound memory from mBASE to the end,
/// each sample the input from the voices fed to it reflected off the same
/// side and the other, combed, put through two all-pass filters, and given
/// out at the depth. The work area is its own buffer here, not shared
/// sound memory. The registers are the race's preset
/// ([`hwtr_game::snd::STUDIO_LARGE`]).
#[derive(Default)]
pub struct Reverb {
    on: bool,
    /// The first 20 voices are fed to it (0x800a6b94).
    voices: bool,
    /// The output volume, both sides (vLOUT, vROUT).
    depth: i16,
    regs: [i16; 32],
    ring: Vec<i16>,
    at: usize,
    /// Every other output sample runs it; the output holds between.
    odd: bool,
    out: (i32, i32),
}

impl Reverb {
    fn on(&mut self) {
        if self.ring.is_empty() {
            let base = hwtr_game::snd::STUDIO_LARGE_BASE as usize * 8;
            self.ring = vec![0; (0x8_0000 - base) / 2];
            self.regs = hwtr_game::snd::STUDIO_LARGE.map(|r| r as i16);
        }
        self.on = true;
    }

    /// `SsUtReverbOff`: off, the voices off it, its depth 0.
    fn off(&mut self) {
        self.on = false;
        self.voices = false;
        self.depth = 0;
        self.out = (0, 0);
    }

    /// The half word `off` half words from where it is, round the ring.
    fn at(&self, off: i32) -> i32 {
        let n = self.ring.len() as i32;
        self.ring[(self.at as i32 + off).rem_euclid(n) as usize] as i32
    }

    fn put(&mut self, off: i32, v: i32) {
        let n = self.ring.len() as i32;
        self.ring[(self.at as i32 + off).rem_euclid(n) as usize] = v.clamp(-0x8000, 0x7fff) as i16;
    }

    /// One output sample's reverb from the voices' share `input`.
    fn sample(&mut self, input: (i32, i32)) -> (i32, i32) {
        if !self.on || self.ring.is_empty() {
            return (0, 0);
        }
        self.odd = !self.odd;
        if self.odd {
            return self.out;
        }
        let mul = |a: i32, b: i32| (a * b) >> 15;
        let r = |k: usize| self.regs[k] as i32;
        // An address register: in 8-byte steps, as half words.
        let a = |k: usize| (self.regs[k] as u16 as i32) * 4;
        let (d_apf1, d_apf2, v_iir) = (a(0), a(1), r(2));
        let v_comb = [r(3), r(4), r(5), r(6)];
        let (v_wall, v_apf1, v_apf2) = (r(7), r(8), r(9));
        let (m_lsame, m_rsame) = (a(10), a(11));
        let (m_lcomb1, m_rcomb1, m_lcomb2, m_rcomb2) = (a(12), a(13), a(14), a(15));
        let (d_lsame, d_rsame, m_ldiff, m_rdiff) = (a(16), a(17), a(18), a(19));
        let (m_lcomb3, m_rcomb3, m_lcomb4, m_rcomb4) = (a(20), a(21), a(22), a(23));
        let (d_ldiff, d_rdiff) = (a(24), a(25));
        let (m_lapf1, m_rapf1, m_lapf2, m_rapf2) = (a(26), a(27), a(28), a(29));
        let (v_lin, v_rin) = (r(30), r(31));
        let clamp = |v: i32| v.clamp(-0x8000, 0x7fff);
        let l_in = mul(v_lin, clamp(input.0));
        let r_in = mul(v_rin, clamp(input.1));
        // The reflections: same side, then across.
        let reflect = |s: &Self, input: i32, from: i32, to: i32| {
            let last = s.at(to - 1);
            mul(input + mul(s.at(from), v_wall) - last, v_iir) + last
        };
        let (ls, rs) = (reflect(self, l_in, d_lsame, m_lsame), reflect(self, r_in, d_rsame, m_rsame));
        let (ld, rd) = (reflect(self, l_in, d_rdiff, m_ldiff), reflect(self, r_in, d_ldiff, m_rdiff));
        self.put(m_lsame, ls);
        self.put(m_rsame, rs);
        self.put(m_ldiff, ld);
        self.put(m_rdiff, rd);
        // The early echo, then the two all-pass filters.
        let comb = |s: &Self, m: [i32; 4]| (0..4).fold(0, |acc, k| acc + mul(v_comb[k], s.at(m[k])));
        let mut l = comb(self, [m_lcomb1, m_lcomb2, m_lcomb3, m_lcomb4]);
        let mut rr = comb(self, [m_rcomb1, m_rcomb2, m_rcomb3, m_rcomb4]);
        for (m_l, m_r, d, v) in [(m_lapf1, m_rapf1, d_apf1, v_apf1), (m_lapf2, m_rapf2, d_apf2, v_apf2)] {
            let (old_l, old_r) = (self.at(m_l - d), self.at(m_r - d));
            l = clamp(l - mul(v, old_l));
            rr = clamp(rr - mul(v, old_r));
            self.put(m_l, l);
            self.put(m_r, rr);
            l = mul(l, v) + old_l;
            rr = mul(rr, v) + old_r;
        }
        let depth = self.depth as i32;
        self.out = (mul(clamp(l), depth), mul(clamp(rr), depth));
        self.at = (self.at + 1) % self.ring.len();
        self.out
    }
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
            Phase::Attack => {
                (true, self.adsr1 & 0x8000 != 0, ((self.adsr1 >> 10) & 31) as u32, ((self.adsr1 >> 8) & 3) as u32)
            }
            Phase::Decay => (false, true, ((self.adsr1 >> 4) & 15) as u32, 0),
            Phase::Sustain => (
                self.adsr2 & 0x4000 == 0,
                self.adsr2 & 0x8000 != 0,
                ((self.adsr2 >> 8) & 31) as u32,
                ((self.adsr2 >> 6) & 3) as u32,
            ),
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
    /// The reverb on or off (`SsUtReverbOn`, `SsUtReverbOff`).
    pub fn set_reverb(&mut self, on: bool) {
        if on {
            self.reverb.on();
        } else {
            self.reverb.off();
        }
    }

    /// `SsUtSetReverbDepth` at `depth` (0 to 127), the first 20 voices fed
    /// to it.
    pub fn set_reverb_depth(&mut self, depth: u8) {
        self.reverb.depth = hwtr_game::snd::reverb_depth(depth);
        self.reverb.voices = true;
    }

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
    /// The effects of `bank`, by id, from the executable's table
    /// ([`hwtr_game::snd::effects_table`]); `dude` under cheat option 16.
    pub fn new(byte: &dyn Fn(u32) -> u8, bank: hwtr_game::snd::Bank, samples: &[u8], dude: bool) -> Effects {
        let table = hwtr_game::snd::effects_table(byte, dude);
        Effects { bank, samples: samples.into(), notes: hwtr_game::snd::Tables::read(byte), table, mono: false }
    }

    /// Effect `id`'s tone, program and note.
    pub fn record(&self, id: u8) -> [u32; 3] {
        self.table.get(id as usize).copied().unwrap_or_default()
    }

    /// Effect `id` at `volume` (0 to 127) as a voice's settings.
    pub fn voice(&self, id: u8, volume: i32) -> Option<hwtr_game::snd::Voice> {
        let [tone, program, note] = self.table.get(id as usize).copied().unwrap_or_default();
        hwtr_game::snd::key_on(
            &self.bank,
            &self.notes,
            program as usize,
            tone as usize,
            note as i16 as i32,
            0,
            volume,
            volume,
            self.mono,
            0,
        )
    }
}

impl Source for Spu {
    fn rate(&self) -> u32 {
        44100
    }

    fn render(&mut self, out: &mut [i16]) {
        for frame in out.chunks_exact_mut(2) {
            let (mut l, mut r) = (0i32, 0i32);
            let mut wet = (0i32, 0i32);
            for (k, v) in self.voices.iter_mut().enumerate() {
                let (a, b) = v.sample();
                l += a;
                r += b;
                if k < 20 && self.reverb.voices {
                    wet = (wet.0 + a, wet.1 + b);
                }
            }
            let (rl, rr) = self.reverb.sample(wet);
            l += rl;
            r += rr;
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

    /// An impulse through "studio large" comes back as a tail that dies
    /// away, and at depth 0 as nothing.
    #[test]
    fn the_reverb_rings_and_dies_away() {
        let run = |depth: u8| {
            let mut spu = Spu::default();
            spu.set_reverb(true);
            spu.set_reverb_depth(depth);
            let mut out = Vec::new();
            for k in 0..44_100 * 3 {
                let input = if k < 64 { (20_000, 20_000) } else { (0, 0) };
                out.push(spu.reverb.sample(input).0);
            }
            out
        };
        let loud = |s: &[i32]| s.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0);
        let tail = run(100);
        assert!(loud(&tail[..2205]) > 0 || loud(&tail[2205..22_050]) > 0, "it rings");
        assert!(loud(&tail[2205..22_050]) > 50, "an echo comes back");
        assert!(loud(&tail[44_100 * 2..]) < loud(&tail[2205..22_050]) / 4, "it dies away");
        assert_eq!(loud(&run(0)), 0, "nothing at depth 0");
    }

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
        let v = hwtr_game::snd::key_on(
            &bank,
            &notes,
            word(record + 4) as usize,
            word(record) as usize,
            word(record + 8) as i32,
            0,
            127,
            127,
            false,
            0,
        )
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
