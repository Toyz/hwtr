//! The effects against the original: the rings' update (0x8002f354) over
//! records in any state, a puff of dust (0x80030fc8), the trail points
//! (0x80029230) and their skid marks and dust (0x8002b888), and a car's
//! contact spark (0x8002e9f8); the rings, trails and random seed compared.

mod common;

use hwtr_game::car::Car;
use hwtr_game::effects::{self, CAPACITY, Effects};
use hwtr_game::rand::Rand;
use hwtr_hle::original::car::CARS;
use hwtr_hle::original::object::CAR_OBJECT;
use hwtr_hle::original::rand::SEED;
use hwtr_hle::original::{InMemory, Ram, effects as codec};

/// Scratch memory for the arguments the original takes by pointer.
const ARGS: u32 = 0x801f_8000;

fn scramble(rng: &mut common::Rng, e: &mut Effects) {
    for (k, p) in e.pools.iter_mut().enumerate() {
        let cap = CAPACITY[k];
        p.oldest = rng.below(cap as u32) as usize;
        p.next = rng.below(cap as u32) as usize;
        for r in &mut p.records {
            r.pos = [0; 3].map(|_| rng.word() as i32 >> 4);
            r.life = [0, 0, 1, 2, 3, 90, 750, 0xffff][rng.below(8) as usize];
            r.flags = [0x1001, 0x1101, 6, 0x806, 0x20, 0x80, 0][rng.below(7) as usize];
            r.kind = [0, 1, 2, 19, 255][rng.below(5) as usize];
            r.frame = rng.below(256) as u8;
            r.colour = [0; 3].map(|_| rng.below(300) as u16);
            r.buf = rng.below(cap as u32) as usize;
        }
        for v in p.vel.iter_mut().flatten().chain(p.accel.iter_mut().flatten()) {
            *v = [0; 3].map(|_| rng.word() as i32 >> 12);
        }
    }
}

