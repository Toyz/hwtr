//! The cars' hits and tyres as heard, against the original: a contact
//! with the track (0x80035a88: its impact, scrape and timer), two bodies
//! crashing (0x80035c7c and 0x80016a18, with the tone's random number), a
//! player's tyres keyed (0x80016490), the tyres' input (0x80045a84) and
//! the mixer's volumes for those voices (0x80017928). The sound chip is
//! stood in for: keys on and off are logged, and which voices play is the
//! test's choice.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use hwtr_game::car::Car;
use hwtr_game::collision::pairs::crash_hit;
use hwtr_game::collision::walls::contact_volume;
use hwtr_game::collision::world::Hit;
use hwtr_game::engines::{self, Bank, Change, Effect, Engines, HitTables, Listener};
use hwtr_game::math::Tables;
use hwtr_game::rand::Rand;
use hwtr_hle::original::car::{CAR_SIZE, CARS};
use hwtr_hle::original::rand::SEED;
use hwtr_hle::original::{InMemory, Ram};

const LISTENER: u32 = 0x8011_ac60;
const LISTENER_VEL: u32 = 0x8011_ac70;
const FORWARD: u32 = 0x8011_ac80;
const UP: u32 = 0x8011_ac90;
const VOLUME: u32 = 0x800d_24c8;
const SOUNDS: u32 = 0x8011_a840;
const SOUND_SIZE: u32 = 104;
const TIMERS: u32 = 0x8012_8e94;
const EFFECTS: u32 = 0x8011_aec0;
const IMPORTANCE: u32 = 0x8011_acc0;
const PLAYERS: u32 = 0x800d_24d0;
const COUNT: u32 = 0x800d_24d1;
const FIRST: u32 = 0x800d_24d2;
const HUSH: u32 = 0x800d_2623;
const FIFTH: u32 = 0x800d_0c38;
const THREE_FIFTHS: u32 = 0x800d_0c3c;
const SHADOW: u32 = 0x8014_31e4;
/// Scratch memory for the contacts, pairs and bodies made up here.
const ARGS: u32 = 0x801f_8000;

/// A key on as libsnd's `SsUtKeyOnV` (0x800a67c4) gets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Keyed {
    voice: i32,
    vab: i32,
    program: i32,
    tone: i32,
    note: i32,
    fine: i32,
    left: i32,
    right: i32,
}

struct Rig {
    keyed: Rc<RefCell<Vec<Keyed>>>,
    off: Rc<RefCell<Vec<i32>>>,
    alive: Rc<RefCell<[bool; 24]>>,
}

fn playing(alive: &[bool; 24], voice: i32) -> bool {
    (0..24).contains(&voice) && alive[voice as usize]
}

/// The sound chip stood in for.
fn rig(m: &mut hwtr_cpu::Machine) -> Rig {
    let rig = Rig { keyed: Rc::default(), off: Rc::default(), alive: Rc::new(RefCell::new([false; 24])) };
    let keyed = rig.keyed.clone();
    m.hook(0x800a_67c4, move |cpu, bus| {
        let arg = |bus: &mut hwtr_cpu::Bus, k: u32| bus.read(cpu.r[29] + 16 + 4 * k, 4).unwrap() as i16 as i32;
        let k = Keyed {
            voice: cpu.r[4] as i16 as i32,
            vab: cpu.r[5] as i16 as i32,
            program: cpu.r[6] as i16 as i32,
            tone: cpu.r[7] as i16 as i32,
            note: arg(bus, 0),
            fine: arg(bus, 1),
            left: arg(bus, 2),
            right: arg(bus, 3),
        };
        // libsnd keys nothing on voice -1.
        if k.voice >= 0 {
            keyed.borrow_mut().push(k);
        }
        cpu.r[4] as i16 as i32 as u32
    });
    let off = rig.off.clone();
    m.hook(0x800a_6b20, move |cpu, _| {
        off.borrow_mut().push(cpu.r[4] as i16 as i32);
        0
    });
    let alive = rig.alive.clone();
    m.hook(0x8001_9c18, move |cpu, bus| {
        let voice = bus.read(cpu.r[4], 2).unwrap() as u16 as i16 as i32;
        playing(&alive.borrow(), voice) as u32
    });
    let alive = rig.alive.clone();
    m.hook(0x8001_9cf8, move |cpu, _| playing(&alive.borrow(), cpu.r[4] as i16 as i32) as u32);
    rig
}

