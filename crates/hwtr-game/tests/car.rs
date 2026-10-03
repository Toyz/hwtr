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
                        ram.set_u8(w + car::wheel::ON_GROUND, (rng.below(4) != 0) as u8);
                        ram.set_u8(w + car::wheel::SLIPPING, (rng.below(3) == 0) as u8);
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
                            ram.set_u8(at + WHEELS + i * WHEEL_SIZE + car::wheel::SLIPPING, 1);
                        }
                        ram.set_i32(at + car::ACCEL, 4096);
                        ram.set_i32(at + car::BRAKE, 4096);
                    }
                }
                let mut port = m.bus.ram.clone();
                car::drivetrain(&t, &mut Ram(&mut port), at);
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
                            ram.set_u8(w + car::wheel::ON_GROUND, 0);
                        }
                        ram.set_u8(w + car::wheel::SURFACE, [2, 6, 0][rng.below(3) as usize]);
                        ram.set_vec3(w + car::wheel::CONTACT_VEL, [big(&mut rng), big(&mut rng), big(&mut rng)]);
                        ram.set_vec3(w + car::wheel::CONTACT, [big(&mut rng), big(&mut rng), big(&mut rng)]);
                        ram.set_i32(w + car::wheel::SPRING, rng.below(2000 << 12) as i32);
                        ram.set_i32(w + car::wheel::FRICTION, rng.below(2 << 12) as i32);
                        if rng.below(3) == 0 {
                            ram.set_u8(w + car::wheel::FLAGS, ram.u8(w + car::wheel::FLAGS) ^ car::wheel::REAR);
                        }
                    }
                    ram.set_i32(at + car::ACCEL, rng.below(4097) as i32);
                    ram.set_i32(at + car::BRAKE, rng.below(4097) as i32);
                    ram.set_u8(at + car::HANDBRAKE, rng.below(2) as u8);
                    ram.set_i32(at + car::FLAGS, rng.word() as i32);
                    ram.set_u8(at + 0x865, rng.below(2) as u8);
                    ram.set_i32(at + 0x6b4, rng.word() as i32);
                    ram.set_u8(at + 0x86a, rng.below(2) as u8);
                    ram.set_u8(at + 0x86b, rng.below(2) as u8);
                    ram.set_vec3(at + car::SPIN, [0; 3].map(|_| (rng.word() as i32) >> (16 + rng.below(16))));
                    for b in [10, 33, 34, 35, 36, 37, 38, 39, 40, 41, 51] {
                        ram.set_u8(car::TUNING + b, rng.word() as u8);
                    }
                    // Every wheel in the air.
                    if round % 6 == 2 {
                        for i in 0..wheels {
                            ram.set_u8(at + WHEELS + i * WHEEL_SIZE + car::wheel::ON_GROUND, 0);
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
                car::car_physics(&t, &mut Ram(&mut port), at);
                m.call(0x8004_27a8, &[at]).unwrap();
                let pads: Vec<(u32, u32)> =
                    (0..wheels).map(|i| (at + WHEELS + i * WHEEL_SIZE + car::wheel::HEADING + 12, 4)).collect();
                common::same_ram(&m.bus.ram, &port, at, &pads, &format!("{name} car {k} round {round}"));
            }
        }
    }
}