#[test]
fn effects_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-drive") else { return };
    let t = hwtr_hle::original::tables(&m.bus.ram);
    let start = m.bus.ram.clone();
    let fps = Ram(&mut m.bus.ram).i32(0x800d_2578);
    let mut rng = common::Rng(0xeffe_c700_0000_0001);
    let (mut tried, mut same) = (0, 0);
    let mut miss = None;
    let mut check = |what: String, ours: &Effects, seed: u32, m: &mut hwtr_cpu::Machine| {
        let theirs = codec::read(&Ram(&mut m.bus.ram));
        tried += 1;
        if theirs == *ours && m.bus.read_u32(SEED) == seed {
            same += 1;
        } else if miss.is_none() {
            let k = (0..4).find(|&k| theirs.pools[k] != ours.pools[k]);
            let detail = match k {
                Some(k) => format!("pool {k}:\n original {:?}\n port     {:?}", theirs.pools[k], ours.pools[k]),
                None => format!("trails or seed: original seed {:#x}, port {seed:#x}", m.bus.read_u32(SEED)),
            };
            miss = Some(format!("{what}: {detail}"));
        }
    };
    for round in 0..250 {
        m.bus.ram.copy_from_slice(&start);
        let mut ours = codec::read(&Ram(&mut m.bus.ram));
        scramble(&mut rng, &mut ours);
        let seed = rng.word();
        codec::write(&ours, &mut Ram(&mut m.bus.ram));
        let ours0 = codec::read(&Ram(&mut m.bus.ram));
        let mut ours = ours0.clone();
        Ram(&mut m.bus.ram).set_i32(SEED, seed as i32);
        let mut rand = Rand { seed };
        match round % 5 {
            4 => {
                // The puffs drawn: faded, the faded killed.
                {
                    let mut ram = Ram(&mut m.bus.ram);
                    for k in 0..8 {
                        ram.set_i32(ARGS + 4 * k, 0);
                    }
                    ram.set_i16(ARGS, 4096);
                    ram.set_i16(ARGS + 8, 4096);
                    ram.set_i16(ARGS + 16, 4096);
                }
                let cam = [[4096, 0, 0], [0, 4096, 0], [0, 0, 4096]];
                let mut puffs_only = ours.clone();
                let _ = puffs_only.draw(&t, &cam, fps, false);
                ours.pools[0] = puffs_only.pools[0].clone();
                m.call(0x8002_f618, &[codec::HEADERS[0], ARGS]).unwrap();
                check(format!("round {round} puffs drawn"), &ours, rand.seed, &mut m);
            }
            0 => {
                ours.update(false);
                m.call(0x8002_f354, &[]).unwrap();
                check(format!("round {round} update"), &ours, rand.seed, &mut m);
            }
            1 => {
                let pos = [0; 3].map(|_| rng.word() as i32 >> 4);
                let (frame0, surface) = (rng.below(20) as u8, rng.below(16) as u8);
                Ram(&mut m.bus.ram).set_vec3(ARGS, pos);
                ours.dust(&mut rand, fps, pos, frame0, surface, None);
                m.call(0x8003_0fc8, &[ARGS, frame0 as u32, surface as u32, 0]).unwrap();
                check(format!("round {round} dust"), &ours, rand.seed, &mut m);
            }
            2 => {
                // A few cars' wheels report, then the trails are emitted.
                let mut calls = Vec::new();
                for _ in 0..1 + rng.below(12) {
                    let (car, wheel) = (rng.below(6) as usize, rng.below(4) as usize);
                    let on = rng.below(3) != 0;
                    let surface = rng.below(16) as u8;
                    let outer = [0; 3].map(|_| rng.word() as i32 >> 6);
                    let inner = [0; 3].map(|_| rng.word() as i32 >> 6);
                    calls.push((car, wheel, on, surface, outer, inner));
                }
                for &(car, wheel, on, surface, outer, inner) in &calls {
                    ours.trail_push(car, wheel, 4, on, surface, outer, inner);
                    let mut ram = Ram(&mut m.bus.ram);
                    ram.set_vec3(ARGS, outer);
                    ram.set_vec3(ARGS + 16, inner);
                    m.call(0x8002_9230, &[car as u32, wheel as u32, on as u32, surface as u32, ARGS, ARGS + 16]).unwrap();
                }
                check(format!("round {round} trail points"), &ours, rand.seed, &mut m);
                ours.emit_trails(&mut rand, fps, &[true; 6]);
                m.call(0x8002_b888, &[]).unwrap();
                check(format!("round {round} trails"), &ours, rand.seed, &mut m);
            }
            _ => {
                // Player one's car moving any way, touching anything.
                let mut car = Car::read(&Ram(&mut m.bus.ram), CARS);
                car.body.vel = [0; 3].map(|_| (rng.word() as i32) >> (9 + rng.below(6)));
                car.write(&mut Ram(&mut m.bus.ram), CARS);
                let car = Car::read(&Ram(&mut m.bus.ram), CARS);
                let obj = Ram(&mut m.bus.ram).i32(CARS + CAR_OBJECT) as u32;
                let half = Ram(&mut m.bus.ram).vec3(obj + hwtr_hle::original::object::HALF);
                let point = [0; 3].map(|_| rng.word() as i32 >> 6);
                let n = t.normalize([0; 3].map(|_| (rng.word() as i32) >> 16));
                let normal = if rng.below(2) == 0 { hwtr_game::math::column(&car.body.rot, rng.below(3) as usize) } else { n };
                let surface = rng.below(4) as u8;
                {
                    let mut ram = Ram(&mut m.bus.ram);
                    ram.set_i32(ARGS, obj as i32);
                    ram.set_vec3(ARGS + 4, point);
                    ram.set_vec3(ARGS + 20, normal);
                    ram.set_u8(ARGS + 36, surface);
                }
                if let Some(s) = effects::contact_spark(&mut rand, &t, &car.body, half, point, normal, surface) {
                    ours.spark(s);
                }
                m.call(0x8002_e9f8, &[ARGS]).unwrap();
                check(format!("round {round} spark"), &ours, rand.seed, &mut m);
            }
        }
    }
    assert_eq!(same, tried, "{same} of {tried} the same; first miss: {}", miss.unwrap_or_default());
}
