//! The race's effects (0x8002bdb4 sets them up; the world draw runs them
//! once a drawn frame): four rings of 56-byte records that overwrite their
//! oldest when full (0x8002c2b8) — puffs of dust and smoke that face the
//! camera, skid marks on the ground, sparks thrown off a scraping car, and
//! tumbling debris — and the skid trails each wheel leaves.
//!
//! Each frame `update` (0x8002f354) ages every record and moves it (its
//! velocity, then its acceleration into the velocity), and compacts the
//! dead ones out in a way that can leave two records sharing one record's
//! velocity buffers, as the original does; `emit_trails` (0x8002b888)
//! turns the wheels' new trail points into skid marks and dust; `draw`
//! (0x8002f618) fades the puffs and kills the faded, and gives the
//! quads to draw. After the frame, `car_pose`'s part of 0x80049ecc gives
//! each wheel's trail point (0x80029230).

use crate::car::Car;
use crate::math::{Matrix, Vec3, column, div_fx, fx};
use crate::rand::Rand;

/// The pools: puffs, skid marks, sparks, debris (0x800d25c0, 0x800d25d0,
/// 0x800d25d8, 0x800d25c8), in the order the update walks them.
pub const PUFFS: usize = 0;
pub const SKIDS: usize = 1;
pub const SPARKS: usize = 2;
pub const DEBRIS: usize = 3;
pub const CAPACITY: [usize; 4] = [64, 64, 20, 48];

/// The effects that are on (0x800d0d98): 1 puffs, 2 skid marks, 4 sparks,
/// 64 wrecks, 128 the boost's flame. Every race has them all.
const ENABLED: u32 = 0x1ff;
/// Puffs only above 22 frames a second (4.12).
const FPS_FLOOR: i32 = 0x1_5fff;

/// The effects sheet's textures (0x80117ab0 +28/+30, from 0x800bd0d0 and
/// the blend bits at 0x800bd130): clut, tpage.
fn sheet(t: &crate::math::Tables, k: usize) -> (u16, u16) {
    t.sprite_slots[k]
}

/// A 56-byte record: position (+0x10), flags (+0x24), frames left
/// (+0x26), colour (+0x2c), kind (+0x32) and sprite growth (+0x33).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Record {
    pub pos: Vec3,
    pub flags: u16,
    pub life: u16,
    pub colour: [u16; 3],
    pub kind: u8,
    pub frame: u8,
    /// A debris chunk's texture (+0x28, +0x2a), and whether it blends and
    /// fades (+0x34).
    pub clut: u16,
    pub tpage: u16,
    pub semi: bool,
    /// The side buffers (velocity, acceleration, corners) this record
    /// uses: its own slot's, until a compaction copies another's.
    pub buf: usize,
}

/// A ring (its header: the oldest record and the next to give), its
/// records and their side buffers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pool {
    pub oldest: usize,
    pub next: usize,
    pub records: Vec<Record>,
    pub vel: Option<Vec<Vec3>>,
    pub accel: Option<Vec<Vec3>>,
    /// A skid mark's four corners from the origin (0x80121d3c).
    pub corners: Vec<[Vec3; 4]>,
    /// A debris chunk's quad and turning (0x8012450c, 0x80123d8c).
    pub chunks: Vec<Chunk>,
}

/// A debris chunk's quad (its corners, in its own axes, and texels) and
/// how it turns: its first orientation, the axis it spins about, the angle
/// it has turned and the turn a frame (radians, 4.12).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Chunk {
    pub verts: [[i16; 3]; 4],
    pub uv: [[u8; 2]; 4],
    pub base: [i32; 4],
    pub axis: Vec3,
    pub angle: i32,
    pub step: i32,
}

/// A face that can fly off as a chunk: a car model's root face or a
/// prop's quad (corners, texels in the order the chunk keeps them, texture).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChunkFace {
    pub verts: [[i16; 3]; 4],
    pub uv: [[u8; 2]; 4],
    pub clut: u16,
    pub tpage: u16,
}

/// A wreck's ember (0x8011f59c, 72 bytes): frames since it began, frames
/// left, where it is, its velocity and the small turn it gets every frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ember {
    pub age: u8,
    pub life: i32,
    pub pos: Vec3,
    pub vel: Vec3,
    pub turn: Matrix,
}

/// A smoke column (0x8012510c, 64 bytes): the animation's loop, where it
/// is and drifts, its offset on the screen, the frames it runs between,
/// the animation's frame (-1 when idle) and pace, the size added, and
/// whether it follows a car (kind 1) and which.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Column {
    pub loops: i8,
    pub loop_end: i8,
    pub loop_restart: i8,
    pub pos: Vec3,
    pub vel: Vec3,
    pub colour: [u8; 3],
    pub jitter: [i32; 2],
    pub start: u32,
    pub end: u32,
    pub ticks: u16,
    pub frame: i8,
    pub per: u8,
    pub add: u8,
    pub kind: u8,
    pub slot: u8,
}

/// What a step leaves for the frame, not part of the original's record:
/// equal to anything, so it does not count when cars are compared.
#[derive(Clone, Debug, Default)]
pub struct Pending<T>(pub Option<T>);

impl<T> PartialEq for Pending<T> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl<T> Eq for Pending<T> {}

/// The random numbers a wreck's effects draw, taken as the wreck happens
/// (inside 0x8004619c, which calls 0x80029e10) so the game's sequence is
/// kept; the effects use them after the step.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WreckDraws {
    pub slot: u8,
    pub vel: Vec3,
    pub human: bool,
    pub rands: Vec<u32>,
}

impl WreckDraws {
    /// 0x80029e10 mode 0's draws: the smoke columns' (two each), the
    /// embers' (seven each), and fifteen for each of up to 48 chunks.
    pub fn take(rand: &mut Rand, slot: u8, vel: Vec3, human: bool, faces: usize) -> WreckDraws {
        let n = 10 + 140 + 15 * faces.min(48);
        WreckDraws { slot, vel, human, rands: (0..n).map(|_| rand.rand()).collect() }
    }
}

/// The random numbers a knocked prop's debris draws (0x8002e27c), taken
/// as the knock happens in the collision's pair loop (0x8006b958).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PropDraws {
    pub volume: u16,
    pub vel: Vec3,
    pub rands: Vec<u32>,
}

impl PropDraws {
    /// A prop with volume flags `flags` and `quads` quads: two for each
    /// smoke column (flag 2); for each of ten dust puffs (flag 1) two for
    /// its place, one for its growth with flag 0x20, and the puff's two;
    /// without flag 0x20, fifteen for each of up to 48 quads.
    pub fn take(rand: &mut Rand, volume: u16, vel: Vec3, flags: u32, quads: usize) -> PropDraws {
        let mut n = 0;
        if flags & 2 != 0 {
            n += 10;
        }
        if flags & 1 != 0 {
            n += 10 * (4 + (flags & 0x20 != 0) as usize);
        }
        if flags & 0x20 == 0 {
            n += 15 * quads.min(48);
        }
        PropDraws { volume, vel, rands: (0..n).map(|_| rand.rand()).collect() }
    }
}

