//! The engines' sound: each car's engine bank playing on a voice of its
//! own, its pitch bent by the engine's revs and its volume by how far the
//! car is from the camera and to which side.
//!
//! At the race's start and whenever a car is put back on the road
//! (0x80015ebc), the car's engine is keyed on its voice (the car's slot) at
//! the kind's note, silent; a player's car also keys its overrun (program
//! 1, note 57) on the voice past the cars. A wreck (0x80016004) and the
//! pause menu let them go; leaving the pause keys them again.
//!
//! Each frame (0x800354ac, after the race steps) each car's revs are
//! clamped to 1000-30000 and slewed toward at most about 1000 a frame, and
//! the mixer runs: with one player (0x80017928) the volume comes from the
//! car's distance to the camera and its side (0x80018280), and the voice
//! the overrun would be on is bent around its centre by a Doppler factor
//! (0x80018760); with two (0x80017268) every car is at a third of the
//! effects volume. Either way 0x8001a36c bends the engine by the revs
//! between the car's idle and redline, and for a player's car splits the
//! volume by the throttle between the engine (on throttle) and the overrun
//! (off it), each side slewed 1.25 a frame (0x8001a318).
//!
//! The original keeps this in a 104-byte record a car at 0x8011a840 and a
//! 16-byte one at 0x80128e94; here it is [`CarSound`]. Bends are libsnd's
//! pitch bends (0x800a63d8), which [`crate::snd::bend_pitch`] turns into a
//! pitch; volumes are libsnd's `SsUtSetVVol` (0x800a6c64), 0-127 a side.

use crate::camera::Camera;
use crate::math::{Tables, Vec3, add, cross, div, div_fx, dot, fx, sub};

/// The engine kinds' table (0x800bd9c8), 22 bytes each: 11 kinds, then the
/// same 11 with the smaller banks the computer cars past the first get.
pub const KINDS: u32 = 0x800b_d9c8;
pub const KIND_COUNT: usize = 22;
/// The cars' engines (0x800bd160), 28 bytes each, found by name.
pub const CAR_ENGINES: u32 = 0x800b_d160;
pub const CAR_ENGINE_COUNT: usize = 42;

/// An engine kind: its bank's name, the note it is keyed at, and the bends
/// at idle and at redline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kind {
    pub bank: String,
    pub key_note: i16,
    pub bend_idle: i16,
    pub bend_redline: i16,
}

/// A car's engine as its sound sees it: the kind and the revs it ranges
/// over (12-bit fixed point).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CarEngine {
    pub kind: u16,
    pub redline: i32,
    pub idle: i32,
}

fn name_at(byte: &dyn Fn(u32) -> u8, at: u32) -> String {
    (0..16).map(|k| byte(at + k)).take_while(|&b| b != 0).map(char::from).collect()
}

fn i32_at(byte: &dyn Fn(u32) -> u8, at: u32) -> i32 {
    i32::from_le_bytes(std::array::from_fn(|k| byte(at + k as u32)))
}

fn i16_at(byte: &dyn Fn(u32) -> u8, at: u32) -> i16 {
    i16::from_le_bytes([byte(at), byte(at + 1)])
}

/// The engine kinds, from the executable.
pub fn kinds(byte: &dyn Fn(u32) -> u8) -> Vec<Kind> {
    (0..KIND_COUNT as u32)
        .map(|k| {
            let at = KINDS + 22 * k;
            Kind {
                bank: name_at(byte, at),
                key_note: i16_at(byte, at + 0x10),
                bend_idle: i16_at(byte, at + 0x12),
                bend_redline: i16_at(byte, at + 0x14),
            }
        })
        .collect()
}

/// 0x8001aa84: car `name`'s engine (the last entry of that name), if the
/// table has it.
pub fn car_engine(byte: &dyn Fn(u32) -> u8, name: &str) -> Option<CarEngine> {
    (0..CAR_ENGINE_COUNT as u32)
        .map(|k| CAR_ENGINES + 28 * k)
        .filter(|&at| name_at(byte, at) == name)
        .last()
        .map(|at| CarEngine {
            kind: i16_at(byte, at + 0x10) as u16,
            redline: i32_at(byte, at + 0x14),
            idle: i32_at(byte, at + 0x18),
        })
}

