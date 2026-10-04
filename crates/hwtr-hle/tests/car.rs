//! The car update against the original, on cars from saved races.

mod common;

use hwtr_game::car::{Car, Tuning};
use hwtr_game::math::{Tables, div_fx, fx};
use hwtr_hle::original::InMemory;
use hwtr_hle::original::Ram;
use hwtr_hle::original::car::{self as car, CAR_SIZE, CARS, WHEEL_SIZE, WHEELS};

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
                            ram.set_u8(w + car::wheel::FLAGS, ram.u8(w + car::wheel::FLAGS) ^ 1);
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
            let mut roof = (0, false);
            on_car(&mut m.bus.ram, at, |car, tuning| {
                car.air_armed = hwtr_game::car::Armed { along: rng.below(2) != 0, across: rng.below(2) != 0 };
                car.air_lock.active = rng.below(2) != 0;
                if round % 25 != 0 {
                    let perm =
                        [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]][rng.below(6) as usize];
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
                car.rights_itself = rng.below(2) != 0;
                car.roll_way = rng.below(2) != 0;
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
                assert!(original.wrecked, "{name} round {round}: the original wrecked the car too");
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
                car.wrecked = rng.below(8) == 0;
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
            // The car takes the generator's first draws, the wreck's effects
            // (0x8002e574, through 0x80029e10) a few hundred more.
            assert_eq!(original, Car::read(&Ram(&mut port), at), "{name} round {round}");
            assert_eq!(m.bus.read_u32(SEED), rand.seed, "{name} round {round}: the seed");
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
            &[
                args[0] as u32,
                args[1] as u32,
                args[2] as u32,
                args[3] as u32,
                args[4] as u32,
                args[5] as u32,
                aloft,
                out,
                out + 4,
                out + 8,
            ],
        )
        .unwrap();
        let (turbos, stunt, points) =
            (m.bus.read_u32(out) as u8, m.bus.read_u32(out + 4) as u16, m.bus.read_u32(out + 8) as i32);
        let ported = award(&t.stunts, (args[0], args[1]), (args[2], args[3]), (args[4], args[5]), aloft);
        match ported {
            Some(a) => {
                assert_eq!(
                    (a.points, a.stunt, a.turbos),
                    (points, stunt, turbos),
                    "round {round}: {args:?} aloft {aloft}"
                );
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
                car.airborne = rng.below(2) != 0;
                car.grounded = [0, 0, 2, 4][rng.below(4) as usize];
                car.grounded_level = (rng.below(3) == 0) as u8;
                car.body.spin = [0; 3].map(|_| (rng.word() as i32) >> (13 + rng.below(8)));
                car.stunt_spin = [0; 3].map(|_| (rng.word() as i32) >> (13 + rng.below(8)));
                car.stunt_turn = [0; 3].map(|_| (rng.word() as i32) >> (10 + rng.below(12)));
                car.stunt_peak = [0; 3].map(|_| (rng.word() as i32) >> (10 + rng.below(12)));
                car.air_ms = [0, 999, 1500, 2500, 3900, rng.below(9000)][rng.below(6) as usize];
                car.turbos = rng.below(11) as u8;
                car.turbo_hint = rng.below(2) != 0;
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

/// How the road feels through the pad (0x80045ff4), for the cars of
/// `desert1-drive` with random ground under their wheels and random speeds.
#[test]
fn road_feel_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-drive") else { return };
    let tables = hwtr_game::math::Tables::from_exe(&exe);
    let mut rng = common::Rng(0xfee1_0001);
    let mut felt = 0;
    for round in 0..3000 {
        let slot = rng.below(6);
        let at = CARS + slot * CAR_SIZE;
        let wheels = m.bus.read(at + car::WHEEL_COUNT, 1).unwrap();
        for w in 0..wheels {
            let wheel = at + WHEELS + w * WHEEL_SIZE;
            m.bus.write(wheel + 0x3c, 1, (rng.below(3) != 0) as u32).unwrap();
            m.bus.write(wheel + 0x3d, 1, rng.below(16)).unwrap();
        }
        let speed = (rng.word() >> rng.below(14)) as i32 * if rng.below(5) == 0 { -1 } else { 1 };
        m.bus.write_u32(at + car::SPEED, speed as u32);
        m.call(0x8004_5ff4, &[slot, common::OUT, common::OUT + 1]).unwrap();
        let (rough, speed) = (m.bus.read(common::OUT, 1).unwrap() as u8, m.bus.read(common::OUT + 1, 1).unwrap() as u8);
        let ported = Car::read(&Ram(&mut m.bus.ram), at);
        assert_eq!(ported.road_feel(&tables.surface_rumble), (rough, speed), "round {round}, car {slot}");
        felt += (rough != 0 && speed > 10) as u32;
    }
    assert!(felt > 300, "{felt} rough rounds");
}

#[test]
fn unsticking_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let mut rng = common::Rng(0x0275_71c4);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let cars = m.bus.read_u32(car::CAR_COUNT);
        let start = m.bus.ram.clone();
        for round in 0..512 {
            let k = rng.below(cars);
            let at = CARS + k * CAR_SIZE;
            m.bus.ram.copy_from_slice(&start);
            if round > 0 {
                // Any steering, size, mass, sums and place.
                let mut ram = Ram(&mut m.bus.ram);
                let big = |rng: &mut common::Rng| (rng.word() as i32) >> (6 + rng.below(24));
                ram.set_i32(at + car::STEER, rng.below(8193) as i32 - 4096);
                ram.set_i32(at + car::WIDTH, big(&mut rng).abs());
                ram.set_i32(at + car::LENGTH, big(&mut rng).abs());
                ram.set_i32(at + car::BODY + hwtr_hle::original::body::MASS, big(&mut rng).abs());
                ram.set_vec3(at + car::FORCE, [big(&mut rng), big(&mut rng), big(&mut rng)]);
                ram.set_vec3(at + car::POS, [big(&mut rng), big(&mut rng), big(&mut rng)]);
                ram.set_vec3(at + car::DRAG_POINT, [big(&mut rng), big(&mut rng), big(&mut rng)]);
            }
            let mut port = m.bus.ram.clone();
            on_car(&mut port, at, |car, _| car.unstick());
            m.call(0x8004_b478, &[at]).unwrap();
            common::same_ram(&m.bus.ram, &port, at, &[], &format!("{name} car {k} round {round}"));
        }
    }
}

