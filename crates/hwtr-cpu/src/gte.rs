//! The GTE (COP2): registers and all commands.
//!
//! Arithmetic follows the hardware as documented by psx-spx and as Mednafen
//! implements it, which passes the hardware test suites: MAC1-3 are checked
//! against 44 bits after every addition and wrapped to 44 bits, MAC0 against
//! 32, IR saturation flags, the UNR reciprocal for the perspective divide, and
//! the quirks of MVMVA (the far-colour vector's first product, the garbage
//! matrix for mx = 3).

/// FLAG bits.
pub mod flag {
    pub const IR0_SAT: u32 = 1 << 12;
    pub const SY2_SAT: u32 = 1 << 13;
    pub const SX2_SAT: u32 = 1 << 14;
    pub const MAC0_NEG: u32 = 1 << 15;
    pub const MAC0_POS: u32 = 1 << 16;
    pub const DIVIDE: u32 = 1 << 17;
    pub const SZ_OTZ_SAT: u32 = 1 << 18;
    pub const B_SAT: u32 = 1 << 19;
    pub const G_SAT: u32 = 1 << 20;
    pub const R_SAT: u32 = 1 << 21;
    pub const IR3_SAT: u32 = 1 << 22;
    pub const IR2_SAT: u32 = 1 << 23;
    pub const IR1_SAT: u32 = 1 << 24;
    pub const MAC3_NEG: u32 = 1 << 25;
    pub const MAC2_NEG: u32 = 1 << 26;
    pub const MAC1_NEG: u32 = 1 << 27;
    pub const MAC3_POS: u32 = 1 << 28;
    pub const MAC2_POS: u32 = 1 << 29;
    pub const MAC1_POS: u32 = 1 << 30;
    pub const ERROR: u32 = 1 << 31;
    /// The bits whose OR is bit 31.
    pub const ERROR_MASK: u32 = 0x7f87_e000;
}

pub type Matrix = [[i16; 3]; 3];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Gte {
    // Data registers.
    pub v: [[i16; 3]; 3],
    pub rgbc: [u8; 4],
    pub otz: u16,
    /// IR0..IR3.
    pub ir: [i16; 4],
    /// SXY0..SXY2, the screen XY FIFO.
    pub sxy: [[i16; 2]; 3],
    /// SZ0..SZ3, the Z FIFO.
    pub sz: [u16; 4],
    /// RGB0..RGB2, the colour FIFO.
    pub rgb: [[u8; 4]; 3],
    pub res1: u32,
    /// MAC0..MAC3.
    pub mac: [i32; 4],
    pub lzcs: u32,
    // Control registers.
    pub rt: Matrix,
    pub tr: [i32; 3],
    pub l: Matrix,
    pub bk: [i32; 3],
    pub lr: Matrix,
    pub fc: [i32; 3],
    pub ofx: i32,
    pub ofy: i32,
    pub h: u16,
    pub dqa: i16,
    pub dqb: i32,
    pub zsf3: i16,
    pub zsf4: i16,
    pub flag: u32,
}

fn lo16(v: u32) -> i16 {
    v as u16 as i16
}

fn hi16(v: u32) -> i16 {
    (v >> 16) as u16 as i16
}

fn pack(lo: i16, hi: i16) -> u32 {
    (lo as u16 as u32) | ((hi as u16 as u32) << 16)
}

fn mat_get(m: &Matrix, word: usize) -> u32 {
    let f = |i: usize| m[i / 3][i % 3];
    if word == 4 { f(8) as i32 as u32 } else { pack(f(word * 2), f(word * 2 + 1)) }
}

fn mat_set(m: &mut Matrix, word: usize, v: u32) {
    let mut set = |i: usize, x: i16| m[i / 3][i % 3] = x;
    if word == 4 {
        set(8, lo16(v));
    } else {
        set(word * 2, lo16(v));
        set(word * 2 + 1, hi16(v));
    }
}

