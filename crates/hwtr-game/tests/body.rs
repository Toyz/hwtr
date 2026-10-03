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

/// A near-rotation: identity plus small noise, or anything.
fn rotation(rng: &mut common::Rng, wild: bool) -> [[i16; 3]; 3] {
    let mut m = [[0i16; 3]; 3];
    for (i, row) in m.iter_mut().enumerate() {
        for (j, x) in row.iter_mut().enumerate() {
            let near = if i == j { 4096 } else { 0 } + (rng.word() as i32 >> 24);
            *x = if wild { rng.word() as i16 } else { near as i16 };
        }
    }
    m
}

#[test]
fn align_and_damp_spin_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let mut rng = common::Rng(0xa119_0000_0000_0007);
    let Some(mut m) = common::state(&exe, "desert1-speed") else { return };
    let start = m.bus.ram.clone();
    let b = CARS + BODY;
    for round in 0..4000 {
        m.bus.ram.copy_from_slice(&start);
        let axis = rng.below(4) as u8;
        // A unit-ish direction, sometimes one of the body's own columns.
        let rot = rotation(&mut rng, round % 5 == 0);
        // Every seventh round, exactly at the 60-degree threshold: an
        // identity rotation and a direction ½ along the axis.
        let (rot, axis) =
            if round % 7 == 0 { ([[4096, 0, 0], [0, 4096, 0], [0, 0, 4096]], axis.min(2)) } else { (rot, axis) };
        let dir: [i32; 3] = if round % 7 == 0 {
            let mut d = [0; 3].map(|_| (rng.word() as i32) >> 20);
            d[axis as usize] = 2048 + rng.below(3) as i32 - 1;
            d
        } else if round % 3 == 0 {
            let j = rng.below(3) as usize;
            [0, 1, 2].map(|i| rot[i][j] as i32)
        } else {
            [0; 3].map(|_| (rng.word() as i32) >> 19)
        };
        {
            let mut ram = Ram(&mut m.bus.ram);
            ram.set_matrix(b + body::ROT, &rot);
            for k in 0..3 {
                ram.set_i64(b + body::ANG_MOMENTUM + 8 * k, (rng.word() as i32 as i64) << rng.below(24));
            }
        }
        let mut port = m.bus.ram.clone();
        let mut ram = Ram(&mut port);
        let mut body = Body::read(&ram, b);
        if round % 2 == 0 {
            body.align(axis, dir);
            body.write(&mut ram, b);
            m.call(0x8007_1bc0, &[b, axis as u32, dir[0] as u32, dir[1] as u32, dir[2] as u32, 0]).unwrap();
        } else {
            body.damp_spin();
            body.write(&mut ram, b);
            // The original takes the car, whose body this is.
            m.call(0x8003_d5cc, &[b - BODY]).unwrap();
        }
        common::same_ram(&m.bus.ram, &port, b, &[], &format!("round {round}"));
    }
}
