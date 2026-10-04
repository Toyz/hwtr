//! The car update against the original, on cars from saved races.

mod common;

use hwtr_hle::original::InMemory;
use hwtr_hle::original::car::{self as car, CAR_SIZE, CARS, WHEEL_SIZE, WHEELS};
use hwtr_game::car::{Car, Tuning, Wheel};
use hwtr_game::math::{Tables, div_fx, fx};
use hwtr_hle::original::Ram;

/// Runs `f` on the car at `at`, read out of `ram` and written back.
fn on_car<R>(ram: &mut [u8], at: u32, f: impl FnOnce(&mut Car, &Tuning) -> R) -> R {
    let mut ram = Ram(ram);
    let (mut car, tuning) = (Car::read(&ram, at), hwtr_hle::original::car::tuning(&ram));
    let r = f(&mut car, &tuning);
    car.write(&mut ram, at);
    r
}

const STATES: [&str; 3] = ["desert1-race", "desert1-drive", "desert1-speed"];

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
                        ram.set_u8(w + car::wheel::GROUND, (rng.below(3) != 0) as u8);
                        let n = [0; 3].map(|_| (rng.word() as i32) >> (18 + rng.below(14)));
                        ram.set_vec3(w + car::wheel::NORMAL, n);
                        // Every fourth round, normals at the threshold
                        // against gravity straight up or down.
                        if round % 4 == 0 {
                            ram.set_vec3(w + car::wheel::NORMAL, [0, 0, -2046 - rng.below(5) as i32]);
                        }
                    }
                    if round % 4 == 0 {
                        ram.set_vec3(at + car::GRAVITY_DIR, [0, 0, 4096]);
                    }
                }
                let mut port = m.bus.ram.clone();
                on_car(&mut port, at, |car, _| car.place_wheels(&t));
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
                let down = on_car(&mut port, at, |car, tuning| car.aero(tuning));
                let (a, b) = (down.front, down.rear);
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

#[test]
fn drivetrain_matches_the_original() {
    use car::engine;
    let Some(exe) = common::exe() else { return };
    let t = Tables::from_exe(&exe);
    let mut rng = common::Rng(0xd21e_7a11_0000_0001);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let cars = m.bus.read_u32(car::CAR_COUNT);
        let start = m.bus.ram.clone();
        for round in 0..256 {
            for k in 0..cars {
                let at = CARS + k * CAR_SIZE;
                let e = at + car::ENGINE;
                m.bus.ram.copy_from_slice(&start);
                // Cars not under full physics (state 2) have no engine set
                // up; as saved, only those that are.
                if round == 0 && m.bus.ram[((at + 0x891) & 0x1f_ffff) as usize] != 2 {
                    continue;
                }
                if round > 0 {
                    let mut ram = Ram(&mut m.bus.ram);
                    let big = |rng: &mut common::Rng| (rng.word() as i32) >> (4 + rng.below(28));
                    for i in 0..4 {
                        let w = at + WHEELS + i * WHEEL_SIZE;
                        ram.set_u8(w + car::wheel::FLAGS, rng.word() as u8);
                        ram.set_u8(w + car::wheel::GROUND, (rng.below(4) != 0) as u8);
                        ram.set_u8(w + car::wheel::SLIP, (rng.below(3) == 0) as u8);
                        // Never 1: half of it would divide by zero, which
                        // the game's wheels never ask for.
                        let d = [0, -5, 2, 2 + rng.below(0x40000) as i32][rng.below(4) as usize];
                        ram.set_i32(w + car::wheel::DIAMETER, if round % 3 == 0 { 0x1_8000 } else { d });
                        ram.set_vec3(w + car::wheel::CONTACT, [big(&mut rng), big(&mut rng), big(&mut rng)]);
                        // Pointing up, so the mean of any of them is not zero
                        // (the game divides by its length).
                        let mut n = [0; 3].map(|_| (rng.word() as i32) >> (18 + rng.below(14)));
                        n[2] = 0x800 + (n[2] & 0x7ff);
                        ram.set_vec3(w + car::wheel::NORMAL, n);
                    }
                    ram.set_vec3(at + car::VEL, [big(&mut rng), big(&mut rng), big(&mut rng)]);
                    ram.set_vec3(at + car::SPIN, [0; 3].map(|_| (rng.word() as i32) >> (16 + rng.below(16))));
                    ram.set_i32(at + car::ACCEL, rng.below(4097) as i32);
                    ram.set_i32(at + car::BRAKE, rng.below(4097) as i32);
                    ram.set_u8(e + engine::REVERSE, rng.below(2) as u8);
                    let gears = rng.below(7);
                    ram.set_i32(e + engine::GEARS, gears as i32);
                    ram.set_u8(e + engine::GEAR, rng.below(gears.max(1)) as u8);
                    for g in 0..6 {
                        ram.set_i32(e + engine::GEAR_RATIOS + 4 * g, 0x800 + rng.below(0x4000) as i32);
                    }
                    ram.set_i32(e + engine::REVERSE_RATIO, 0x800 + rng.below(0x4000) as i32);
                    ram.set_i32(e + engine::FINAL_DRIVE, 0x2000 + rng.below(0x4000) as i32);
                    let idle = rng.below(2000 << 12) as i32;
                    ram.set_i32(e + engine::IDLE, idle);
                    ram.set_i32(e + engine::REDLINE, idle + 0x1000 + rng.below(8000 << 12) as i32);
                    ram.set_i32(e + engine::RPM, rng.below(9000 << 12) as i32);
                    // Kept by the in-air path and with no forward gears; the
                    // game divides by it.
                    ram.set_i32(e + engine::RATIO, 0x800 + rng.below(0x8000) as i32);
                    ram.set_i32(e + engine::PEAK_TORQUE, rng.below(500 << 12) as i32);
                    for b in 0..17 {
                        ram.set_u8(e + engine::TORQUE_CURVE + b, rng.word() as u8);
                    }
                    // Every fourth round, no wheel grips at full throttle:
                    // the engine sits exactly at the redline.
                    if round % 4 == 1 {
                        for i in 0..4 {
                            ram.set_u8(at + WHEELS + i * WHEEL_SIZE + car::wheel::SLIP, 1);
                        }
                        ram.set_i32(at + car::ACCEL, 4096);
                        ram.set_i32(at + car::BRAKE, 4096);
                    }
                }
                let mut port = m.bus.ram.clone();
                on_car(&mut port, at, |car, _| car.drivetrain(&t));
                m.call(0x8006_0138, &[at]).unwrap();
                common::same_ram(&m.bus.ram, &port, at, &[], &format!("{name} car {k} round {round}"));
            }
        }
    }
}

