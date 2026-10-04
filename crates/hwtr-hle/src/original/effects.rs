//! The original's memory as the effects: the four rings' headers
//! (0x800d25c0, 0x800d25d0, 0x800d25d8, 0x800d25c8: capacity, oldest,
//! next, records), their 56-byte records and side buffers, and the skid
//! trails (0x8011ec2c on), read into and written from [`Effects`]. A
//! record's buffer is the slot its velocity (or corner) pointer points at.

use hwtr_game::effects::{CAPACITY, Chunk, Column, Effects, Ember, Pool, Record, Trail};

use super::Ram;

pub const HEADERS: [u32; 4] = [0x800d_25c0, 0x800d_25d0, 0x800d_25d8, 0x800d_25c8];
pub const RECORDS: [u32; 4] = [0x8011_fb3c, 0x8012_0f3c, 0x8012_293c, 0x8012_2e8c];
const VEL: [u32; 4] = [0x8012_0c3c, 0, 0x8012_2d9c, 0x8012_3b4c];
const ACCEL: [u32; 4] = [0x8012_093c, 0, 0, 0x8012_390c];
const CORNERS: u32 = 0x8012_1d3c;
/// The debris' quads (32 bytes: four of s16 x, y, z, u8 u, v), normals and
/// turning blocks (40 bytes).
const CHUNK_QUADS: u32 = 0x8012_450c;
const CHUNK_NORMALS: u32 = 0x8012_4b0c;
const CHUNK_TURNS: u32 = 0x8012_3d8c;
const EMBERS: u32 = 0x8011_f59c;
const COLUMNS: u32 = 0x8012_510c;
const FLASH: u32 = 0x800d_0db8;
/// Taken off the puffs' growth (28 under the small cars).
const PUFF_SHRINK: u32 = 0x800d_0db6;
const CHUNK_COLOUR: u32 = 0x8012_6c8c + 64;
/// The cars' models (car_get_model 0x80021b58: three a slot); a model's
/// root node is at +4, its colour at +0x44.
pub const MODELS: u32 = 0x8011_d3c0;

/// Car `slot`'s full model, if loaded.
pub fn model(ram: &Ram, slot: u32) -> u32 {
    model_at(ram, slot, 0)
}

/// Car `slot`'s model at detail level `lod` (0 full, 1 medium, 2 low).
pub fn model_at(ram: &Ram, slot: u32, lod: u32) -> u32 {
    ram.i32(MODELS + 12 * slot + 4 * lod) as u32
}
const RECORD: u32 = 56;

pub const FRAME_COUNT: u32 = 0x800d_0b68;
const TRAIL_TICK: u32 = 0x8011_ec2c;
const TRAILS: u32 = 0x8011_ec30;
const STATE: u32 = 0x8011_f530;
const COUNT: u32 = 0x8011_f554;
const SURFACE: u32 = 0x8011_f578;
const CALLS: u32 = 0x800d_0da0;
const CALLS_FRAME: u32 = 0x800d_0da4;
const ORIGIN_SET: u32 = 0x800d_0da8;
const ORIGIN: u32 = 0x8012_7acc;

fn record_at(k: usize, i: usize) -> u32 {
    RECORDS[k] + RECORD * i as u32
}

/// The slot a record's buffers belong to.
fn buf_of(ram: &Ram, k: usize, r: u32, i: usize) -> usize {
    let (ptr, base, size) = match k {
        1 => (ram.i32(r) as u32, CORNERS, 48),
        _ if VEL[k] != 0 => (ram.i32(r + 0x1c) as u32, VEL[k], 12),
        _ => return i,
    };
    (ptr.wrapping_sub(base) / size) as usize
}

