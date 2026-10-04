//! The engines' sound against the original: the distance, side and volume
//! of a sound to the camera, the Doppler shift, the slews, the bend from
//! the revs and the throttle's split (0x8001a36c), and libsnd's pitch
//! bend, in a race with its engines keyed.

mod common;

use hwtr_game::engines::{self, CarEngine, Engines, Listener, doppler, level, pan, slew};
use hwtr_game::math::Tables;
use hwtr_game::snd::{self, Bank};
use hwtr_hle::original::Ram;

const LISTENER: u32 = 0x8011_ac60;
const LISTENER_VEL: u32 = 0x8011_ac70;
const FORWARD: u32 = 0x8011_ac80;
const UP: u32 = 0x8011_ac90;
const VOLUME: u32 = 0x800d_24c8;
const SOUNDS: u32 = 0x8011_a840;
const SOUND_SIZE: u32 = 104;
const SHADOW: u32 = 0x8014_31e4;

fn vec(rng: &mut common::Rng, size: u32) -> [i32; 3] {
    std::array::from_fn(|_| rng.below(2 * size) as i32 - size as i32)
}

/// A unit-ish axis: 16-bit components as the camera's matrix has them.
fn axis(rng: &mut common::Rng) -> [i32; 3] {
    std::array::from_fn(|_| rng.below(8192) as i32 - 4096)
}

#[test]
fn distance_side_and_volume_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let t = Tables::from_exe(&exe);
    let mut rng = common::Rng(0xe9_9195);
    let mut heard = 0;
    for round in 0..3000 {
        let near = [1 << 12, 1 << 20, 1 << 24, 1 << 27][rng.below(4) as usize];
        let listener = Listener {
            pos: vec(&mut rng, near),
            vel: vec(&mut rng, 1 << 22),
            forward: axis(&mut rng),
            up: axis(&mut rng),
        };
        let point: [i32; 3] = std::array::from_fn(|k| listener.pos[k].wrapping_add(rng.below(2 * near) as i32 - near as i32));
        let volume = rng.below(128) as i32;
        {
            let mut ram = Ram(&mut m.bus.ram);
            ram.set_vec3(LISTENER, listener.pos);
            ram.set_vec3(LISTENER_VEL, listener.vel);
            ram.set_vec3(FORWARD, listener.forward);
            ram.set_vec3(UP, listener.up);
            ram.set_i32(VOLUME, volume);
            ram.set_vec3(common::OUT, point);
        }
        let [x, y, z] = point.map(|c| c as u32);
        let [lx, ly, lz] = listener.pos.map(|c| c as u32);
        let d = m.call(0x8001_8e04, &[lx, ly, lz, 0, x, y, z, 0]).unwrap() as i32;
        assert_eq!(d, engines::distance(&t, listener.pos, point), "round {round}: distance");
        let p = m.call(0x8001_8590, &[common::OUT]).unwrap() as i32;
        assert_eq!(p, pan(&listener, point), "round {round}: side");
        let packed = m.call(0x8001_8280, &[common::OUT]).unwrap();
        let [left, right] = level(&t, &listener, point, volume);
        assert_eq!((packed >> 8 & 255, packed & 255), (left as u32, right as u32), "round {round}: volume");
        heard += (packed != 0) as u32;
    }
    assert!(heard > 300, "{heard} heard");
}

#[test]
fn doppler_and_slews_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let t = Tables::from_exe(&exe);
    let mut rng = common::Rng(0xd0_991e);
    for round in 0..3000 {
        let size = [1 << 10, 1 << 20, 1 << 24, 1 << 26][rng.below(4) as usize];
        let (heard, made) = (vec(&mut rng, size), vec(&mut rng, size));
        let bend = rng.below(128) as u16;
        Ram(&mut m.bus.ram).set_vec3(LISTENER_VEL, heard);
        let [x, y, z] = made.map(|c| c as u32);
        let got = m.call(0x8001_8760, &[x, y, z, 0, bend as u32]).unwrap() as i32;
        assert_eq!(got, doppler(&t, heard, made, bend), "round {round}: {heard:?} {made:?} {bend}");
        let (from, to) = (rng.below(1 << 20) as i32 - (1 << 19), rng.below(1 << 20) as i32 - (1 << 19));
        let got = m.call(0x8001_a318, &[from as u32, to as u32]).unwrap() as i32;
        assert_eq!(got, slew(from, to), "round {round}: slew {from} to {to}");
    }
}

/// The race's cars' engine kinds and banks as the state has them.
fn kinds_in(m: &mut hwtr_cpu::Machine, cars: usize) -> Vec<u16> {
    let ram = Ram(&mut m.bus.ram);
    (0..cars as u32).map(|k| ram.i16(SOUNDS + SOUND_SIZE * k + 0x56) as u16).collect()
}

