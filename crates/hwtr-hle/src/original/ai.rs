//! The original's memory as the computer cars' drivers: the records at
//! 0x801315f4 and the race's settings, read into and written from the port's
//! types.

use super::Ram;
use hwtr_game::ai::{Ai, AiRand, Choice, Driver, Line};

pub const DRIVERS: u32 = 0x8013_15f4;
pub const DRIVER_SIZE: u32 = 592;
/// Where a car keeps its driver's route at its last steady place.
pub const CAR_RESPAWN_LINE: u32 = 0x7d0;

pub const LAP: u32 = 0x800d_26dc;
pub const PACE: u32 = 0x800d_26e4;
pub const RUBBER_MIN: u32 = 0x800d_26e8;
pub const RUBBER_MAX: u32 = 0x800d_26ec;
pub const LEAD: u32 = 0x800d_26f0;
pub const SHARE: u32 = 0x800d_26f4;
pub const BAND: u32 = 0x800d_26f8;
pub const LAST_LAP: u32 = 0x800d_26fc;
pub const LAPS: u32 = 0x800d_2700;
pub const PLAYERS: u32 = 0x800d_2701;
pub const LEADER: u32 = 0x800d_2704;
pub const SEED: u32 = 0x800d_2710;
pub const MIRRORED: u32 = 0x800d_0fec;

fn i64_at(ram: &Ram, a: u32) -> i64 {
    (ram.i32(a) as u32 as i64) | ((ram.i32(a + 4) as i64) << 32)
}

fn set_i64(ram: &mut Ram, a: u32, v: i64) {
    ram.set_i32(a, v as i32);
    ram.set_i32(a + 4, (v >> 32) as i32);
}

/// A route place at `at` (148 bytes).
pub fn line(ram: &Ram, at: u32) -> Line {
    let count = (ram.i32(at + 0x28) as u32).min(2);
    let choice = ram.i32(at + 0x54);
    Line {
        target: ram.vec3(at),
        left: ram.i32(at + 0x10),
        right: ram.i32(at + 0x14),
        turn: ram.i32(at + 0x18),
        flags: ram.i16(at + 0x1c) as u16,
        distance: ram.i32(at + 0x20),
        air: ram.i32(at + 0x24),
        choices: (0..count)
            .map(|k| Choice {
                mask: ram.i32(at + 0x2c + 4 * k) as u32,
                weights: std::array::from_fn(|b| ram.u8(at + 0x34 + 16 * k + b as u32)),
            })
            .collect(),
        choice: (choice >= 0).then_some(choice as usize),
        from: ram.vec3(at + 0x58),
        to: ram.vec3(at + 0x68),
        from_widths: (ram.i32(at + 0x78), ram.i32(at + 0x7c)),
        to_widths: (ram.i32(at + 0x80), ram.i32(at + 0x84)),
        at: ram.i16(at + 0x88) as u16,
        length: ram.i32(at + 0x8c),
        along: ram.i32(at + 0x90),
    }
}

pub fn write_line(ram: &mut Ram, at: u32, l: &Line) {
    ram.set_vec3(at, l.target);
    ram.set_i32(at + 0x10, l.left);
    ram.set_i32(at + 0x14, l.right);
    ram.set_i32(at + 0x18, l.turn);
    ram.set_i16(at + 0x1c, l.flags as i16);
    ram.set_i32(at + 0x20, l.distance);
    ram.set_i32(at + 0x24, l.air);
    ram.set_i32(at + 0x28, l.choices.len() as i32);
    for (k, c) in l.choices.iter().enumerate().take(2) {
        ram.set_i32(at + 0x2c + 4 * k as u32, c.mask as i32);
        for (b, w) in c.weights.iter().enumerate() {
            ram.set_u8(at + 0x34 + 16 * k as u32 + b as u32, *w);
        }
    }
    ram.set_i32(at + 0x54, l.choice.map_or(-1, |c| c as i32));
    ram.set_vec3(at + 0x58, l.from);
    ram.set_vec3(at + 0x68, l.to);
    ram.set_i32(at + 0x78, l.from_widths.0);
    ram.set_i32(at + 0x7c, l.from_widths.1);
    ram.set_i32(at + 0x80, l.to_widths.0);
    ram.set_i32(at + 0x84, l.to_widths.1);
    ram.set_i16(at + 0x88, l.at as i16);
    ram.set_i32(at + 0x8c, l.length);
    ram.set_i32(at + 0x90, l.along);
}