pub fn read(ram: &Ram) -> Effects {
    let mut e = Effects::new();
    for k in 0..4 {
        let h = HEADERS[k];
        let mut p = Pool::new(k);
        p.oldest = ram.u8(h + 1) as usize;
        p.next = ram.u8(h + 2) as usize;
        for i in 0..CAPACITY[k] {
            let r = record_at(k, i);
            p.records[i] = Record {
                pos: ram.vec3(r + 0x10),
                flags: ram.i16(r + 0x24) as u16,
                life: ram.i16(r + 0x26) as u16,
                colour: [0, 1, 2].map(|c| ram.i16(r + 0x2c + 2 * c) as u16),
                kind: ram.u8(r + 0x32),
                frame: ram.u8(r + 0x33),
                clut: ram.i16(r + 0x28) as u16,
                tpage: ram.i16(r + 0x2a) as u16,
                semi: ram.u8(r + 0x34) != 0,
                buf: buf_of(ram, k, r, i),
            };
            if k == 3 {
                let (q, c) = (CHUNK_QUADS + 32 * i as u32, CHUNK_TURNS + 40 * i as u32);
                p.chunks[i] = Chunk {
                    verts: std::array::from_fn(|n| std::array::from_fn(|a| ram.i16(q + 8 * n as u32 + 2 * a as u32))),
                    uv: std::array::from_fn(|n| [ram.u8(q + 8 * n as u32 + 6), ram.u8(q + 8 * n as u32 + 7)]),
                    base: std::array::from_fn(|n| ram.i32(c + 4 * n as u32)),
                    axis: ram.vec3(c + 0x10),
                    angle: ram.i32(c + 0x20),
                    step: ram.i32(c + 0x24),
                };
            }
            if let Some(v) = &mut p.vel {
                v[i] = ram.vec3(VEL[k] + 12 * i as u32);
            }
            if let Some(a) = &mut p.accel {
                a[i] = ram.vec3(ACCEL[k] + 12 * i as u32);
            }
            if k == 1 {
                let c = CORNERS + 48 * i as u32;
                p.corners[i] = std::array::from_fn(|q| ram.vec3(c + 12 * q as u32));
            }
        }
        e.pools[k] = p;
    }
    e.frame_count = ram.i32(FRAME_COUNT) as u32;
    e.trail_tick = ram.i16(TRAIL_TICK) as u16;
    for car in 0..6u32 {
        for w in 0..6u32 {
            let t = TRAILS + 384 * car + 64 * w;
            e.trails[car as usize][w as usize] = Trail {
                outer: ram.vec3(t),
                inner: ram.vec3(t + 16),
                new_outer: ram.vec3(t + 32),
                new_inner: ram.vec3(t + 48),
                state: ram.u8(STATE + 6 * car + w),
                count: ram.u8(COUNT + 6 * car + w),
                surface: ram.u8(SURFACE + 6 * car + w),
            };
        }
    }
    e.calls = ram.i32(CALLS) as u32;
    e.calls_frame = ram.i32(CALLS_FRAME) as u32;
    e.origin_set = ram.u8(ORIGIN_SET) != 0;
    e.origin = ram.vec3(ORIGIN);
    for (i, em) in e.embers.iter_mut().enumerate() {
        let a = EMBERS + 72 * i as u32;
        *em = Ember {
            age: ram.u8(a),
            life: ram.i32(a + 4),
            pos: ram.vec3(a + 8),
            vel: ram.vec3(a + 24),
            turn: std::array::from_fn(|r| std::array::from_fn(|c| ram.i16(a + 40 + 6 * r as u32 + 2 * c as u32))),
        };
    }
    for (i, col) in e.columns.iter_mut().enumerate() {
        let a = COLUMNS + 64 * i as u32;
        *col = Column {
            loops: ram.u8(a) as i8,
            loop_end: ram.u8(a + 1) as i8,
            loop_restart: ram.u8(a + 2) as i8,
            pos: ram.vec3(a + 4),
            vel: ram.vec3(a + 20),
            colour: [ram.u8(a + 36), ram.u8(a + 37), ram.u8(a + 38)],
            jitter: [ram.i32(a + 40), ram.i32(a + 44)],
            start: ram.i32(a + 48) as u32,
            end: ram.i32(a + 52) as u32,
            ticks: ram.i16(a + 56) as u16,
            frame: ram.u8(a + 58) as i8,
            per: ram.u8(a + 59),
            add: ram.u8(a + 60),
            kind: ram.u8(a + 61),
            slot: ram.u8(a + 62),
        };
    }
    e.flash = [ram.i16(FLASH), ram.i16(FLASH + 2)];
    e.puff_shrink = ram.u8(PUFF_SHRINK);
    e.chunk_colour = [ram.u8(CHUNK_COLOUR), ram.u8(CHUNK_COLOUR + 1), ram.u8(CHUNK_COLOUR + 2)];
    for slot in 0..6 {
        let m = model(ram, slot as u32);
        if m == 0 {
            continue;
        }
        let cvs = ram.i32(m + 16) as u32;
        for lod in 0..3 {
            let root = ram.i32(model_at(ram, slot as u32, lod) + 4) as u32;
            e.root_colour[slot][lod as usize] = ram.i32(root + 0x44) as u32 & 0xff_ffff;
        }
        e.wrecked[slot] = ram.u8(cvs + 0x28) == 1;
        e.lift[slot] = ram.i32(cvs + 0x18);
        e.flames[slot] = hwtr_game::effects::Flame {
            on: ram.u8(cvs + 0x1f0) != 0,
            start: ram.i32(cvs + 0x24) as u32,
            count: ram.u8(cvs + 0x1ec),
        };
        e.pulse[slot] = (ram.i32(0x8011_ec14 + 4 * slot as u32), ram.i32(0x8011_ebf4 + 4 * slot as u32) as u32);
        e.lights[slot] = read_lights(ram, cvs);
    }
    e
}