/// A wheel's node as the original poses it (0x80020a14): any steering,
/// angle and lift, its node's rotation and place and the model's lift
/// (+0x18) compared; a wrecked car's left alone.
#[test]
fn wheel_poses_match_the_original() {
    use hwtr_game::car::draw::WheelPose;
    let Some(exe) = common::exe() else { return };
    let t = Tables::from_exe(&exe);
    let mut rng = common::Rng(0x0d2a_0b1e);
    let mut posed = 0;
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let start = m.bus.ram.clone();
        let cars = m.bus.read_u32(car::CAR_COUNT);
        for round in 0..1000 {
            m.bus.ram.copy_from_slice(&start);
            let slot = rng.below(cars);
            let model = hwtr_hle::original::effects::model(&Ram(&mut m.bus.ram), slot);
            let k = rng.below(4);
            let big = |rng: &mut common::Rng| (rng.word() as i32) >> (4 + rng.below(20));
            let (steer, lift, angle) = (big(&mut rng), big(&mut rng), big(&mut rng));
            let wrecked = rng.below(8) == 0;
            {
                let mut ram = Ram(&mut m.bus.ram);
                let cvs = ram.i32(model + 16) as u32;
                let byte = ram.u8(cvs + 0x28);
                ram.set_u8(
                    cvs + 0x28,
                    if wrecked {
                        1
                    } else if byte == 1 {
                        2
                    } else {
                        byte
                    },
                );
            }
            // Ground kind 10: no trail point.
            m.call(
                0x8002_0a14,
                &[slot, k, steer as u32, lift as u32, angle as u32, 0, 10, 0, common::OUT, common::OUT],
            )
            .unwrap();
            let ram = Ram(&mut m.bus.ram);
            let cvs = ram.i32(model + 16) as u32;
            let node = ram.i32(model + 8) as u32 + 72 * k;
            let rec = ram.i32(model + 12) as u32 + 32 * k;
            let rot: [[i16; 3]; 3] =
                std::array::from_fn(|i| std::array::from_fn(|j| ram.i16(node + 6 * i as u32 + 2 * j as u32)));
            let at = ram.vec3(node + 20);
            let what = format!("{name} round {round}: car {slot} wheel {k} steer {steer} lift {lift} angle {angle}");
            if wrecked {
                let before = Ram(&mut start.clone()).vec3(node + 20);
                assert_eq!(at, before, "{what}: a wreck's wheel left alone");
                continue;
            }
            let p = WheelPose::new(&t, steer, angle, lift);
            assert_eq!(rot, p.rot, "{what}: rotation");
            let (x, y, z) = (ram.i32(rec), ram.i32(rec + 4), ram.i32(rec + 8));
            assert_eq!(at, [x << 13, y << 13, ((z << 12).wrapping_add(lift)) << 1], "{what}: place");
            assert_eq!(ram.i32(cvs + 0x18), lift.wrapping_mul(2), "{what}: the model's lift");
            posed += 1;
        }
    }
    assert!(posed > 2000, "{posed} posed");
}