fn voice_at(ram: &Ram, at: u32) -> Option<usize> {
    let v = ram.i16(at);
    (v >= 0).then_some(v as usize)
}

/// The engines' sound as the state holds it, for the port.
fn engines_from(m: &mut hwtr_cpu::Machine, byte: &dyn Fn(u32) -> u8) -> Engines {
    let ram = Ram(&mut m.bus.ram);
    let (players, cars) = (ram.u8(PLAYERS) as usize, ram.u8(COUNT) as usize);
    let mut e = Engines::new(engines::kinds(byte), &vec![None; cars], players, Effect::default());
    e.first = ram.u8(FIRST) as usize;
    e.importance = std::array::from_fn(|v| ram.u8(IMPORTANCE + v as u32));
    e.effects = (0..61u32)
        .map(|id| {
            let at = EFFECTS + 16 * id;
            Effect { tone: ram.i16(at) as u8, program: ram.i16(at + 4) as u8, note: ram.i16(at + 8) as u8 }
        })
        .collect();
    e.hits = HitTables::read(byte);
    e.listener =
        Listener { pos: ram.vec3(LISTENER), vel: ram.vec3(LISTENER_VEL), forward: ram.vec3(FORWARD), up: ram.vec3(UP) };
    e.volume = ram.i32(VOLUME);
    for (slot, car) in e.cars.iter_mut().enumerate() {
        let at = SOUNDS + SOUND_SIZE * slot as u32;
        let timer = TIMERS + 16 * slot as u32;
        car.pos = ram.vec3(at);
        car.vel = ram.vec3(at + 0x10);
        car.bend = ram.i16(at + 0x54) as u16;
        car.tyre_level = ram.i32(at + 0x38);
        car.scrape_level = ram.i32(at + 0x3c);
        car.impact_level = ram.i32(at + 0x40);
        car.tyre_voice = voice_at(&ram, at + 0x4c);
        car.scrape_voice = voice_at(&ram, at + 0x4e);
        car.crash_voice = voice_at(&ram, at + 0x50);
        car.impact_voice = voice_at(&ram, at + 0x52);
        car.tyres = ram.u8(timer + 1);
        car.scrape = ram.u8(timer + 8);
        car.scrape_ms = ram.i32(timer + 12) as u32;
        car.tyres_keyed = if slot >= 4 {
            0
        } else if ram.u8(FIFTH + slot as u32) != 0 {
            16
        } else if ram.u8(THREE_FIFTHS + slot as u32) != 0 {
            23
        } else {
            0
        };
    }
    e
}

fn keyed_of(changes: &[Change]) -> Vec<Keyed> {
    changes
        .iter()
        .filter_map(|c| match *c {
            Change::KeyOn { voice, bank, program, tone, note, fine, left, right } => Some(Keyed {
                voice: voice as i32,
                vab: match bank {
                    Bank::Effects => 0,
                    Bank::Crashes => 1,
                    Bank::Dialog => 3,
                    Bank::Car(_) => -1,
                },
                program: program as i32,
                tone: tone as i32,
                note: note as i32,
                fine: fine as i32,
                left: left as i8 as i32,
                right: right as i8 as i32,
            }),
            _ => None,
        })
        .collect()
}

fn offs_of(changes: &[Change]) -> Vec<i32> {
    changes
        .iter()
        .filter_map(|c| match *c {
            Change::KeyOff { voice } => Some(voice as i32),
            _ => None,
        })
        .collect()
}