/// Where the sound is heard from: player one's camera (0x8003a5d4), its
/// position, velocity, and its forward and up axes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Listener {
    pub pos: Vec3,
    pub vel: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
}

impl Listener {
    pub fn of(camera: &Camera) -> Listener {
        let column = |j: usize| camera.rot.map(|row| row[j] as i32);
        Listener { pos: camera.pos, vel: camera.vel, forward: column(1), up: column(2) }
    }
}

/// 1/12, as the distances are scaled.
fn twelfth() -> i32 {
    div_fx(4096, 0xc000)
}

/// How far a sound carries, in twelfths.
fn range() -> i32 {
    fx(0xbb_8000, twelfth())
}

/// 0x80018e04: the distance from the listener to `point`, in twelfths,
/// at most [`range`] (and that if any axis is that far alone).
pub fn distance(t: &Tables, listener: Vec3, point: Vec3) -> i32 {
    let scale = twelfth();
    let range = range();
    let d = sub(point, listener).map(|c| fx(c, scale));
    if d.iter().any(|c| c.wrapping_abs() >= range) {
        return range;
    }
    t.length(d).min(range)
}

/// 0x80018590: how far to the listener's right `point` is, -4096 (left)
/// to 4096 (right), full at 3000 units.
pub fn pan(listener: &Listener, point: Vec3) -> i32 {
    let right = cross(listener.forward, listener.up);
    (dot(right, sub(point, listener.pos)) / 3000).clamp(-4096, 4096)
}

/// 0x80018280: a sound at `point`'s volume left and right (0-255 each) at
/// effects volume `volume` (0-127): falling off with distance to nothing
/// at the range, split by the side.
pub fn level(t: &Tables, listener: &Listener, point: Vec3, volume: i32) -> [u8; 2] {
    let range = range();
    let d = distance(t, listener.pos, point);
    if d >= range {
        return [0, 0];
    }
    let near = div_fx(range - d, range).min(4096);
    let right = fx(pan(listener, point), 2048) + 2048;
    let v = volume << 12;
    let side = |share: i32| ((fx(v, fx(share, near)) as u32 >> 12) & 255) as u8;
    [side(4096 - right), side(right)]
}

/// A velocity's direction, as 0x80018760 takes it: normalised only when
/// every axis is at least 0.1 (a negative axis leaves it as it is).
fn direction(t: &Tables, v: Vec3) -> Vec3 {
    if v.iter().any(|&c| c < 410) {
        return v;
    }
    let len = t.length(v);
    v.map(|c| div_fx(c, len))
}

/// 0x80018760: `bend` shifted by the listener's and the source's speeds
/// (both negated when they head the same way), 0 to 127.
pub fn doppler(t: &Tables, listener_vel: Vec3, vel: Vec3, bend: u16) -> i32 {
    let mut heard = div(t.length(listener_vel), 0x34bf).0;
    let mut made = div(t.length(vel), 0x34bf).0;
    if dot(direction(t, listener_vel), direction(t, vel)) > 0 {
        heard = -heard;
        made = -made;
    }
    let ratio = div_fx(heard + 4096, made + 4096);
    let shifted = fx((bend as i32) << 12, ratio);
    ((shifted << 4) >> 16).clamp(0, 127)
}

/// 0x8001a318: `from` toward `to` by at most 1.25.
pub fn slew(from: i32, to: i32) -> i32 {
    const STEP: i32 = 0x1_4000;
    if from < to {
        from + (to - from).min(STEP)
    } else {
        from - (from - to).min(STEP)
    }
}

/// 0x800354ac's revs: `rpm` clamped to 1000-30000, then `from` toward it
/// by at most about 1000.
pub fn slew_rpm(from: i32, rpm: i32) -> i32 {
    let rpm = rpm.clamp(0x3e_8000, 0x753_0000);
    let most = fx(0x271_0000, 0x6_4000 / 1000);
    if from < rpm { from + (rpm - from).min(most) } else { from - (from - rpm).min(most) }
}