/// The UNR reciprocal table, 257 entries, written as psx-spx gives it.
#[allow(clippy::manual_div_ceil)]
fn unr(i: usize) -> u32 {
    (((0x40000 / (i as u32 + 0x100)) + 1) / 2).saturating_sub(0x101)
}

/// Which matrix MVMVA multiplies by.
#[derive(Clone, Copy)]
enum Mat {
    Rot,
    Light,
    Color,
    /// mx = 3: the "garbage" matrix the hardware forms from RGBC, IR0 and RT.
    Garbage,
}

/// Which translation vector MVMVA adds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cv {
    Tr,
    Bk,
    Fc,
    None,
}

fn put_matrix(w: &mut crate::state::Writer, m: &Matrix) {
    for row in m {
        for &x in row {
            w.u16(x as u16);
        }
    }
}

fn get_matrix(r: &mut crate::state::Reader) -> crate::state::Result<Matrix> {
    let mut m = [[0i16; 3]; 3];
    for row in &mut m {
        for x in row {
            *x = r.u16()? as i16;
        }
    }
    Ok(m)
}

impl Gte {
    /// Every register exactly, for save states.
    pub fn save(&self, w: &mut crate::state::Writer) {
        for v in &self.v {
            for &x in v {
                w.u16(x as u16);
            }
        }
        w.0.extend(self.rgbc);
        w.u16(self.otz);
        for &x in &self.ir {
            w.u16(x as u16);
        }
        for p in &self.sxy {
            w.u16(p[0] as u16);
            w.u16(p[1] as u16);
        }
        for &z in &self.sz {
            w.u16(z);
        }
        for c in &self.rgb {
            w.0.extend(c);
        }
        w.u32(self.res1);
        for &m in &self.mac {
            w.u32(m as u32);
        }
        w.u32(self.lzcs);
        put_matrix(w, &self.rt);
        for &t in &self.tr {
            w.u32(t as u32);
        }
        put_matrix(w, &self.l);
        for &t in &self.bk {
            w.u32(t as u32);
        }
        put_matrix(w, &self.lr);
        for &t in &self.fc {
            w.u32(t as u32);
        }
        w.u32(self.ofx as u32);
        w.u32(self.ofy as u32);
        w.u16(self.h);
        w.u16(self.dqa as u16);
        w.u32(self.dqb as u32);
        w.u16(self.zsf3 as u16);
        w.u16(self.zsf4 as u16);
        w.u32(self.flag);
    }

    pub fn load(r: &mut crate::state::Reader) -> crate::state::Result<Gte> {
        let mut g = Gte::default();
        for v in &mut g.v {
            for x in v {
                *x = r.u16()? as i16;
            }
        }
        for c in &mut g.rgbc {
            *c = r.u8()?;
        }
        g.otz = r.u16()?;
        for x in &mut g.ir {
            *x = r.u16()? as i16;
        }
        for p in &mut g.sxy {
            p[0] = r.u16()? as i16;
            p[1] = r.u16()? as i16;
        }
        for z in &mut g.sz {
            *z = r.u16()?;
        }
        for c in &mut g.rgb {
            for b in c {
                *b = r.u8()?;
            }
        }
        g.res1 = r.u32()?;
        for m in &mut g.mac {
            *m = r.u32()? as i32;
        }
        g.lzcs = r.u32()?;
        g.rt = get_matrix(r)?;
        for t in &mut g.tr {
            *t = r.u32()? as i32;
        }
        g.l = get_matrix(r)?;
        for t in &mut g.bk {
            *t = r.u32()? as i32;
        }
        g.lr = get_matrix(r)?;
        for t in &mut g.fc {
            *t = r.u32()? as i32;
        }
        g.ofx = r.u32()? as i32;
        g.ofy = r.u32()? as i32;
        g.h = r.u16()?;
        g.dqa = r.u16()? as i16;
        g.dqb = r.u32()? as i32;
        g.zsf3 = r.u16()? as i16;
        g.zsf4 = r.u16()? as i16;
        g.flag = r.u32()?;
        Ok(g)
    }