#[test]
fn the_revs_bend_and_the_throttle_split_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let view = exe.view();
    let byte = |a: u32| view.u8(a).unwrap_or(0);
    let tables = snd::Tables::read(&byte);
    let (players, cars) = {
        let ram = Ram(&mut m.bus.ram);
        (ram.u8(0x800d_24d0) as usize, ram.u8(0x800d_24d1) as usize)
    };
    assert!(players >= 1 && cars > players, "{players} player(s) of {cars}");
    let kinds = engines::kinds(&byte);
    let in_state = kinds_in(&mut m, cars);
    let effect = engines::Effect { program: 0, tone: 0, note: 57 };
    let mut port = Engines::new(kinds.clone(), &vec![None; cars], players, effect);
    for (car, &kind) in port.cars.iter_mut().zip(&in_state) {
        car.kind = kind;
    }
    let banks: Vec<Option<Bank>> = in_state
        .iter()
        .map(|&k| {
            let name = kinds[k as usize].bank.to_uppercase();
            let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../work/big/DESERT1BIG/{name}VH"));
            std::fs::read(path).ok().and_then(|vh| Bank::from_vh(&vh, 0))
        })
        .collect();
    let mut rng = common::Rng(0x0e_e15);
    let mut pitched = 0;
    for round in 0..2000 {
        let slot = rng.below(cars as u32) as usize;
        let at = SOUNDS + SOUND_SIZE * slot as u32;
        let idle = 0x3e_8000 + rng.below(0x80_0000) as i32;
        let engine = CarEngine { kind: in_state[slot], redline: idle + 0x100_0000 + rng.below(0x200_0000) as i32, idle };
        let rpm = rng.below(0x800_0000) as i32;
        let throttle = rng.below(4097) as i32;
        let levels: [i32; 4] = std::array::from_fn(|_| rng.below(128 << 12) as i32);
        let (left, right) = (rng.below(103) as i32, rng.below(103) as i32);
        {
            let mut ram = Ram(&mut m.bus.ram);
            ram.set_i32(at + 0x20, throttle);
            ram.set_i32(at + 0x24, rpm);
            ram.set_i32(at + 0x28, engine.idle);
            ram.set_i32(at + 0x2c, engine.redline);
            for (k, l) in levels.iter().enumerate() {
                ram.set_i32(at + 0x58 + 4 * k as u32, *l);
            }
        }
        let car = &mut port.cars[slot];
        car.engine = engine;
        car.rpm = rpm;
        car.throttle = throttle;
        car.levels = levels;
        m.call(0x8001_a36c, &[slot as u32, left as u32, right as u32]).unwrap();
        let changes = port.mix(slot, left, right);
        let ram = Ram(&mut m.bus.ram);
        let original_levels: [i32; 4] = std::array::from_fn(|k| ram.i32(at + 0x58 + 4 * k as u32));
        assert_eq!(original_levels, port.cars[slot].levels, "round {round}: levels");
        assert_eq!(ram.i16(at + 0x54) as u16, port.cars[slot].bend, "round {round}: bend");
        for c in changes {
            match c {
                engines::Change::Volume { voice, left, right } => {
                    let r = SHADOW + 16 * voice as u32;
                    let got = (ram.i16(r) as u16, ram.i16(r + 2) as u16);
                    assert_eq!(got, (left as u16 * 129, right as u16 * 129), "round {round}: voice {voice}'s volume");
                }
                engines::Change::Bend { voice, program, bend } => {
                    // The voice must still be the engine's (libsnd checks).
                    let owner = 0x8014_33c0 + 56 * voice as u32;
                    if ram.i16(owner + 0x14) != program as i16 {
                        continue;
                    }
                    let Some(bank) = &banks[slot] else { continue };
                    let note = ram.i16(owner + 0xe) as i32;
                    let tone = ram.i16(owner + 0x16) as usize;
                    let Some(pitch) = snd::bend_pitch(bank, &tables, program as usize, tone, note, bend) else { continue };
                    let got = ram.i16(SHADOW + 16 * voice as u32 + 4) as u16;
                    assert_eq!(got, pitch, "round {round}: voice {voice}'s pitch at bend {bend}");
                    pitched += 1;
                }
                _ => {}
            }
        }
    }
    assert!(pitched > 500, "{pitched} pitched");
}