#[test]
fn car_physics_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let t = Tables::from_exe(&exe);
    let mut rng = common::Rng(0xca2f_0000_5eed_0003);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let cars = m.bus.read_u32(car::CAR_COUNT);
        let start = m.bus.ram.clone();
        for round in 0..400 {
            for k in 0..cars {
                let at = CARS + k * CAR_SIZE;
                // Only cars under full physics have an engine to run.
                if start[((at + 0x891) & 0x1f_ffff) as usize] != 2 {
                    continue;
                }
                m.bus.ram.copy_from_slice(&start);
                let mut wheels = 4;
                if round > 0 {
                    let mut ram = Ram(&mut m.bus.ram);
                    let big = |rng: &mut common::Rng| (rng.word() as i32) >> (6 + rng.below(26));
                    if round % 5 == 0 {
                        // Six wheels: the rear pair twice.
                        wheels = 6;
                        for i in 4..6 {
                            for b in 0..WHEEL_SIZE {
                                let v = ram.u8(at + WHEELS + (i - 2) * WHEEL_SIZE + b);
                                ram.set_u8(at + WHEELS + i * WHEEL_SIZE + b, v);
                            }
                        }
                        ram.set_u8(at + car::WHEEL_COUNT, 6);
                        ram.set_u8(at + car::REAR_WHEELS, 4);
                    }
                    for i in 0..wheels {
                        let w = at + WHEELS + i * WHEEL_SIZE;
                        if rng.below(4) == 0 {
                            ram.set_u8(w + car::wheel::GROUND, 0);
                        }
                        ram.set_u8(w + car::wheel::SURFACE, [2, 6, 0][rng.below(3) as usize]);
                        ram.set_vec3(w + car::wheel::CONTACT_VEL, [big(&mut rng), big(&mut rng), big(&mut rng)]);
                        ram.set_vec3(w + car::wheel::CONTACT, [big(&mut rng), big(&mut rng), big(&mut rng)]);
                        ram.set_i32(w + car::wheel::SPRING, rng.below(2000 << 12) as i32);
                        ram.set_i32(w + car::wheel::FRICTION, rng.below(2 << 12) as i32);
                        if rng.below(3) == 0 {
                            ram.set_u8(w + car::wheel::FLAGS, ram.u8(w + car::wheel::FLAGS) ^ Wheel::REAR);
                        }
                    }
                    ram.set_i32(at + car::ACCEL, rng.below(4097) as i32);
                    ram.set_i32(at + car::BRAKE, rng.below(4097) as i32);
                    ram.set_u8(at + car::HANDBRAKE, rng.below(2) as u8);
                    ram.set_i32(at + car::FLAGS, rng.word() as i32);
                    ram.set_u8(at + 0x865, rng.below(2) as u8);
                    ram.set_i32(at + 0x6b4, rng.word() as i32);
                    ram.set_u8(at + car::AIR_CONTROL, rng.below(2) as u8);
                    ram.set_u8(at + car::AIR_ARMED, rng.below(4) as u8);
                    ram.set_vec3(at + car::SPIN, [0; 3].map(|_| (rng.word() as i32) >> (16 + rng.below(16))));
                    for b in [10, 33, 34, 35, 36, 37, 38, 39, 40, 41, 51] {
                        ram.set_u8(car::TUNING + b, rng.word() as u8);
                    }
                    // Every wheel in the air.
                    if round % 6 == 2 {
                        for i in 0..wheels {
                            ram.set_u8(at + WHEELS + i * WHEEL_SIZE + car::wheel::GROUND, 0);
                        }
                    }
                    // At rest with no spring force and frictionless tyres: no
                    // force at all, so the grip and the tyre's force are both
                    // exactly zero.
                    if round % 7 == 3 {
                        for i in 0..wheels {
                            let w = at + WHEELS + i * WHEEL_SIZE;
                            ram.set_vec3(w + car::wheel::CONTACT_VEL, [0; 3]);
                            ram.set_i32(w + car::wheel::SPRING, 0);
                            ram.set_i32(w + car::wheel::FRICTION, 0);
                        }
                        // The wheels' contact velocities come from these.
                        ram.set_vec3(at + car::VEL, [0; 3]);
                        ram.set_vec3(at + car::SPIN, [0; 3]);
                        ram.set_i32(at + car::ACCEL, 0);
                        ram.set_i32(at + car::BRAKE, 0);
                    }
                }
                let mut port = m.bus.ram.clone();
                on_car(&mut port, at, |car, tuning| car.physics(&t, tuning));
                m.call(0x8004_27a8, &[at]).unwrap();
                let pads: Vec<(u32, u32)> =
                    (0..wheels).map(|i| (at + WHEELS + i * WHEEL_SIZE + car::wheel::HEADING + 12, 4)).collect();
                common::same_ram(&m.bus.ram, &port, at, &pads, &format!("{name} car {k} round {round}"));
            }
        }
    }
}