/// Where a car is drawn, for the effects that start from it or follow it:
/// its model's place (0x80049ecc's), rotation, and its handling's origin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CarPose {
    pub at: Vec3,
    pub rot: Matrix,
    pub origin: Vec3,
}

impl CarPose {
    /// The car's centre as its wreck takes it (0x8002e574): the model's
    /// place and its rotated origin.
    pub fn centre(&self) -> Vec3 {
        let o = crate::math::apply_matrix_lv(&self.rot, self.origin);
        crate::math::add(self.at, o)
    }
}

impl Pool {
    pub fn new(k: usize) -> Pool {
        let cap = CAPACITY[k];
        Pool {
            oldest: 0,
            next: 0,
            records: (0..cap).map(|i| Record { buf: i, ..Record::default() }).collect(),
            vel: matches!(k, PUFFS | SPARKS | DEBRIS).then(|| vec![[0; 3]; cap]),
            accel: matches!(k, PUFFS | DEBRIS).then(|| vec![[0; 3]; cap]),
            corners: vec![[[0; 3]; 4]; if k == SKIDS { cap } else { 0 }],
            chunks: vec![Chunk::default(); if k == DEBRIS { cap } else { 0 }],
        }
    }

    fn cap(&self) -> usize {
        self.records.len()
    }

    /// 0x8002c2b8: the record at `next`, as it is (fields not set keep
    /// what they had); the oldest is dropped when the ring is full.
    fn alloc(&mut self) -> usize {
        let i = self.next;
        self.next = if i + 1 == self.cap() { 0 } else { i + 1 };
        if self.next == self.oldest {
            self.oldest = if self.oldest + 1 == self.cap() { 0 } else { self.oldest + 1 };
        }
        i
    }

    /// The live slots from the oldest to the newest, wrapping.
    fn slots(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut i = self.oldest;
        loop {
            if i == self.cap() {
                i = 0;
            }
            if i == self.next {
                break;
            }
            out.push(i);
            i += 1;
        }
        out
    }
}

/// One wheel's trail (0x8011ec30 +384 a car +64 a wheel): the last
/// committed edge points and the new ones, how far along it is (0 none, 1
/// one pair, 2 a segment ready), the skid's build-up and the ground's kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Trail {
    pub outer: Vec3,
    pub inner: Vec3,
    pub new_outer: Vec3,
    pub new_inner: Vec3,
    pub state: u8,
    pub count: u8,
    pub surface: u8,
}

/// A quad to draw: its corners (world, 20.12) in perimeter order, their
/// texels, the texture, a flat colour (128 is 1), and whether it blends
/// by its texture page's mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectQuad {
    pub corners: [Vec3; 4],
    pub uv: [[u8; 2]; 4],
    pub clut: u16,
    pub tpage: u16,
    pub colour: [u8; 3],
    pub semi: bool,
}

/// A spark, its place and velocity, as the collision makes it (0x8002e9f8
/// with 0x8002c32c's jitter); the effects take it in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spark {
    pub pos: Vec3,
    pub vel: Vec3,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effects {
    pub pools: [Pool; 4],
    /// Frames drawn while not paused (0x800d0b68).
    pub frame_count: u32,
    /// 0x8011ec2c: 2 when a wheel pushed a trail point since the last
    /// frame.
    pub trail_tick: u16,
    pub trails: [[Trail; 6]; 6],
    /// The frame of the trail calls, how many this frame, and whether the
    /// skid marks' origin is set (0x800d0da4, 0x800d0da0, 0x800d0da8).
    pub calls_frame: u32,
    pub calls: u32,
    pub origin_set: bool,
    /// Skid marks are kept from this point so their numbers stay small
    /// (0x80127acc).
    pub origin: Vec3,
    pub embers: [Ember; 20],
    pub columns: [Column; 5],
    /// Each player's screen flash after a wreck (0x800d0db8).
    pub flash: [i16; 2],
    /// The chunks' shared colour (the one quad 0x80126c8c they all draw
    /// through), which every chunk drawn fades.
    pub chunk_colour: [u8; 3],
    /// Each car's model's colour (its root node's, +0x44: grey 0x808080,
    /// 0x181818 when wrecked, pulsing while it boosts), whether its
    /// effects take it as wrecked (cvs +0x28), its boost flame, and the
    /// flame's colour pulse (0x8011ec14 phase, -1 off; 0x8011ebf4 when it
    /// last stepped, by the system clock).
    pub root_colour: [u32; 6],
    pub wrecked: [bool; 6],
    pub flames: [Flame; 6],
    pub pulse: [(i32, u32); 6],
}

/// A car's boost flame (cvs +0x1f0 on, +0x24 the frame it began, +0x1ec
/// frames shown).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flame {
    pub on: bool,
    pub start: u32,
    pub count: u8,
}

/// The cars' exhaust offsets by car number (0x800be088, 41 bytes a car;
/// the first three are x, y, z in model units).
pub const EXHAUSTS: u32 = 0x800b_e088;

impl Default for Effects {
    fn default() -> Effects {
        Effects::new()
    }
}

/// Puff templates (0x800be774 type 0, 0x800be794 type 1, 0x800be754 type
/// 16, 0x800be7b4 type 19): corner offsets in the camera's plane, texels.
const PUFF_0: [([i16; 2], [u8; 2]); 4] = [([16, -16], [0xbf, 0]), ([-16, -16], [0x80, 0]), ([-16, 16], [0x80, 0x3f]), ([16, 16], [0xbf, 0x3f])];
const PUFF_1: [([i16; 2], [u8; 2]); 4] = [([16, -16], [0xff, 0]), ([-16, -16], [0xc0, 0]), ([-16, 16], [0xc0, 0x3f]), ([16, 16], [0xff, 0x3f])];
const PUFF_16: [([i16; 2], [u8; 2]); 4] = [([8, 0], [0x1f, 0x20]), ([-8, 0], [0, 0x20]), ([-8, 16], [0, 0x3f]), ([8, 16], [0x1f, 0x3f])];
const PUFF_19: [([i16; 2], [u8; 2]); 4] = [([8, 0], [0x5f, 0x40]), ([-8, 0], [0x40, 0x40]), ([-8, 16], [0x40, 0x5f]), ([8, 16], [0x5f, 0x5f])];
/// The skid quads' texels (0x800be7d4) and the sparks' (0x800be7f4).
const SKID_UV: [[u8; 2]; 4] = [[0x3f, 0x1f], [0x3f, 0], [0, 0], [0, 0x1f]];
const SPARK_UV: [[u8; 2]; 4] = [[0x5f, 0], [0x40, 0], [0x40, 0x1f], [0x5f, 0x1f]];

