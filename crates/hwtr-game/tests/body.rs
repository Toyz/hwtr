//! The rigid-body integrator against the original, on the cars' bodies from
//! saved races.

mod common;

use hwtr_game::body::{Body, layout as body};
use hwtr_game::car::layout::{BODY, CAR_COUNT, CAR_SIZE, CARS};
use hwtr_game::math::Tables;
use hwtr_game::ram::Ram;

const STATES: [&str; 3] = ["desert1-race", "desert1-drive", "desert1-speed"];

#[test]
fn integrate_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let t = Tables::from_exe(&exe);
    let mut rng = common::Rng(0xb0d1_0000_1234_0005);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let cars = m.bus.read_u32(CAR_COUNT);
        let start = m.bus.ram.clone();
        for round in 0..300 {
            for k in 0..cars {
                let b = CARS + k * CAR_SIZE + BODY;
                m.bus.ram.copy_from_slice(&start);
                let mut dt = 0x88; // 1/30 s
                if round > 0 {
                    let mut ram = Ram(&mut m.bus.ram);
                    let big = |rng: &mut common::Rng| (rng.word() as i32) >> (4 + rng.below(28));
                    let wide =
                        |rng: &mut common::Rng| ((rng.word() as i64) << 32 | rng.word() as i64) >> (16 + rng.below(48));
                    dt = rng.below(0x400) as i32;
                    ram.set_u8(b + body::ASLEEP, (rng.below(8) == 0) as u8);
                    ram.set_vec3(b + body::FORCE, [big(&mut rng), big(&mut rng), big(&mut rng)]);
                    ram.set_vec3(b + body::MOMENTUM, [big(&mut rng), big(&mut rng), big(&mut rng)]);
                    for i in 0..3 {
                        ram.set_i64(b + body::TORQUE + 8 * i, wide(&mut rng));
                        ram.set_i64(b + body::ANG_MOMENTUM + 8 * i, wide(&mut rng));
                    }
                    if round % 3 == 0 {
                        // A rotation that is not one, and a random inertia.
                        for i in 0..9 {
                            ram.set_i16(b + body::ROT + 2 * i, rng.word() as i16);
                            ram.set_i64(b + body::INV_INERTIA + 8 * i, wide(&mut rng) >> 20);
                        }
                    }
                    if round % 5 == 1 {
                        ram.set_i32(b + body::INV_MASS, rng.below(0x2000) as i32);
                        ram.set_i32(b + body::GRAVITY, rng.below(0x20_0000) as i32);
                    }
                }
                let mut port = m.bus.ram.clone();
                let mut ram = Ram(&mut port);
                let mut body = Body::read(&ram, b);
                body.integrate(&t, dt);
                body.write(&mut ram, b);
                m.call(0x8006_c504, &[b, dt as u32]).unwrap();
                common::same_ram(&m.bus.ram, &port, b, &[], &format!("{name} car {k} round {round}"));
            }
        }
    }
}