#[test]
fn wheel_spin_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let mut rng = common::Rng(0x5917_0000_0000_0004);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let cars = m.bus.read_u32(car::CAR_COUNT);
        let start = m.bus.ram.clone();
        for round in 0..256 {
            for k in 0..cars {
                let at = CARS + k * CAR_SIZE;
                m.bus.ram.copy_from_slice(&start);
                if round > 0 {
                    let mut ram = Ram(&mut m.bus.ram);
                    let big = |rng: &mut common::Rng| (rng.word() as i32) >> (6 + rng.below(26));
                    for i in 0..4 {
                        let w = at + WHEELS + i * WHEEL_SIZE;
                        ram.set_u8(w + car::wheel::GROUND, rng.below(2) as u8);
                        ram.set_u8(w + car::wheel::SLIP, rng.below(2) as u8);
                        ram.set_u8(w + car::wheel::FLAGS, rng.word() as u8);
                        ram.set_i32(w + car::wheel::DIAMETER, 0x1000 + rng.below(0x40000) as i32);
                        ram.set_i32(w + car::wheel::SPIN_RATE, rng.word() as i32);
                    }
                    ram.set_vec3(at + car::VEL, [big(&mut rng), big(&mut rng), big(&mut rng)]);
                    ram.set_i32(at + car::SPEED, big(&mut rng).abs());
                    // Pedals often equal, and either side of half.
                    let pedal =
                        |rng: &mut common::Rng| [0, 2048, 2049, 4096, rng.below(4097) as i32][rng.below(5) as usize];
                    ram.set_i32(at + car::ACCEL, pedal(&mut rng));
                    ram.set_i32(at + car::BRAKE, pedal(&mut rng));
                    ram.set_u8(at + car::HANDBRAKE, rng.below(2) as u8);
                    ram.set_u8(at + car::ENGINE + car::engine::REVERSE, rng.below(2) as u8);
                    ram.set_i32(at + car::ENGINE + car::engine::WHEEL_RPM, big(&mut rng));
                }
                let mut port = m.bus.ram.clone();
                on_car(&mut port, at, |car, _| car.spin_wheels());
                m.call(0x8004_4fc4, &[at]).unwrap();
                common::same_ram(&m.bus.ram, &port, at, &[], &format!("{name} car {k} round {round}"));
            }
        }
    }
}

