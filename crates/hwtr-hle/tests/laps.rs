//! Laps and checkpoints against the original's.

mod common;

use hwtr_game::car::Car;
use hwtr_game::laps::{Course, Laps};
use hwtr_game::math::Tables;
use hwtr_hle::original::car::CARS;
use hwtr_hle::original::race::{SETUP, setup};
use hwtr_hle::original::world::{collision, course};
use hwtr_hle::original::{InMemory, Ram};

const STATES: [&str; 3] = ["desert1-race", "desert1-drive", "desert1-speed"];
/// The race clock, and where `bestline_load` keeps the lap length.
const TIME: u32 = 0x800d_0e34;
const LAP_LENGTH: u32 = 0x8013_23d8;

#[test]
fn the_course_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let t = Tables::from_exe(&exe);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let ram = Ram(&mut m.bus.ram);
        let setup = setup(&ram, SETUP);
        let (world, _) = collision(&ram);
        let ported = Course::new(&setup, &world.scp, ram.i32(LAP_LENGTH));
        assert_eq!(ported, course(&ram), "{name}");
        assert_eq!(t.checkpoints(&setup.track, setup.track_number), setup.checkpoints, "{name}");
    }
}

#[test]
fn passing_checkpoints_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let mut rng = common::Rng(0x1a95_0000_0000_0001);
    let mut laps_ended = 0;
    let mut finished = 0;
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let start = m.bus.ram.clone();
        let mut course = course(&Ram(&mut m.bus.ram));
        assert!(course.checkpoints > 1 && course.laps > 0, "{name}: {course:?}");
        for round in 0..600 {
            m.bus.ram.copy_from_slice(&start);
            // Somewhere in the race: some laps run, some checkpoints of
            // this one passed (in order or not), now and then finished.
            let mut car = Car::read(&Ram(&mut m.bus.ram), CARS);
            let done = rng.below(course.laps as u32) as u8;
            let mut ends: Vec<u32> = (0..done).map(|_| rng.below(600_000)).collect();
            ends.sort();
            let mut passed = [false; 10];
            let in_order = rng.below(3) != 0;
            let count = rng.below(course.checkpoints as u32) as usize;
            for (k, p) in passed.iter_mut().enumerate().take(course.checkpoints as usize - 1) {
                *p = if in_order { k < count } else { rng.below(2) == 0 };
            }
            car.laps = Laps {
                start: 0,
                done,
                passed,
                passed_count: if in_order { count as u8 } else { rng.below(course.checkpoints as u32) as u8 },
                ends: ends.clone(),
                best: if rng.below(3) == 0 { 0 } else { rng.below(200_000) },
                finished: rng.below(20) == 0,
                place: 0,
            };
            // Now and then a quiet race (race flag 2).
            course.quiet = rng.below(4) == 0;
            let number =
                if rng.below(3) == 0 { course.checkpoints } else { 1 + rng.below(course.checkpoints as u32) as u8 };
            let time = ends.last().copied().unwrap_or(0) + rng.below(200_000);
            {
                let mut ram = Ram(&mut m.bus.ram);
                car.write(&mut ram, CARS);
                ram.set_i32(TIME, time as i32);
                ram.set_u8(hwtr_hle::original::world::QUIET, course.quiet as u8);
            }
            let mut car = Car::read(&Ram(&mut m.bus.ram), CARS);
            let event = car.laps.pass(&course, number, time);
            m.call(0x8006_137c, &[0, number as u32]).unwrap();
            let original = Car::read(&Ram(&mut m.bus.ram), CARS);
            assert_eq!(original.laps, car.laps, "{name} round {round}: checkpoint {number}, {event:?}");
            if let Some(hwtr_game::laps::LapEvent::Lap { .. }) = event {
                laps_ended += 1;
                finished += car.laps.finished as u32;
            }
        }
    }
    assert!(laps_ended > 20 && finished > 0, "{laps_ended} laps ended, {finished} races finished");
}