/// The one-player mixer as a whole (0x80017928): every car's engine
/// voices' volumes and pitches after it, from the state's own engines,
/// with the cars and the camera moved about.
#[test]
fn the_mixer_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let t = Tables::from_exe(&exe);
    let view = exe.view();
    let byte = |a: u32| view.u8(a).unwrap_or(0);
    let tables = snd::Tables::read(&byte);
    let (players, cars) = {
        let ram = Ram(&mut m.bus.ram);
        (ram.u8(0x800d_24d0) as usize, ram.u8(0x800d_24d1) as usize)
    };
    assert_eq!(players, 1, "a one-player race");
    let kinds = engines::kinds(&byte);
    let in_state = kinds_in(&mut m, cars);
    let effect = engines::Effect { program: Ram(&mut m.bus.ram).i16(0x8011_aec4) as u8, tone: 0, note: 57 };
    let mut port = Engines::new(kinds.clone(), &vec![None; cars], players, effect);
    let banks: Vec<Option<Bank>> = in_state
        .iter()
        .map(|&k| {
            let name = kinds[k as usize].bank.to_uppercase();
            let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../work/big/DESERT1BIG/{name}VH"));
            std::fs::read(path).ok().and_then(|vh| Bank::from_vh(&vh, 0))
        })
        .collect();
    // The sound chip isn't played here, so every voice reads as stopped:
    // the engines' voices are taken as playing and the others as not.
    let engine_voices = 2 * cars as u32;
    m.hook(0x8001_9c18, move |cpu, bus| {
        let voice = bus.read(cpu.r[4], 2).unwrap() as u16 as i16;
        (voice >= 0 && (voice as u32) < engine_voices) as u32
    });
    m.stub(0x8001_9cf8, 0);
    let mut rng = common::Rng(0x313_0e5);
    let mut checked = 0;
    for round in 0..300 {
        let listener = {
            let ram = Ram(&mut m.bus.ram);
            Listener { pos: ram.vec3(LISTENER), vel: ram.vec3(LISTENER_VEL), forward: ram.vec3(FORWARD), up: ram.vec3(UP) }
        };
        let volume = Ram(&mut m.bus.ram).i32(VOLUME);
        let mut inputs = Vec::new();
        for slot in 0..cars {
            let at = SOUNDS + SOUND_SIZE * slot as u32;
            let mut ram = Ram(&mut m.bus.ram);
            let pos: [i32; 3] = std::array::from_fn(|k| listener.pos[k] + rng.below(1 << 25) as i32 - (1 << 24));
            let vel = vec(&mut rng, 1 << 22);
            ram.set_vec3(at, pos);
            ram.set_vec3(at + 0x10, vel);
            ram.set_i32(at + 0x20, rng.below(4097) as i32);
            ram.set_i32(at + 0x24, 0x3e_8000 + rng.below(0x600_0000) as i32);
            let car = &mut port.cars[slot];
            car.kind = in_state[slot];
            car.engine = CarEngine { kind: car.kind, redline: ram.i32(at + 0x2c), idle: ram.i32(at + 0x28) };
            car.rpm = ram.i32(at + 0x24);
            car.throttle = ram.i32(at + 0x20);
            car.levels = std::array::from_fn(|k| ram.i32(at + 0x58 + 4 * k as u32));
            car.bend = ram.i16(at + 0x54) as u16;
            inputs.push(engines::EngineInput { rpm: car.rpm, pedals: 0, pos, vel });
        }
        m.call(0x8001_7928, &[]).unwrap();
        let changes = port.mixer(&t, &inputs, &listener, volume, &|_| true);
        let ram = Ram(&mut m.bus.ram);
        // The last change to each voice is what the shadow registers hold.
        let mut volumes = std::collections::HashMap::new();
        let mut pitches = std::collections::HashMap::new();
        for c in changes {
            match c {
                engines::Change::Volume { voice, left, right } => {
                    volumes.insert(voice, (left as u16 * 129, right as u16 * 129));
                }
                engines::Change::Bend { voice, program, bend } => {
                    let owner = 0x8014_33c0 + 56 * voice as u32;
                    if ram.i16(owner + 0x14) != program as i16 {
                        continue;
                    }
                    let slot = if voice < cars { voice } else { voice - cars };
                    let Some(bank) = &banks[slot] else { continue };
                    let note = ram.i16(owner + 0xe) as i32;
                    let tone = ram.i16(owner + 0x16) as usize;
                    if let Some(p) = snd::bend_pitch(bank, &tables, program as usize, tone, note, bend) {
                        pitches.insert(voice, p);
                    }
                }
                _ => {}
            }
        }
        for (voice, v) in volumes {
            let r = SHADOW + 16 * voice as u32;
            assert_eq!((ram.i16(r) as u16, ram.i16(r + 2) as u16), v, "round {round}: voice {voice}'s volume");
            checked += 1;
        }
        for (voice, p) in pitches {
            assert_eq!(ram.i16(SHADOW + 16 * voice as u32 + 4) as u16, p, "round {round}: voice {voice}'s pitch");
            checked += 1;
        }
        for slot in 0..cars {
            let at = SOUNDS + SOUND_SIZE * slot as u32;
            assert_eq!(ram.i16(at + 0x54) as u16, port.cars[slot].bend, "round {round}: car {slot}'s bend");
        }
    }
    assert!(checked > 1000, "{checked} checked");
}
