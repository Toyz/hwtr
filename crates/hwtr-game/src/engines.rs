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
//! A car's hits and its tyres are heard too (see [`Engines::hit`] and
//! [`Engines::frame`]): a contact with the track strikes an impact and
//! keys a scrape that dies 200 ms after the last contact; two bodies
//! meeting crash, from the crashes bank; and a player's tyres roll or skid
//! on the ground under them. The mixer keeps each of those voices' volume
//! with the car's distance and side.
//!
//! The original keeps this in a 104-byte record a car at 0x8011a840 and a
//! 16-byte one at 0x80128e94; here it is [`CarSound`]. Bends are libsnd's
//! pitch bends (0x800a63d8), which [`crate::snd::bend_pitch`] turns into a
//! pitch; volumes are libsnd's `SsUtSetVVol` (0x800a6c64), 0-127 a side.

use crate::camera::Camera;
use crate::collision::world::Hit;
use crate::math::{Tables, Vec3, add, cross, div, div_fx, dot, fx, sub};

/// The engine kinds' table (0x800bd9c8), 22 bytes each: 11 kinds, then the
/// same 11 with the smaller banks the computer cars past the first get.
pub const KINDS: u32 = 0x800b_d9c8;
pub const KIND_COUNT: usize = 22;
/// The cars' engines (0x800bd160), 28 bytes each, found by name.
pub const CAR_ENGINES: u32 = 0x800b_d160;
pub const CAR_ENGINE_COUNT: usize = 42;

/// The tables the hits and the tyres are heard by, from the executable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HitTables {
    /// By ground kind, the effect a rolling tyre and a skidding one sound
    /// (0x800be888).
    pub tyres: Vec<[u8; 2]>,
    /// By surface, the effect a contact strikes and the one it scrapes
    /// (0x800be8a4).
    pub contacts: Vec<[u8; 2]>,
    /// By ground kind, which of a car's wheels' grounds is heard
    /// (0x800bea6c): the highest.
    pub priority: Vec<u8>,
    /// The voices each player's tyres and scrape are keyed on (0x800d0c28,
    /// 0x800d0c2c).
    pub tyre_voices: [usize; 4],
    pub scrape_voices: [usize; 4],
}

/// How many ground kinds the tables cover.
const GROUNDS: u32 = 14;

impl HitTables {
    pub fn read(byte: &dyn Fn(u32) -> u8) -> HitTables {
        let pairs = |at: u32| (0..GROUNDS).map(|k| [byte(at + 2 * k), byte(at + 2 * k + 1)]).collect();
        let voices = |at: u32| std::array::from_fn(|k| i16_at(byte, at + 2 * k as u32) as usize);
        HitTables {
            tyres: pairs(0x800b_e888),
            contacts: pairs(0x800b_e8a4),
            priority: (0..GROUNDS).map(|k| byte(0x800b_ea6c + k)).collect(),
            tyre_voices: voices(0x800d_0c28),
            scrape_voices: voices(0x800d_0c2c),
        }
    }
}

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
    (0..CAR_ENGINE_COUNT as u32).map(|k| CAR_ENGINES + 28 * k).filter(|&at| name_at(byte, at) == name).last().map(
        |at| CarEngine {
            kind: i16_at(byte, at + 0x10) as u16,
            redline: i32_at(byte, at + 0x14),
            idle: i32_at(byte, at + 0x18),
        },
    )
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
    if from < to { from + (to - from).min(STEP) } else { from - (from - to).min(STEP) }
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
    /// Where the car was heard from last, and how fast that point moved.
    pub pos: Vec3,
    pub vel: Vec3,
    /// The tyres' effect playing (0 for none), and the one that keyed
    /// their voice: 16 is heard at a fifth, 23 at three fifths.
    pub tyres: u8,
    pub tyres_keyed: u8,
    /// The scrape's effect, and how long until it stops, ms.
    pub scrape: u8,
    pub scrape_ms: u32,
    /// The tyres', the scrape's and the impact's volumes, 0 to 4096.
    pub tyre_level: i32,
    pub scrape_level: i32,
    pub impact_level: i32,
    /// The voices the tyres, the scrape, the crash and the impact were
    /// keyed on.
    pub tyre_voice: Option<usize>,
    pub scrape_voice: Option<usize>,
    pub crash_voice: Option<usize>,
    pub impact_voice: Option<usize>,
}