    // ---- register access -------------------------------------------------

    pub fn read_data(&self, r: usize) -> u32 {
        match r {
            0 | 2 | 4 => pack(self.v[r / 2][0], self.v[r / 2][1]),
            1 | 3 | 5 => self.v[r / 2][2] as i32 as u32,
            6 => u32::from_le_bytes(self.rgbc),
            7 => self.otz as u32,
            8..=11 => self.ir[r - 8] as i32 as u32,
            12..=14 => pack(self.sxy[r - 12][0], self.sxy[r - 12][1]),
            15 => pack(self.sxy[2][0], self.sxy[2][1]),
            16..=19 => self.sz[r - 16] as u32,
            20..=22 => u32::from_le_bytes(self.rgb[r - 20]),
            23 => self.res1,
            24..=27 => self.mac[r - 24] as u32,
            28 | 29 => {
                let c = |x: i16| ((x as i32) >> 7).clamp(0, 0x1f) as u32;
                c(self.ir[1]) | (c(self.ir[2]) << 5) | (c(self.ir[3]) << 10)
            }
            30 => self.lzcs,
            31 => {
                let x = self.lzcs;
                if x & 0x8000_0000 != 0 { x.leading_ones() } else { x.leading_zeros() }
            }
            _ => unreachable!(),
        }
    }

    pub fn write_data(&mut self, r: usize, v: u32) {
        match r {
            0 | 2 | 4 => {
                self.v[r / 2][0] = lo16(v);
                self.v[r / 2][1] = hi16(v);
            }
            1 | 3 | 5 => self.v[r / 2][2] = lo16(v),
            6 => self.rgbc = v.to_le_bytes(),
            7 => self.otz = v as u16,
            8..=11 => self.ir[r - 8] = lo16(v),
            12..=14 => self.sxy[r - 12] = [lo16(v), hi16(v)],
            15 => {
                self.sxy[0] = self.sxy[1];
                self.sxy[1] = self.sxy[2];
                self.sxy[2] = [lo16(v), hi16(v)];
            }
            16..=19 => self.sz[r - 16] = v as u16,
            20..=22 => self.rgb[r - 20] = v.to_le_bytes(),
            23 => self.res1 = v,
            24..=27 => self.mac[r - 24] = v as i32,
            28 => {
                self.ir[1] = ((v & 0x1f) << 7) as i16;
                self.ir[2] = (((v >> 5) & 0x1f) << 7) as i16;
                self.ir[3] = (((v >> 10) & 0x1f) << 7) as i16;
            }
            29 | 31 => {}
            30 => self.lzcs = v,
            _ => unreachable!(),
        }
    }

    pub fn read_ctrl(&self, r: usize) -> u32 {
        match r {
            0..=4 => mat_get(&self.rt, r),
            5..=7 => self.tr[r - 5] as u32,
            8..=12 => mat_get(&self.l, r - 8),
            13..=15 => self.bk[r - 13] as u32,
            16..=20 => mat_get(&self.lr, r - 16),
            21..=23 => self.fc[r - 21] as u32,
            24 => self.ofx as u32,
            25 => self.ofy as u32,
            // H reads back sign-extended, a hardware bug.
            26 => self.h as i16 as i32 as u32,
            27 => self.dqa as i32 as u32,
            28 => self.dqb as u32,
            29 => self.zsf3 as i32 as u32,
            30 => self.zsf4 as i32 as u32,
            31 => self.flag,
            _ => unreachable!(),
        }
    }

    pub fn write_ctrl(&mut self, r: usize, v: u32) {
        match r {
            0..=4 => mat_set(&mut self.rt, r, v),
            5..=7 => self.tr[r - 5] = v as i32,
            8..=12 => mat_set(&mut self.l, r - 8, v),
            13..=15 => self.bk[r - 13] = v as i32,
            16..=20 => mat_set(&mut self.lr, r - 16, v),
            21..=23 => self.fc[r - 21] = v as i32,
            24 => self.ofx = v as i32,
            25 => self.ofy = v as i32,
            26 => self.h = v as u16,
            27 => self.dqa = lo16(v),
            28 => self.dqb = v as i32,
            29 => self.zsf3 = lo16(v),
            30 => self.zsf4 = lo16(v),
            31 => {
                self.flag = v & 0x7fff_f000;
                self.finish_flag();
            }
            _ => unreachable!(),
        }
    }