#[test]
fn zone_effects_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let t = Tables::from_exe(&exe);
    let mut rng = common::Rng(0x2e0e_0000_0000_0002);
    let mut ahead = 0;
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let start = m.bus.ram.clone();
        let (mut world, addresses) = collision(&Ram(&mut m.bus.ram));
        let id = world.objects.iter().position(|o| o.car == Some(0)).expect("the player's object");
        // Not the trigger zones (not yet ported) nor the special zones
        // (their own test).
        let zones: Vec<u16> =
            (0..world.scp.zones.len() as u16).filter(|&z| world.scp.zones[z as usize].flags & 0x4002 == 0).collect();
        for round in 0..600 {
            m.bus.ram.copy_from_slice(&start);
            let zone = zones[rng.below(zones.len() as u32) as usize];
            let driven = rng.below(4) != 0;
            let time = rng.below(600_000);
            // The race's options, its cheats: 0x80 shows the headlights.
            world.options = [0, 0x80, 0x7f, 0xff][rng.below(4) as usize];
            let mut car = Car::read(&Ram(&mut m.bus.ram), CARS);
            car.laps.passed_count = rng.below(world.course.checkpoints as u32) as u8;
            car.laps.passed = std::array::from_fn(|k| k < car.laps.passed_count as usize);
            {
                let mut ram = Ram(&mut m.bus.ram);
                car.write(&mut ram, CARS);
                ram.set_i32(TIME, time as i32);
                ram.set_i32(hwtr_hle::original::world::OPTIONS, world.options as i32);
            }
            let mut car = Car::read(&Ram(&mut m.bus.ram), CARS);
            world.zone_effects(&t, &mut car, zone, driven.then_some(time));
            m.call(0x8005_c3a4, &[addresses[id], zone as u32, !driven as u32]).unwrap();
            let original = Car::read(&Ram(&mut m.bus.ram), CARS);
            assert_eq!(original, car, "{name} round {round}: zone {zone} {:?}", world.scp.zones[zone as usize]);
            ahead += (car.lap_distance >= world.course.lap_length) as u32;
        }
    }
    assert!(ahead > 0, "no zone was on the lap ahead");
}

/// The special zones (flag 0x4000): the desert's launchers as they are,
/// and road zones made boost pads (0x4800) in memory, a driving car
/// entering them at any speed and heading, every wheel down or not.
#[test]
fn boost_pads_and_launchers_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let t = hwtr_game::math::Tables::from_exe(&exe);
    let mut rng = common::Rng(0xb005_7ad5);
    let (mut launched, mut boosted) = (0, 0);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let (world, addresses) = collision(&Ram(&mut m.bus.ram));
        let id = world.objects.iter().position(|o| o.car == Some(0)).expect("the player's object");
        let launchers: Vec<u16> = (0..world.scp.zones.len() as u16)
            .filter(|&z| world.scp.zones[z as usize].flags & 0x4800 == 0x4000)
            .collect();
        assert!(!launchers.is_empty(), "{name}: launchers");
        // Some plain zones made pads.
        let plain: Vec<u16> = (0..world.scp.zones.len() as u16)
            .filter(|&z| world.scp.zones[z as usize].flags & 0x4802 == 0)
            .step_by(37)
            .collect();
        {
            let mut ram = Ram(&mut m.bus.ram);
            let zones = ram.i32(ram.i32(hwtr_hle::original::world::SCP) as u32 + 216) as u32;
            for &z in &plain {
                let at = zones + 20 * z as u32;
                ram.set_i16(at, (ram.i16(at) as u16 | 0x4800) as i16);
            }
        }
        let start = m.bus.ram.clone();
        let (mut world, _) = collision(&Ram(&mut m.bus.ram));
        for round in 0..800 {
            m.bus.ram.copy_from_slice(&start);
            let pad = rng.below(2) == 0;
            let zone = if pad {
                plain[rng.below(plain.len() as u32) as usize]
            } else {
                launchers[rng.below(launchers.len() as u32) as usize]
            };
            let mut car = Car::read(&Ram(&mut m.bus.ram), CARS);
            car.state = 2;
            car.wrecked = false;
            let size = [1 << 14, 1 << 18, 1 << 20, 1 << 22][rng.below(4) as usize];
            car.body.vel = std::array::from_fn(|_| rng.below(2 * size) as i32 - size as i32);
            car.body.speed = t.length(car.body.vel);
            car.body.momentum = car.body.vel.map(|c| hwtr_game::math::fx(c, car.body.mass));
            car.boost = (rng.below(3) == 0).then(|| 0x8_2000);
            car.grounded =
                if rng.below(2) == 0 { car.wheels.len() as u8 } else { rng.below(car.wheels.len() as u32) as u8 };
            if rng.below(2) == 0 {
                // Facing any way about the vertical.
                let a = rng.below(25736) as i32;
                let (s, c) = (t.sin(a) as i16, t.cos(a) as i16);
                car.body.rot = [[c, s.wrapping_neg(), 0], [s, c, 0], [0, 0, 0x1000]];
            }
            Ram(&mut m.bus.ram).set_i32(TIME, 0);
            car.write(&mut Ram(&mut m.bus.ram), CARS);
            let mut car = Car::read(&Ram(&mut m.bus.ram), CARS);
            world.zone_effects(&t, &mut car, zone, None);
            m.call(0x8005_c3a4, &[addresses[id], zone as u32, 1]).unwrap();
            let mut original = Car::read(&Ram(&mut m.bus.ram), CARS);
            // The effects' asks are the port's own.
            original.sounds = car.sounds.clone();
            original.turbo_fired = car.turbo_fired.clone();
            assert_eq!(original, car, "{name} round {round}: zone {zone} {:?}", world.scp.zones[zone as usize]);
            if car.boost.is_some() {
                if pad { boosted += 1 } else { launched += 1 }
            }
        }
    }
    assert!(launched > 200 && boosted > 200, "{launched} launched, {boosted} boosted");
}

