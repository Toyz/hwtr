//! A car's lights against the original: the frame's end setting their
//! targets and the glows' strength (0x80049ecc), and the car draw
//! (0x80022064) drawing the glows and beams, fading the body and the
//! headlights, and loading the tail lights' palette.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use hwtr_game::car::Car;
use hwtr_game::effects::CarPose;
use hwtr_game::lights::{self, Fade, LampDraw, Lamps};
use hwtr_game::rand::Rand;
use hwtr_hle::original::car::{CAR_COUNT, CAR_SIZE, CARS};
use hwtr_hle::original::rand::SEED;
use hwtr_hle::original::{InMemory, Ram, effects as codec};

/// Scratch memory for the camera the car draw takes.
const ARGS: u32 = 0x801f_8000;
/// The draw mode (0x8001401c's byte; 3 is the frozen results) and the pause
/// byte.
const MODE: u32 = 0x800d_246c;
const PAUSED: u32 = 0x800d_261c;
const STATES: [&str; 3] = ["desert1-race", "desert1-drive", "desert1-speed"];

/// A car's lamps as fxp_parse left them in its view state.
fn lamps_of(ram: &Ram, cvs: u32) -> Lamps {
    let vec = |p: u32| ram.vec3(p);
    let ptr = |a: u32| ram.i32(a) as u32;
    Lamps {
        glows: (0..ram.u8(cvs + 0x2c) as u32)
            .map(|k| (vec(ptr(cvs + 0x30 + 16 * k)), vec(ptr(cvs + 0x34 + 16 * k))))
            .collect(),
        headlights: (0..ram.u8(cvs + 0x2d) as u32)
            .map(|k| (vec(ptr(cvs + 0x134 + 12 * k)), vec(ptr(cvs + 0x138 + 12 * k))))
            .collect(),
    }
}

/// The FXP parser reads the records where fxp_parse points the view state:
/// the part sits in memory as loaded.
#[test]
fn fxp_lamps_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let mut seen = 0;
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let ram = Ram(&mut m.bus.ram);
        for slot in 0..ram.i32(CAR_COUNT) as u32 {
            let cvs = ram.i32(codec::model(&ram, slot) + 16) as u32;
            let theirs = lamps_of(&ram, cvs);
            let (a, b) = (ram.u8(cvs + 0x2c) as u32, ram.u8(cvs + 0x2d) as u32);
            let first = if a > 0 { ram.i32(cvs + 0x30) as u32 - 4 } else { ram.i32(cvs + 0x134) as u32 - 4 };
            let bytes: Vec<u8> = (0..4 + 40 * a + 92 * b).map(|k| ram.u8(first + k)).collect();
            let f = hwtr_data::car::fxp(&bytes).unwrap();
            assert_eq!(Lamps { glows: f.glows, headlights: f.headlights }, theirs, "{name}: car {slot}");
            seen += 1;
        }
    }
    assert!(seen >= 12, "{seen} cars");
}

/// 0x8002b05c's palettes: the tail lights off are each colour at half.
#[test]
fn tail_light_palettes_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let ram = Ram(&mut m.bus.ram);
    for slot in 0..6u32 {
        let skin: [u16; 7] = std::array::from_fn(|k| ram.i16(0x8011_d410 + 14 * slot + 2 * k as u32) as u16);
        let off: [u16; 7] = std::array::from_fn(|k| ram.i16(0x8011_d470 + 14 * slot + 2 * k as u32) as u16);
        assert_eq!(lights::tail_lights(&skin, true), skin);
        assert_eq!(lights::tail_lights(&skin, false), off, "car {slot}");
    }
}

fn scramble_lights(rng: &mut common::Rng) -> hwtr_game::lights::Lights {
    let byte =
        |rng: &mut common::Rng| [0u8, 4, 5, 44, 48, 52, 100, 108, 112, 116, 123, 128, 133, 250][rng.below(14) as usize];
    hwtr_game::lights::Lights {
        body_target: byte(rng),
        lamp_target: byte(rng),
        lamp: byte(rng),
        fading: rng.below(2) == 0,
        brake: rng.below(2) == 0,
        headlights: rng.below(2) == 0,
        glow: if rng.below(3) == 0 { 0 } else { rng.below(4097) as i32 },
    }
}

