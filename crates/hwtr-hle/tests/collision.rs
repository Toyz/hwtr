//! The collision code against the original, on the track loaded in a saved
//! race.

mod common;

use hwtr_game::collision::Scp;
use hwtr_hle::original::InMemory;
use hwtr_hle::original::Ram;

/// Where the game keeps its pointer to the loaded SCP.
const SCP: u32 = 0x800d_2658;

/// The SCP as the saved race has it in RAM.
pub fn scp_from(ram: &mut [u8]) -> Scp {
    let ram = Ram(ram);
    let at = ram.i32(SCP) as u32;
    let sizes = [20, 24, 12, 20, 12, 32];
    let len = 240 + (0..6).map(|k| ram.i32(at + 4 * k) as usize * sizes[k as usize]).sum::<usize>();
    let bytes: Vec<u8> = (0..len as u32).map(|k| ram.u8(at + k)).collect();
    Scp::parse(&bytes).expect("the SCP parses")
}

#[test]
fn zone_at_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-speed") else { return };
    let scp = scp_from(&mut m.bus.ram);
    assert!(scp.zones.len() > 100, "{} zones", scp.zones.len());
    let mut rng = common::Rng(0x2011_e000_0000_0009);
    let car = Ram(&mut m.bus.ram).vec3(0x8012_8fcc + 0x13c - 0x30);
    for round in 0..3000 {
        // Around the car, and anywhere on the track's scale.
        let p = if round % 2 == 0 {
            car.map(|c| c.wrapping_add((rng.word() as i32) >> (8 + rng.below(12))))
        } else {
            [0; 3].map(|_| (rng.word() as i32) >> rng.below(8))
        };
        let want = m.call(0x8004_dc50, &[p[0] as u32, p[1] as u32, p[2] as u32, 0]).unwrap() as u16;
        assert_eq!(scp.zone_at(p), want, "zone_at({p:?})");
    }
}

use hwtr_game::car::Car;
use hwtr_game::collision::Collision;
use hwtr_hle::original::car::{CAR_COUNT, CAR_SIZE, CARS};

const STATES: [&str; 4] = ["desert1-race", "desert1-drive", "desert1-speed", "desert1-air"];

fn cars_from(ram: &mut [u8]) -> Vec<Car> {
    let ram = Ram(ram);
    (0..ram.i32(CAR_COUNT) as u32).map(|k| Car::read(&ram, CARS + k * CAR_SIZE)).collect()
}