/// Skid mark colours by the ground's kind (0x800be864 into 0x800be834) and
/// dust colours (0x800be854 into 0x800be844).
const SKID_COLOUR_OF: [u8; 16] = [0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 3, 3, 0, 0, 0];
const SKID_COLOURS: [[u8; 3]; 8] = [
    [0xee, 0xee, 0xee],
    [0xb8, 0x98, 0x63],
    [0xe8, 0x66, 0x17],
    [0xaa, 0xaa, 0xaa],
    [0xee, 0xee, 0xee],
    [0xb8, 0x98, 0x63],
    [0xe8, 0x66, 0x17],
    [0xee, 0xee, 0xee],
];
const DUST_COLOUR_OF: [u8; 16] = [1, 0, 0, 0, 2, 0, 1, 1, 0, 0, 0, 3, 3, 0, 0, 0];
const DUST_COLOURS: [[u8; 3]; 4] = [[0xee, 0xee, 0xee], [0xb8, 0x98, 0x63], [0xe8, 0x66, 0x17], [0xee, 0xee, 0xee]];

/// 0x8001eb7c: frames a second (4.12, plus a half) for a frame of
/// `frame_ms`.
pub fn fps(frame_ms: u32) -> i32 {
    div_fx(0x3e_8000, (frame_ms as i32) << 12).wrapping_add(2048)
}

impl Effects {
    /// 0x8002bdb4: every ring empty.
    pub fn new() -> Effects {
        Effects {
            pools: std::array::from_fn(Pool::new),
            frame_count: 0,
            trail_tick: 1,
            trails: [[Trail::default(); 6]; 6],
            calls_frame: 0,
            calls: 0,
            origin_set: false,
            origin: [0; 3],
            embers: [Ember::default(); 20],
            columns: [Column { frame: -1, ticks: 0xffff, ..Column::default() }; 5],
            flash: [0; 2],
            chunk_colour: [176; 3],
            root_colour: [0x80_8080; 6],
            wrecked: [false; 6],
            flames: [Flame::default(); 6],
            pulse: [(-1, 0); 6],
        }
    }

    /// Whether car `slot`'s model is blackened.
    pub fn charred(&self, slot: usize) -> bool {
        self.root_colour.get(slot) == Some(&0x18_1818)
    }

    /// 0x8002e51c: car `slot`'s model blackened, or grey again.
    fn set_charred(&mut self, slot: u8, on: bool) {
        if let Some(c) = self.root_colour.get_mut(slot as usize) {
            *c = if on { 0x18_1818 } else { 0x80_8080 };
        }
    }

    /// 0x8002af60: a turbo lights car `slot`'s boost flame (a player's car
    /// that is shown).
    pub fn flame_start(&mut self, slot: u8, human: bool, shown: bool) {
        if human && shown && let Some(f) = self.flames.get_mut(slot as usize) {
            *f = Flame { on: true, start: self.frame_count, count: 0 };
        }
    }

    /// 0x8002aff4 (with 0x8002bd48): the flame out, the colour pulse
    /// stopped (grey again unless wrecked), and the wreck mark cleared.
    fn flame_stop(&mut self, slot: u8) {
        let s = slot as usize;
        if s >= 6 {
            return;
        }
        self.pulse[s].0 = -1;
        if !self.wrecked[s] {
            self.root_colour[s] = 0x80_8080;
        }
        self.wrecked[s] = false;
        self.flames[s].on = false;
        self.flames[s].count = 0;
    }

    /// 0x8002bc04: the colour pulse, a step every 20 ms of the system
    /// clock `now`: grey from 128 to 248 and back over 32 steps.
    fn pulse_step(&mut self, slot: usize, now: u32) {
        let (phase, last) = &mut self.pulse[slot];
        if *phase < 0 {
            *last = now;
            *phase = 0;
            return;
        }
        let dt = if now < *last { *last - now } else { now - *last };
        let steps = dt / 20;
        if steps == 0 {
            return;
        }
        *last = last.wrapping_add(steps * 20);
        let mut ph = (*phase as u32).wrapping_add(steps) & 0x1f;
        *phase = ph as i32;
        if ph & 0x10 != 0 {
            ph = 31 - ph;
        }
        let c = ph * 8 + 128;
        self.root_colour[slot] = c << 16 | c << 8 | c;
    }