    fn finish_flag(&mut self) {
        if self.flag & flag::ERROR_MASK != 0 {
            self.flag |= flag::ERROR;
        }
    }

    // ---- saturation and overflow helpers --------------------------------

    /// Checks a MAC1-3 intermediate against 44 bits and wraps it to 44 bits.
    fn a_mv(&mut self, which: usize, value: i64) -> i64 {
        if value >= 1 << 43 {
            self.flag |= flag::MAC1_POS >> which;
        }
        if value < -(1 << 43) {
            self.flag |= flag::MAC1_NEG >> which;
        }
        (value << 20) >> 20
    }

    /// Checks a MAC0 value against 32 bits.
    fn f(&mut self, value: i64) -> i64 {
        if value < -0x8000_0000 {
            self.flag |= flag::MAC0_NEG;
        }
        if value > 0x7fff_ffff {
            self.flag |= flag::MAC0_POS;
        }
        value
    }

    /// IR1-3 saturation: which is 0..2.
    fn lm_b(&mut self, which: usize, value: i32, lm: bool) -> i32 {
        let min = if lm { 0 } else { -0x8000 };
        if value < min {
            self.flag |= flag::IR1_SAT >> which;
            return min;
        }
        if value > 0x7fff {
            self.flag |= flag::IR1_SAT >> which;
            return 0x7fff;
        }
        value
    }

    /// IR3 in RTPS/RTPT: the flag follows the unshifted value >> 12, the
    /// saturation follows MAC3.
    fn lm_b_ptz(&mut self, value: i32, ftv: i32, lm: bool) -> i32 {
        let min = if lm { 0 } else { -0x8000 };
        if !(-0x8000..=0x7fff).contains(&ftv) {
            self.flag |= flag::IR3_SAT;
        }
        value.clamp(min, 0x7fff)
    }

    /// Colour FIFO component saturation: which is 0..2.
    fn lm_c(&mut self, which: usize, value: i32) -> u8 {
        if value & !0xff != 0 {
            self.flag |= flag::R_SAT >> which;
            value.clamp(0, 0xff) as u8
        } else {
            value as u8
        }
    }

    /// SZ3 and OTZ saturation. When chained after a MAC0 computation, a MAC0
    /// overflow saturates directly.
    fn lm_d(&mut self, value: i64, unchained: bool) -> u16 {
        if !unchained {
            if self.flag & flag::MAC0_NEG != 0 {
                self.flag |= flag::SZ_OTZ_SAT;
                return 0;
            }
            if self.flag & flag::MAC0_POS != 0 {
                self.flag |= flag::SZ_OTZ_SAT;
                return 0xffff;
            }
        }
        if value < 0 {
            self.flag |= flag::SZ_OTZ_SAT;
            0
        } else if value > 0xffff {
            self.flag |= flag::SZ_OTZ_SAT;
            0xffff
        } else {
            value as u16
        }
    }

    /// Screen X/Y saturation: which is 0 (x) or 1 (y).
    fn lm_g(&mut self, which: usize, value: i32) -> i16 {
        if !(-0x400..=0x3ff).contains(&value) {
            self.flag |= flag::SX2_SAT >> which;
        }
        value.clamp(-0x400, 0x3ff) as i16
    }

    /// IR0 saturation.
    fn lm_h(&mut self, value: i64) -> i16 {
        if !(0..=0x1000).contains(&value) {
            self.flag |= flag::IR0_SAT;
        }
        value.clamp(0, 0x1000) as i16
    }