/// Runs the original pass at `addr` and the port's `pass` from the same
/// state, a few steps in a row, comparing the world and the cars after each.
fn check_pass(addr: u32, pass: impl Fn(&mut Collision, &mut Vec<Car>)) {
    let Some(exe) = common::exe() else { return };
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { continue };
        let (mut world, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
        let mut cars = cars_from(&mut m.bus.ram);
        let mut rng = common::Rng(0xc011_0000_0000_000a);
        for step in 0..12 {
            // Move the cars, the same on both sides, so their points cross
            // zones.
            for (k, car) in cars.iter_mut().enumerate() {
                let push = [0; 3].map(|_| (rng.word() as i32) >> (12 + rng.below(8)));
                car.body.pos = hwtr_game::math::add(car.body.pos, push);
                if step % 3 == 0 {
                    car.body.asleep = false;
                }
                car.write(&mut Ram(&mut m.bus.ram), CARS + k as u32 * CAR_SIZE);
            }
            pass(&mut world, &mut cars);
            m.call(addr, &[]).unwrap();
            let (original, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
            let original_cars = cars_from(&mut m.bus.ram);
            for (k, (a, b)) in original.objects.iter().zip(&world.objects).enumerate() {
                assert_eq!(a, b, "{name} step {step}: object {k}");
            }
            assert_eq!(original.members, world.members, "{name} step {step}: zone members");
            assert_eq!(original_cars, cars, "{name} step {step}: cars");
            // Carry on from the original's state, so one slip does not
            // cascade.
            world = original;
            cars = original_cars;
        }
    }
}

#[test]
fn update_points_matches_the_original() {
    check_pass(0x8004_e47c, |world, cars| world.update_points(cars));
}

#[test]
fn track_zones_matches_the_original() {
    // Points first (from the moved bodies, the same on both sides), then the
    // zones they are in.
    check_pass(0x8005_15e0, |world, cars| world.track_zones(cars));
}

#[test]
fn points_then_zones_match_the_original() {
    let Some(exe) = common::exe() else { return };
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { continue };
        let (mut world, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
        let mut cars = cars_from(&mut m.bus.ram);
        let mut rng = common::Rng(0xc011_0000_0000_000b);
        let mut changes = 0;
        for step in 0..40 {
            for (k, car) in cars.iter_mut().enumerate() {
                let push = [0; 3].map(|_| (rng.word() as i32) >> (11 + rng.below(6)));
                car.body.pos = hwtr_game::math::add(car.body.pos, push);
                car.body.asleep = false;
                car.write(&mut Ram(&mut m.bus.ram), CARS + k as u32 * CAR_SIZE);
            }
            let before: Vec<Vec<u16>> = world.objects.iter().map(|o| o.point_zones.clone()).collect();
            world.update_points(&mut cars);
            world.track_zones(&cars);
            m.call(0x8004_e47c, &[]).unwrap();
            m.call(0x8005_15e0, &[]).unwrap();
            let (original, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
            changes += original.objects.iter().zip(&before).filter(|(o, b)| o.point_zones != **b).count();
            for (k, (a, b)) in original.objects.iter().zip(&world.objects).enumerate() {
                assert_eq!(a, b, "{name} step {step}: object {k}");
            }
            assert_eq!(original.members, world.members, "{name} step {step}: zone members");
            world = original;
            cars = cars_from(&mut m.bus.ram);
        }
        assert!(changes > 0, "{name}: no point changed zone");
    }
}

#[test]
fn wheels_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let t = hwtr_game::math::Tables::from_exe(&exe);
    let mut grounded = 0;
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { continue };
        let (mut world, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
        let mut cars = cars_from(&mut m.bus.ram);
        let mut rng = common::Rng(0x7ee1_0000_0000_000c);
        for step in 0..40 {
            // Nudge the cars up and down and about, so the wheels reach the
            // ground at every depth.
            for (k, car) in cars.iter_mut().enumerate() {
                let mut push = [0; 3].map(|_| (rng.word() as i32) >> (14 + rng.below(6)));
                push[2] = (rng.word() as i32) >> (13 + rng.below(6));
                car.body.pos = hwtr_game::math::add(car.body.pos, push);
                car.body.asleep = false;
                car.write(&mut Ram(&mut m.bus.ram), CARS + k as u32 * CAR_SIZE);
            }
            world.update_points(&mut cars);
            world.track_zones(&cars);
            m.call(0x8004_e47c, &[]).unwrap();
            m.call(0x8005_15e0, &[]).unwrap();
            world.wheels(&t, &mut cars);
            m.call(0x8005_1bc0, &[]).unwrap();
            let original = cars_from(&mut m.bus.ram);
            grounded += original.iter().flat_map(|c| &c.wheels).filter(|w| w.on_ground).count();
            for (k, (a, b)) in original.iter().zip(&cars).enumerate() {
                for (i, (wa, wb)) in a.wheels.iter().zip(&b.wheels).enumerate() {
                    assert_eq!(wa, wb, "{name} step {step}: car {k} wheel {i}");
                }
                assert_eq!(a, b, "{name} step {step}: car {k}");
            }
            let (w, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
            world = w;
            cars = original;
        }
    }
    assert!(grounded > 0, "no wheel touched the ground");
}

#[test]
fn ground_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let t = hwtr_game::math::Tables::from_exe(&exe);
    let (mut from_wheels, mut from_zones) = (0, 0);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { continue };
        // Every other road zone turns gravity toward its surface, as a
        // loop's do (Desert has none).
        let scp_at = Ram(&mut m.bus.ram).i32(hwtr_hle::original::world::SCP) as u32;
        let zones = Ram(&mut m.bus.ram).i32(scp_at) as u32;
        for z in (1..zones).step_by(2) {
            let at = scp_at + 240 + 20 * z;
            let flags = Ram(&mut m.bus.ram).u8(at + 1);
            if flags & 0x08 != 0 {
                Ram(&mut m.bus.ram).set_u8(at + 1, flags | 0x20);
            }
        }
        let (mut world, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
        let mut cars = cars_from(&mut m.bus.ram);
        let mut rng = common::Rng(0x6e0d_0000_0000_000d);
        for step in 0..40 {
            for (k, car) in cars.iter_mut().enumerate() {
                let mut push = [0; 3].map(|_| (rng.word() as i32) >> (10 + rng.below(8)));
                push[2] = (rng.word() as i32) >> (13 + rng.below(6));
                car.body.pos = hwtr_game::math::add(car.body.pos, push);
                car.body.asleep = false;
                // Now and then the car's zone holds gravity.
                if rng.below(4) == 0 {
                    car.flags ^= 0x400;
                }
                // And now and then gravity turns, so not every road is a
                // floor.
                if rng.below(5) == 0 {
                    car.body.gravity_dir =
                        [[0, 0, -0x1000], [0, 0, 0x1000], [0x1000, 0, 0], [0, -0xb50, -0xb50]][rng.below(4) as usize];
                }
                car.write(&mut Ram(&mut m.bus.ram), CARS + k as u32 * CAR_SIZE);
            }
            world.update_points(&mut cars);
            world.track_zones(&cars);
            world.wheels(&t, &mut cars);
            for addr in [0x8004_e47c, 0x8005_15e0, 0x8005_1bc0] {
                m.call(addr, &[]).unwrap();
            }
            world.ground(&t, &mut cars);
            m.call(0x8005_36b4, &[]).unwrap();
            let original = cars_from(&mut m.bus.ram);
            for (k, (a, b)) in original.iter().zip(&cars).enumerate() {
                assert_eq!(a.ground, b.ground, "{name} step {step}: car {k} ground");
                assert_eq!(a, b, "{name} step {step}: car {k}");
                if a.ground.floor.found {
                    if a.ground.origin == [0; 3] { from_wheels += 1 } else { from_zones += 1 }
                }
            }
            let (w, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
            world = w;
            cars = original;
        }
    }
    assert!(from_wheels > 0 && from_zones > 0, "floors from wheels {from_wheels}, from zones {from_zones}");
}

#[test]
fn walls_match_the_original() {
    use hwtr_hle::original::world::CONTACT_COUNT;
    let Some(exe) = common::exe() else { return };
    let t = hwtr_game::math::Tables::from_exe(&exe);
    let (mut pushed, mut contacts, mut through) = (0, 0, 0);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { continue };
        let (mut world, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
        let mut cars = cars_from(&mut m.bus.ram);
        let mut rng = common::Rng(0x3a11_0000_0000_000e);
        for step in 0..60 {
            // Shove the cars about, hard enough now and then to go through a
            // wall, and spin them so their points move every way.
            for (k, car) in cars.iter_mut().enumerate() {
                let mut push = [0; 3].map(|_| (rng.word() as i32) >> (11 + rng.below(8)));
                push[2] = (rng.word() as i32) >> (12 + rng.below(6));
                car.body.pos = hwtr_game::math::add(car.body.pos, push);
                car.body.vel = [0; 3].map(|_| (rng.word() as i32) >> (8 + rng.below(8)));
                car.body.spin = [0; 3].map(|_| (rng.word() as i32) >> (16 + rng.below(6)));
                car.body.asleep = false;
                car.flags &= !0x3800;
                car.write(&mut Ram(&mut m.bus.ram), CARS + k as u32 * CAR_SIZE);
            }
            world.update_points(&mut cars);
            world.track_zones(&cars);
            world.wheels(&t, &mut cars);
            world.ground(&t, &mut cars);
            for addr in [0x8004_e47c, 0x8005_15e0, 0x8005_1bc0, 0x8005_36b4] {
                m.call(addr, &[]).unwrap();
            }
            let before: Vec<_> = cars.iter().map(|c| c.body.pos).collect();
            world.contacts.clear();
            Ram(&mut m.bus.ram).set_i16(CONTACT_COUNT, 0);
            world.walls(&t, &mut cars);
            m.call(0x8005_4964, &[]).unwrap();
            let (original, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
            let original_cars = cars_from(&mut m.bus.ram);
            assert_eq!(original.contacts, world.contacts, "{name} step {step}: contacts");
            for (k, (a, b)) in original_cars.iter().zip(&cars).enumerate() {
                assert_eq!(a, b, "{name} step {step}: car {k}");
            }
            pushed += original_cars.iter().zip(&before).filter(|(c, p)| c.body.pos != **p).count();
            contacts += original.contacts.len();
            through += original_cars.iter().filter(|c| c.flags & 0x800 != 0 || c.body.asleep).count();
            world = original;
            cars = original_cars;
        }
    }
    eprintln!("pushed {pushed}, contacts {contacts}, through {through}");
    assert!(pushed > 0 && contacts > 0 && through > 0, "pushed {pushed}, contacts {contacts}, through {through}");
}

#[test]
fn computer_walls_match_the_original() {
    use hwtr_hle::original::world::CONTACT_COUNT;
    let Some(exe) = common::exe() else { return };
    let t = hwtr_game::math::Tables::from_exe(&exe);
    let (mut boxed, mut on_road, mut pushed, mut contacts, mut through) = (0, 0, 0, 0, 0);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { continue };
        let (mut world, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
        let mut cars = cars_from(&mut m.bus.ram);
        let mut rng = common::Rng(0xb0c5_0000_0000_0011);
        for step in 0..80 {
            // Wreck or finish some computer cars, turn them every way (half
            // turns about an axis), shove them about and set them moving.
            for (k, car) in cars.iter_mut().enumerate() {
                let mut push = [0; 3].map(|_| (rng.word() as i32) >> (11 + rng.below(8)));
                push[2] = (rng.word() as i32) >> (12 + rng.below(6));
                car.body.pos = hwtr_game::math::add(car.body.pos, push);
                car.body.vel = [0; 3].map(|_| (rng.word() as i32) >> (8 + rng.below(8)));
                car.body.spin = [0; 3].map(|_| (rng.word() as i32) >> (16 + rng.below(6)));
                car.body.asleep = rng.below(8) == 0;
                car.flags &= !0x3800;
                if car.flags & 1 == 0 {
                    car.wrecked = rng.below(3) != 0;
                    car.laps.finished = rng.below(3) == 0;
                    let axis = rng.below(4) as usize;
                    if axis < 3 {
                        for row in car.body.rot.iter_mut() {
                            for (j, c) in row.iter_mut().enumerate() {
                                if j != axis {
                                    *c = c.wrapping_neg();
                                }
                            }
                        }
                    }
                }
                car.write(&mut Ram(&mut m.bus.ram), CARS + k as u32 * CAR_SIZE);
            }
            world.update_points(&mut cars);
            world.track_zones(&cars);
            world.wheels(&t, &mut cars);
            world.ground(&t, &mut cars);
            world.walls(&t, &mut cars);
            for addr in [0x8004_e47c, 0x8005_15e0, 0x8005_1bc0, 0x8005_36b4, 0x8005_4964] {
                m.call(addr, &[]).unwrap();
            }
            let before: Vec<_> = cars.iter().map(|c| c.body.pos).collect();
            world.contacts.clear();
            Ram(&mut m.bus.ram).set_i16(CONTACT_COUNT, 0);
            world.computer_walls(&t, &mut cars);
            m.call(0x8005_72f0, &[]).unwrap();
            let (original, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
            let original_cars = cars_from(&mut m.bus.ram);
            assert_eq!(original.contacts, world.contacts, "{name} step {step}: contacts");
            for (k, (a, b)) in original_cars.iter().zip(&cars).enumerate() {
                assert_eq!(a, b, "{name} step {step}: car {k}");
            }
            boxed += cars.iter().filter(|c| c.flags & 1 == 0 && (c.wrecked || c.laps.finished)).count();
            on_road += world
                .computers
                .iter()
                .filter(|&id| {
                    let o = &world.objects[id];
                    let c = &cars[o.car.unwrap() as usize];
                    (c.wrecked || c.laps.finished) && world.scp.zones[o.point_zones[0] as usize].is_road()
                })
                .count();
            pushed += original_cars.iter().zip(&before).filter(|(c, p)| c.body.pos != **p).count();
            contacts += original.contacts.len();
            through += original_cars.iter().filter(|c| c.flags & 0x800 != 0).count();
            world = original;
            cars = original_cars;
        }
    }
    eprintln!("boxed {boxed}, on the road {on_road}, pushed {pushed}, contacts {contacts}, through {through}");
    assert!(on_road > 0 && pushed > 0 && contacts > 0 && through > 0);
}

#[test]
fn contact_impulses_match_the_original() {
    use hwtr_hle::original::world::{CONTACT_COUNT, CONTACT_SIZE, CONTACTS};
    let Some(exe) = common::exe() else { return };
    let t = hwtr_game::math::Tables::from_exe(&exe);
    let (mut pushed, mut sliding) = (0, 0);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { continue };
        let (mut world, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
        let mut cars = cars_from(&mut m.bus.ram);
        let mut rng = common::Rng(0x1a9c_0000_0000_000f);
        for step in 0..60 {
            for (k, car) in cars.iter_mut().enumerate() {
                let mut push = [0; 3].map(|_| (rng.word() as i32) >> (11 + rng.below(8)));
                push[2] = (rng.word() as i32) >> (12 + rng.below(6));
                car.body.pos = hwtr_game::math::add(car.body.pos, push);
                car.body.vel = [0; 3].map(|_| (rng.word() as i32) >> (8 + rng.below(8)));
                car.body.momentum = car.body.vel.map(|c| c.wrapping_mul(8));
                car.body.spin = [0; 3].map(|_| (rng.word() as i32) >> (16 + rng.below(6)));
                car.body.asleep = false;
                car.flags &= !0x3800;
                car.write(&mut Ram(&mut m.bus.ram), CARS + k as u32 * CAR_SIZE);
            }
            world.update_points(&mut cars);
            world.track_zones(&cars);
            world.wheels(&t, &mut cars);
            world.ground(&t, &mut cars);
            world.contacts.clear();
            Ram(&mut m.bus.ram).set_i16(CONTACT_COUNT, 0);
            world.walls(&t, &mut cars);
            for addr in [0x8004_e47c, 0x8005_15e0, 0x8005_1bc0, 0x8005_36b4, 0x8005_4964] {
                m.call(addr, &[]).unwrap();
            }
            // The original's loop, impulse by impulse, with the friction the
            // port works out.
            let mut port = cars.clone();
            for (k, c) in world.contacts.iter().enumerate() {
                let slot = world.objects[c.object].car.unwrap() as usize;
                let friction = hwtr_game::collision::walls::contact_friction(&t, c.surface, false);
                let size = port[slot].body.impulse(&t, c.point, c.normal, 0x800, friction);
                let original = m.call(0x8006_dc08, &[CONTACTS + k as u32 * CONTACT_SIZE, friction as u32]).unwrap();
                assert_eq!(original as i32, size, "{name} step {step}: contact {k} impulse");
                let after = cars_from(&mut m.bus.ram);
                assert_eq!(after[slot].body, port[slot].body, "{name} step {step}: contact {k} body");
                pushed += (size != 0) as u32;
                sliding += (friction != 0 && size != 0) as u32;
            }
            let (w, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
            world = w;
            cars = cars_from(&mut m.bus.ram);
        }
    }
    assert!(pushed > 0 && sliding > 0, "pushed {pushed}, sliding {sliding}");
    // As the original passes it for a player's car on the road's floor
    // (seen in 0x8006dc08's argument during a race).
    assert_eq!(hwtr_game::collision::walls::contact_friction(&t, 2, false), 0x400);
}

#[test]
fn fences_match_the_original() {
    use hwtr_hle::original::world::CONTACT_COUNT;
    let Some(exe) = common::exe() else { return };
    let (mut pushed, mut contacts, mut tried) = (0, 0, 0);
    let mut rng = common::Rng(0xfe9c_0000_0000_0001);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { continue };
        let start = m.bus.ram.clone();
        let scp = scp_from(&mut m.bus.ram);
        // The fences the zones use.
        let used: Vec<usize> = scp
            .zones
            .iter()
            .flat_map(|z| z.first_fence as usize..z.first_fence as usize + z.fence_count as usize)
            .filter(|&k| k < scp.fences.len())
            .collect();
        assert!(!used.is_empty(), "{name}: no fences");
        for round in 0..300 {
            m.bus.ram.copy_from_slice(&start);
            // The player's car on or near a fence, moving any way.
            let f = scp.fences[used[rng.below(used.len() as u32) as usize]];
            let mut cars = cars_from(&mut m.bus.ram);
            let car = &mut cars[0];
            let off = [0; 3].map(|_| (rng.word() as i32) >> (13 + rng.below(6)));
            car.body.pos = [0, 1, 2].map(|i| f.centre[i].wrapping_sub(car.body.centre[i]).wrapping_add(off[i]));
            car.body.vel = [0; 3].map(|_| (rng.word() as i32) >> (8 + rng.below(8)));
            car.body.asleep = false;
            car.write(&mut Ram(&mut m.bus.ram), CARS);
            // Into the fence's zone, then tracked from there.
            m.call(0x8004_c5f4, &[CARS, scp.zone_at(f.centre) as u32]).unwrap();
            for addr in [0x8004_e47c, 0x8005_15e0] {
                m.call(addr, &[]).unwrap();
            }
            Ram(&mut m.bus.ram).set_i16(CONTACT_COUNT, 0);
            let (mut world, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
            let mut cars = cars_from(&mut m.bus.ram);
            let before = cars[0].body.pos;
            world.fences(&mut cars);
            m.call(0x8005_a4cc, &[]).unwrap();
            let (original, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
            let original_cars = cars_from(&mut m.bus.ram);
            assert_eq!(original.contacts, world.contacts, "{name} round {round}: contacts");
            assert_eq!(original_cars[0], cars[0], "{name} round {round}: the car");
            tried += (original.objects.iter().find(|o| o.car == Some(0)).unwrap().zones.entries.len() > 1) as u32;
            pushed += (original_cars[0].body.pos != before) as u32;
            contacts += original.contacts.len();
        }
    }
    eprintln!("across zones {tried}, pushed {pushed}, contacts {contacts}");
    assert!(pushed > 10 && contacts > 10, "across zones {tried}, pushed {pushed}, contacts {contacts}");
}

#[test]
fn pairs_match_the_original() {
    use hwtr_hle::original::world::{CONTACT_SIZE, PAIR_COUNT, PAIRS};
    let Some(exe) = common::exe() else { return };
    let mut rng = common::Rng(0x9a12_0000_0000_0001);
    let (mut pairs, mut wrecks, mut cached, mut debris) = (0, 0, 0, 0);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { continue };
        let start = m.bus.ram.clone();
        let t = hwtr_hle::original::tables(&m.bus.ram);
        let tuning = hwtr_hle::original::car::tuning(&Ram(&mut m.bus.ram));
        for round in 0..120 {
            m.bus.ram.copy_from_slice(&start);
            // A car driven at another from a little way off, a step at a
            // time, either fast or slow.
            let mut cars = cars_from(&mut m.bus.ram);
            let (a, b) = (rng.below(cars.len() as u32) as usize, 1 + rng.below(cars.len() as u32 - 1) as usize);
            let b = if a == b { (b + 1) % cars.len() } else { b };
            let target = cars[a].body.pos;
            let off = [0; 3].map(|_| (rng.word() as i32) >> (12 + rng.below(3)));
            let off = [off[0], off[1], off[2] >> 4];
            cars[b].body.pos = hwtr_game::math::add(target, off);
            let speed = [1, 2, 4, 8][rng.below(4) as usize];
            let toward = off.map(|c| c.wrapping_neg() / (8 * speed));
            for (k, car) in cars.iter_mut().enumerate() {
                car.body.asleep = false;
                let fast = if round % 2 == 0 { 400 } else { 40 };
                car.body.vel = if k == b { toward.map(|c| c.wrapping_mul(fast)) } else { [0; 3] };
                car.write(&mut Ram(&mut m.bus.ram), CARS + k as u32 * CAR_SIZE);
            }
            let objects = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram)).0.objects.len();
            for step in 0..10 * speed {
                {
                    let mut ram = Ram(&mut m.bus.ram);
                    let mut car = Car::read(&ram, CARS + b as u32 * CAR_SIZE);
                    car.body.pos = hwtr_game::math::add(car.body.pos, toward);
                    car.write(&mut ram, CARS + b as u32 * CAR_SIZE);
                    ram.set_i16(PAIR_COUNT, 0);
                }
                m.call(0x8004_e47c, &[]).unwrap();
                let (mut world, addrs) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
                // A wreck's wheels fly off as objects with bodies of their own
                // (0x8007c9b0, not yet ported): the round ends there.
                if world.objects.len() != objects {
                    debris += 1;
                    break;
                }
                let mut cars = cars_from(&mut m.bus.ram);
                world.pairs.clear();
                world.find_pairs(&t, &mut cars);
                m.call(0x8004_e938, &[]).unwrap();
                let (original, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
                let original_cars = cars_from(&mut m.bus.ram);
                let at = format!("{name} round {round} step {step}");
                assert_eq!(original.pairs, world.pairs, "{at}: pairs");
                assert_eq!(original.separations, world.separations, "{at}: separations");
                assert_eq!(original.depths, world.depths, "{at}: depths");
                assert_eq!(original.objects, world.objects, "{at}: objects");
                assert_eq!(original_cars, cars, "{at}: cars after the push");
                cached += world
                    .pairs
                    .iter()
                    .filter(|p| {
                        world.separations.contains_key(&(world.objects[p.b].id, world.objects[p.a].id))
                            || world.separations.contains_key(&(world.objects[p.a].id, world.objects[p.b].id))
                    })
                    .count();
                // Each pair's crash checks and impulse, by both.
                let mut rand = hwtr_game::rand::Rand { seed: m.bus.read_u32(hwtr_hle::original::rand::SEED) };
                let mut world = original;
                let mut cars = original_cars;
                for k in 0..world.pairs.len() {
                    let p = world.pairs[k];
                    let before: Vec<bool> = cars.iter().map(|c| c.wrecked).collect();
                    world.pair_crashes(&t, &mut cars, &mut rand, k);
                    let pa = PAIRS + k as u32 * CONTACT_SIZE;
                    for (side, obj) in [(0u32, p.a), (1, p.b)] {
                        if world.objects[obj].car.is_some() {
                            m.call(0x8007_e000, &[pa, side]).unwrap();
                        }
                    }
                    let crashed = cars_from(&mut m.bus.ram);
                    // A player's wreck draws random numbers the port does not
                    // (its wheels flying off): past one, the wrecks differ.
                    // A wreck crumples the car's model and a player's throws
                    // its wheels, drawing random numbers the port does not: only
                    // the pair's first wreck, a computer car's, can match.
                    let new = |slot: Option<u8>| slot.filter(|&s| cars[s as usize].wrecked && !before[s as usize]);
                    let first = new(world.objects[p.a].car).or(new(world.objects[p.b].car));
                    for (c, o) in cars.iter().zip(&crashed) {
                        assert_eq!(o.wrecked, c.wrecked, "{at}: pair {k} crash");
                        let comparable =
                            !c.wrecked || before[c.slot as usize] || (first == Some(c.slot) && c.flags & 1 == 0);
                        if comparable {
                            assert_eq!(o, c, "{at}: pair {k} crash");
                        }
                    }
                    // A wreck's random draws differ (the model's crumple is not
                    // yet ported): go on from the original's cars.
                    cars = crashed;
                    world.pair_impulse_only(&t, &tuning, &mut cars, k);
                    m.call(0x8006_f20c, &[pa]).unwrap();
                    let original_cars = cars_from(&mut m.bus.ram);
                    for (c, o) in cars.iter().zip(&original_cars) {
                        assert_eq!(o, c, "{at}: pair {k} impulse");
                    }
                    wrecks += cars.iter().zip(&before).filter(|(c, w)| c.wrecked && !**w).count();
                    pairs += 1;
                    let _ = &addrs;
                }
            }
        }
    }
    eprintln!("pairs {pairs}, cached axes {cached}, wrecks {wrecks}, rounds ended by debris {debris}");
    assert!(pairs > 50, "pairs {pairs}");
}

/// The pad's jolt (iface_controls+0x28, as the running game fills it),
/// hooked: (car slot, level) for each call.
fn hook_jolts(m: &mut hwtr_cpu::Machine) -> std::rc::Rc<std::cell::RefCell<Vec<(u8, u8)>>> {
    let at = m.bus.read_u32(0x8012_fcdc + 0x28);
    let log: std::rc::Rc<std::cell::RefCell<Vec<(u8, u8)>>> = Default::default();
    let l = log.clone();
    m.hook(at, move |cpu, _| {
        l.borrow_mut().push((cpu.r[4] as u8, cpu.r[5] as u8));
        0
    });
    log
}

const JOLT_WAIT: u32 = 0x800d_0e2c;
const SCRATCH: u32 = 0x801f_8000;

/// A player's car striking a wall jolts its pad (0x8005fd4c): any speed,
/// any wall, the wait from the last jolt anywhere.
#[test]
fn wall_jolts_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let mut rng = common::Rng(0x3a11_7017);
    let (mut jolted, mut not) = (0, 0);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let log = hook_jolts(&mut m);
        let start = m.bus.ram.clone();
        for round in 0..500 {
            m.bus.ram.copy_from_slice(&start);
            log.borrow_mut().clear();
            let (world, addrs) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
            let id = world.objects.iter().position(|o| o.car == Some(0)).expect("the player's object");
            let normal = {
                let t = hwtr_game::math::Tables::from_exe(&exe);
                t.normalize([0; 3].map(|_| rng.below(8192) as i32 - 4096))
            };
            let mut ram = Ram(&mut m.bus.ram);
            let mut car = Car::read(&ram, CARS);
            car.body.vel = [0; 3].map(|_| (rng.word() as i32) >> (8 + rng.below(6)));
            car.write(&mut ram, CARS);
            let waits = [[0, 0], [0, 4], [3, 0], [10, 1]][rng.below(4) as usize];
            ram.set_i32(JOLT_WAIT, waits[0]);
            ram.set_i32(JOLT_WAIT + 4, waits[1]);
            ram.set_i32(SCRATCH, addrs[id] as i32);
            ram.set_vec3(SCRATCH + 20, normal);
            let car = Car::read(&ram, CARS);
            let (mut jolts, mut wait) = (Vec::new(), waits);
            hwtr_game::collision::walls::wall_jolt(&mut jolts, &mut wait, 0, &car, normal);
            m.call(0x8005_fd4c, &[SCRATCH]).unwrap();
            let ram = Ram(&mut m.bus.ram);
            let what = format!("{name} round {round}: vel {:?} into {normal:?}", car.body.vel);
            assert_eq!(*log.borrow(), jolts, "{what}: jolts");
            assert_eq!([ram.i32(JOLT_WAIT), ram.i32(JOLT_WAIT + 4)], wait, "{what}: waits");
            if jolts.is_empty() { not += 1 } else { jolted += 1 }
        }
    }
    assert!(jolted > 200 && not > 200, "{jolted} jolted, {not} not");
}

/// A player's car meeting another object jolts its pad (0x8005fed0): any
/// object of the race, made knockable, lifting, heavy or not, the car
/// all-terrain or not, any speeds, any tuning, any wait.
#[test]
fn pair_jolts_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let t = hwtr_game::math::Tables::from_exe(&exe);
    let mut rng = common::Rng(0x9a12_7017);
    let (mut set, mut by_speed, mut none) = (0, 0, 0);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let log = hook_jolts(&mut m);
        let start = m.bus.ram.clone();
        for round in 0..800 {
            m.bus.ram.copy_from_slice(&start);
            log.borrow_mut().clear();
            let (world, addrs) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
            let side = world.objects.iter().position(|o| o.car == Some(0)).expect("the player's object");
            let other = loop {
                let k = rng.below(world.objects.len() as u32) as usize;
                if k != side {
                    break k;
                }
            };
            let mut ram = Ram(&mut m.bus.ram);
            let o = addrs[other];
            let flags = ram.i32(o + 4) & !6 | [0, 2, 4, 6][rng.below(4) as usize];
            ram.set_i32(o + 4, flags);
            ram.set_i32(o + 0x8c, [0, 9000, 10000, 10001, 30000][rng.below(5) as usize]);
            let cars = ram.i32(CAR_COUNT) as u32;
            for k in 0..cars {
                let mut c = Car::read(&ram, CARS + k * CAR_SIZE);
                c.body.vel = [0; 3].map(|_| (rng.word() as i32) >> (8 + rng.below(6)));
                c.all_terrain = rng.below(4) == 0;
                c.write(&mut ram, CARS + k * CAR_SIZE);
            }
            let h = CARS + 0x6b4;
            ram.set_u8(h, ram.u8(h) & !1 | rng.below(4).min(1) as u8 * (rng.below(3) == 0) as u8);
            ram.set_u8(0x8013_6a48, rng.below(256) as u8);
            ram.set_u8(0x8013_6a49, rng.below(256) as u8);
            let waits = [[0, 0], [0, 4], [7, 0], [20, 1]][rng.below(4) as usize];
            ram.set_i32(JOLT_WAIT, waits[0]);
            ram.set_i32(JOLT_WAIT + 4, waits[1]);
            ram.set_i32(SCRATCH, addrs[side] as i32);
            ram.set_i32(SCRATCH + 4, o as i32);
            let tuning =
                hwtr_game::car::Tuning::from_prm(&(0..256).map(|k| ram.u8(0x8013_6a18 + k)).collect::<Vec<_>>());
            let (mut world, _) = hwtr_hle::original::world::collision(&ram);
            let cars_now = cars_from(&mut m.bus.ram);
            world.pair_jolt(&t, side, other, &tuning, &cars_now);
            m.call(0x8005_fed0, &[SCRATCH, 0]).unwrap();
            let ram = Ram(&mut m.bus.ram);
            let what = format!("{name} round {round}: other {:?} flags {flags:#x}", world.objects[other].kind);
            assert_eq!(*log.borrow(), world.jolts, "{what}: jolts");
            assert_eq!([ram.i32(JOLT_WAIT), ram.i32(JOLT_WAIT + 4)], world.jolt_wait, "{what}: waits");
            match (world.jolts.len(), world.jolt_wait != waits) {
                (0, _) => none += 1,
                (_, true) => by_speed += 1,
                _ => set += 1,
            }
        }
    }
    assert!(set > 200 && by_speed > 200 && none > 100, "{set} set, {by_speed} by speed, {none} none");
}