/// A car's shadow (0x80029478) against the port's: the car's model root
/// placed and turned any way, the ground under it any plane, the four
/// quads' corners compared (none on steep ground).
#[test]
fn shadows_match_the_original() {
    use hwtr_game::car::draw::{shadow_quads, shadow_table};
    let Some(exe) = common::exe() else { return };
    let t = Tables::from_exe(&exe);
    let view = exe.view();
    let table = shadow_table(&|a| view.u8(a).unwrap_or(0));
    let mut rng = common::Rng(0x05ad_0e5);
    let (mut cast, mut none) = (0, 0);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let start = m.bus.ram.clone();
        let cars = m.bus.read_u32(car::CAR_COUNT);
        for round in 0..600 {
            m.bus.ram.copy_from_slice(&start);
            let slot = rng.below(cars);
            let mut ram = Ram(&mut m.bus.ram);
            let model = hwtr_hle::original::effects::model(&ram, slot);
            let cvs = ram.i32(model + 16) as u32;
            let root = ram.i32(model + 4) as u32;
            let car = Car::read(&ram, CARS + CAR_SIZE * slot);
            // Placed anywhere, turned about any axis.
            let pos: [i32; 3] = std::array::from_fn(|_| (rng.word() as i32) >> (8 + rng.below(4)));
            let a = rng.below(4096) as i32;
            let rot = t.rot_axis(rng.below(3) as usize, a);
            for i in 0..3 {
                for j in 0..3 {
                    ram.set_i16(root + 6 * i + 2 * j, rot[i as usize][j as usize]);
                }
            }
            ram.set_vec3(root + 20, pos.map(|c| c << 1));
            let n = t.normalize([0; 3].map(|_| rng.below(8192) as i32 - 4096 + if rng.below(2) == 0 { 0 } else { 0 }));
            let n = if rng.below(4) == 0 { n } else { t.normalize([n[0] / 4, n[1] / 4, 4096]) };
            let d = (rng.word() as i32) >> (6 + rng.below(8));
            ram.set_u8(0x800d_25a0 + slot, 1);
            ram.set_vec3(0x8011_dbe4 + 20 * slot, n);
            ram.set_i32(0x8011_dbe4 + 20 * slot + 16, d);
            ram.set_u8(cvs + 0x1ef, 1);
            // Any cheat, the model's scale as it sets it.
            let options = [0u32, 0, 2, 4, 32, 6][rng.below(6) as usize];
            ram.set_u8(0x800d_2468, options as u8);
            ram.set_i32(cvs + 0x14, hwtr_game::car::draw::model_scale(options));
            let id = ram.u8(cvs + 0x10) as usize;
            let prims = 0x8011_e124 + 304 * slot;
            for k in 0..4 * 76 {
                ram.set_u8(prims + k, 0xee);
            }
            m.call(0x8002_9478, &[slot, model]).unwrap();
            let ram = Ram(&mut m.bus.ram);
            let rot = if hwtr_game::car::draw::body_scaled(options) {
                hwtr_game::car::draw::scale_columns(&rot, hwtr_game::car::draw::model_scale(options))
            } else {
                rot
            };
            let ours = shadow_quads(&car.handling, table[id], pos, &rot, n, d);
            let what = format!("{name} round {round}: car {slot} (id {id}) at {pos:?} on {n:?} {d}");
            match ours {
                Some(quads) => {
                    for (q, quad) in quads.iter().enumerate() {
                        for (v, p) in quad.iter().enumerate() {
                            let at = prims + 76 * q as u32 + 8 * v as u32;
                            let theirs = [ram.i16(at), ram.i16(at + 2), ram.i16(at + 4)];
                            assert_eq!(theirs, p.map(|c| (c >> 12) as i16), "{what}: quad {q} corner {v}");
                        }
                    }
                    cast += 1;
                }
                None => {
                    assert_eq!(ram.u8(prims), 0xee, "{what}: no shadow");
                    none += 1;
                }
            }
        }
    }
    assert!(cast > 1000 && none > 100, "{cast} cast, {none} none");
}