/// Driver `k`, its car's record at `car` (for the saved route).
pub fn driver(ram: &Ram, k: u32, car: u32) -> Driver {
    let at = DRIVERS + k * DRIVER_SIZE;
    let v = |o: u32| ram.vec3(at + o);
    let w = |o: u32| ram.i32(at + o);
    Driver {
        car: ram.u8(at),
        line: line(ram, at + 8),
        active: ram.flag(at + 0x9c),
        pos: v(0xa0),
        prev_pos: v(0xb0),
        base: v(0xc0),
        prev_base: v(0xd0),
        rot: ram.matrix(at + 0xe0),
        ups: std::array::from_fn(|k| v(0x100 + 16 * k as u32)),
        up_at: w(0x130) as u32,
        travels: std::array::from_fn(|k| v(0x134 + 16 * k as u32)),
        travel_at: w(0x164) as u32,
        up: v(0x168),
        travel: v(0x178),
        swerve: w(0x188),
        swerve_to: w(0x18c),
        swerve_speed: w(0x190),
        swerve_accel: w(0x194),
        swerve_flip: ram.flag(at + 0x198),
        pace: w(0x19c),
        speed: w(0x1a0),
        launch: w(0x1a4),
        progress: w(0x1a8),
        drift: v(0x1ac),
        shove: v(0x1bc),
        spin: std::array::from_fn(|k| i64_at(ram, at + 0x1d0 + 8 * k as u32)),
        spin_shove: v(0x1e8),
        knocked: w(0x1f8),
        knock_pace: w(0x1fc),
        knock_swerve: w(0x200),
        rubber_min: w(0x204),
        rubber_max: w(0x208),
        rubber: w(0x20c),
        rubber_at: w(0x210) as u32,
        slip: w(0x214),
        lean: w(0x218),
        lean_way: w(0x21c),
        stunt: ram.i16(at + 0x220) as u16,
        stunt_ms: w(0x224),
        stunt_rot: ram.matrix(at + 0x228),
        skill: w(0x248),
        jumped: ram.flag(at + 0x24c),
        racing: ram.flag(at + 0x24d),
        respawn_line: line(ram, car + CAR_RESPAWN_LINE),
    }
}

pub fn write_driver(ram: &mut Ram, k: u32, car: u32, d: &Driver) {
    let at = DRIVERS + k * DRIVER_SIZE;
    ram.set_u8(at, d.car);
    write_line(ram, at + 8, &d.line);
    ram.set_flag(at + 0x9c, d.active);
    for (o, v) in [(0xa0, d.pos), (0xb0, d.prev_pos), (0xc0, d.base), (0xd0, d.prev_base)] {
        ram.set_vec3(at + o, v);
    }
    ram.set_matrix(at + 0xe0, &d.rot);
    for k in 0..3u32 {
        ram.set_vec3(at + 0x100 + 16 * k, d.ups[k as usize]);
        ram.set_vec3(at + 0x134 + 16 * k, d.travels[k as usize]);
        set_i64(ram, at + 0x1d0 + 8 * k, d.spin[k as usize]);
    }
    ram.set_i32(at + 0x130, d.up_at as i32);
    ram.set_i32(at + 0x164, d.travel_at as i32);
    ram.set_vec3(at + 0x168, d.up);
    ram.set_vec3(at + 0x178, d.travel);
    for (o, v) in [
        (0x188, d.swerve),
        (0x18c, d.swerve_to),
        (0x190, d.swerve_speed),
        (0x194, d.swerve_accel),
        (0x19c, d.pace),
        (0x1a0, d.speed),
        (0x1a4, d.launch),
        (0x1a8, d.progress),
        (0x1f8, d.knocked),
        (0x1fc, d.knock_pace),
        (0x200, d.knock_swerve),
        (0x204, d.rubber_min),
        (0x208, d.rubber_max),
        (0x20c, d.rubber),
        (0x210, d.rubber_at as i32),
        (0x214, d.slip),
        (0x218, d.lean),
        (0x21c, d.lean_way),
        (0x224, d.stunt_ms),
        (0x248, d.skill),
    ] {
        ram.set_i32(at + o, v);
    }
    ram.set_flag(at + 0x198, d.swerve_flip);
    ram.set_vec3(at + 0x1ac, d.drift);
    ram.set_vec3(at + 0x1bc, d.shove);
    ram.set_vec3(at + 0x1e8, d.spin_shove);
    ram.set_i16(at + 0x220, d.stunt as i16);
    ram.set_matrix(at + 0x228, &d.stunt_rot);
    ram.set_flag(at + 0x24c, d.jumped);
    ram.set_flag(at + 0x24d, d.racing);
    write_line(ram, car + CAR_RESPAWN_LINE, &d.respawn_line);
}

/// The drivers and the race's settings; `cars` gives each car record's
/// address, by slot.
pub fn ai(ram: &Ram, cars: &[u32]) -> Ai {
    let leader = ram.i32(LEADER) as u32;
    Ai {
        drivers: (0..6u32).map(|k| driver(ram, k, cars.get(k as usize).copied().unwrap_or(0))).collect(),
        rand: AiRand { seed: ram.i32(SEED) as u32 },
        lap: ram.i32(LAP),
        laps: ram.u8(LAPS),
        pace: ram.i32(PACE),
        rubber_min: ram.i32(RUBBER_MIN),
        rubber_max: ram.i32(RUBBER_MAX),
        lead: ram.i32(LEAD),
        share: ram.i32(SHARE),
        band: ram.i32(BAND),
        last_lap: ram.i32(LAST_LAP),
        leader: cars.iter().position(|&c| c == leader && leader != 0).map(|k| k as u8),
        players: ram.u8(PLAYERS),
        mirrored: ram.flag(MIRRORED),
    }
}

pub fn write_ai(ram: &mut Ram, cars: &[u32], ai: &Ai) {
    for (k, d) in ai.drivers.iter().enumerate() {
        write_driver(ram, k as u32, cars.get(k).copied().unwrap_or(0), d);
    }
    ram.set_i32(SEED, ai.rand.seed as i32);
}