#[test]
fn finish_estimates_match_the_original() {
    use hwtr_hle::original::ai::{DRIVER_SIZE, DRIVERS};
    use hwtr_hle::original::car::{CAR_COUNT, CAR_SIZE};
    let Some(exe) = common::exe() else { return };
    let mut rng = common::Rng(0xe571_0000_0000_0015);
    let mut estimated = 0;
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        for _ in 0..40 {
            let (laps, lap_length, count) = {
                let mut ram = Ram(&mut m.bus.ram);
                let course = course(&ram);
                // The states are still counting down: the start (0x80061264)
                // would keep the best line's lap length here.
                assert_eq!(ram.i32(LAP_LENGTH), course.lap_length, "{name}");
                ram.set_i32(0x800d_0e48, course.lap_length);
                (course.laps, course.lap_length, ram.i32(CAR_COUNT) as u32)
            };
            let now = rng.word() >> 12;
            Ram(&mut m.bus.ram).set_i32(TIME, now as i32);
            for k in 0..count {
                let at = CARS + k * CAR_SIZE;
                let mut car = Car::read(&Ram(&mut m.bus.ram), at);
                // Any lap, any race left, any times so far.
                car.laps.finished = rng.below(5) == 0;
                car.laps.done = rng.below(laps as u32) as u8;
                car.laps.best = if rng.below(3) == 0 { 0 } else { rng.word() >> 16 };
                let mut end = 0u32;
                car.laps.ends = (0..car.laps.done.max(1))
                    .map(|_| {
                        end = end.wrapping_add(rng.word() >> 14);
                        end
                    })
                    .collect();
                car.handling.ai_pace = 0x800 + rng.below(0x1000) as i32;
                car.write(&mut Ram(&mut m.bus.ram), at);
                let progress = (rng.word() as i32) >> (6 + rng.below(10));
                Ram(&mut m.bus.ram).set_i32(DRIVERS + k * DRIVER_SIZE + 0x1a8, progress);
            }
            for k in 0..count {
                let at = CARS + k * CAR_SIZE;
                let mut car = Car::read(&Ram(&mut m.bus.ram), at);
                let progress = Ram(&mut m.bus.ram).i32(DRIVERS + k * DRIVER_SIZE + 0x1a8);
                let progress = if k < 6 { progress } else { 0 };
                let mut rand =
                    hwtr_game::rand::Rand { seed: Ram(&mut m.bus.ram).i32(hwtr_hle::original::rand::SEED) as u32 };
                let estimate = car.flags & 1 == 0 && !car.laps.finished;
                if estimate {
                    car.laps.estimate(progress, laps, lap_length, car.handling.ai_pace, now, &mut rand);
                    estimated += 1;
                }
                m.call(0x8006_1824, &[k, common::OUT, common::OUT + 4]).unwrap();
                let ram = Ram(&mut m.bus.ram);
                let original = Car::read(&ram, at);
                assert_eq!(original.laps, car.laps, "{name} car {k}");
                let time = (laps as usize).checked_sub(1).and_then(|l| car.laps.ends.get(l)).copied().unwrap_or(0);
                assert_eq!(ram.i32(common::OUT) as u32, time, "{name} car {k}: time");
                assert_eq!(ram.i32(common::OUT + 4) as u32, car.laps.best, "{name} car {k}: best");
                assert_eq!(ram.i32(hwtr_hle::original::rand::SEED) as u32, rand.seed, "{name} car {k}: rand");
            }
        }
    }
    eprintln!("estimated {estimated}");
    assert!(estimated > 0);
}