#[test]
fn air_control_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let mut rng = common::Rng(0xa1f0_0000_0000_0008);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let cars = m.bus.read_u32(car::CAR_COUNT);
        let start = m.bus.ram.clone();
        for round in 0..600 {
            for k in 0..cars {
                let at = CARS + k * CAR_SIZE;
                m.bus.ram.copy_from_slice(&start);
                if round > 0 {
                    let mut ram = Ram(&mut m.bus.ram);
                    // Stick values at and around the dead zone's edges.
                    let stick = |rng: &mut common::Rng| {
                        [0, 818, 819, 820, -818, -819, -820, 4096, -4096, rng.word() as i32 >> 19]
                            [rng.below(10) as usize]
                    };
                    ram.set_i32(at + car::STICK, stick(&mut rng));
                    ram.set_i32(at + car::STICK + 4, stick(&mut rng));
                    ram.set_u8(at + car::HANDBRAKE, rng.below(2) as u8);
                    ram.set_u8(at + car::AIR_ARMED, rng.below(4) as u8);
                    ram.set_u8(at + car::AIR_LOCK, rng.below(2) as u8);
                    ram.set_i32(at + car::AIR_LOCK_AXIS, rng.below(4) as i32);
                    let j = rng.below(3);
                    let dir = [0, 1, 2].map(|i| ram.i16(at + car::ROT + 2 * (3 * i + j)) as i32);
                    ram.set_vec3(at + car::AIR_LOCK_DIR, dir);
                    // At and around 15 mph (15 × 17.6 in/s), as the game rounds it.
                    let mph15 = fx(15 << 12, div_fx(176 << 12, 10 << 12));
                    let speed = [mph15, mph15 + 1, mph15 - 1, rng.below(0x200_000) as i32][rng.below(4) as usize];
                    ram.set_i32(at + car::SPEED, speed);
                    for power in [car::AIR_PITCH, car::AIR_ROLL, car::AIR_YAW] {
                        ram.set_i32(at + power, rng.below(0x20_000) as i32);
                    }
                    for i in 0..3 {
                        ram.set_i64(at + car::BODY + 0x130 + 8 * i, (rng.word() as i32 as i64) << rng.below(20));
                    }
                }
                let mut port = m.bus.ram.clone();
                on_car(&mut port, at, |car, _| car.air_control());
                m.call(0x8003_d71c, &[at]).unwrap();
                common::same_ram(&m.bus.ram, &port, at, &[], &format!("{name} car {k} round {round}"));
            }
        }
    }
}

