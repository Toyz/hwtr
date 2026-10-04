//! The original's memory as the effects: the four rings' headers
//! (0x800d25c0, 0x800d25d0, 0x800d25d8, 0x800d25c8: capacity, oldest,
//! next, records), their 56-byte records and side buffers, and the skid
//! trails (0x8011ec2c on), read into and written from [`Effects`]. A
//! record's buffer is the slot its velocity (or corner) pointer points at.

use hwtr_game::effects::{CAPACITY, Effects, Pool, Record, Trail};

use super::Ram;

pub const HEADERS: [u32; 4] = [0x800d_25c0, 0x800d_25d0, 0x800d_25d8, 0x800d_25c8];
pub const RECORDS: [u32; 4] = [0x8011_fb3c, 0x8012_0f3c, 0x8012_293c, 0x8012_2e8c];
const VEL: [u32; 4] = [0x8012_0c3c, 0, 0x8012_2d9c, 0x8012_3b4c];
const ACCEL: [u32; 4] = [0x8012_093c, 0, 0, 0x8012_390c];
const CORNERS: u32 = 0x8012_1d3c;
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
                buf: buf_of(ram, k, r, i),
            };
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
    e
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
}