/// Makes the state's sound records any way: listener, cars' places,
/// levels, voices, scrapes and the effect voices' importance.
fn scramble(m: &mut hwtr_cpu::Machine, rng: &mut common::Rng, rig: &Rig) {
    let mut ram = Ram(&mut m.bus.ram);
    let cars = ram.u8(COUNT) as u32;
    let listener: [i32; 3] = std::array::from_fn(|_| rng.below(1 << 24) as i32 - (1 << 23));
    ram.set_vec3(LISTENER, listener);
    ram.set_vec3(LISTENER_VEL, std::array::from_fn(|_| rng.below(1 << 22) as i32 - (1 << 21)));
    ram.set_i32(VOLUME, rng.below(128) as i32);
    ram.set_u8(HUSH, 0);
    for v in 0..24 {
        ram.set_u8(IMPORTANCE + v, rng.below(3) as u8);
    }
    let voices = [-1i16, 14, 16, 17, 19, 20, 21, 22, 23];
    for slot in 0..cars {
        let at = SOUNDS + SOUND_SIZE * slot;
        let near = [1 << 16, 1 << 22, 1 << 25][rng.below(3) as usize];
        ram.set_vec3(at, std::array::from_fn(|k| listener[k] + rng.below(2 * near) as i32 - near as i32));
        ram.set_vec3(at + 0x10, std::array::from_fn(|_| rng.below(1 << 22) as i32 - (1 << 21)));
        ram.set_i16(at + 0x54, rng.below(128) as i16);
        for k in 0..3 {
            ram.set_i32(at + 0x38 + 4 * k, rng.below(4097) as i32);
        }
        ram.set_i16(at + 0x48, -1);
        for k in 0..4 {
            ram.set_i16(at + 0x4c + 2 * k, voices[rng.below(voices.len() as u32) as usize]);
        }
        let timer = TIMERS + 16 * slot;
        ram.set_u8(timer + 8, [0, 40, 41, 42, 43][rng.below(5) as usize]);
        ram.set_i32(timer + 12, [0, 0, 1, 25, 200][rng.below(5) as usize]);
        // The muffles are four bytes each, and only the first two cars'
        // are ever keyed.
        let keyed = if slot < 2 { rng.below(3) } else { 0 };
        if slot < 4 {
            ram.set_u8(FIFTH + slot, (keyed == 1) as u8);
            ram.set_u8(THREE_FIFTHS + slot, (keyed == 2) as u8);
        }
    }
    let mut alive = rig.alive.borrow_mut();
    for v in alive.iter_mut() {
        *v = rng.below(2) == 0;
    }
    rig.keyed.borrow_mut().clear();
    rig.off.borrow_mut().clear();
}

fn check_records(m: &mut hwtr_cpu::Machine, e: &Engines, slot: usize, what: &str) {
    let ram = Ram(&mut m.bus.ram);
    let at = SOUNDS + SOUND_SIZE * slot as u32;
    let timer = TIMERS + 16 * slot as u32;
    let car = &e.cars[slot];
    assert_eq!(ram.u8(timer + 8), car.scrape, "{what}: scrape");
    assert_eq!(ram.i32(timer + 12) as u32, car.scrape_ms, "{what}: scrape timer");
    assert_eq!(ram.i32(at + 0x3c), car.scrape_level, "{what}: scrape volume");
    assert_eq!(ram.i32(at + 0x40), car.impact_level, "{what}: impact volume");
    assert_eq!(voice_at(&ram, at + 0x4e), car.scrape_voice, "{what}: scrape voice");
    assert_eq!(voice_at(&ram, at + 0x50), car.crash_voice, "{what}: crash voice");
    assert_eq!(voice_at(&ram, at + 0x52), car.impact_voice, "{what}: impact voice");
}