/// A player's wreck's wheels in the world (0x8007c9b0's objects): the
/// original wrecks the player's car at any speed and spin, then, a step at
/// a time from its state, its collision step (0x8004de6c) and the wheels'
/// step (0x8007c894) against the port's: every flying wheel's body, every
/// object, every car and the seed after each.
#[test]
fn flying_wheels_collide_as_in_the_original() {
    use hwtr_game::collision::world::Step;
    use hwtr_hle::original::object::{FLYING, FLYING_SIZE};
    use hwtr_hle::original::rand::SEED;
    let Some(exe) = common::exe() else { return };
    let mut rng = common::Rng(0xf1_c011);
    let (mut steps, mut touching) = (0, 0);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { continue };
        let t = hwtr_hle::original::tables(&m.bus.ram);
        let tuning = hwtr_hle::original::car::tuning(&Ram(&mut m.bus.ram));
        let start = m.bus.ram.clone();
        for round in 0..6 {
            m.bus.ram.copy_from_slice(&start);
            {
                let mut ram = Ram(&mut m.bus.ram);
                for k in 0..8 {
                    ram.set_u8(FLYING + FLYING_SIZE * k, 0);
                }
                let mut car = Car::read(&ram, CARS);
                car.body.vel = [0; 3].map(|_| (rng.word() as i32) >> (6 + rng.below(6)));
                car.body.spin = [0; 3].map(|_| (rng.word() as i32) >> (15 + rng.below(4)));
                car.body.asleep = false;
                car.write(&mut ram, CARS);
            }
            m.call(0x8004_619c, &[CARS, 0, 0]).unwrap();
            for step in 0..60 {
                let (mut world, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
                assert!(world.flying.iter().any(Option::is_some), "{name} round {round}: wheels thrown");
                let mut cars = cars_from(&mut m.bus.ram);
                let (seed, time, clock) = {
                    let ram = Ram(&mut m.bus.ram);
                    (ram.i32(SEED) as u32, ram.i32(0x800d_0e34) as u32, ram.i32(0x800d_240c) as u32)
                };
                let mut rand = hwtr_game::rand::Rand { seed };
                let mut st = Step { tuning: &tuning, rand: &mut rand, time, clock };
                world.update(&t, &mut cars, &mut st);
                m.call(0x8004_de6c, &[]).unwrap();
                world.add_flying();
                hwtr_game::flying::step(&t, &mut world.flying);
                m.call(0x8007_c894, &[]).unwrap();
                let at = format!("{name} round {round} step {step}");
                assert_eq!(m.bus.read_u32(SEED), rand.seed, "{at}: seed");
                let (original, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
                for (k, (a, b)) in original.flying.iter().zip(&world.flying).enumerate() {
                    assert_eq!(a.as_ref().map(|f| &f.body), b.as_ref().map(|f| &f.body), "{at}: wheel {k}");
                }
                for (k, (a, b)) in original.objects.iter().zip(&world.objects).enumerate() {
                    assert_eq!(a, b, "{at}: object {k}");
                }
                assert_eq!(cars_from(&mut m.bus.ram), cars, "{at}: cars");
                touching += original.contacts.iter().filter(|c| original.objects[c.object].flying.is_some()).count();
                steps += 1;
            }
        }
    }
    eprintln!("{steps} steps, {touching} wheel contacts");
    assert!(steps > 1000 && touching > 50, "{steps} steps, {touching} wheel contacts");
}