/// A car's engine sound as the race keeps it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CarSound {
    pub engine: CarEngine,
    /// The kind it plays: the car's, or the smaller bank's for a computer
    /// car past the first (0x8001924c).
    pub kind: u16,
    /// The revs, slewed.
    pub rpm: i32,
    /// The throttle, 0 to 4096 (a player's pedals, eased).
    pub throttle: i32,
    /// The volumes slewed toward: engine left and right, overrun left and
    /// right, 12-bit.
    pub levels: [i32; 4],
    /// The last bend set (the effect's note at key on).
    pub bend: u16,
}

/// What the engines ask of the sound chip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// Key `voice` on from car `car`'s engine bank (libsnd's `SsUtKeyOnV`).
    KeyOn { voice: usize, car: usize, program: u8, tone: u8, note: u8, fine: u8 },
    KeyOff { voice: usize },
    /// Bend `voice`, if it plays `program` (0x800a63d8).
    Bend { voice: usize, program: u8, bend: i32 },
    /// `voice`'s volume, 0-127 a side (0x800a6c64).
    Volume { voice: usize, left: u8, right: u8 },
}

/// A car's state this frame, as the sound reads it (0x80045a84).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EngineInput {
    /// The engine's revs (0 when wrecked).
    pub rpm: i32,
    /// A player's eased pedals, 0 to 255.
    pub pedals: i16,
    /// Where the car's centre is, and how fast that point moves.
    pub pos: Vec3,
    pub vel: Vec3,
}

/// The effect the engines key with: effect 0's program, tone and note.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Effect {
    pub program: u8,
    pub tone: u8,
    pub note: u8,
}

/// Every car's engine sound.
#[derive(Clone, Debug, Default)]
pub struct Engines {
    pub cars: Vec<CarSound>,
    pub kinds: Vec<Kind>,
    pub players: usize,
    pub effect: Effect,
}

impl Engines {
    /// 0x8001924c: the cars' engines (None where the table lacks the car).
    pub fn new(kinds: Vec<Kind>, engines: &[Option<CarEngine>], players: usize, effect: Effect) -> Engines {
        let cars = engines
            .iter()
            .enumerate()
            .map(|(slot, e)| {
                let engine = e.unwrap_or_default();
                let kind = if players < slot { engine.kind + 11 } else { engine.kind };
                CarSound { engine, kind, ..CarSound::default() }
            })
            .collect();
        Engines { cars, kinds, players, effect }
    }

    /// The bank car `slot` plays.
    pub fn kind(&self, slot: usize) -> Option<&Kind> {
        self.kinds.get(self.cars.get(slot)?.kind as usize)
    }

    /// The voice that 0x8011a840 +0x4a names: a player's overrun, a
    /// computer car's engine.
    pub fn watched(&self, slot: usize) -> usize {
        if slot < self.players { self.cars.len() + slot } else { slot }
    }

    /// 0x80015ebc: car `slot`'s engine keyed on silent, and a player's
    /// overrun.
    pub fn key_on(&mut self, slot: usize) -> Vec<Change> {
        let Some(note) = self.kind(slot).map(|k| k.key_note as u8) else { return Vec::new() };
        let e = self.effect;
        let mut out = vec![Change::KeyOn { voice: slot, car: slot, program: e.program, tone: e.tone, note, fine: 64 }];
        self.cars[slot].bend = e.note as u16;
        if slot < self.players {
            out.push(Change::KeyOn { voice: self.cars.len() + slot, car: slot, program: 1, tone: 0, note: 57, fine: 64 });
        }
        out
    }

    /// 0x80016004: car `slot`'s voices let go.
    pub fn key_off(&self, slot: usize) -> Vec<Change> {
        let mut out = vec![Change::KeyOff { voice: slot }];
        if slot < self.players {
            out.push(Change::KeyOff { voice: self.cars.len() + slot });
        }
        out
    }