#[test]
fn a_contact_sounds_as_in_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let t = Tables::from_exe(&exe);
    let view = exe.view();
    let byte = |a: u32| view.u8(a).unwrap_or(0);
    let rig = rig(&mut m);
    let mut rng = common::Rng(0xc0_47ac);
    let cars = Ram(&mut m.bus.ram).u8(COUNT) as u32;
    let (mut impacts, mut scrapes) = (0, 0);
    for round in 0..3000 {
        scramble(&mut m, &mut rng, &rig);
        let slot = rng.below(cars);
        let surface = rng.below(14) as u8;
        let speed = rng.below(0x70_0000) as i32;
        {
            let mut ram = Ram(&mut m.bus.ram);
            let car = CARS + CAR_SIZE * slot;
            ram.set_i32(car + 0x13c, speed);
            ram.set_i32(ARGS, (ARGS + 0x40) as i32);
            ram.set_i32(ARGS + 0x40 + 104, car as i32);
            ram.set_u8(ARGS + 36, surface);
        }
        let mut e = engines_from(&mut m, &byte);
        m.call(0x8003_5a88, &[ARGS]).unwrap();
        let alive = *rig.alive.borrow();
        let hit = Hit::Track { slot: slot as u8, surface, volume: contact_volume(speed) };
        let changes = e.hit(&t, &hit, &|v| alive[v]);
        let what = format!("round {round}: car {slot} on {surface} at {speed:#x}");
        assert_eq!(*rig.keyed.borrow(), keyed_of(&changes), "{what}: keyed");
        assert_eq!(*rig.off.borrow(), offs_of(&changes), "{what}: let go");
        check_records(&mut m, &e, slot as usize, &what);
        impacts += rig.keyed.borrow().iter().filter(|k| k.voice < 20).count();
        scrapes += rig.keyed.borrow().iter().filter(|k| k.voice >= 20).count();
    }
    assert!(impacts > 200 && scrapes > 200, "{impacts} impacts, {scrapes} scrapes");
}

#[test]
fn a_crash_sounds_as_in_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let t = Tables::from_exe(&exe);
    let view = exe.view();
    let byte = |a: u32| view.u8(a).unwrap_or(0);
    let rig = rig(&mut m);
    let mut rng = common::Rng(0xc4_a5e5);
    let cars = Ram(&mut m.bus.ram).u8(COUNT) as u32;
    let mut crashes = 0;
    for round in 0..3000 {
        scramble(&mut m, &mut rng, &rig);
        let seed = rng.word();
        let side = |rng: &mut common::Rng| {
            let car = (rng.below(3) != 0).then(|| rng.below(cars) as u8);
            let size = [1 << 16, 1 << 20, 1 << 22][rng.below(3) as usize];
            let vel: [i32; 3] = std::array::from_fn(|_| rng.below(2 * size) as i32 - size as i32);
            (car, vel)
        };
        let (a, b) = (side(&mut rng), side(&mut rng));
        {
            let mut ram = Ram(&mut m.bus.ram);
            ram.set_i32(SEED, seed as i32);
            for (k, (car, vel)) in [a, b].into_iter().enumerate() {
                let body = ARGS + 0x40 + 0x40 * k as u32;
                let object = ARGS + 0x100 + 0x200 * k as u32;
                ram.set_i32(ARGS + 4 * k as u32, body as i32);
                ram.set_i32(body + 100, if car.is_some() { object as i32 } else { 0 });
                ram.set_i32(body + 104, car.map_or(0, |s| (CARS + CAR_SIZE * s as u32) as i32));
                ram.set_vec3(object + 252, vel);
            }
        }
        let mut e = engines_from(&mut m, &byte);
        m.call(0x8003_5c7c, &[ARGS]).unwrap();
        let mut rand = Rand { seed };
        let vel = |s: (Option<u8>, [i32; 3])| if s.0.is_some() { s.1 } else { [0; 3] };
        let alive = *rig.alive.borrow();
        let mut changes = Vec::new();
        let slot = a.0.max(b.0);
        if let Some(slot) = slot
            && let Some(hit) = crash_hit(&t, slot, hwtr_game::math::sub(vel(a), vel(b)), &mut rand)
        {
            changes = e.hit(&t, &hit, &|v| alive[v]);
        }
        let what = format!("round {round}: {a:?} and {b:?}");
        assert_eq!(m.bus.read_u32(SEED), rand.seed, "{what}: seed");
        assert_eq!(*rig.keyed.borrow(), keyed_of(&changes), "{what}: keyed");
        assert_eq!(*rig.off.borrow(), offs_of(&changes), "{what}: let go");
        if let Some(slot) = slot {
            check_records(&mut m, &e, slot as usize, &what);
        }
        crashes += rig.keyed.borrow().len();
    }
    assert!(crashes > 500, "{crashes} crashes");
}