/// A car's lights in its view state.
pub fn read_lights(ram: &Ram, cvs: u32) -> hwtr_game::lights::Lights {
    hwtr_game::lights::Lights {
        body_target: ram.u8(cvs + 0x1e9),
        lamp_target: ram.u8(cvs + 0x1ea),
        lamp: ram.u8(cvs + 0x1eb),
        fading: ram.u8(cvs + 0x1f1) != 0,
        brake: ram.u8(cvs + 0x1f2) != 0,
        headlights: ram.i32(cvs + 0x20) & 0x20 != 0,
        glow: ram.i32(cvs + 0x130),
    }
}

pub fn write_lights(ram: &mut Ram, cvs: u32, l: &hwtr_game::lights::Lights) {
    ram.set_u8(cvs + 0x1e9, l.body_target);
    ram.set_u8(cvs + 0x1ea, l.lamp_target);
    ram.set_u8(cvs + 0x1eb, l.lamp);
    ram.set_u8(cvs + 0x1f1, l.fading as u8);
    ram.set_u8(cvs + 0x1f2, l.brake as u8);
    let mask = ram.i32(cvs + 0x20) & !0x20;
    ram.set_i32(cvs + 0x20, mask | if l.headlights { 0x20 } else { 0 });
    ram.set_i32(cvs + 0x130, l.glow);
}