    /// 0x8002b05c: car `slot`'s boost flame, for 97 frames from its start:
    /// on each side a body and a tip, flickering in length (a random draw
    /// each, unless paused) and fading, behind the car (`pose`, its
    /// handling `h`, car number `car_id`, and the exhaust table), while the
    /// car's colour pulses.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_flame(&mut self, t: &crate::math::Tables, rand: &mut Rand, slot: u8, pose: &CarPose, h: &crate::car::handling::Handling, exhaust: [i8; 3], special: bool, paused: bool, now: u32) -> Vec<EffectQuad> {
        let s = slot as usize;
        let mut out = Vec::new();
        if s >= 6 || !self.flames[s].on || self.wrecked[s] {
            return out;
        }
        if self.frame_count.wrapping_sub(self.flames[s].start) >= 97 {
            self.flame_stop(slot);
            return out;
        }
        if !paused {
            self.pulse_step(s, now);
            self.flames[s].count = self.flames[s].count.wrapping_add(1);
        }
        let k = if h.wheel_count < 5 { 2 } else { 4 };
        let (width, diameter) = (h.widths[k], h.diameters[k]);
        let (wa, wb) = (h.mounts[k], h.mounts.get(k + 1).copied().unwrap_or_default());
        let (clut, tpage) = sheet(t, 15);
        let col = 112u8.wrapping_sub(self.flames[s].count);
        for q in 0..4 {
            let mut off = [0i32; 3];
            off[0] = if q & 2 != 0 {
                (h.size[0] >> 1).wrapping_add((exhaust[0] as i32) << 12)
            } else {
                (h.size[0].wrapping_neg() >> 1).wrapping_sub((exhaust[0] as i32) << 12)
            };
            let mut len = -fx(7000, 0x1_e000);
            if !paused {
                let r = rand.below(1375) as i32;
                len = fx(len, ((r << 12) / 1000) + 512);
            }
            let l = (len >> 12) as i16;
            let (verts, uv): ([[i16; 3]; 4], [[u8; 2]; 4]) = if q & 1 != 0 {
                let w = (if q & 2 != 0 { width.wrapping_neg() } else { width } >> 12) as i16;
                ([[0, 0, 0], [w, 0, 0], [w, l, 0], [0, l, 0]], [[0xff, 0x5f], [0xff, 0x40], [0x80, 0x40], [0x80, 0x5f]])
            } else {
                off[0] = if q & 2 != 0 {
                    wa[0].wrapping_add(if special { 29952 } else { 1280 })
                } else {
                    wb[0].wrapping_sub(if special { 17664 } else { 1280 })
                };
                let z = ((diameter - 20480) >> 12) as i16;
                ([[0, 0, 0], [0, 0, z], [0, l, 0], [0, l, 0]], [[0xff, 0x5f], [0xff, 0x40], [0x80, 0x40], [0x80, 0x5f]])
            };
            off[1] = off[1].wrapping_add((exhaust[1] as i32) << 12);
            off[2] = off[2].wrapping_add((exhaust[2] as i32) << 12);
            let corners = verts.map(|v| {
                let local = [0, 1, 2].map(|i| off[i].wrapping_add((v[i] as i32) << 12));
                crate::math::add(pose.at, crate::math::apply_matrix_lv(&pose.rot, local))
            });
            out.push(EffectQuad { corners, uv, clut, tpage, colour: [col; 3], semi: true });
        }
        out
    }

    /// 0x80030fc8: a puff at `pos` (sprite growth from `frame0`): dust the
    /// colour of the ground of kind `surface`, drifting at random (or by
    /// `vel`) and rising, 750 frames; on kind 0, grey smoke for 250.
    pub fn dust(&mut self, rand: &mut Rand, fps: i32, pos: Vec3, frame0: u8, surface: u8, vel: Option<Vec3>) {
        if ENABLED & 1 == 0 || fps <= FPS_FLOOR {
            return;
        }
        let (r1, r2) = (rand.rand(), rand.rand());
        self.dust_with(r1, r2, pos, frame0, surface, vel);
    }

    /// The puff of 0x80030fc8 from its two random numbers.
    fn dust_with(&mut self, r1: u32, r2: u32, pos: Vec3, frame0: u8, surface: u8, vel: Option<Vec3>) {
        let p = &mut self.pools[PUFFS];
        let i = p.alloc();
        let buf = p.records[i].buf;
        let v = vel.unwrap_or([((r1 & 3) << 12) as i32, ((r2 & 3) << 12) as i32, ((r1 & 1) << 12) as i32]);
        if let Some(vels) = &mut p.vel {
            vels[buf] = v;
        }
        if let Some(acc) = &mut p.accel {
            acc[buf] = [0, 0, 10];
        }
        let c = DUST_COLOURS[DUST_COLOUR_OF[surface as usize & 15] as usize];
        let r = &mut p.records[i];
        r.pos = pos;
        r.flags = 0x1001;
        r.frame = frame0;
        r.kind = 0;
        r.colour = c.map(u16::from);
        r.life = 750;
        if surface == 0 {
            r.life = 250;
            r.colour = [80; 3];
            r.kind = 1;
            r.flags |= 0x100;
        }
    }

    /// 0x800303cc: a skid mark through the trail's last and new edge
    /// points, coloured by the ground, kept until its slot is wanted.
    fn skid(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, surface: u8) {
        if !self.origin_set {
            self.origin = a;
            self.origin_set = true;
        }
        let o = self.origin;
        let p = &mut self.pools[SKIDS];
        let i = p.alloc();
        let buf = p.records[i].buf;
        let rel = |v: Vec3| [0, 1, 2].map(|k| v[k].wrapping_sub(o[k]));
        p.corners[buf] = [rel(a), rel(b), rel(d), rel(c)];
        let r = &mut p.records[i];
        r.pos = o;
        r.flags = if surface.wrapping_sub(11) < 2 { 0x806 } else { 6 };
        r.life = 0xffff;
        r.kind = 2;
        r.colour = SKID_COLOURS[SKID_COLOUR_OF[surface as usize & 15] as usize].map(u16::from);
    }

    /// 0x8002c32c's record: a spark flying straight for 90 frames.
    pub fn spark(&mut self, s: Spark) {
        let p = &mut self.pools[SPARKS];
        let i = p.alloc();
        let buf = p.records[i].buf;
        if let Some(vels) = &mut p.vel {
            vels[buf] = s.vel;
        }
        let r = &mut p.records[i];
        r.pos = s.pos;
        r.flags = 0x20;
        r.frame = 4;
        r.life = 90;
        r.kind = 255;
    }

    /// 0x8003119c: an ember's spark, still, 40 frames.
    fn spark_puff(&mut self, fps: i32, pos: Vec3, frame0: u8) {
        if ENABLED & 1 == 0 || fps <= FPS_FLOOR {
            return;
        }
        let p = &mut self.pools[PUFFS];
        let i = p.alloc();
        let buf = p.records[i].buf;
        if let Some(v) = &mut p.vel {
            v[buf] = [0; 3];
        }
        if let Some(a) = &mut p.accel {
            a[buf] = [0; 3];
        }
        let r = &mut p.records[i];
        r.pos = pos;
        r.flags = 0x1101;
        r.kind = 19;
        r.colour = [128; 3];
        r.frame = frame0;
        r.life = 40;
    }

    /// 0x8002d800: twenty embers from `pos`, each thrown a random way at
    /// 8 to 38 units a frame and turned a little more every frame, 40
    /// frames; seven random numbers each.
    fn embers_spawn(&mut self, t: &crate::math::Tables, pos: Vec3, rands: &mut impl Iterator<Item = u32>) {
        let mut next = || rands.next().unwrap_or(0);
        for e in &mut self.embers {
            e.life = 40;
            e.age = 0;
            e.pos = pos;
            let mut v = [
                div_fx(((next() & 0xff) << 12) as i32, 0x8_0000).wrapping_sub(4096),
                div_fx(((next() & 0xff) << 12) as i32, 0x8_0000).wrapping_sub(4096),
                2867,
            ];
            let len = t.length(v);
            v = v.map(|c| div_fx(c, len));
            let speed = fx(((next() & 15) << 12) as i32, 8192).wrapping_add(0x8000);
            e.vel = v.map(|c| fx(c, speed));
            let bits = next();
            let mut m: Matrix = [[4096, 0, 0], [0, 4096, 0], [0, 0, 4096]];
            for (axis, bit) in [(2, 1), (1, 2), (0, 4)] {
                let mut a = (((next() & 63) << 12) as i32) / 1024;
                if bits & bit != 0 {
                    a = -a;
                }
                m = crate::math::gte_mul(&m, &t.rot_axis(axis, ((a << 12) / 25736) & 0xfff));
            }
            e.turn = m;
        }
    }

    /// 0x8002d70c (and the flash it draws first, 0x8002e128): each live
    /// ember moves, its velocity turns, and it leaves a spark. Runs paused
    /// or not.
    fn embers_update(&mut self, fps: i32) {
        for f in &mut self.flash {
            if *f != 0 {
                *f = f.wrapping_sub(3);
                if *f < 130 {
                    *f = 0;
                }
            }
        }
        for k in 0..self.embers.len() {
            let e = &mut self.embers[k];
            e.life -= 1;
            if e.life <= 0 {
                e.life = 0;
                continue;
            }
            e.age = e.age.wrapping_add(1);
            e.pos = [0, 1, 2].map(|i| e.pos[i].wrapping_add(e.vel[i]));
            e.vel = crate::math::apply_matrix_lv(&e.turn, e.vel);
            let (pos, frame) = (e.pos, e.age.wrapping_add(10));
            self.spark_puff(fps, pos, frame);
        }
    }

    /// 0x8003063c: five smoke columns from `pos`, three frames apart, each
    /// 20 frames, drifting by an eighth of `vel` (kind 0) or following car
    /// `slot` (kind 1); two random numbers each.
    fn columns_spawn(&mut self, pos: Vec3, vel: Vec3, kind: u8, slot: u8, rands: &mut impl Iterator<Item = u32>) {
        let mut next = || rands.next().unwrap_or(0);
        for (i, c) in self.columns.iter_mut().enumerate() {
            let r1 = next() % 10_000;
            let jx = div_fx((r1 << 12) as i32, 0x3_2000).wrapping_sub(50 << 12);
            let jy = ((next() % 25) << 12) as i32;
            let mut start = self.frame_count.wrapping_add(3 * i as u32);
            start = start.wrapping_add((start == u32::MAX) as u32);
            *c = Column {
                loops: 0,
                loop_end: 6,
                loop_restart: 4,
                pos,
                vel,
                colour: [238; 3],
                jitter: [jx, jy],
                start,
                end: start.wrapping_add(19),
                ticks: 0,
                frame: 5,
                per: 2,
                add: 60,
                kind,
                slot,
            };
        }
    }

    /// 0x8002c5dc: a chunk's flight from `pos` with a fourteenth of `vel`
    /// plus a random push as strong as that, falling 2 units a frame², and
    /// its random turning; fifteen random numbers. Every chunk's colour
    /// is reset.
    fn chunk_init(&mut self, t: &crate::math::Tables, pos: Vec3, vel: Vec3, i: usize, rands: &mut impl Iterator<Item = u32>) {
        let mut next = || rands.next().unwrap_or(0);
        self.chunk_colour = [176; 3];
        let d = vel.map(|c| div_fx(c, 0xe000));
        let s2 = fx(d[0], d[0]).wrapping_add(fx(d[1], d[1])).wrapping_add(fx(d[2], d[2]));
        let speed = t.sqrt_steps(s2) << 6;
        let jit = |r: u32| div_fx((((r & 127) as i32) - 64) << 12, 0x8_0000);
        let p = [pos[0].wrapping_add(jit(next())), pos[1].wrapping_add(jit(next())), pos[2].wrapping_add(jit(next()))];
        let push = |r: u32| fx(div_fx((((r & 127) as i32) - 64) << 12, 0x7_f000), speed);
        let v = [d[0].wrapping_add(push(next())), d[1].wrapping_add(push(next())), d[2].wrapping_add(push(next()))];
        let bits = next();
        let mut m: Matrix = [[4096, 0, 0], [0, 4096, 0], [0, 0, 4096]];
        let turn = |m: &mut Matrix, axis: usize, scale: i32, neg: bool, r: u32| {
            let mut a = div_fx(((r & 63) << 12) as i32, scale);
            if neg {
                a = -a;
            }
            *m = crate::math::gte_mul(m, &t.rot_axis(axis, ((a << 12) / 25736) & 0xfff));
        };
        for (axis, bit) in [(2, 1), (1, 2), (0, 4)] {
            turn(&mut m, axis, 0x2_0000, bits & bit != 0, next());
        }
        let base = crate::math::matrix_to_quat(t, &m);
        for (axis, bit) in [(2, 16), (1, 32), (0, 64)] {
            turn(&mut m, axis, 0x1_0000, bits & bit != 0, next());
        }
        let axis = m[0].map(i32::from);
        let mut angle = div_fx(((next() & 63) << 12) as i32, 0x8_0000);
        if bits & 8 != 0 {
            angle = -angle;
        }
        let mut step = div_fx(((next() & 63) << 12) as i32, 0x8_0000);
        if bits & 16 != 0 {
            step = -step;
        }
        let pool = &mut self.pools[DEBRIS];
        let buf = pool.records[i].buf;
        if let Some(vels) = &mut pool.vel {
            vels[buf] = v;
        }
        if let Some(acc) = &mut pool.accel {
            acc[buf] = [0, 0, -8192];
        }
        if let Some(c) = pool.chunks.get_mut(buf) {
            c.base = base;
            c.axis = axis;
            c.angle = angle;
            c.step = step;
        }
        let r = &mut pool.records[i];
        r.pos = p;
        r.frame = 0;
    }

    /// 0x80029e10 mode 0 with 0x8002e574: car `draws.slot`'s wreck from
    /// `pose`: five smoke columns following it, its model blackened, a
    /// player's screen flashing, twenty embers, and its model's faces
    /// (`faces`, up to 48) flying off as chunks with an eighth of its
    /// velocity.
    pub fn car_wreck(&mut self, t: &crate::math::Tables, draws: &WreckDraws, pose: &CarPose, faces: &[ChunkFace]) {
        let mut rands = draws.rands.iter().copied();
        let centre = pose.centre();
        self.flame_stop(draws.slot);
        if let Some(w) = self.wrecked.get_mut(draws.slot as usize) {
            *w = true;
        }
        self.columns_spawn(centre, draws.vel, 1, draws.slot, &mut rands);
        self.set_charred(draws.slot, true);
        if draws.human && let Some(f) = self.flash.get_mut(draws.slot as usize) {
            *f = 160;
        }
        self.embers_spawn(t, centre, &mut rands);
        let vel = draws.vel.map(|c| div_fx(c, 0x8000));
        for face in faces.iter().take(48) {
            let i = self.pools[DEBRIS].alloc();
            self.pools[DEBRIS].records[i].semi = true;
            self.chunk_init(t, centre, vel, i, &mut rands);
            let p = &mut self.pools[DEBRIS];
            let buf = p.records[i].buf;
            if let Some(c) = p.chunks.get_mut(buf) {
                c.verts = face.verts.map(|v| v.map(|x| x.wrapping_shl(1)));
                c.uv = face.uv;
            }
            let r = &mut p.records[i];
            r.flags = 0xd8;
            r.life = 20;
            r.kind = 255;
            r.clut = face.clut;
            r.tpage = face.tpage;
        }
    }

    /// 0x8002e27c: a knocked prop (flags `flags`, centre `pos`, height
    /// `height`): smoke columns from 100 units above its foot (flag 2),
    /// a ring of ten puffs of dust around its foot (flag 1), and unless
    /// flag 0x20 its quads (`quads`, up to 48) thrown off as chunks that
    /// last 180 frames.
    pub fn prop_debris(&mut self, t: &crate::math::Tables, draws: &PropDraws, flags: u32, pos: Vec3, height: i32, quads: &[ChunkFace]) {
        let mut rands = draws.rands.iter().copied();
        if flags & 2 != 0 {
            let p = [pos[0], pos[1], pos[2].wrapping_sub(height >> 1).wrapping_add(0x6_4000)];
            self.columns_spawn(p, [0; 3], 0, 255, &mut rands);
        }
        if flags & 1 != 0 {
            for n in 0..10u32 {
                let sign = if n & 1 != 0 { 4096 } else { -4096 };
                let r1 = rands.next().unwrap_or(0) % 100;
                let r2 = rands.next().unwrap_or(0) % 100;
                let p = [
                    pos[0].wrapping_add(fx(((r1 << 12) as i32).wrapping_add(0x3_2000), sign)),
                    pos[1].wrapping_add(fx(((r2 << 12) as i32).wrapping_add(0x3_2000), sign)),
                    pos[2].wrapping_sub(0x6_4000),
                ];
                let (frame0, surface) = if flags & 0x20 != 0 { ((rands.next().unwrap_or(0) % 100) as u8, 2) } else { (80, 6) };
                let (a, b) = (rands.next().unwrap_or(0), rands.next().unwrap_or(0));
                self.dust_with(a, b, p, frame0, surface, None);
            }
        }
        if flags & 0x20 != 0 {
            return;
        }
        for q in quads.iter().take(48) {
            let i = self.pools[DEBRIS].alloc();
            self.pools[DEBRIS].records[i].semi = true;
            self.chunk_init(t, pos, draws.vel, i, &mut rands);
            let p = &mut self.pools[DEBRIS];
            let buf = p.records[i].buf;
            if let Some(c) = p.chunks.get_mut(buf) {
                c.verts = q.verts;
                c.uv = q.uv;
            }
            let r = &mut p.records[i];
            r.flags = 0x98;
            r.life = 180;
            r.kind = 255;
            r.clut = q.clut;
            r.tpage = q.tpage;
        }
    }

    /// 0x80029e10 mode 2: a car shown wrecked (a snapshot put back).
    pub fn car_charred(&mut self, slot: u8) {
        self.flame_stop(slot);
        if let Some(w) = self.wrecked.get_mut(slot as usize) {
            *w = true;
        }
        self.set_charred(slot, true);
    }

    /// 0x80029f04: the puffs and sparks of every car gone, the smoke
    /// columns idle, car `slot` its own colour again.
    pub fn car_reset(&mut self, slot: u8) {
        for k in [PUFFS, SPARKS] {
            for r in &mut self.pools[k].records {
                r.life = 0;
            }
        }
        for c in &mut self.columns {
            c.frame = -1;
        }
        if self.wrecked.get(slot as usize) == Some(&true) {
            self.set_charred(slot, false);
        }
        if let Some(w) = self.wrecked.get_mut(slot as usize) {
            *w = false;
        }
    }

    /// 0x8002f354 (less the wreck's embers): every record older by a
    /// frame and moved; the dead dropped from the old end, or overwritten
    /// by the record before them.
    pub fn update(&mut self, paused: bool, fps: i32) {
        self.embers_update(fps);
        for p in &mut self.pools {
            let mut i = p.oldest;
            loop {
                if i == p.cap() {
                    i = 0;
                }
                if i == p.next {
                    break;
                }
                if p.records[i].life != 0 {
                    if !paused {
                        let r = &mut p.records[i];
                        r.life -= 1;
                        let buf = r.buf;
                        if let Some(vels) = &mut p.vel {
                            let v = vels[buf];
                            r.pos = [0, 1, 2].map(|k| r.pos[k].wrapping_add(v[k]));
                            if let Some(acc) = &p.accel {
                                let a = acc[buf];
                                vels[buf] = [0, 1, 2].map(|k| v[k].wrapping_add(a[k]));
                            }
                        }
                        if r.flags & 1 != 0 || r.flags & 0x20 != 0 {
                            if r.kind == 0 || r.kind == 19 {
                                if r.life & 2 != 0 {
                                    r.frame = r.frame.wrapping_add(7);
                                }
                            } else if r.life & 2 != 0 && r.kind == 1 {
                                r.frame = r.frame.wrapping_add(4);
                            }
                        }
                        if r.flags & 0x10 != 0
                            && let Some(c) = p.chunks.get_mut(buf)
                        {
                            c.angle = c.angle.wrapping_add(c.step);
                        }
                    }
                } else if i == p.oldest {
                    p.oldest = if i + 1 == p.cap() { 0 } else { i + 1 };
                } else {
                    let j = if i != 0 { i } else { p.cap() } - 1;
                    p.records[i] = p.records[j];
                    p.records[j].life = 0;
                }
                i += 1;
            }
        }
    }

    /// 0x8002b888: each wheel's ready trail segment becomes a skid mark,
    /// and its inner edge puffs dust (a car not shown leaves none from
    /// its front wheels). `shown` is each car's slot's visibility.
    pub fn emit_trails(&mut self, rand: &mut Rand, fps: i32, shown: &[bool]) {
        if self.trail_tick != 2 {
            return;
        }
        for car in 0..6 {
            let Some(&visible) = shown.get(car) else { continue };
            for w in 0..6 {
                if !visible && w < 2 {
                    continue;
                }
                let t = self.trails[car][w];
                if t.state != 2 {
                    continue;
                }
                let mut inner = t.inner;
                if ENABLED & 2 != 0 {
                    self.skid(t.outer, t.inner, t.new_outer, t.new_inner, t.surface);
                    let t = &mut self.trails[car][w];
                    t.outer = t.new_outer;
                    t.inner = t.new_inner;
                    t.state = 1;
                    inner = t.inner;
                }
                inner[2] = inner[2].wrapping_add(0xa000);
                self.dust(rand, fps, inner, 0, t.surface, None);
            }
        }
    }

    /// 0x80029230: car `car`'s wheel `wheel` this frame: skidding (`on`)
    /// on ground of kind `surface`, its trail's edge points.
    pub fn trail_push(&mut self, car: usize, wheel: usize, wheels: usize, on: bool, surface: u8, outer: Vec3, inner: Vec3) {
        let (Some(row), true) = (self.trails.get_mut(car), wheel < 6) else { return };
        let t = &mut row[wheel];
        t.surface = surface;
        if self.calls_frame != self.frame_count {
            self.calls_frame = self.frame_count;
            self.calls = 0;
        }
        self.calls = self.calls.wrapping_add(1);
        if self.calls == wheels as u32 && !on {
            self.origin_set = false;
        }
        if !on {
            t.state = 0;
            t.count = t.count.saturating_sub(1);
            return;
        }
        if t.count != 80 {
            t.count += 1;
        }
        if t.state == 0 {
            (t.outer, t.inner, t.state) = (outer, inner, 1);
        } else {
            (t.new_outer, t.new_inner, t.state) = (outer, inner, 2);
        }
        if self.trail_tick == 1 {
            self.trail_tick = 2;
        }
    }

    /// The world draw's end: the trail tick runs down to 1 (0x8001ff20);
    /// and the frame counted unless paused (0x80014d64).
    pub fn frame_done(&mut self, paused: bool) {
        self.trail_tick = self.trail_tick.saturating_sub(1).max(1);
        if !paused {
            self.frame_count = self.frame_count.wrapping_add(1);
        }
    }

    /// 0x80049ecc's trails: for each car within 600 units of a camera
    /// (`eyes`, on every axis), each wheel's edge points: its contact, and
    /// while it skids on the ground (not the frame after a reset) the
    /// contact moved its tyre's half width across it (out on wheels 0, 2
    /// and 4, in on the others). On ground of kind 10 none is kept. The
    /// reset's mark is cleared for every car.
    pub fn car_pose(&mut self, cars: &mut [Car], eyes: &[Vec3]) {
        let near = fx(0xc_8000, 0xc000);
        for car in cars.iter_mut() {
            let centre = crate::math::add(car.body.pos, car.body.centre);
            let seen = eyes.iter().any(|e| (0..3).map(|k| centre[k].wrapping_sub(e[k]).wrapping_abs()).max().unwrap_or(0) < near);
            if seen {
                let wheels = car.wheels.len();
                for (k, w) in car.wheels.iter().enumerate() {
                    let (h, n) = (w.heading, w.normal);
                    let mut side = [
                        fx(h[1], n[2]).wrapping_sub(fx(n[1], h[2])),
                        fx(n[0], h[2]).wrapping_sub(fx(h[0], n[2])),
                        fx(h[0], n[1]).wrapping_sub(fx(n[0], h[1])),
                    ];
                    let on = w.slipping && w.on_ground && !car.just_reset;
                    if on {
                        side = side.map(|c| fx(c, w.width));
                        side = if matches!(k, 0 | 2 | 4) {
                            [0, 1, 2].map(|i| w.contact[i].wrapping_add(side[i]))
                        } else {
                            [0, 1, 2].map(|i| w.contact[i].wrapping_sub(side[i]))
                        };
                    }
                    if w.surface != 10 {
                        self.trail_push(car.slot as usize, k, wheels, on, w.surface, w.contact, side);
                    }
                }
            }
            car.just_reset = false;
        }
    }

    /// 0x8002f618 for each pool: the quads, `cam` the camera's axes (the
    /// puffs face it). Puffs fade 7 a frame (grey smoke 3 every third
    /// frame; all 21 below 22 frames a second) and die at nothing.
    pub fn draw(&mut self, t: &crate::math::Tables, cam: &Matrix, fps: i32, paused: bool) -> Vec<EffectQuad> {
        let mut out = Vec::new();
        let mut orient: Matrix = [[4096, 0, 0], [0, 4096, 0], [0, 0, 4096]];
        let right = column(cam, 0);
        let down = column(cam, 2).map(i32::wrapping_neg);
        let frame_count = self.frame_count;
        for k in [PUFFS, SKIDS, SPARKS, DEBRIS] {
            let mut fade: u16 = 0;
            let mut tmpl = PUFF_0;
            let p = &mut self.pools[k];
            for i in p.slots() {
                let buf = p.records[i].buf;
                let r = &mut p.records[i];
                if r.flags & 1 == 0 && r.flags & 8 != 0
                    && let Some(c) = p.chunks.get(buf)
                {
                    let a = crate::math::quat_to_matrix(crate::math::quat_from_axis_angle(t, c.angle, c.axis));
                    let b = crate::math::quat_to_matrix(c.base);
                    orient = std::array::from_fn(|row| {
                        std::array::from_fn(|col| (0..3).fold(0i32, |s, k| s.wrapping_add(fx(a[row][k] as i32, b[k][col] as i32))) as i16)
                    });
                }
                if r.flags & 0x1000 != 0 {
                    tmpl = match r.kind {
                        1 => PUFF_1,
                        0 => PUFF_0,
                        16 => PUFF_16,
                        19 => PUFF_19,
                        _ => tmpl,
                    };
                    let (clut, tpage) = sheet(t, r.kind as usize % 24);
                    if !paused {
                        if fps <= FPS_FLOOR {
                            fade = 21;
                        } else if r.kind != 1 {
                            fade = 7;
                        } else if frame_count % 3 == 0 {
                            fade = 3;
                        }
                        r.colour = r.colour.map(|c| c.wrapping_sub(fade));
                        if r.colour.iter().any(|&c| c as i16 <= 0) {
                            r.life = 0;
                            continue;
                        }
                    }
                    let grow = r.frame as i16;
                    let accel = p.accel.as_ref().map_or([0; 3], |a| a[buf]);
                    let mut offs = tmpl.map(|(o, _)| o);
                    offs[0][0] = offs[0][0].wrapping_add(grow);
                    offs[2][1] = offs[2][1].wrapping_add(grow);
                    offs[3][0] = offs[3][0].wrapping_add(grow);
                    offs[3][1] = offs[3][1].wrapping_add(grow);
                    let corners = offs.map(|[x, y]| {
                        let (x, y) = (((x as i32) << 12).wrapping_add(accel[0]), ((y as i32) << 12).wrapping_add(accel[1]));
                        [0, 1, 2].map(|c| r.pos[c].wrapping_add(fx(right[c], x)).wrapping_add(fx(down[c], y)))
                    });
                    out.push(EffectQuad {
                        corners,
                        uv: tmpl.map(|(_, uv)| uv),
                        clut,
                        tpage,
                        colour: r.colour.map(|c| c as u8),
                        semi: true,
                    });
                } else if r.kind == 2 {
                    let c = p.corners.get(buf).copied().unwrap_or_default();
                    let corners = c.map(|v| [0, 1, 2].map(|i| (v[i].wrapping_add(r.pos[i]) >> 12) << 12));
                    let (clut, mut tpage) = sheet(t, 2);
                    if r.flags & 0x800 != 0 {
                        tpage = (tpage & 0xff9f) | 0x20;
                    }
                    out.push(EffectQuad { corners, uv: SKID_UV, clut, tpage, colour: r.colour.map(|c| c as u8), semi: true });
                } else if r.flags & 0x20 != 0 {
                    let v = p.vel.as_ref().map_or([0; 3], |v| v[buf]).map(|c| c >> 12);
                    let rel = [[2, 0, 0], [0, 0, 0], v, [v[0].wrapping_add(2), v[1], v[2]]];
                    let corners = rel.map(|q| [0, 1, 2].map(|i| (q[i].wrapping_add(r.pos[i] >> 12)) << 12));
                    let (clut, tpage) = sheet(t, 3);
                    out.push(EffectQuad { corners, uv: SPARK_UV, clut, tpage, colour: [128; 3], semi: false });
                } else if r.flags & 0x80 != 0 {
                    let c = p.chunks.get(buf).copied().unwrap_or_default();
                    if !paused {
                        if r.semi {
                            self.chunk_colour = self.chunk_colour.map(|v| v.wrapping_sub(3));
                        }
                        if self.chunk_colour[0] == 0 {
                            r.life = 0;
                            continue;
                        }
                    }
                    let corners = c.verts.map(|v| {
                        let v = v.map(|x| (x as i32) << 12);
                        [0, 1, 2].map(|i| {
                            (0..3).fold(r.pos[i], |s, k| s.wrapping_add(fx(orient[i][k] as i32, v[k])))
                        })
                    });
                    out.push(EffectQuad { corners, uv: c.uv, clut: r.clut, tpage: r.tpage, colour: self.chunk_colour, semi: r.semi });
                }
            }
        }
        out
    }

    /// 0x800307d0: the smoke columns: each running one (from its start
    /// frame) drifts or follows its car (`poses`), is drawn as a billboard
    /// growing 4 units a step from the sheet's ten smoke frames, steps
    /// every second frame, sheds grey smoke in its last two steps, and
    /// stops at its end frame.
    pub fn draw_columns(&mut self, t: &crate::math::Tables, rand: &mut Rand, cam: &Matrix, fps: i32, paused: bool, poses: &[CarPose]) -> Vec<EffectQuad> {
        const OFFSETS: [[i32; 2]; 5] = [[0, -0xa000], [0x4_b000, 0xa000], [-0x4_b000, 0xf000], [0x3_2000, 0x4_1000], [-0x3_2000, 0x4_1000]];
        const U: [u8; 10] = [0x80, 0xc0, 0, 0x40, 0x80, 0xc0, 0, 0x40, 0x80, 0xc0];
        let right = column(cam, 0);
        let down = column(cam, 2).map(i32::wrapping_neg);
        let mut out = Vec::new();
        for i in 0..5 {
            let c = self.columns[i];
            if c.frame < 0 || self.frame_count < c.start {
                continue;
            }
            let mut c = c;
            if !paused {
                if c.kind == 0 {
                    c.pos = [0, 1, 2].map(|k| c.pos[k].wrapping_add(c.vel[k] >> 3));
                } else if c.kind == 1 && let Some(p) = poses.get(c.slot as usize) {
                    c.pos = p.centre();
                }
            }
            let st = c.frame as i32 - 5;
            let size = 128 + c.add as i32 + 4 * st;
            let (hx, hy) = ((size >> 1) << 12, (size >> 1) << 12);
            let offs = [[size, 0], [0, 0], [0, size], [size, size]];
            let us = [0xbfu8, 0x80, 0x80, 0xbf].map(|u| U[st as usize % 10].wrapping_add(u.wrapping_sub(128)));
            let vs = [0u8, 0, 0x3f, 0x3f].map(|v| if st < 6 { v } else { v + 64 });
            let shift = [OFFSETS[i][0].wrapping_add(c.jitter[0]).wrapping_sub(hx), OFFSETS[i][1].wrapping_add(c.jitter[1]).wrapping_sub(hy)];
            let corners = offs.map(|[x, y]| {
                let (x, y) = (((x as i32) << 12).wrapping_add(shift[0]), ((y as i32) << 12).wrapping_add(shift[1]));
                [0, 1, 2].map(|k| c.pos[k].wrapping_add(fx(right[k], x)).wrapping_add(fx(down[k], y)))
            });
            let (clut, tpage) = sheet(t, c.frame as usize);
            out.push(EffectQuad { corners, uv: std::array::from_fn(|k| [us[k], vs[k]]), clut, tpage, colour: [128; 3], semi: true });
            if !paused {
                c.ticks = c.ticks.wrapping_add(1);
                if c.per != 0 && (c.ticks as i16) % (c.per as i16) == 0 {
                    let mut s = c.frame as i32 - 5;
                    if c.loops > 0 && c.loop_end as i32 + 1 == s {
                        s = c.loop_restart as i32;
                        c.loops -= 1;
                    } else {
                        s = (s + 1) % 10;
                    }
                    c.frame = (5 + s) as i8;
                }
                c.colour = c.colour.map(|v| v.wrapping_sub(4));
            }
            self.columns[i] = c;
            if c.frame >= 13 {
                self.dust(rand, fps, c.pos, 40, 0, None);
            }
            if self.frame_count >= c.end {
                self.columns[i].frame = -1;
            }
        }
        out
    }
}

