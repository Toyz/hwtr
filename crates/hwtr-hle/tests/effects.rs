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
    for round in 0..400 {
        m.bus.ram.copy_from_slice(&start);
        let mut ours = codec::read(&Ram(&mut m.bus.ram));
        scramble(&mut rng, &mut ours);
        let seed = rng.word();
        codec::write(&ours, &mut Ram(&mut m.bus.ram));
        let ours0 = codec::read(&Ram(&mut m.bus.ram));
        let mut ours = ours0.clone();
        Ram(&mut m.bus.ram).set_i32(SEED, seed as i32);
        let mut rand = Rand { seed };
        match round % 8 {
            7 => {
                // Player one's boost flame lit, then drawn frame after frame
                // to its end.
                let slot = 0u32;
                let (model, pose, h, exhaust, special, human, shown) = {
                    let ram = Ram(&mut m.bus.ram);
                    let model = codec::model(&ram, slot);
                    let root = ram.i32(model + 4) as u32;
                    let cvs = ram.i32(model + 16) as u32;
                    let rot: hwtr_game::math::Matrix = std::array::from_fn(|r| std::array::from_fn(|c| ram.i16(root + 6 * r as u32 + 2 * c as u32)));
                    let at = ram.vec3(root + 0x14).map(|c| c >> 1);
                    let car = Car::read(&ram, CARS);
                    let id = ram.u8(cvs + 0x10) as u32;
                    let ex = std::array::from_fn(|k| ram.u8(effects::EXHAUSTS + 41 * id + k as u32) as i8);
                    let flags = ram.i32(cvs + 0x20) as u32;
                    (model, effects::CarPose { at, rot, origin: car.handling.origin }, car.handling.clone(), ex, id == 8 || id == 21, flags & 0x80 != 0, ram.u8(cvs + 0x1ef) != 0)
                };
                ours.flame_start(slot as u8, human, shown);
                m.call(0x8002_af60, &[slot]).unwrap();
                check(format!("round {round} flame lit"), &ours, rand.seed, &mut m);
                assert!(ours.flames[0].on, "the flame lit");
                {
                    let mut ram = Ram(&mut m.bus.ram);
                    for k in 0..8 {
                        ram.set_i32(ARGS + 0x40 + 4 * k, 0);
                    }
                    ram.set_i16(ARGS + 0x40, 4096);
                    ram.set_i16(ARGS + 0x48, 4096);
                    ram.set_i16(ARGS + 0x50, 4096);
                }
                for f in 0..100u32 {
                    let now = Ram(&mut m.bus.ram).i32(0x800d_240c) as u32;
                    let _ = ours.draw_flame(&t, &mut rand, slot as u8, &pose, &h, exhaust, special, false, now);
                    m.call(0x8002_b05c, &[model, ARGS + 0x40]).unwrap();
                    check(format!("round {round} flame frame {f}"), &ours, rand.seed, &mut m);
                    ours.frame_count = ours.frame_count.wrapping_add(1);
                    let mut ram = Ram(&mut m.bus.ram);
                    ram.set_i32(codec::FRAME_COUNT, ours.frame_count as i32);
                    ram.set_i32(0x800d_240c, now.wrapping_add(17) as i32);
                }
            }
            6 => {
                // A world volume knocked over: its dust, smoke and quads.
                let (o, flags, pos, height, quads) = {
                    let ram = Ram(&mut m.bus.ram);
                    let header = ram.i32(0x800d_2540) as u32;
                    let (n, table) = (ram.i32(header + 48) as u32, ram.i32(header + 52) as u32);
                    let o = table + 84 * rng.below(n.max(1));
                    let object = ram.i32(o + 4) as u32;
                    let valid = object % 4 == 0 && (0x8000_0000..0x8020_0000).contains(&object);
                    let quads: Vec<effects::ChunkFace> = if !valid {
                        Vec::new()
                    } else {
                        let (nq, at) = (ram.i32(object + 40) as u32, ram.i32(object + 44) as u32);
                        (0..nq)
                            .map(|k| {
                                let q = at + 76 * k;
                                effects::ChunkFace {
                                    verts: std::array::from_fn(|c| std::array::from_fn(|a| ram.i16(q + 8 * c as u32 + 2 * a as u32))),
                                    uv: std::array::from_fn(|c| [ram.u8(q + 8 * c as u32 + 6), ram.u8(q + 8 * c as u32 + 7)]),
                                    clut: ram.i16(q + 68) as u16,
                                    tpage: ram.i16(q + 70) as u16,
                                }
                            })
                            .collect()
                    };
                    (o, ram.i32(o) as u32, ram.vec3(o + 8), ram.i32(o + 32), quads)
                };
                // Any of the debris' flags, whatever this volume's are.
                let flags = (flags & !0x23) | [0, 1, 2, 3, 0x21, 0x23, 0x20][rng.below(7) as usize];
                Ram(&mut m.bus.ram).set_i32(o, flags as i32);
                let object = Ram(&mut m.bus.ram).i32(o + 4) as u32;
                if (quads.is_empty() || object % 4 != 0) && flags & 0x20 == 0 {
                    continue;
                }
                let vel = [0; 3].map(|_| rng.word() as i32 >> 8);
                Ram(&mut m.bus.ram).set_vec3(ARGS, vel);
                let draws = effects::PropDraws::take(&mut rand, 0, vel, flags, quads.len());
                ours.prop_debris(&t, &draws, flags, pos, height, &quads);
                m.call(0x8002_e27c, &[o, ARGS]).unwrap();
                check(format!("round {round} prop debris (flags {flags:#x}, {} quads)", quads.len()), &ours, rand.seed, &mut m);
            }
            5 => {
                // A wreck's smoke, embers and chunks from car 0's model.
                let slot = 0u32;
                let (faces, pose, human, model) = {
                    let ram = Ram(&mut m.bus.ram);
                    let model = codec::model(&ram, slot);
                    let root = ram.i32(model + 4) as u32;
                    let cvs = ram.i32(model + 16) as u32;
                    let cwh = ram.i32(cvs + 0x1e4) as u32;
                    let rot: hwtr_game::math::Matrix = std::array::from_fn(|r| std::array::from_fn(|c| ram.i16(root + 6 * r as u32 + 2 * c as u32)));
                    let at = ram.vec3(root + 0x14).map(|c| c >> 1);
                    let pose = effects::CarPose { at, rot, origin: ram.vec3(cwh + 0x80) };
                    let (verts, n, face_at) = (ram.i32(root + 0x2c) as u32, ram.i32(root + 0x34), ram.i32(root + 0x38) as u32);
                    let (clut, tpage) = (ram.i16(root + 0x40) as u16, ram.i16(root + 0x42) as u16);
                    let faces: Vec<effects::ChunkFace> = (0..n.max(0) as u32)
                        .map(|f| {
                            let fa = face_at + 20 * f;
                            let uvat = |o: u32| [ram.u8(fa + o), ram.u8(fa + o + 1)];
                            effects::ChunkFace {
                                verts: std::array::from_fn(|k| {
                                    let v = verts + 8 * ram.u8(fa + k as u32) as u32;
                                    [ram.i16(v), ram.i16(v + 2), ram.i16(v + 4)]
                                }),
                                uv: [uvat(10), uvat(14), uvat(8), uvat(12)],
                                clut,
                                tpage,
                            }
                        })
                        .collect();
                    (faces, pose, ram.u8(cvs + 0x1ee) != 0, model)
                };
                let vel = [0; 3].map(|_| rng.word() as i32 >> 8);
                Ram(&mut m.bus.ram).set_vec3(ARGS, vel);
                let draws = effects::WreckDraws::take(&mut rand, slot as u8, vel, human, faces.len());
                ours.car_wreck(&t, &draws, &pose, &faces);
                let _ = model;
                m.call(0x8002_9e10, &[slot, 0, vel[0] as u32, vel[1] as u32, vel[2] as u32]).unwrap();
                check(format!("round {round} wreck"), &ours, rand.seed, &mut m);
                // Then a few frames of it: embers, the update, the
                // columns and the chunks drawn.
                {
                    let mut ram = Ram(&mut m.bus.ram);
                    for k in 0..8 {
                        ram.set_i32(ARGS + 0x40 + 4 * k, 0);
                    }
                    ram.set_i16(ARGS + 0x40, 4096);
                    ram.set_i16(ARGS + 0x48, 4096);
                    ram.set_i16(ARGS + 0x50, 4096);
                }
                let cam = [[4096, 0, 0], [0, 4096, 0], [0, 0, 4096]];
                for f in 0..12 {
                    ours.update(false, fps);
                    m.call(0x8002_f354, &[]).unwrap();
                    check(format!("round {round} wreck frame {f} update"), &ours, rand.seed, &mut m);
                    let mut chunks_only = ours.clone();
                    let _ = chunks_only.draw(&t, &cam, fps, false);
                    ours.pools[3] = chunks_only.pools[3].clone();
                    ours.chunk_colour = chunks_only.chunk_colour;
                    m.call(0x8002_f618, &[codec::HEADERS[3], ARGS + 0x40]).unwrap();
                    check(format!("round {round} wreck frame {f} chunks drawn"), &ours, rand.seed, &mut m);
                    let _ = ours.draw_columns(&t, &mut rand, &cam, fps, false, &[pose]);
                    m.call(0x8003_07d0, &[ARGS + 0x40]).unwrap();
                    check(format!("round {round} wreck frame {f} columns"), &ours, rand.seed, &mut m);
                    ours.frame_done(false);
                    Ram(&mut m.bus.ram).set_i32(codec::FRAME_COUNT, ours.frame_count as i32);
                    let tick = Ram(&mut m.bus.ram).i16(0x8011_ec2c);
                    Ram(&mut m.bus.ram).set_i16(0x8011_ec2c, (tick - 1).max(1));
                }
            }
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
                ours.update(false, fps);
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