    fn mac_to_ir(&mut self, lm: bool) {
        for i in 0..3 {
            self.ir[i + 1] = self.lm_b(i, self.mac[i + 1], lm) as i16;
        }
    }

    fn mac_to_rgb_fifo(&mut self) {
        self.rgb[0] = self.rgb[1];
        self.rgb[1] = self.rgb[2];
        let r = self.lm_c(0, self.mac[1] >> 4);
        let g = self.lm_c(1, self.mac[2] >> 4);
        let b = self.lm_c(2, self.mac[3] >> 4);
        self.rgb[2] = [r, g, b, self.rgbc[3]];
    }

    /// The perspective divide: H / SZ3 by the UNR method, 0..0x1ffff.
    fn divide(&mut self) -> i64 {
        let (h, sz3) = (self.h as u32, self.sz[3] as u32);
        if h < sz3 * 2 {
            let z = (sz3 as u16).leading_zeros();
            let n = (h as u64) << z;
            let d = (sz3 << z) as u64;
            let u = unr(((d - 0x7fc0) >> 7) as usize) as u64 + 0x101;
            let d = (0x200_0080u64.wrapping_sub(d * u)) >> 8;
            let d = (0x80 + d * u) >> 8;
            (((n * d) + 0x8000) >> 16).min(0x1ffff) as i64
        } else {
            self.flag |= flag::DIVIDE;
            0x1ffff
        }
    }

    // ---- shared command bodies ----------------------------------------------

    fn matrix_row(&self, m: Mat, i: usize) -> [i32; 3] {
        match m {
            Mat::Rot => self.rt[i].map(i32::from),
            Mat::Light => self.l[i].map(i32::from),
            Mat::Color => self.lr[i].map(i32::from),
            Mat::Garbage => match i {
                0 => {
                    let r = (self.rgbc[0] as i32) << 4;
                    [-r, r, self.ir[0] as i32]
                }
                1 => [self.rt[0][2] as i32; 3],
                _ => [self.rt[1][1] as i32; 3],
            },
        }
    }

    fn cv(&self, cv: Cv, i: usize) -> i64 {
        match cv {
            Cv::Tr => self.tr[i] as i64,
            Cv::Bk => self.bk[i] as i64,
            Cv::Fc => self.fc[i] as i64,
            Cv::None => 0,
        }
    }

    fn mul_mat_vec(&mut self, m: Mat, v: [i16; 3], cv: Cv, sf: u32, lm: bool) {
        for i in 0..3 {
            let row = self.matrix_row(m, i);
            let p = [row[0] * v[0] as i32, row[1] * v[1] as i32, row[2] * v[2] as i32];
            let mut t = self.cv(cv, i) << 12;
            t = self.a_mv(i, t + p[0] as i64);
            if cv == Cv::Fc {
                // The hardware loses the far-colour term and the first
                // product, keeping only their saturation flag.
                self.lm_b(i, (t >> sf) as i32, false);
                t = 0;
            }
            t = self.a_mv(i, t + p[1] as i64);
            t = self.a_mv(i, t + p[2] as i64);
            self.mac[i + 1] = (t >> sf) as i32;
        }
        self.mac_to_ir(lm);
    }

    /// The rotate-translate for RTPS/RTPT, which also pushes SZ3.
    fn mul_mat_vec_pt(&mut self, v: [i16; 3], sf: u32, lm: bool) {
        let mut t = [0i64; 3];
        for (i, ti) in t.iter_mut().enumerate() {
            let row = self.rt[i];
            *ti = (self.tr[i] as i64) << 12;
            for k in 0..3 {
                *ti = self.a_mv(i, *ti + (row[k] as i32 * v[k] as i32) as i64);
            }
            self.mac[i + 1] = (*ti >> sf) as i32;
        }
        self.ir[1] = self.lm_b(0, self.mac[1], lm) as i16;
        self.ir[2] = self.lm_b(1, self.mac[2], lm) as i16;
        self.ir[3] = self.lm_b_ptz(self.mac[3], (t[2] >> 12) as i32, lm) as i16;
        self.sz[0] = self.sz[1];
        self.sz[1] = self.sz[2];
        self.sz[2] = self.sz[3];
        self.sz[3] = self.lm_d(t[2] >> 12, true);
    }

