//! The collision code against the original, on the track loaded in a saved
//! race.

mod common;

use hwtr_hle::original::InMemory;
use hwtr_game::collision::Scp;
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
use hwtr_hle::original::car::{CAR_COUNT, CAR_SIZE, CARS};
use hwtr_game::collision::Collision;

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
                    car.body.gravity_dir = [[0, 0, -0x1000], [0, 0, 0x1000], [0x1000, 0, 0], [0, -0xb50, -0xb50]]
                        [rng.below(4) as usize];
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