#[test]
fn standings_match_the_original() {
    use hwtr_hle::original::ai::{DRIVER_SIZE, DRIVERS};
    use hwtr_hle::original::car::{CAR_COUNT, CAR_SIZE};
    /// Where 0x80033aa0 leaves each place's car (0x80064ccc) and each car's
    /// points.
    const ORDER: u32 = 0x800d_2688;
    const POINTS: u32 = 0x800d_25f8;
    let Some(exe) = common::exe() else { return };
    let mut rng = common::Rng(0x57a0_0000_0000_0016);
    let (mut unplaced, mut scored) = (0, 0);
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        // The snapshot and the sound's end are not what is checked here.
        m.hook(0x8007_feb0, |_, _| 0);
        m.hook(0x8003_64cc, |_, _| 0);
        for _ in 0..60 {
            let (course, count) = {
                let mut ram = Ram(&mut m.bus.ram);
                let course = course(&ram);
                ram.set_i32(0x800d_0e48, course.lap_length);
                (course, ram.i32(CAR_COUNT) as u32)
            };
            let now = rng.word() >> 12;
            Ram(&mut m.bus.ram).set_i32(TIME, now as i32);
            let mut cars = Vec::new();
            for k in 0..count {
                let at = CARS + k * CAR_SIZE;
                let mut car = Car::read(&Ram(&mut m.bus.ram), at);
                car.laps.finished = rng.below(2) == 0;
                car.laps.done = if car.laps.finished { course.laps } else { rng.below(course.laps as u32) as u8 };
                car.laps.best = rng.word() >> 18;
                // Some ties, and some cars without a time.
                let mut end = 0u32;
                car.laps.ends = (0..car.laps.done.max(1))
                    .map(|_| {
                        end = end.wrapping_add((rng.below(4) << 14) * (rng.below(3) != 0) as u32);
                        end
                    })
                    .collect();
                if car.laps.done == 0 && car.laps.ends == [0] {
                    // As the original's record reads with no laps.
                    car.laps.ends.clear();
                }
                car.handling.ai_pace = 0x800 + rng.below(0x1000) as i32;
                car.write(&mut Ram(&mut m.bus.ram), at);
                Ram(&mut m.bus.ram).set_i32(DRIVERS + k * DRIVER_SIZE + 0x1a8, (rng.word() as i32) >> 12);
                cars.push(car);
            }
            let progress: Vec<i32> =
                (0..6).map(|k| Ram(&mut m.bus.ram).i32(DRIVERS + k * DRIVER_SIZE + 0x1a8)).collect();
            let mut rand =
                hwtr_game::rand::Rand { seed: Ram(&mut m.bus.ram).i32(hwtr_hle::original::rand::SEED) as u32 };
            let ours = hwtr_game::race::standings(
                &mut cars,
                |slot| progress.get(slot as usize).copied().unwrap_or(0),
                &course,
                now,
                &mut rand,
            );
            m.call(0x8003_3aa0, &[]).unwrap();
            let ram = Ram(&mut m.bus.ram);
            for (place, s) in ours.iter().enumerate() {
                assert_eq!(ram.u8(ORDER + place as u32), s.car, "{name}: place {place}");
                assert_eq!(ram.u8(POINTS + s.car as u32), s.points, "{name}: car {}'s points", s.car);
                unplaced += (s.time == 0) as u32;
                scored += (s.points != 0) as u32;
            }
            for (k, car) in cars.iter().enumerate() {
                assert_eq!(Car::read(&ram, CARS + k as u32 * CAR_SIZE).laps, car.laps, "{name}: car {k}");
            }
            assert_eq!(ram.i32(hwtr_hle::original::rand::SEED) as u32, rand.seed, "{name}: rand");
        }
    }
    eprintln!("unplaced {unplaced}, scored {scored}");
    assert!(unplaced > 0 && scored > 0);
}