    /// One frame: the revs and throttles taken in, then the mixer, heard
    /// from `listener` with one player. `alive` says whether a voice still
    /// plays; `volume` is the effects volume (0-127).
    pub fn frame(
        &mut self,
        t: &Tables,
        inputs: &[EngineInput],
        listener: &Listener,
        volume: i32,
        alive: &dyn Fn(usize) -> bool,
    ) -> Vec<Change> {
        for (car, input) in self.cars.iter_mut().zip(inputs) {
            car.rpm = slew_rpm(car.rpm, input.rpm);
            car.throttle = ((input.pedals as i32) << 12) / 255;
        }
        self.mixer(t, inputs, listener, volume, alive)
    }

    /// The mixer (0x80017928 with one player, 0x80017268 with two) over
    /// the revs and throttles taken in.
    pub fn mixer(
        &mut self,
        t: &Tables,
        inputs: &[EngineInput],
        listener: &Listener,
        volume: i32,
        alive: &dyn Fn(usize) -> bool,
    ) -> Vec<Change> {
        let mut out = Vec::new();
        let third = volume / 3;
        for (slot, input) in inputs.iter().enumerate().take(self.cars.len()) {
            let watched = self.watched(slot);
            if !alive(watched) {
                continue;
            }
            if self.players >= 2 {
                out.extend(self.mix(slot, third, third));
                continue;
            }
            let [left, right] = level(t, listener, input.pos, volume);
            let shift = doppler(t, listener.vel, input.vel, self.cars[slot].bend);
            out.extend(self.mix(slot, left as i32 * 2 / 5, right as i32 * 2 / 5));
            if let Some(kind) = self.kind(slot) {
                let bend = (kind.key_note as i32 + shift) as i16 as i32;
                out.push(Change::Bend { voice: watched, program: self.effect.program, bend });
            }
        }
        out
    }

    /// 0x8001a36c: car `slot`'s bend from its revs, and its volume `left`
    /// and `right` (0-127), split by the throttle for a player.
    pub fn mix(&mut self, slot: usize, left: i32, right: i32) -> Vec<Change> {
        let mut out = Vec::new();
        let Some(kind) = self.kind(slot).cloned() else { return out };
        let cars = self.cars.len();
        let program = self.effect.program;
        let car = &mut self.cars[slot];
        let span = car.engine.redline - car.engine.idle;
        let share = div_fx(car.rpm - car.engine.idle, span);
        let (low, high) = ((kind.bend_idle as i32) << 12, (kind.bend_redline as i32) << 12);
        let bend = ((fx(share, high - low).wrapping_add(low) as u32 >> 12) & 255) as i32;
        car.bend = bend as u16;
        let (left, right) = (left as i8 as i32, right as i8 as i32);
        if slot < self.players {
            let (l, r) = (left << 12, right << 12);
            let (on_l, on_r) = (fx(l, car.throttle), fx(r, car.throttle));
            let targets = [on_l, on_r, l - on_l, r - on_r];
            for (level, target) in car.levels.iter_mut().zip(targets) {
                *level = slew(*level, target);
            }
            let byte = |v: i32| ((v as u32 >> 12) & 255) as u8;
            let [el, er, ol, or] = car.levels.map(byte);
            out.push(Change::Bend { voice: cars + slot, program: 1, bend });
            out.push(Change::Volume { voice: cars + slot, left: ol, right: or });
            out.push(Change::Bend { voice: slot, program, bend });
            out.push(Change::Volume { voice: slot, left: el, right: er });
        } else {
            out.push(Change::Bend { voice: slot, program, bend });
            out.push(Change::Volume { voice: slot, left: (left / 2) as u8, right: (right / 2) as u8 });
        }
        out
    }
}

/// 0x80045a84: car `car`'s sound input: its centre and that point's
/// velocity, its revs (none when wrecked), and `pedals`.
pub fn input_of(car: &crate::car::Car, pedals: i16) -> EngineInput {
    let b = &car.body;
    EngineInput {
        rpm: if car.wrecked { 0 } else { car.engine.rpm },
        pedals,
        pos: add(b.pos, b.centre),
        vel: add(b.vel, cross(b.spin, b.centre)),
    }
}