/// `cars_update`'s boost end: a car short of its boost's speed by more
/// than 30 mph has its boost flame put out (iface_general+0xd8,
/// 0x8002aff4, hooked) before its update; any speed and boost speed.
#[test]
fn a_boost_ends_as_in_the_original() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let Some(exe) = common::exe() else { return };
    let mut rng = common::Rng(0xb005_7e4d);
    let (mut ended, mut kept) = (0, 0);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let out: Rc<RefCell<Vec<u32>>> = Rc::default();
        let o = out.clone();
        m.hook(0x8002_aff4, move |cpu, _| {
            o.borrow_mut().push(cpu.r[4] & 0xff);
            0
        });
        let start = m.bus.ram.clone();
        let cars = m.bus.read_u32(car::CAR_COUNT);
        for round in 0..300 {
            m.bus.ram.copy_from_slice(&start);
            out.borrow_mut().clear();
            let mut expected = Vec::new();
            for slot in 0..cars {
                let at = CARS + CAR_SIZE * slot;
                let mut ours = on_car(&mut m.bus.ram, at, |c, _| {
                    c.body.speed = rng.below(0x80_0000) as i32;
                    c.boost = (rng.below(3) != 0).then(|| rng.below(0x90_0000) as i32);
                    c.wrecked = false;
                    c.clone()
                });
                ours.run_timers(25);
                if ours.flame_out.0.is_some() {
                    expected.push(slot);
                    ended += 1;
                } else if ours.boost.is_some() {
                    kept += 1;
                }
            }
            m.call(0x8004_064c, &[25]).unwrap();
            assert_eq!(*out.borrow(), expected, "{name} round {round}");
        }
    }
    assert!(ended > 300 && kept > 300, "{ended} ended, {kept} kept");
}

/// The flying wheels' table (0x801323f4, 680 bytes a wheel).
const FLYING: u32 = 0x8013_23f4;