#[test]
fn righting_matches_the_original() {
    use hwtr_game::car::righting::Righting;
    use hwtr_game::rand::Rand;
    use hwtr_hle::original::rand::SEED;
    let Some(exe) = common::exe() else { return };
    let mut rng = common::Rng(0x2167_0000_0000_0010);
    let mut seen = std::collections::BTreeMap::new();
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let start = m.bus.ram.clone();
        for round in 0..400 {
            m.bus.ram.copy_from_slice(&start);
            let at = CARS;
            // The car turned to one of the 24 ways a box can lie (its own
            // way now and then), its timers somewhere along, the floor under
            // it straight up or as saved.
            let seed = rng.word();
            let mut roof = (0, 0);
            on_car(&mut m.bus.ram, at, |car, tuning| {
                car.air_armed = rng.below(4) as u8;
                car.air_lock.active = rng.below(2) as u8;
                if round % 25 != 0 {
                    let perm = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]][rng.below(6) as usize];
                    let signs = [0, 1, 2].map(|_| if rng.below(2) == 0 { 1i16 } else { -1 });
                    let mut rot = [[0i16; 3]; 3];
                    for (j, &i) in perm.iter().enumerate() {
                        rot[i][j] = 0x1000 * signs[j];
                    }
                    car.body.rot = rot;
                }
                if rng.below(2) == 0 {
                    car.ground.floor.normal = [0, 0, 0x1000];
                }
                // Including the step that crosses each threshold exactly.
                let limits = [tuning.right_side_ms, tuning.right_end_ms, tuning.right_roof_ms].map(|t| t as u32);
                let wreck = tuning.wreck_roof_tens as u32 * 10;
                car.righting = [0, 1, 2].map(|k| {
                    let edge = if k == 2 && rng.below(2) == 0 { wreck } else { limits[k] };
                    [0, 25, 400, 1000, rng.below(3000), edge.saturating_sub(25), edge.saturating_sub(24)]
                        [rng.below(7) as usize]
                });
                car.rights_itself = rng.below(2) as u8;
                car.roll_way = rng.below(2) as u8;
                car.body.force = [0; 3];
                car.body.torque = [0; 3];
                roof = (car.righting[2], car.rights_itself);
            });
            m.bus.write_u32(SEED, seed);
            let mut port = m.bus.ram.clone();
            let mut rand = Rand { seed };
            let outcome = on_car(&mut port, at, |car, tuning| car.right_itself(tuning, &mut rand));
            Ram(&mut port).set_i32(SEED, rand.seed as i32);
            m.call(0x8004_6ac0, &[at, 0]).unwrap();
            let original = Car::read(&Ram(&mut m.bus.ram), at);
            *seen.entry(format!("{outcome:?}")).or_insert(0) += 1;
            if outcome == Righting::Wreck {
                assert_eq!(original.wrecked, 1, "{name} round {round}: the original wrecked the car too");
                continue;
            }
            let ported = Car::read(&Ram(&mut port), at);
            assert_eq!(original, ported, "{name} round {round} (roof {roof:?})");
            assert_eq!(m.bus.read_u32(SEED), rand.seed, "{name} round {round}: the seed");
        }
    }
    assert!(seen.len() == 2, "outcomes {seen:?}");
}

#[test]
fn wreck_matches_the_original_for_computer_cars() {
    use hwtr_game::rand::Rand;
    use hwtr_hle::original::rand::SEED;
    let Some(exe) = common::exe() else { return };
    let mut rng = common::Rng(0x3eec_0000_0000_0011);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let start = m.bus.ram.clone();
        let cars = m.bus.read_u32(car::CAR_COUNT);
        for round in 0..200 {
            m.bus.ram.copy_from_slice(&start);
            // A computer car (the player's wreck also throws its wheels off,
            // not yet ported), moving and spinning every way.
            let at = CARS + (1 + rng.below(cars - 1)) * CAR_SIZE;
            let flip = rng.below(2);
            on_car(&mut m.bus.ram, at, |car, _| {
                assert_eq!(car.flags & 1, 0, "{name}: a computer car");
                car.wrecked = (rng.below(8) == 0) as u8;
                car.body.vel = [0; 3].map(|_| (rng.word() as i32) >> (6 + rng.below(8)));
                car.body.spin = [0; 3].map(|_| (rng.word() as i32) >> (14 + rng.below(6)));
            });
            let seed = rng.word();
            m.bus.write_u32(SEED, seed);
            let mut port = m.bus.ram.clone();
            let mut rand = Rand { seed };
            on_car(&mut port, at, |car, _| car.wreck(flip != 0, &mut rand));
            m.call(0x8004_619c, &[at, 0, flip]).unwrap();
            let original = Car::read(&Ram(&mut m.bus.ram), at);
            // The car takes the generator's first draws; the wreck's effects
            // then draw a few hundred more to crumple the model (0x8002e574,
            // not yet ported), so the seeds part here.
            assert_eq!(original, Car::read(&Ram(&mut port), at), "{name} round {round}");
        }
    }
}