#[test]
fn tyres_key_as_in_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let t = Tables::from_exe(&exe);
    let view = exe.view();
    let byte = |a: u32| view.u8(a).unwrap_or(0);
    let rig = rig(&mut m);
    let mut rng = common::Rng(0x7e_e5);
    let cars = Ram(&mut m.bus.ram).u8(COUNT) as u32;
    let mut keyed = 0;
    for round in 0..2000 {
        scramble(&mut m, &mut rng, &rig);
        let slot = rng.below(cars);
        let id = [16, 18, 19, 20, 21, 22, 23][rng.below(7) as usize];
        let mut e = engines_from(&mut m, &byte);
        m.call(0x8001_6490, &[slot, id as u32]).unwrap();
        let changes = e.tyres_on(&t, slot as usize, id);
        let what = format!("round {round}: car {slot}'s tyres {id}");
        assert_eq!(*rig.keyed.borrow(), keyed_of(&changes), "{what}: keyed");
        let ram = Ram(&mut m.bus.ram);
        assert_eq!(ram.u8(FIFTH + slot) != 0, id == 16, "{what}: a fifth");
        assert_eq!(ram.u8(THREE_FIFTHS + slot) != 0, id == 23, "{what}: three fifths");
        assert_eq!(
            voice_at(&ram, SOUNDS + SOUND_SIZE * slot + 0x4c),
            e.cars[slot as usize].tyre_voice,
            "{what}: voice"
        );
        keyed += rig.keyed.borrow().len();
    }
    assert!(keyed > 200, "{keyed} keyed");
}

#[test]
fn the_tyres_input_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let view = exe.view();
    let byte = |a: u32| view.u8(a).unwrap_or(0);
    let tables = HitTables::read(&byte);
    let mut rng = common::Rng(0x1e_9075);
    let cars = Ram(&mut m.bus.ram).u8(COUNT) as u32;
    let mut heard = 0;
    for round in 0..2000 {
        let slot = rng.below(cars);
        let at = CARS + CAR_SIZE * slot;
        let mut car = Car::read(&Ram(&mut m.bus.ram), at);
        for w in &mut car.wheels {
            w.on_ground = rng.below(3) != 0;
            w.slipping = rng.below(2) == 0;
            w.surface = rng.below(14) as u8;
        }
        car.wrecked = rng.below(8) == 0;
        car.body.speed = rng.below(0x80_0000) as i32;
        car.write(&mut Ram(&mut m.bus.ram), at);
        let car = Car::read(&Ram(&mut m.bus.ram), at);
        let out = ARGS;
        m.call(
            0x8004_5a84,
            &[slot, out, out + 0x10, out + 0x20, out + 0x24, out + 0x28, out + 0x2c, out + 0x30, out + 0x34],
        )
        .unwrap();
        let ram = Ram(&mut m.bus.ram);
        let port = engines::input_of(&car, 0, &tables.priority);
        let what = format!("round {round}: car {slot}");
        assert_eq!(ram.u8(out + 0x28), port.ground, "{what}: ground");
        assert_eq!(ram.i32(out + 0x2c), port.roll, "{what}: roll");
        assert_eq!(ram.i32(out + 0x30), port.skid, "{what}: skid");
        assert_eq!(ram.vec3(out), port.pos, "{what}: place");
        heard += (port.ground != 0) as u32;
    }
    assert!(heard > 500, "{heard} heard");
}