/// 0x80049ecc's end for every car, with any revs, zone and brake bits,
/// wrecks and lights: the glows' strength and the lights' targets.
#[test]
fn light_targets_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let mut rng = common::Rng(0x11_6475);
    let (mut glowing, mut aimed) = (0, 0);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let start = m.bus.ram.clone();
        let cars = m.bus.read_u32(CAR_COUNT) as usize;
        for round in 0..300 {
            m.bus.ram.copy_from_slice(&start);
            let mut expected = Vec::new();
            {
                let mut ram = Ram(&mut m.bus.ram);
                for slot in 0..cars {
                    let at = CARS + CAR_SIZE * slot as u32;
                    let mut c = Car::read(&ram, at);
                    let band = c.engine.redline - c.engine.idle;
                    c.engine.rpm = c.engine.idle + (rng.below((band + 2000) as u32) as i32) - 1000;
                    c.flags_8 = [0, 1, 2, 3, 4, 5, 6, 7, 8, 12][rng.below(10) as usize];
                    c.wrecked = rng.below(8) == 0;
                    c.write(&mut ram, at);
                    let cvs = ram.i32(codec::model(&ram, slot as u32) + 16) as u32;
                    let wrecked = rng.below(8) == 0;
                    let human = rng.below(2) == 0;
                    ram.set_u8(cvs + 0x28, if wrecked { 1 } else { 0 });
                    ram.set_u8(cvs + 0x1ee, human as u8);
                    let mut l = scramble_lights(&mut rng);
                    codec::write_lights(&mut ram, cvs, &l);
                    let c = Car::read(&ram, at);
                    l.glow = lights::glow_level(&c.engine, c.wrecked);
                    l.aim(c.flags_8, human, wrecked);
                    glowing += (l.glow > 0) as u32;
                    aimed += (human && !wrecked) as u32;
                    expected.push((cvs, l));
                }
            }
            m.call(0x8004_9ecc, &[17]).unwrap();
            let ram = Ram(&mut m.bus.ram);
            for (slot, (cvs, ours)) in expected.iter().enumerate() {
                assert_eq!(codec::read_lights(&ram, *cvs), *ours, "{name} round {round}: car {slot}");
            }
        }
    }
    assert!(glowing > 500 && aimed > 1000, "{glowing} glowing, {aimed} aimed");
}

/// What the car draw gave its lamps' quads: the matrix's translation and
/// the corners, for each lamp, glows and beams apart; and the palettes it
/// loaded.
#[derive(Default)]
struct Drawn {
    matrix: [i32; 3],
    glows: Vec<LampDraw>,
    beams: Vec<LampDraw>,
    palettes: Vec<([i16; 4], [u16; 7])>,
}

fn hook(m: &mut hwtr_cpu::Machine) -> Rc<RefCell<Drawn>> {
    let drawn = Rc::new(RefCell::new(Drawn::default()));
    let d = drawn.clone();
    m.hook(0x8001_5128, move |cpu, bus| {
        let a = cpu.r[4];
        d.borrow_mut().matrix = std::array::from_fn(|k| bus.read(a + 20 + 4 * k as u32, 4).unwrap() as i32);
        0
    });
    let d = drawn.clone();
    m.hook(0x8001_0678, move |cpu, bus| {
        let obj = cpu.r[4];
        let count = bus.read(obj + 40, 4).unwrap();
        let quads = bus.read(obj + 44, 4).unwrap();
        let mut h = |a: u32| bus.read(a, 2).unwrap() as u16 as i16;
        let quads = (0..count)
            .map(|q| {
                std::array::from_fn(|v| {
                    let at = quads + 76 * q + 8 * v as u32;
                    [h(at), h(at + 2), h(at + 4)]
                })
            })
            .collect();
        let mut d = d.borrow_mut();
        let draw = LampDraw { origin: d.matrix, quads };
        match obj {
            0x8011_e064 => d.glows.push(draw),
            0x8011_e0a4 => d.beams.push(draw),
            _ => panic!("drew {obj:#x}"),
        }
        0
    });
    let d = drawn.clone();
    m.hook(0x800a_2a8c, move |cpu, bus| {
        let (data, rect) = (cpu.r[4] + 16, cpu.r[5]);
        let mut h = |a: u32| bus.read(a, 2).unwrap() as u16;
        let r = std::array::from_fn(|k| h(rect + 2 * k as u32) as i16);
        d.borrow_mut().palettes.push((r, std::array::from_fn(|k| h(data + 2 * k as u32))));
        0
    });
    drawn
}

