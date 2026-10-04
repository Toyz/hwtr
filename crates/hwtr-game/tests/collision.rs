//! The collision code against the original, on the track loaded in a saved
//! race.

mod common;

use hwtr_game::collision::Scp;
use hwtr_game::ram::Ram;

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
use hwtr_game::car::layout::{CAR_COUNT, CAR_SIZE, CARS};
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
        let (mut world, _) = Collision::read(&Ram(&mut m.bus.ram));
        let mut cars = cars_from(&mut m.bus.ram);
        let mut rng = common::Rng(0xc011_0000_0000_000a);
        for step in 0..12 {
            // Move the cars, the same on both sides, so their points cross
            // zones.
            for (k, car) in cars.iter_mut().enumerate() {
                let push = [0; 3].map(|_| (rng.word() as i32) >> (12 + rng.below(8)));
                car.body.pos = hwtr_game::math::add(car.body.pos, push);
                if step % 3 == 0 {
                    car.body.asleep = 0;
                }
                car.write(&mut Ram(&mut m.bus.ram), CARS + k as u32 * CAR_SIZE);
            }
            pass(&mut world, &mut cars);
            m.call(addr, &[]).unwrap();
            let (original, _) = Collision::read(&Ram(&mut m.bus.ram));
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
        let (mut world, _) = Collision::read(&Ram(&mut m.bus.ram));
        let mut cars = cars_from(&mut m.bus.ram);
        let mut rng = common::Rng(0xc011_0000_0000_000b);
        let mut changes = 0;
        for step in 0..40 {
            for (k, car) in cars.iter_mut().enumerate() {
                let push = [0; 3].map(|_| (rng.word() as i32) >> (11 + rng.below(6)));
                car.body.pos = hwtr_game::math::add(car.body.pos, push);
                car.body.asleep = 0;
                car.write(&mut Ram(&mut m.bus.ram), CARS + k as u32 * CAR_SIZE);
            }
            let before: Vec<Vec<u16>> = world.objects.iter().map(|o| o.point_zones.clone()).collect();
            world.update_points(&mut cars);
            world.track_zones(&cars);
            m.call(0x8004_e47c, &[]).unwrap();
            m.call(0x8005_15e0, &[]).unwrap();
            let (original, _) = Collision::read(&Ram(&mut m.bus.ram));
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
        let (mut world, _) = Collision::read(&Ram(&mut m.bus.ram));
        let mut cars = cars_from(&mut m.bus.ram);
        let mut rng = common::Rng(0x7ee1_0000_0000_000c);
        for step in 0..40 {
            // Nudge the cars up and down and about, so the wheels reach the
            // ground at every depth.
            for (k, car) in cars.iter_mut().enumerate() {
                let mut push = [0; 3].map(|_| (rng.word() as i32) >> (14 + rng.below(6)));
                push[2] = (rng.word() as i32) >> (13 + rng.below(6));
                car.body.pos = hwtr_game::math::add(car.body.pos, push);
                car.body.asleep = 0;
                car.write(&mut Ram(&mut m.bus.ram), CARS + k as u32 * CAR_SIZE);
            }
            world.update_points(&mut cars);
            world.track_zones(&cars);
            m.call(0x8004_e47c, &[]).unwrap();
            m.call(0x8005_15e0, &[]).unwrap();
            world.wheels(&t, &mut cars);
            m.call(0x8005_1bc0, &[]).unwrap();
            let original = cars_from(&mut m.bus.ram);
            grounded += original.iter().flat_map(|c| &c.wheels).filter(|w| w.on_ground()).count();
            for (k, (a, b)) in original.iter().zip(&cars).enumerate() {
                for (i, (wa, wb)) in a.wheels.iter().zip(&b.wheels).enumerate() {
                    assert_eq!(wa, wb, "{name} step {step}: car {k} wheel {i}");
                }
                assert_eq!(a, b, "{name} step {step}: car {k}");
            }
            let (w, _) = Collision::read(&Ram(&mut m.bus.ram));
            world = w;
            cars = original;
        }
    }
    assert!(grounded > 0, "no wheel touched the ground");
}