#[test]
fn the_mixer_sets_the_hits_volumes_as_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let t = Tables::from_exe(&exe);
    let view = exe.view();
    let byte = |a: u32| view.u8(a).unwrap_or(0);
    let rig = rig(&mut m);
    let mut rng = common::Rng(0x313_4175);
    let cars = Ram(&mut m.bus.ram).u8(COUNT) as usize;
    let mut checked = 0;
    for round in 0..500 {
        scramble(&mut m, &mut rng, &rig);
        // Only the hits' voices play, not the engines'.
        for v in 0..14 {
            rig.alive.borrow_mut()[v] = false;
        }
        let mut ram = Ram(&mut m.bus.ram);
        for v in 14..24 {
            ram.set_i16(SHADOW + 16 * v as u32, -1);
        }
        // Nor do the world's sounds (0x8011aab0, 12 of 36 bytes).
        for k in 0..12 {
            ram.set_i16(0x8011_aab0 + 36 * k, -1);
        }
        let mut e = engines_from(&mut m, &byte);
        let listener = e.listener;
        let inputs: Vec<_> =
            e.cars.iter().map(|c| engines::EngineInput { pos: c.pos, vel: c.vel, ..Default::default() }).collect();
        m.call(0x8001_7928, &[]).unwrap();
        let alive = *rig.alive.borrow();
        let changes = e.mixer(&t, &inputs, &listener, e.volume, &|v| alive[v]);
        let mut volumes = std::collections::HashMap::new();
        for c in changes {
            if let Change::Volume { voice, left, right } = c {
                volumes.insert(voice, (left as u16 * 129, right as u16 * 129));
            }
        }
        let ram = Ram(&mut m.bus.ram);
        for v in 14..24u32 {
            let got = (ram.i16(SHADOW + 16 * v) as u16, ram.i16(SHADOW + 16 * v + 2) as u16);
            let want = volumes.get(&(v as usize)).copied().unwrap_or((0xffff, 0xffff));
            let low = ram.i16(SHADOW + 16 * v) as u16;
            if want == (0xffff, 0xffff) {
                assert_eq!(low, 0xffff, "round {round}: voice {v} set by the original only ({got:?})");
            } else {
                assert_eq!(got, want, "round {round}: voice {v}'s volume ({cars} cars)");
                checked += 1;
            }
        }
    }
    assert!(checked > 1000, "{checked} checked");
}

/// The commentator's state (0x800d2628): the line, 0 while one waits,
/// the time.
const LINE: u32 = 0x800d_2628;
const SPOKEN: u32 = 0x800d_2629;
const SINCE: u32 = 0x800d_262c;

#[test]
fn the_commentator_asks_and_speaks_as_in_the_original() {
    use hwtr_game::race::{Commentary, dialog_tone};
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let view = exe.view();
    let byte = |a: u32| view.u8(a).unwrap_or(0);
    let rig = rig(&mut m);
    let mut rng = common::Rng(0xd1a_106);
    let mut spoken = 0;
    for round in 0..3000 {
        scramble(&mut m, &mut rng, &rig);
        // Asked for: any state, any line.
        let c = Commentary {
            line: rng.below(4) as u8,
            waiting: rng.below(2) == 0,
            ms: [0, 300, 1000, 1001, 5000][rng.below(5) as usize],
        };
        {
            let mut ram = Ram(&mut m.bus.ram);
            ram.set_u8(LINE, c.line);
            ram.set_u8(SPOKEN, (!c.waiting) as u8);
            ram.set_i32(SINCE, c.ms as i32);
        }
        let line = rng.below(4) as u8;
        m.call(0x8003_6484, &[line as u32]).unwrap();
        let mut ours = c;
        ours.ask(line);
        let ram = Ram(&mut m.bus.ram);
        let theirs = Commentary { line: ram.u8(LINE), waiting: ram.u8(SPOKEN) == 0, ms: ram.i32(SINCE) as u32 };
        assert_eq!(theirs, ours, "round {round}: {c:?} asked for {line}");
        // Spoken: the tone drawn, keyed from the dialog bank.
        let line = rng.below(6) as u8;
        let seed = rng.word();
        Ram(&mut m.bus.ram).set_i32(SEED, seed as i32);
        let e = engines_from(&mut m, &byte);
        m.call(0x8001_9754, &[line as u32]).unwrap();
        let mut rand = Rand { seed };
        let tone = dialog_tone(line, &mut rand);
        assert_eq!(m.bus.read_u32(SEED), rand.seed, "round {round}: line {line}'s seed");
        let alive = *rig.alive.borrow();
        let volume = (Ram(&mut m.bus.ram).i16(VOLUME) as i32 * 3 / 8) as u8;
        let changes = e.dialog(tone, volume, &|v| alive[v]);
        assert_eq!(*rig.keyed.borrow(), keyed_of(&changes), "round {round}: line {line} keyed");
        assert_eq!(*rig.off.borrow(), offs_of(&changes), "round {round}: line {line} let go");
        spoken += rig.keyed.borrow().len();
    }
    assert!(spoken > 1000, "{spoken} spoken");
}