/// A player's wreck throwing its wheels (0x8007c9b0): the car moving and
/// spinning any way, its wheels rolling and placed anywhere, some slots
/// taken; each wheel's body, box, reach and zone, the slots taken and the
/// seed compared.
#[test]
fn wheels_fly_off_as_in_the_original() {
    use hwtr_game::flying::{FlyingWheel, SLOTS};
    use hwtr_game::rand::Rand;
    use hwtr_hle::original::rand::SEED;
    let Some(exe) = common::exe() else { return };
    let t = Tables::from_exe(&exe);
    let mut rng = common::Rng(0xf1_7e5);
    let mut thrown = 0;
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let start = m.bus.ram.clone();
        for round in 0..300 {
            m.bus.ram.copy_from_slice(&start);
            let seed = rng.word();
            let taken: Vec<bool> = (0..SLOTS).map(|_| rng.below(4) == 0).collect();
            let car = {
                let mut ram = Ram(&mut m.bus.ram);
                let mut car = Car::read(&ram, CARS);
                car.body.vel = [0; 3].map(|_| (rng.word() as i32) >> (6 + rng.below(8)));
                car.body.spin = [0; 3].map(|_| (rng.word() as i32) >> (14 + rng.below(6)));
                for w in &mut car.wheels {
                    w.spin_rate = (rng.word() as i32) >> (6 + rng.below(10));
                    w.world = hwtr_game::math::add(car.body.pos, [0; 3].map(|_| (rng.word() as i32) >> 12));
                }
                car.write(&mut ram, CARS);
                for (k, &on) in taken.iter().enumerate() {
                    ram.set_u8(FLYING + 680 * k as u32, on as u8);
                }
                ram.set_i32(SEED, seed as i32);
                Car::read(&ram, CARS)
            };
            let mut table: [Option<FlyingWheel>; SLOTS] =
                std::array::from_fn(|k| taken[k].then(|| hwtr_game::flying::throw(&t, &car, 0, &mut Rand { seed: 0 })));
            let mut rand = Rand { seed };
            hwtr_game::flying::throw_all(&t, &car, &mut table, &mut rand);
            m.call(0x8007_c9b0, &[CARS]).unwrap();
            let what = format!("{name} round {round}");
            assert_eq!(m.bus.read_u32(SEED), rand.seed, "{what}: seed");
            let ram = Ram(&mut m.bus.ram);
            for k in 0..SLOTS as u32 {
                let at = FLYING + 680 * k;
                let ours = &table[k as usize];
                assert_eq!(ram.u8(at) != 0, ours.is_some(), "{what}: slot {k} taken");
                let Some(f) = ours.as_ref().filter(|_| !taken[k as usize]) else { continue };
                assert_eq!((ram.u8(at + 1), ram.u8(at + 2)), (f.car, f.wheel), "{what}: slot {k}'s car and wheel");
                let body = hwtr_game::body::Body::read(&ram, at + 152);
                assert_eq!(body, f.body, "{what}: slot {k}'s body");
                assert_eq!(ram.vec3(at + 0x20), f.half, "{what}: slot {k}'s box");
                assert_eq!(ram.i32(at + 0x60), f.radius, "{what}: slot {k}'s reach");
                assert_eq!(ram.u8(at + 0x64), 6, "{what}: slot {k}'s kind");
                thrown += 1;
            }
        }
    }
    assert!(thrown > 1000, "{thrown} thrown");
}

/// The flying wheels' steps (0x8007c894): thrown by the original, then 40
/// steps of each, every body compared after each.
#[test]
fn flying_wheels_step_as_in_the_original() {
    use hwtr_game::flying::{FlyingWheel, SLOTS};
    let Some(exe) = common::exe() else { return };
    let t = Tables::from_exe(&exe);
    let mut rng = common::Rng(0x57e9_f1);
    let mut stepped = 0;
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let start = m.bus.ram.clone();
        for round in 0..40 {
            m.bus.ram.copy_from_slice(&start);
            {
                let mut ram = Ram(&mut m.bus.ram);
                let mut car = Car::read(&ram, CARS);
                car.body.vel = [0; 3].map(|_| (rng.word() as i32) >> (6 + rng.below(8)));
                car.body.spin = [0; 3].map(|_| (rng.word() as i32) >> (14 + rng.below(6)));
                for w in &mut car.wheels {
                    w.spin_rate = (rng.word() as i32) >> (6 + rng.below(10));
                }
                car.write(&mut ram, CARS);
                for k in 0..SLOTS as u32 {
                    ram.set_u8(FLYING + 680 * k, 0);
                }
            }
            m.call(0x8007_c9b0, &[CARS]).unwrap();
            let mut table: [Option<FlyingWheel>; SLOTS] = {
                let ram = Ram(&mut m.bus.ram);
                std::array::from_fn(|k| {
                    let at = FLYING + 680 * k as u32;
                    (ram.u8(at) != 0).then(|| FlyingWheel {
                        car: ram.u8(at + 1),
                        wheel: ram.u8(at + 2),
                        body: hwtr_game::body::Body::read(&ram, at + 152),
                        half: ram.vec3(at + 0x20),
                        radius: ram.i32(at + 0x60),
                        object: None,
                    })
                })
            };
            for step in 0..40 {
                m.call(0x8007_c894, &[]).unwrap();
                hwtr_game::flying::step(&t, &mut table);
                let ram = Ram(&mut m.bus.ram);
                for (k, f) in table.iter().enumerate() {
                    let Some(f) = f else { continue };
                    let body = hwtr_game::body::Body::read(&ram, FLYING + 680 * k as u32 + 152);
                    assert_eq!(body, f.body, "{name} round {round} step {step}: wheel {k}");
                    stepped += 1;
                }
            }
        }
    }
    assert!(stepped > 10000, "{stepped} stepped");
}