    fn transform_xy(&mut self, hd: i64) {
        let x = self.f(self.ofx as i64 + self.ir[1] as i64 * hd) >> 16;
        self.mac[0] = x as i32;
        let sx = self.lm_g(0, x as i32);
        let y = self.f(self.ofy as i64 + self.ir[2] as i64 * hd) >> 16;
        self.mac[0] = y as i32;
        let sy = self.lm_g(1, y as i32);
        self.sxy[0] = self.sxy[1];
        self.sxy[1] = self.sxy[2];
        self.sxy[2] = [sx, sy];
    }

    fn transform_dq(&mut self, hd: i64) {
        let v = self.dqb as i64 + self.dqa as i64 * hd;
        self.mac[0] = self.f(v) as i32;
        self.ir[0] = self.lm_h(v >> 12);
    }

    fn depth_cue(&mut self, mult_ir: bool, from_fifo: bool, sf: u32, lm: bool) {
        let src = if from_fifo { self.rgb[0] } else { self.rgbc };
        let c = [(src[0] as i64) << 4, (src[1] as i64) << 4, (src[2] as i64) << 4];
        let ir = [self.ir[1] as i64, self.ir[2] as i64, self.ir[3] as i64];
        for i in 0..3 {
            let base = if mult_ir { c[i] * ir[i] } else { c[i] << 12 };
            let t = self.a_mv(i, ((self.fc[i] as i64) << 12) - base) >> sf;
            self.mac[i + 1] = t as i32;
            let sat = self.lm_b(i, self.mac[i + 1], false) as i64;
            let t = self.a_mv(i, base + self.ir[0] as i64 * sat) >> sf;
            self.mac[i + 1] = t as i32;
        }
        self.mac_to_rgb_fifo();
        self.mac_to_ir(lm);
    }

    fn ir_vec(&self) -> [i16; 3] {
        [self.ir[1], self.ir[2], self.ir[3]]
    }

    fn color_times_ir(&mut self, sf: u32, lm: bool) {
        for i in 0..3 {
            self.mac[i + 1] = ((((self.rgbc[i] as i64) << 4) * self.ir[i + 1] as i64) >> sf) as i32;
        }
        self.mac_to_ir(lm);
        self.mac_to_rgb_fifo();
    }

    fn norm_color(&mut self, v: usize, sf: u32, lm: bool) {
        self.mul_mat_vec(Mat::Light, self.v[v], Cv::None, sf, lm);
        self.mul_mat_vec(Mat::Color, self.ir_vec(), Cv::Bk, sf, lm);
        self.mac_to_rgb_fifo();
    }

    fn norm_color_color(&mut self, v: usize, sf: u32, lm: bool) {
        self.mul_mat_vec(Mat::Light, self.v[v], Cv::None, sf, lm);
        self.mul_mat_vec(Mat::Color, self.ir_vec(), Cv::Bk, sf, lm);
        self.color_times_ir(sf, lm);
    }

    fn norm_color_depth_cue(&mut self, v: usize, sf: u32, lm: bool) {
        self.mul_mat_vec(Mat::Light, self.v[v], Cv::None, sf, lm);
        self.mul_mat_vec(Mat::Color, self.ir_vec(), Cv::Bk, sf, lm);
        self.depth_cue(true, false, sf, lm);
    }

    // ---- commands -----------------------------------------------------------

