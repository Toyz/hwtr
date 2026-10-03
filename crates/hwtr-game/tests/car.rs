//! The car update against the original, on cars from saved races.

mod common;

use hwtr_game::car::{self, CAR_SIZE, CARS, WHEEL_SIZE, WHEELS};
use hwtr_game::math::Tables;
use hwtr_game::ram::Ram;

const STATES: [&str; 2] = ["desert1-race", "desert1-drive"];

/// The words the original fills from uninitialised stack: each wheel's
/// heading padding.
fn wheel_padding(car: u32) -> Vec<(u32, u32)> {
    (0..4).map(|i| (car + WHEELS + i * WHEEL_SIZE + car::wheel::HEADING + 12, 4)).collect()
}

#[test]
fn update_wheels_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let t = Tables::from_exe(&exe);
    let mut rng = common::Rng(0x77ee_1234_5678_9abc);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let cars = m.bus.read_u32(car::CAR_COUNT);
        assert!(cars > 0 && cars <= 8, "{name}: {cars} cars");
        let start = m.bus.ram.clone();
        // Each car as saved, then each again with its steering, wheel flags,
        // ground contact and normals scrambled to reach every path.
        for round in 0..64 {
            for k in 0..cars {
                let at = CARS + k * CAR_SIZE;
                m.bus.ram.copy_from_slice(&start);
                if round > 0 {
                    let mut ram = Ram(&mut m.bus.ram);
                    ram.set_i32(at + car::STEER, (rng.word() as i32) >> rng.below(32));
                    for i in 0..4 {
                        let w = at + WHEELS + i * WHEEL_SIZE;
                        ram.set_u8(w + car::wheel::FLAGS, rng.word() as u8);
                        ram.set_u8(w + car::wheel::ON_GROUND, (rng.below(3) != 0) as u8);
                        let n = [0; 3].map(|_| (rng.word() as i32) >> (18 + rng.below(14)));
                        ram.set_vec3(w + car::wheel::NORMAL, n);
                        // Every fourth round, normals at the threshold
                        // against a straight-up body.
                        if round % 4 == 0 {
                            ram.set_vec3(w + car::wheel::NORMAL, [0, 0, -2046 - rng.below(5) as i32]);
                        }
                    }
                    if round % 4 == 0 {
                        ram.set_vec3(at + car::UP, [0, 0, 4096]);
                    }
                }
                let mut port = m.bus.ram.clone();
                car::update_wheels(&t, &mut Ram(&mut port), at);
                m.call(0x8004_0a90, &[at]).unwrap();
                common::same_ram(&m.bus.ram, &port, at, &wheel_padding(at), &format!("{name} car {k} round {round}"));
            }
        }
    }
}

#[test]
fn aero_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let mut rng = common::Rng(0xae20_0000_1111_2222);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let cars = m.bus.read_u32(car::CAR_COUNT);
        let start = m.bus.ram.clone();
        for round in 0..256 {
            for k in 0..cars {
                let at = CARS + k * CAR_SIZE;
                m.bus.ram.copy_from_slice(&start);
                if round > 0 {
                    // Speeds and velocities over every magnitude, flags,
                    // contact, and the tuning bytes.
                    let mut ram = Ram(&mut m.bus.ram);
                    let big = |rng: &mut common::Rng| (rng.word() as i32) >> (8 + rng.below(24));
                    let v = [big(&mut rng), big(&mut rng), big(&mut rng)];
                    ram.set_vec3(at + car::VEL, v);
                    let speed = if round % 2 == 0 { big(&mut rng).abs() } else { 4096 + rng.below(3) as i32 - 1 };
                    ram.set_i32(at + car::SPEED, speed);
                    ram.set_vec3(at + car::DRAG_POINT, [big(&mut rng), big(&mut rng), big(&mut rng)]);
                    ram.set_i32(at + car::FLAGS, rng.word() as i32);
                    ram.set_u8(at + car::GROUNDED, rng.below(3) as u8);
                    for b in [10, 33, 34, 36, 37, 39, 40] {
                        ram.set_u8(car::TUNING + b, rng.word() as u8);
                    }
                }
                let out = common::OUT;
                let mut port = m.bus.ram.clone();
                let (a, b) = car::aero(&mut Ram(&mut port), at);
                {
                    let mut ram = Ram(&mut port);
                    ram.set_i32(out, a);
                    ram.set_i32(out + 4, b);
                }
                m.call(0x8004_1af0, &[at, out, out + 4]).unwrap();
                common::same_ram(&m.bus.ram, &port, at, &[], &format!("{name} car {k} round {round}"));
            }
        }
    }
}