#[test]
fn stunt_awards_match_the_original() {
    use hwtr_game::car::stunt::award;
    let Some(exe) = common::exe() else { return };
    let t = Tables::from_exe(&exe);
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let mut rng = common::Rng(0x5701_0000_0000_0012);
    let out = 0x801f_8000u32;
    let mut named = std::collections::BTreeSet::new();
    for round in 0..20000 {
        // Turns from none to past eight, either way, often near a tier.
        let mut deg = || {
            let v = match rng.below(4) {
                0 => rng.below(3200) as i32,
                1 => (rng.below(9) * 180) as i32 + rng.below(91) as i32 - 45,
                2 => rng.below(400) as i32,
                _ => 0,
            };
            if rng.below(2) == 0 { -v } else { v }
        };
        let args = [deg(), deg(), deg(), deg(), deg(), deg()];
        let aloft = [rng.below(6000), rng.below(1200), 999, 1000, 2000, 4000, 9999][rng.below(7) as usize];
        for k in 0..12 {
            m.bus.write_u32(out + 4 * k, 0xdead_0000 + k);
        }
        m.bus.write_u32(out, 0);
        m.bus.write_u32(out + 8, 0);
        m.bus.write_u32(out + 4, 0);
        m.call(
            0x8008_0148,
            &[args[0] as u32, args[1] as u32, args[2] as u32, args[3] as u32, args[4] as u32, args[5] as u32, aloft, out, out + 4, out + 8],
        )
        .unwrap();
        let (turbos, stunt, points) = (m.bus.read_u32(out) as u8, m.bus.read_u32(out + 4) as u16, m.bus.read_u32(out + 8) as i32);
        let ported = award(&t.stunts, (args[0], args[1]), (args[2], args[3]), (args[4], args[5]), aloft);
        match ported {
            Some(a) => {
                assert_eq!((a.points, a.stunt, a.turbos), (points, stunt, turbos), "round {round}: {args:?} aloft {aloft}");
                named.insert(a.stunt);
            }
            None => assert_eq!((points, turbos), (0, 0), "round {round}: {args:?} aloft {aloft}"),
        }
    }
    assert!(named.len() > 40, "stunts named: {named:?}");
}

#[test]
fn stunt_watch_matches_the_original() {
    use hwtr_game::rand::Rand;
    use hwtr_hle::original::rand::SEED;
    let Some(exe) = common::exe() else { return };
    let t = Tables::from_exe(&exe);
    let mut rng = common::Rng(0x5701_0000_0000_0013);
    let (mut landings, mut seeds_apart) = (0, 0);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let start = m.bus.ram.clone();
        let scoring = m.bus.ram[(0x800d_2640u32 & 0x1f_ffff) as usize] != 0;
        for round in 0..3000 {
            m.bus.ram.copy_from_slice(&start);
            let at = CARS;
            on_car(&mut m.bus.ram, at, |car, _| {
                car.airborne = rng.below(2) as u8;
                car.grounded = [0, 0, 2, 4][rng.below(4) as usize];
                car.grounded_level = (rng.below(3) == 0) as u8;
                car.body.spin = [0; 3].map(|_| (rng.word() as i32) >> (13 + rng.below(8)));
                car.stunt_spin = [0; 3].map(|_| (rng.word() as i32) >> (13 + rng.below(8)));
                car.stunt_turn = [0; 3].map(|_| (rng.word() as i32) >> (10 + rng.below(12)));
                car.stunt_peak = [0; 3].map(|_| (rng.word() as i32) >> (10 + rng.below(12)));
                car.air_ms = [0, 999, 1500, 2500, 3900, rng.below(9000)][rng.below(6) as usize];
                car.turbos = rng.below(11) as u8;
                car.turbo_hint = rng.below(2) as u8;
            });
            let seed = rng.word();
            m.bus.write_u32(SEED, seed);
            let mut port = m.bus.ram.clone();
            let mut rand = Rand { seed };
            let landed = on_car(&mut port, at, |car, _| car.watch_stunt(102, scoring, &t.stunts, &mut rand));
            m.call(0x8003_cb74, &[at, 102]).unwrap();
            let original = Car::read(&Ram(&mut m.bus.ram), at);
            assert_eq!(original, Car::read(&Ram(&mut port), at), "{name} round {round} ({landed:?})");
            if landed.is_some() {
                landings += 1;
                seeds_apart += (m.bus.read_u32(SEED) != rand.seed) as u32;
            } else {
                assert_eq!(m.bus.read_u32(SEED), rand.seed, "{name} round {round}: the seed");
            }
        }
    }
    eprintln!("{landings} landings rewarded, seeds apart after {seeds_apart}");
    assert!(landings > 100);
}