    /// Executes a COP2 command word (the low 25 bits of the instruction).
    pub fn command(&mut self, word: u32) {
        let sf = if word & (1 << 19) != 0 { 12 } else { 0 };
        let lm = word & (1 << 10) != 0;
        self.flag = 0;
        match word & 0x3f {
            0x01 => self.rtps(self.v[0], sf, lm, true),
            0x06 => {
                let s = self.sxy.map(|p| [p[0] as i64, p[1] as i64]);
                let v = s[0][0] * (s[1][1] - s[2][1]) + s[1][0] * (s[2][1] - s[0][1]) + s[2][0] * (s[0][1] - s[1][1]);
                self.mac[0] = self.f(v) as i32;
            }
            0x0c => {
                let (d1, d2, d3) = (self.rt[0][0] as i64, self.rt[1][1] as i64, self.rt[2][2] as i64);
                let (i1, i2, i3) = (self.ir[1] as i64, self.ir[2] as i64, self.ir[3] as i64);
                self.mac[1] = (self.a_mv(0, d2 * i3 - d3 * i2) >> sf) as i32;
                self.mac[2] = (self.a_mv(1, d3 * i1 - d1 * i3) >> sf) as i32;
                self.mac[3] = (self.a_mv(2, d1 * i2 - d2 * i1) >> sf) as i32;
                self.mac_to_ir(lm);
            }
            0x10 => self.depth_cue(false, false, sf, lm),
            0x11 => {
                for i in 0..3 {
                    let ir = self.ir[i + 1] as i64;
                    let t = self.a_mv(i, ((self.fc[i] as i64) << 12) - (ir << 12)) >> sf;
                    self.mac[i + 1] = t as i32;
                }
                for i in 0..3 {
                    let ir = self.ir[i + 1] as i64;
                    let sat = self.lm_b(i, self.mac[i + 1], false) as i64;
                    let t = self.a_mv(i, (ir << 12) + self.ir[0] as i64 * sat) >> sf;
                    self.mac[i + 1] = t as i32;
                }
                self.mac_to_ir(lm);
                self.mac_to_rgb_fifo();
            }
            0x12 => {
                let mx = match (word >> 17) & 3 {
                    0 => Mat::Rot,
                    1 => Mat::Light,
                    2 => Mat::Color,
                    _ => Mat::Garbage,
                };
                let v = match (word >> 15) & 3 {
                    3 => self.ir_vec(),
                    n => self.v[n as usize],
                };
                let cv = match (word >> 13) & 3 {
                    0 => Cv::Tr,
                    1 => Cv::Bk,
                    2 => Cv::Fc,
                    _ => Cv::None,
                };
                self.mul_mat_vec(mx, v, cv, sf, lm);
            }
            0x13 => self.norm_color_depth_cue(0, sf, lm),
            0x14 => {
                self.mul_mat_vec(Mat::Color, self.ir_vec(), Cv::Bk, sf, lm);
                self.depth_cue(true, false, sf, lm);
            }
            0x16 => {
                for v in 0..3 {
                    self.norm_color_depth_cue(v, sf, lm);
                }
            }
            0x1b => self.norm_color_color(0, sf, lm),
            0x1c => {
                self.mul_mat_vec(Mat::Color, self.ir_vec(), Cv::Bk, sf, lm);
                self.color_times_ir(sf, lm);
            }
            0x1e => self.norm_color(0, sf, lm),
            0x20 => {
                for v in 0..3 {
                    self.norm_color(v, sf, lm);
                }
            }
            0x28 => {
                for i in 1..4 {
                    let x = self.ir[i] as i64;
                    self.mac[i] = ((x * x) >> sf) as i32;
                }
                self.mac_to_ir(lm);
            }
            0x29 => self.depth_cue(true, false, sf, lm),
            0x2a => {
                for _ in 0..3 {
                    self.depth_cue(false, true, sf, lm);
                }
            }
            0x2d => {
                let s = self.sz[1] as i64 + self.sz[2] as i64 + self.sz[3] as i64;
                let v = self.f(self.zsf3 as i64 * s);
                self.mac[0] = v as i32;
                self.otz = self.lm_d(v >> 12, false);
            }
            0x2e => {
                let s = self.sz.iter().map(|&z| z as i64).sum::<i64>();
                let v = self.f(self.zsf4 as i64 * s);
                self.mac[0] = v as i32;
                self.otz = self.lm_d(v >> 12, false);
            }
            0x30 => {
                for i in 0..3 {
                    self.rtps(self.v[i], sf, lm, i == 2);
                }
            }
            0x3d => {
                for i in 1..4 {
                    self.mac[i] = ((self.ir[0] as i64 * self.ir[i] as i64) >> sf) as i32;
                }
                self.mac_to_ir(lm);
                self.mac_to_rgb_fifo();
            }
            0x3e => {
                for i in 0..3 {
                    let t = ((self.mac[i + 1] as i64) << sf) + self.ir[0] as i64 * self.ir[i + 1] as i64;
                    self.mac[i + 1] = (self.a_mv(i, t) >> sf) as i32;
                }
                self.mac_to_ir(lm);
                self.mac_to_rgb_fifo();
            }
            0x3f => {
                for v in 0..3 {
                    self.norm_color_color(v, sf, lm);
                }
            }
            _ => {}
        }
        self.finish_flag();
    }