/// 0x8002e9f8 and 0x8002c32c's start: a spark off a player's car's first
/// contact of a step (not one with a road's side, kind 1), if its body
/// moves at over 100 units a second: thrown back along the surface at a
/// fifteenth of the car's sliding speed, plus an inch a frame up, from a
/// random point of the box's face the contact is on (its rotation `rot`,
/// half size `half`), or the contact point; jittered 5 units either way.
pub fn contact_spark(rand: &mut Rand, t: &crate::math::Tables, body: &crate::body::Body, half: Vec3, point: Vec3, normal: Vec3, surface: u8) -> Option<Spark> {
    if surface == 1 {
        return None;
    }
    let mut v = body.vel;
    if t.length(v) <= 0x6_4000 {
        return None;
    }
    let n = normal;
    let d = crate::math::dot(v, n);
    v = [0, 1, 2].map(|k| v[k].wrapping_sub(fx(n[k], d)));
    v = [v[0].wrapping_add(fx(0xa000, n[0])), v[1].wrapping_add(fx(0xa000, n[1])), v[2].wrapping_add(fx(0x3_c000, n[2]))];
    let r = body.rot.map(|row| row.map(|c| c as i32));
    let along = |j: usize| fx(r[0][j], n[0]).wrapping_add(fx(r[1][j], n[1])).wrapping_add(fx(r[2][j], n[2]));
    let (ns, nf, nu) = (along(0), along(1), along(2));
    let mut jitter = |extent: i32| fx(((rand.below(2000) << 12) as i32) / 1000 - 4096, extent);
    let to_world = |l: Vec3| [0, 1, 2].map(|i| fx(r[i][0], l[0]).wrapping_add(fx(r[i][1], l[1])).wrapping_add(fx(r[i][2], l[2])).wrapping_add(body.pos[i]));
    let mut w = if ns.wrapping_abs() >= 3687 {
        let x = if ns > 0 { half[0].wrapping_neg() } else { half[0] };
        let y = jitter(half[1]);
        let z = jitter(half[2]);
        to_world([x, y, z])
    } else if nf.wrapping_abs() >= 3687 {
        let y = if nf > 0 { half[1].wrapping_neg() } else { half[1] };
        let x = jitter(half[0]);
        let z = jitter(half[2]);
        to_world([x, y, z])
    } else if nu.wrapping_abs() >= 3687 {
        let z = if nu > 0 { half[2].wrapping_neg() } else { half[2] };
        let x = jitter(half[0]);
        let y = jitter(half[1]);
        let l = crate::math::apply_matrix_lv(&body.rot, [x, y, z]);
        [0, 1, 2].map(|i| l[i].wrapping_add(body.pos[i]))
    } else {
        point
    };
    if ENABLED & 4 == 0 {
        return None;
    }
    w[0] = w[0].wrapping_add(((rand.below(10_000) << 12) as i32) / 1000 - 20480);
    w[1] = w[1].wrapping_add(((rand.below(10_000) << 12) as i32) / 1000 - 20480);
    let back = |c: i32| div_fx(c, 0xffff_1000u32 as i32);
    let vel = [back(v[0]), back(v[1]), back(v[2]).wrapping_add(4096)];
    Some(Spark { pos: w, vel })
}