pub fn write(e: &Effects, ram: &mut Ram) {
    for k in 0..4 {
        let h = HEADERS[k];
        let p = &e.pools[k];
        ram.set_u8(h + 1, p.oldest as u8);
        ram.set_u8(h + 2, p.next as u8);
        for (i, rec) in p.records.iter().enumerate() {
            let r = record_at(k, i);
            ram.set_vec3(r + 0x10, rec.pos);
            ram.set_i16(r + 0x24, rec.flags as i16);
            ram.set_i16(r + 0x26, rec.life as i16);
            for c in 0..3 {
                ram.set_i16(r + 0x2c + 2 * c as u32, rec.colour[c] as i16);
            }
            ram.set_u8(r + 0x32, rec.kind);
            ram.set_u8(r + 0x33, rec.frame);
            ram.set_i16(r + 0x28, rec.clut as i16);
            ram.set_i16(r + 0x2a, rec.tpage as i16);
            ram.set_u8(r + 0x34, rec.semi as u8);
            if k == 3 {
                ram.set_i32(r + 4, (CHUNK_QUADS + 32 * rec.buf as u32) as i32);
                ram.set_i32(r + 8, (CHUNK_NORMALS + 32 * rec.buf as u32) as i32);
                ram.set_i32(r + 0xc, (CHUNK_TURNS + 40 * rec.buf as u32) as i32);
                let c = &p.chunks[i];
                let (q, t) = (CHUNK_QUADS + 32 * i as u32, CHUNK_TURNS + 40 * i as u32);
                for n in 0..4u32 {
                    for a in 0..3u32 {
                        ram.set_i16(q + 8 * n + 2 * a, c.verts[n as usize][a as usize]);
                    }
                    ram.set_u8(q + 8 * n + 6, c.uv[n as usize][0]);
                    ram.set_u8(q + 8 * n + 7, c.uv[n as usize][1]);
                }
                for n in 0..4u32 {
                    ram.set_i32(t + 4 * n, c.base[n as usize]);
                }
                ram.set_vec3(t + 0x10, c.axis);
                ram.set_i32(t + 0x20, c.angle);
                ram.set_i32(t + 0x24, c.step);
            }
            match k {
                1 => ram.set_i32(r, (CORNERS + 48 * rec.buf as u32) as i32),
                _ if VEL[k] != 0 => {
                    ram.set_i32(r + 0x1c, (VEL[k] + 12 * rec.buf as u32) as i32);
                    let a = if ACCEL[k] != 0 { ACCEL[k] + 12 * rec.buf as u32 } else { 0 };
                    ram.set_i32(r + 0x20, a as i32);
                }
                _ => {}
            }
            if let Some(v) = &p.vel {
                ram.set_vec3(VEL[k] + 12 * i as u32, v[i]);
            }
            if let Some(a) = &p.accel {
                ram.set_vec3(ACCEL[k] + 12 * i as u32, a[i]);
            }
            if k == 1 {
                for (q, c) in p.corners[i].iter().enumerate() {
                    ram.set_vec3(CORNERS + 48 * i as u32 + 12 * q as u32, *c);
                }
            }
        }
    }
    ram.set_i32(FRAME_COUNT, e.frame_count as i32);
    ram.set_i16(TRAIL_TICK, e.trail_tick as i16);
    for car in 0..6u32 {
        for w in 0..6u32 {
            let t = TRAILS + 384 * car + 64 * w;
            let tr = &e.trails[car as usize][w as usize];
            ram.set_vec3(t, tr.outer);
            ram.set_vec3(t + 16, tr.inner);
            ram.set_vec3(t + 32, tr.new_outer);
            ram.set_vec3(t + 48, tr.new_inner);
            ram.set_u8(STATE + 6 * car + w, tr.state);
            ram.set_u8(COUNT + 6 * car + w, tr.count);
            ram.set_u8(SURFACE + 6 * car + w, tr.surface);
        }
    }
    ram.set_i32(CALLS, e.calls as i32);
    ram.set_i32(CALLS_FRAME, e.calls_frame as i32);
    ram.set_u8(ORIGIN_SET, e.origin_set as u8);
    ram.set_vec3(ORIGIN, e.origin);
    for (i, em) in e.embers.iter().enumerate() {
        let a = EMBERS + 72 * i as u32;
        ram.set_u8(a, em.age);
        ram.set_i32(a + 4, em.life);
        ram.set_vec3(a + 8, em.pos);
        ram.set_vec3(a + 24, em.vel);
        for r in 0..3u32 {
            for c in 0..3u32 {
                ram.set_i16(a + 40 + 6 * r + 2 * c, em.turn[r as usize][c as usize]);
            }
        }
    }
    for (i, col) in e.columns.iter().enumerate() {
        let a = COLUMNS + 64 * i as u32;
        ram.set_u8(a, col.loops as u8);
        ram.set_u8(a + 1, col.loop_end as u8);
        ram.set_u8(a + 2, col.loop_restart as u8);
        ram.set_vec3(a + 4, col.pos);
        ram.set_vec3(a + 20, col.vel);
        for c in 0..3 {
            ram.set_u8(a + 36 + c as u32, col.colour[c]);
        }
        ram.set_i32(a + 40, col.jitter[0]);
        ram.set_i32(a + 44, col.jitter[1]);
        ram.set_i32(a + 48, col.start as i32);
        ram.set_i32(a + 52, col.end as i32);
        ram.set_i16(a + 56, col.ticks as i16);
        ram.set_u8(a + 58, col.frame as u8);
        ram.set_u8(a + 59, col.per);
        ram.set_u8(a + 60, col.add);
        ram.set_u8(a + 61, col.kind);
        ram.set_u8(a + 62, col.slot);
    }
    ram.set_i16(FLASH, e.flash[0]);
    ram.set_u8(PUFF_SHRINK, e.puff_shrink);
    ram.set_i16(FLASH + 2, e.flash[1]);
    for c in 0..3 {
        ram.set_u8(CHUNK_COLOUR + c as u32, e.chunk_colour[c]);
    }
}