    fn rtps(&mut self, v: [i16; 3], sf: u32, lm: bool, dq: bool) {
        self.mul_mat_vec_pt(v, sf, lm);
        let hd = self.divide();
        self.transform_xy(hd);
        if dq {
            self.transform_dq(hd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> Gte {
        let mut g = Gte::default();
        g.write_ctrl(0, 0x1000); // RT11 = 1.0
        g.write_ctrl(2, 0x1000); // RT22
        g.write_ctrl(4, 0x1000); // RT33
        g
    }

    #[test]
    fn unr_table_ends() {
        assert_eq!(unr(0), 0xff);
        assert_eq!(unr(0x100), 0);
    }

    #[test]
    fn rtps_projects() {
        let mut g = identity();
        g.write_ctrl(24, 160 << 16); // OFX
        g.write_ctrl(25, 120 << 16); // OFY
        g.write_ctrl(26, 256); // H
        g.write_data(0, pack(100, -50));
        g.write_data(1, 512);
        g.command(0x0018_0001); // RTPS sf=1
        assert_eq!(g.sz[3], 512);
        assert_eq!(g.ir[1], 100);
        // x = 160 + 100 * 256 / 512 = 210, y = 120 - 25 = 95
        assert_eq!(g.read_data(14), pack(210, 95));
        assert_eq!(g.flag, 0);
    }

    #[test]
    fn divide_overflow_sets_flags() {
        let mut g = identity();
        g.write_ctrl(26, 1000);
        g.write_data(1, 10);
        g.command(0x0018_0001);
        assert_ne!(g.flag & flag::DIVIDE, 0);
        assert_ne!(g.flag & flag::ERROR, 0);
    }

    #[test]
    fn nclip_and_avsz3() {
        let mut g = Gte::default();
        g.write_data(12, pack(0, 0));
        g.write_data(13, pack(10, 0));
        g.write_data(14, pack(0, 10));
        g.command(0x0140_0006);
        assert_eq!(g.mac[0], 100);
        g.sz = [0, 100, 200, 300];
        g.write_ctrl(29, 0x555); // ZSF3 ~ 1/3 in 4.12
        g.command(0x0158_002d);
        assert_eq!(g.otz, ((0x555 * 600) >> 12) as u16);
    }

    #[test]
    fn registers_round_trip() {
        let mut g = Gte::default();
        g.write_ctrl(26, 0x8000);
        assert_eq!(g.read_ctrl(26), 0xffff_8000, "H reads back sign-extended");
        g.write_data(30, 0x0000_ffff);
        assert_eq!(g.read_data(31), 16);
        g.write_data(30, 0xffff_0000);
        assert_eq!(g.read_data(31), 16);
        g.write_data(28, 0x7fff);
        assert_eq!(g.read_data(29), 0x7fff);
        g.write_ctrl(31, 0xffff_ffff);
        assert_eq!(g.read_ctrl(31), 0xffff_f000);
    }
}