/// The car draw with any lights, glow, wreck mark, body colour, pause and
/// draw mode, a car seen from any distance short of its full model; no
/// shadow or flame. The glows' and beams' corners and places, the lights
/// after, the body's colour, the palette loaded and the seed compared.
#[test]
fn lamps_draw_as_the_original() {
    let Some(exe) = common::exe() else { return };
    let mut rng = common::Rng(0x1a_3b5);
    let (mut glowed, mut beamed, mut faded) = (0, 0, 0);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let drawn = hook(&mut m);
        let start = m.bus.ram.clone();
        let cars = m.bus.read_u32(CAR_COUNT);
        for round in 0..1500 {
            m.bus.ram.copy_from_slice(&start);
            *drawn.borrow_mut() = Drawn::default();
            let slot = rng.below(cars);
            let seed = rng.word();
            let wrecked = rng.below(8) == 0;
            let frozen = rng.below(10) == 0;
            let paused = rng.below(10) == 0;
            let d2 = 1350 * 1350 + 1 + rng.below(8_000_000);
            let grey = [48u32, 53, 100, 123, 128, 130, 0x18][rng.below(7) as usize];
            let (before, pose, lamps, cvs, body_node, id, skin) = {
                let mut ram = Ram(&mut m.bus.ram);
                let model = codec::model(&ram, slot);
                let cvs = ram.i32(model + 16) as u32;
                let root = ram.i32(model + 4) as u32;
                // The model the draw picks: the medium beyond 1350 units,
                // the low beyond 2700.
                assert_eq!((ram.i32(0x800d_2584), ram.i32(0x800d_2588)), (1350 * 1350, 2700 * 2700));
                let drawn = ram.i32(codec::model_at(&ram, slot, if d2 <= 2700 * 2700 { 1 } else { 2 }) + 4) as u32;
                let l = scramble_lights(&mut rng);
                codec::write_lights(&mut ram, cvs, &l);
                ram.set_u8(cvs + 0x28, wrecked as u8);
                ram.set_u8(cvs + 0x1f0, 0);
                ram.set_u8(cvs + 0x1ef, 1);
                ram.set_u8(0x800d_25a0 + slot, 0);
                ram.set_u8(MODE, if frozen { 3 } else { 0 });
                ram.set_u8(PAUSED, paused as u8);
                ram.set_i32(SEED, seed as i32);
                ram.set_i32(drawn + 0x44, (grey << 16 | grey << 8 | grey) as i32);
                for i in 0..3u32 {
                    for j in 0..3u32 {
                        ram.set_i16(ARGS + 6 * i + 2 * j, if i == j { 4096 } else { 0 });
                    }
                }
                ram.set_vec3(ARGS + 20, [0; 3]);
                let rot: [[i16; 3]; 3] =
                    std::array::from_fn(|i| std::array::from_fn(|j| ram.i16(root + 6 * i as u32 + 2 * j as u32)));
                let at = ram.vec3(root + 20).map(|c| c >> 1);
                let skin: [u16; 7] = std::array::from_fn(|k| ram.i16(0x8011_d410 + 14 * slot + 2 * k as u32) as u16);
                (l, CarPose { at, rot, origin: [0; 3] }, lamps_of(&ram, cvs), cvs, drawn, ram.u8(cvs + 0x10), skin)
            };
            m.call(0x8002_2064, &[slot, ARGS, d2, 0, 1]).unwrap();
            let mut rand = Rand { seed };
            let mut l = before;
            let ours_glows =
                if wrecked { Vec::new() } else { lights::glows(&l, &lamps, id, &mut rand, &pose, paused || frozen) };
            let mut ours_beams = Vec::new();
            let mut body = grey << 16 | grey << 8 | grey;
            if l.lamp != 0 {
                if !wrecked {
                    ours_beams = lights::beams(&l, &lamps, &pose);
                }
                l.fade(Fade::Body, &mut body, wrecked, frozen);
            }
            if l.fading {
                l.fade(Fade::Lamps, &mut body, wrecked, frozen);
            }
            let what =
                format!("{name} round {round}: car {slot} (id {id}) {before:?} wrecked {wrecked} frozen {frozen}");
            let d = drawn.borrow();
            assert_eq!(m.bus.read_u32(SEED), rand.seed, "{what}: seed");
            let ram = Ram(&mut m.bus.ram);
            assert_eq!(d.glows, ours_glows, "{what}: glows");
            assert_eq!(d.beams, ours_beams, "{what}: beams");
            assert_eq!(codec::read_lights(&ram, cvs), l, "{what}: lights");
            assert_eq!(ram.i32(body_node + 0x44) as u32 & 0xff_ffff, body, "{what}: body");
            assert_eq!(
                d.palettes,
                vec![([384, 464 + slot as i16, 7, 1], lights::tail_lights(&skin, l.brake))],
                "{what}"
            );
            glowed += !d.glows.is_empty() as u32;
            beamed += !d.beams.is_empty() as u32;
            faded += (before.lamp != l.lamp || before.body_target != l.body_target) as u32;
        }
    }
    assert!(glowed > 1000 && beamed > 300 && faded > 500, "{glowed} glowed, {beamed} beamed, {faded} faded");
}
