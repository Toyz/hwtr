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
}

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
        }
    }

    /// 0x80030fc8: a puff at `pos` (sprite growth from `frame0`): dust the
    /// colour of the ground of kind `surface`, drifting at random (or by
    /// `vel`) and rising, 750 frames; on kind 0, grey smoke for 250.
    pub fn dust(&mut self, rand: &mut Rand, fps: i32, pos: Vec3, frame0: u8, surface: u8, vel: Option<Vec3>) {
        if ENABLED & 1 == 0 || fps <= FPS_FLOOR {
            return;
        }
        let (r1, r2) = (rand.rand(), rand.rand());
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

    /// 0x8002f354 (less the wreck's embers): every record older by a
    /// frame and moved; the dead dropped from the old end, or overwritten
    /// by the record before them.
    pub fn update(&mut self, paused: bool) {
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
                        if r.flags & 0x10 != 0 {
                            tracing::trace!("a debris chunk's spin: not yet ported");
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
                    tracing::trace!("a debris chunk's draw: not yet ported");
                }
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