/// The bank a voice is keyed from: a car's engine (by slot), the effects
/// (libsnd's VAB 0) or the crashes (VAB 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bank {
    Car(usize),
    Effects,
    Crashes,
}

/// What the engines ask of the sound chip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// Key `voice` on from `bank` at `left` and `right` (0-127; libsnd's
    /// `SsUtKeyOnV`).
    KeyOn {
        voice: usize,
        bank: Bank,
        program: u8,
        tone: u8,
        note: u8,
        fine: u8,
        left: u8,
        right: u8,
    },
    KeyOff {
        voice: usize,
    },
    /// Bend `voice`, if it plays `program` (0x800a63d8).
    Bend {
        voice: usize,
        program: u8,
        bend: i32,
    },
    /// `voice`'s volume, 0-127 a side (0x800a6c64).
    Volume {
        voice: usize,
        left: u8,
        right: u8,
    },
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
    /// The ground its tyres are heard on (0 for none), how much they roll
    /// (the share of wheels down, times the speed over 100 mph, at most
    /// 4096) and the share of them skidding.
    pub ground: u8,
    pub roll: i32,
    pub skid: i32,
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
    /// Every effect by id (0x8011aec0), and the hits' tables.
    pub effects: Vec<Effect>,
    pub hits: HitTables,
    /// The listener and the effects volume the last frame heard with.
    pub listener: Listener,
    pub volume: i32,
    /// The effect voices (the first past the engines' to 19) and the
    /// importance each was last keyed at (0x8011acc0).
    pub first: usize,
    pub importance: [u8; 24],
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
        Engines { cars, kinds, players, effect, ..Engines::default() }
    }

    /// 0x80019abc: an effect voice at `importance`: the first stopped one,
    /// else the first playing something less important, let go (and so
    /// marked). The importance is recorded only by the caller that wants
    /// it.
    pub fn voice(&self, importance: u8, alive: &dyn Fn(usize) -> bool) -> Option<(usize, bool)> {
        let mut weaker = None;
        for v in self.first..20 {
            if !alive(v) {
                return Some((v, false));
            }
            if self.importance[v] < importance && weaker.is_none() {
                weaker = Some(v);
            }
        }
        weaker.map(|v| (v, true))
    }

    fn effect(&self, id: u8) -> Effect {
        self.effects.get(id as usize).copied().unwrap_or_default()
    }

    /// Car `slot`'s volume left and right (0-255) and Doppler-shifted fine
    /// tune, from where it was last heard (0x80018280, 0x80018760).
    fn heard(&self, t: &Tables, slot: usize) -> ([i32; 2], u8) {
        let car = &self.cars[slot];
        let [l, r] = level(t, &self.listener, car.pos, self.volume);
        ([l as i32, r as i32], doppler(t, self.listener.vel, car.vel, car.bend) as u8)
    }

    /// A car's hit heard (0x80035a88, 0x80035c7c).
    ///
    /// A contact strikes its surface's impact (0x80016280) unless the car
    /// has scraped in the last 200 ms, and keys its scrape (0x80016820)
    /// when that changes; either way the scrape goes on 200 ms more at the
    /// contact's volume. A crash is keyed from the crashes bank at the
    /// drawn tone (0x80016a18).
    pub fn hit(&mut self, t: &Tables, hit: &Hit, alive: &dyn Fn(usize) -> bool) -> Vec<Change> {
        let mut out = Vec::new();
        match *hit {
            Hit::Track { slot, surface, volume } => {
                let slot = slot as usize;
                if slot >= self.cars.len() {
                    return out;
                }
                let [impact, scrape] = self.hits.contacts.get(surface as usize).copied().unwrap_or_default();
                if self.cars[slot].scrape_ms != 0 {
                    // A change of scrape stops voice +0x48 (0x800161f4),
                    // which nothing here keys, and starts the new one.
                    if scrape != self.cars[slot].scrape {
                        if scrape != 0 {
                            out.extend(self.scrape_on(t, slot, scrape));
                        }
                        self.cars[slot].scrape = scrape;
                    }
                } else {
                    if impact != 0 {
                        out.extend(self.impact(t, slot, impact, volume, alive));
                    }
                    if scrape != 0 {
                        out.extend(self.scrape_on(t, slot, scrape));
                    }
                    self.cars[slot].scrape = scrape;
                }
                let car = &mut self.cars[slot];
                car.scrape_level = volume.min(4096);
                car.scrape_ms = 200;
            }
            Hit::Crash { slot, kind, volume, tone } => {
                let slot = slot as usize;
                if slot >= self.cars.len() {
                    return out;
                }
                let Some((voice, stolen)) = self.voice(1, alive) else { return out };
                let ([l, r], _) = self.heard(t, slot);
                let side = |v: i32| ((fx(volume, v << 12) >> 12) as i16 / 3) as u8;
                if stolen {
                    out.push(Change::KeyOff { voice });
                }
                let e = self.effect(kind);
                let note = tone.wrapping_add(60);
                out.push(Change::KeyOn {
                    voice,
                    bank: Bank::Crashes,
                    program: e.program,
                    tone,
                    note,
                    fine: note,
                    left: side(l),
                    right: side(r),
                });
                self.cars[slot].crash_voice = Some(voice);
            }
        }
        out
    }

    /// 0x80016280: effect `id` struck for car `slot` at `volume`, halved,
    /// on a stopped voice.
    pub fn impact(
        &mut self,
        t: &Tables,
        slot: usize,
        id: u8,
        volume: i32,
        alive: &dyn Fn(usize) -> bool,
    ) -> Vec<Change> {
        let mut out = Vec::new();
        let picked = self.voice(0, alive);
        let ([l, r], fine) = self.heard(t, slot);
        self.cars[slot].impact_level = volume;
        // With no voice to be had, libsnd is asked to key none.
        self.cars[slot].impact_voice = None;
        let Some((voice, stolen)) = picked else { return out };
        if stolen {
            out.push(Change::KeyOff { voice });
        }
        let side = |v: i32| ((fx(volume, v << 12) >> 12) as i16 / 2) as u8;
        let e = self.effect(id);
        out.push(Change::KeyOn {
            voice,
            bank: Bank::Effects,
            program: e.program,
            tone: e.tone,
            note: e.note,
            fine,
            left: side(l),
            right: side(r),
        });
        self.cars[slot].impact_voice = Some(voice);
        out
    }

    /// 0x80016820: a player's car's scrape `id` keyed on its own voice at
    /// an eighth of its volume.
    pub fn scrape_on(&mut self, t: &Tables, slot: usize, id: u8) -> Vec<Change> {
        if slot >= self.players {
            return Vec::new();
        }
        let ([l, r], fine) = self.heard(t, slot);
        let level = self.cars[slot].scrape_level;
        let side = |v: i32| ((fx(level, v << 12) >> 12) as i16 / 8) as u8;
        let voice = self.hits.scrape_voices[slot % 4];
        let e = self.effect(id);
        self.cars[slot].scrape_voice = Some(voice);
        vec![Change::KeyOn {
            voice,
            bank: Bank::Effects,
            program: e.program,
            tone: e.tone,
            note: e.note,
            fine,
            left: side(l),
            right: side(r),
        }]
    }

    /// 0x800169a8 and 0x800167b0: a voice let go if it plays.
    fn stop(voice: Option<usize>, alive: &dyn Fn(usize) -> bool) -> Option<Change> {
        voice.filter(|&v| alive(v)).map(|voice| Change::KeyOff { voice })
    }

    /// 0x80016490: a player's car's tyres keyed with effect `id` on their
    /// own voice: at a fifth of their volume for 16, three fifths for 23,
    /// else a third (with one player only).
    pub fn tyres_on(&mut self, t: &Tables, slot: usize, id: u8) -> Vec<Change> {
        self.cars[slot].tyres_keyed = id;
        if self.players >= 2 || slot >= self.players {
            return Vec::new();
        }
        let ([l, r], fine) = self.heard(t, slot);
        let level = self.cars[slot].tyre_level;
        let side = |v: i32| {
            let v = (fx(level, v << 12) >> 12) as i16;
            (match id {
                16 => v / 5,
                23 => v * 3 / 5,
                _ => v / 3,
            }) as u8
        };
        let voice = self.hits.tyre_voices[slot % 4];
        let e = self.effect(id);
        self.cars[slot].tyre_voice = Some(voice);
        vec![Change::KeyOn {
            voice,
            bank: Bank::Effects,
            program: e.program,
            tone: e.tone,
            note: e.note,
            fine,
            left: side(l),
            right: side(r),
        }]
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
        let mut out = vec![Change::KeyOn {
            voice: slot,
            bank: Bank::Car(slot),
            program: e.program,
            tone: e.tone,
            note,
            fine: 64,
            left: 0,
            right: 0,
        }];
        self.cars[slot].bend = e.note as u16;
        if slot < self.players {
            out.push(Change::KeyOn {
                voice: self.cars.len() + slot,
                bank: Bank::Car(slot),
                program: 1,
                tone: 0,
                note: 57,
                fine: 64,
                left: 0,
                right: 0,
            });
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

    /// One frame of `ms` (0x800354ac): each car's revs, throttle and
    /// place taken in; a player's tyres keyed, let go or turned to the
    /// ground under them and their volume set; each scrape run down and let
    /// go when it runs out; then the mixer, heard from `listener` with one
    /// player. `alive` says whether a voice still plays; `volume` is the
    /// effects volume (0-127).
    pub fn frame(
        &mut self,
        t: &Tables,
        inputs: &[EngineInput],
        listener: &Listener,
        volume: i32,
        ms: u32,
        alive: &dyn Fn(usize) -> bool,
    ) -> Vec<Change> {
        let mut out = Vec::new();
        self.volume = volume;
        for (slot, input) in inputs.iter().enumerate().take(self.cars.len()) {
            let car = &mut self.cars[slot];
            car.rpm = slew_rpm(car.rpm, input.rpm);
            car.throttle = ((input.pedals as i32) << 12) / 255;
            if self.players < 2 {
                car.pos = input.pos;
                car.vel = input.vel;
            }
            if slot <= self.players {
                let pair = self.hits.tyres.get(input.ground as usize).copied().unwrap_or_default();
                let id = if input.ground != 0 { pair[(input.skid >= input.roll) as usize] } else { 0 };
                let playing = self.cars[slot].tyres;
                if id != 0 {
                    if playing != id {
                        out.extend(Self::stop(self.cars[slot].tyre_voice, alive));
                        out.extend(self.tyres_on(t, slot, id));
                    }
                    self.cars[slot].tyre_level = input.roll.max(input.skid).min(4096);
                } else if playing != 0 {
                    out.extend(Self::stop(self.cars[slot].tyre_voice, alive));
                }
                self.cars[slot].tyres = id;
            }
            let car = &mut self.cars[slot];
            if ms >= car.scrape_ms {
                out.extend(Self::stop(car.scrape_voice, alive));
                car.scrape_ms = 0;
            } else {
                car.scrape_ms -= ms;
            }
        }
        if self.players < 2 {
            self.listener = *listener;
        }
        out.extend(self.mixer(t, inputs, listener, volume, alive));
        out
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
            let engine = alive(watched);
            if self.players >= 2 {
                if engine {
                    out.extend(self.mix(slot, third, third));
                }
                continue;
            }
            let car = &self.cars[slot];
            let playing = |v: Option<usize>| v.filter(|&v| alive(v));
            let (tyres, scrape, crash, impact) = (
                playing(car.tyre_voice),
                playing(car.scrape_voice),
                playing(car.crash_voice),
                playing(car.impact_voice),
            );
            if !engine && tyres.is_none() && scrape.is_none() && crash.is_none() && impact.is_none() {
                continue;
            }
            let [left, right] = level(t, listener, input.pos, volume);
            let shift = doppler(t, listener.vel, input.vel, self.cars[slot].bend);
            let (left, right) = (left as i32 * 2 / 5, right as i32 * 2 / 5);
            if engine {
                out.extend(self.mix(slot, left, right));
                if let Some(kind) = self.kind(slot) {
                    let bend = (kind.key_note as i32 + shift) as i16 as i32;
                    out.push(Change::Bend { voice: watched, program: self.effect.program, bend });
                }
            }
            let car = &self.cars[slot];
            let scaled = |level: i32, v: i32| (fx(level, v << 12) >> 12) as i16;
            let volume = |voice: usize, l: i16, r: i16| Change::Volume { voice, left: l as u8, right: r as u8 };
            if let Some(voice) = tyres {
                let (mut l, mut r) = (scaled(car.tyre_level, left), scaled(car.tyre_level, right));
                if slot < 2 && car.tyres_keyed == 16 {
                    (l, r) = (l / 5, r / 5);
                }
                if slot < 2 && car.tyres_keyed == 23 {
                    (l, r) = (l * 3 / 5, r * 3 / 5);
                }
                out.push(volume(voice, l, r));
            }
            if scrape.is_some() && slot < self.players {
                let voice = self.hits.scrape_voices[slot % 4];
                out.push(volume(voice, scaled(car.scrape_level, left), scaled(car.scrape_level, right)));
            }
            if let Some(voice) = crash {
                out.push(volume(voice, left as i16 / 3, right as i16 / 3));
            }
            if let Some(voice) = impact {
                out.push(volume(voice, scaled(car.impact_level, left) / 2, scaled(car.impact_level, right) / 2));
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
/// velocity, its revs (none when wrecked), `pedals`, and its tyres: the
/// ground of the highest `priority` under a wheel that is down, the share
/// of wheels down times the speed over 100 mph, and the share skidding
/// (none of it when wrecked).
pub fn input_of(car: &crate::car::Car, pedals: i16, priority: &[u8]) -> EngineInput {
    let b = &car.body;
    let rank = |g: u8| priority.get(g as usize).copied().unwrap_or(0);
    let (mut roll, mut skid, mut ground) = (0, 0, 0u8);
    if !car.wrecked {
        for w in car.wheels.iter().filter(|w| w.on_ground) {
            roll += 4096;
            if w.slipping {
                skid += 4096;
            }
            if rank(ground) < rank(w.surface) {
                ground = w.surface;
            }
        }
    }
    let wheels = (car.wheels.len() as i32) << 12;
    let mph = div_fx(0xb_0000, 0xa000);
    let roll = fx(div_fx(roll, wheels), div_fx(b.speed, fx(0x6_4000, mph))).min(4096);
    EngineInput {
        rpm: if car.wrecked { 0 } else { car.engine.rpm },
        pedals,
        pos: add(b.pos, b.centre),
        vel: add(b.vel, cross(b.spin, b.centre)),
        ground,
        roll,
        skid: div_fx(skid, wheels),
    }
}
