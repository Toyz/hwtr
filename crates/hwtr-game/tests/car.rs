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
