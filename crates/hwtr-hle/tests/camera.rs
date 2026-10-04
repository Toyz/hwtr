//! The race camera against the original's.

mod common;

use hwtr_game::camera::Camera;
use hwtr_game::car::Car;
use hwtr_game::math::Tables;
use hwtr_game::rand::Rand;
use hwtr_hle::original::camera::{COUNT, VIEWS, at};
use hwtr_hle::original::car::CARS;
use hwtr_hle::original::rand::SEED;
use hwtr_hle::original::{InMemory, Ram};

const STATES: [&str; 3] = ["desert1-race", "desert1-drive", "desert1-speed"];

#[test]
fn camera_steps_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let t = Tables::from_exe(&exe);
    let mut rng = common::Rng(0xca3e_0000_0000_0014);
    let (mut same, mut tried) = (0, 0);
    let mut first_miss = None;
    let mut per = std::collections::BTreeMap::new();
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let start = m.bus.ram.clone();
        assert_eq!(m.bus.ram[(COUNT & 0x1f_ffff) as usize], 1, "{name}: one camera");
        let count = m.bus.ram[(VIEWS & 0x1f_ffff) as usize];
        for round in 0..600 {
            m.bus.ram.copy_from_slice(&start);
            let (tuning, mut cam, mut car) = {
                let ram = Ram(&mut m.bus.ram);
                (hwtr_hle::original::car::tuning(&ram), Camera::read(&ram, at(0)), Car::read(&ram, CARS))
            };
            // The camera somewhere around the car, moving; the car going at
            // any speed, any way.
            let near = |rng: &mut common::Rng| (rng.word() as i32) >> (14 + rng.below(6));
            cam.pos = hwtr_game::math::add(cam.pos, [0; 3].map(|_| near(&mut rng)));
            cam.vel = [0; 3].map(|_| (rng.word() as i32) >> (10 + rng.below(8)));
            cam.view = [0, 0, 1, 2, rng.below(5) as u8][rng.below(5) as usize];
            cam.snap = rng.below(6) == 0;
            cam.zoom = if rng.below(3) == 0 { (rng.word() as i32) >> 20 } else { 0 };
            cam.shake = [0, 0, 0, 250, rng.below(300)][rng.below(5) as usize];
            cam.button = rng.below(10) == 0;
            // Now and then mid-way through the opening sweep.
            cam.intro_ms = if rng.below(4) == 0 { 1 + rng.below(5000) as i32 } else { 0 };
            car.body.vel = [0; 3].map(|_| (rng.word() as i32) >> (8 + rng.below(10)));
            car.body.speed = t.length(car.body.vel);
            car.boost = (rng.below(4) == 0).then(|| (rng.word() as i32) >> 9);
            car.flags = (car.flags & !4) | if rng.below(4) == 0 { 4 } else { 0 };
            let seed = rng.word();
            {
                let mut ram = Ram(&mut m.bus.ram);
                cam.write(&mut ram, at(0));
                car.write(&mut ram, CARS);
                ram.set_i32(SEED, seed as i32);
            }
            // Read back, as the original will see them.
            let (mut cam, mut car) = {
                let ram = Ram(&mut m.bus.ram);
                (Camera::read(&ram, at(0)), Car::read(&ram, CARS))
            };
            let mut rand = Rand { seed };
            let racing = m.bus.ram[(0x800d_0de9u32 & 0x1f_ffff) as usize] != 0;
            let (world, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
            let time = m.bus.read_u32(0x800d_0e34);
            let around = hwtr_game::camera::Surroundings {
                tables: &t,
                tuning: &tuning,
                views: &t.views.one,
                count,
                racing,
                time,
                flyby: &world.scp.flyby,
                collision: &world,
                spots: &[],
                demo: false,
            };
            cam.step(&around, &mut car, 25, &mut rand);
            m.call(0x8003_69e4, &[25]).unwrap();
            let original = Camera::read(&Ram(&mut m.bus.ram), at(0));
            tried += 1;
            if original == cam && m.bus.read_u32(SEED) == rand.seed {
                same += 1;
                *per.entry(name).or_insert(0) += 1;
            } else if first_miss.is_none() {
                first_miss = Some(format!("{name} round {round}:\n original {original:?}\n port     {cam:?}"));
            }
        }
    }
    eprintln!("the same per state: {per:?}");
    assert_eq!(same, tried, "{same} of {tried} the same; first miss: {}", first_miss.unwrap_or_default());
}

/// The attract race's camera against the original's: the demo flag
/// (0x800d2632), views (0x800d2638) and view count (0x800d2634) set as
/// `camera_load` sets them with no players; each round a camera in a chase
/// view or at a trackside camera, the director due now or not, the cars
/// moving any way. The step and the director (0x8003a690) compared.
#[test]
fn attract_camera_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let t = Tables::from_exe(&exe);
    let mut rng = common::Rng(0xa77a_c700_0000_0001);
    let Some(mut m) = common::state(&exe, "desert1-drive") else { return };
    {
        let mut ram = Ram(&mut m.bus.ram);
        ram.set_u8(0x800d_2632, 1);
        ram.set_u8(0x800d_2634, t.views.demo_count);
        ram.set_i32(0x800d_2638, hwtr_game::camera::Views::DEMO as i32);
    }
    let start = m.bus.ram.clone();
    let spots: Vec<hwtr_game::camera::Spot> = {
        let ram = Ram(&mut m.bus.ram);
        let header = ram.i32(0x800d_2540) as u32;
        let (n, at) = (ram.u8(header + 56) as u32, ram.i32(header + 60) as u32);
        (0..n)
            .map(|k| {
                let s = at + 40 * k;
                hwtr_game::camera::Spot {
                    flags: ram.i32(s) as u32,
                    fov: ram.i32(s + 4),
                    rot: std::array::from_fn(|i| std::array::from_fn(|j| ram.i16(s + 8 + 2 * (3 * i + j) as u32))),
                    pos: [ram.i32(s + 28), ram.i32(s + 32), ram.i32(s + 36)],
                }
            })
            .collect()
    };
    assert!(spots.len() > 3, "the track has trackside cameras");
    let range = Ram(&mut m.bus.ram).i16(0x800d_2630) as u16;
    let (mut same, mut tried, mut cut_to_spot) = (0, 0, 0);
    let mut first_miss = None;
    for round in 0..400 {
        m.bus.ram.copy_from_slice(&start);
        let (tuning, mut cam, mut cars) = {
            let ram = Ram(&mut m.bus.ram);
            let n = ram.i32(hwtr_hle::original::car::CAR_COUNT) as u32;
            let cars: Vec<Car> = (0..n).map(|i| Car::read(&ram, CARS + i * hwtr_hle::original::car::CAR_SIZE)).collect();
            (hwtr_hle::original::car::tuning(&ram), Camera::read(&ram, at(0)), cars)
        };
        let n = cars.len() as u32;
        let mode = [1u8, 2, 3][rng.below(3) as usize];
        cam.mode = Some(match mode {
            1 => hwtr_game::camera::ViewMode::Chase,
            m => hwtr_game::camera::ViewMode::Other(m),
        });
        cam.view = if mode == 1 { rng.below(2) as u8 } else { rng.below(spots.len() as u32) as u8 };
        cam.car = rng.below(n) as u8;
        cam.director_ms = [0, 10, 25, 26, 3000][rng.below(5) as usize];
        cam.snap = rng.below(4) == 0;
        cam.intro_ms = 0;
        cam.flyby = false;
        for car in &mut cars {
            car.body.vel = [0; 3].map(|_| (rng.word() as i32) >> (8 + rng.below(8)));
            // Half of them heading for a trackside camera, which some
            // will be near.
            if rng.below(2) == 0 {
                let spot = &spots[rng.below(spots.len() as u32) as usize];
                let dir = t.normalize(hwtr_game::math::sub(spot.pos, car.body.pos));
                car.body.vel = dir.map(|c| hwtr_game::math::fx(c, 100 << 12));
                if rng.below(2) == 0 {
                    car.body.pos = hwtr_game::math::add(spot.pos, [rng.word() as i32 >> 10, rng.word() as i32 >> 10, 0]);
                }
            }
            car.body.speed = t.length(car.body.vel);
            car.wrecked = rng.below(5) == 0;
        }
        let seed = rng.word();
        {
            let mut ram = Ram(&mut m.bus.ram);
            cam.write(&mut ram, at(0));
            for (i, car) in cars.iter().enumerate() {
                car.write(&mut ram, CARS + i as u32 * hwtr_hle::original::car::CAR_SIZE);
            }
            ram.set_i32(SEED, seed as i32);
        }
        let (mut cam, mut cars) = {
            let ram = Ram(&mut m.bus.ram);
            let cars: Vec<Car> = (0..n).map(|i| Car::read(&ram, CARS + i * hwtr_hle::original::car::CAR_SIZE)).collect();
            (Camera::read(&ram, at(0)), cars)
        };
        let mut rand = Rand { seed };
        let racing = m.bus.ram[(0x800d_0de9u32 & 0x1f_ffff) as usize] != 0;
        let (world, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
        let time = m.bus.read_u32(0x800d_0e34);
        let around = hwtr_game::camera::Surroundings {
            tables: &t,
            tuning: &tuning,
            views: &t.views.demo,
            count: t.views.demo_count,
            racing,
            time,
            flyby: &world.scp.flyby,
            collision: &world,
            spots: &spots,
            demo: true,
        };
        let slot = cam.car as usize;
        cam.step(&around, &mut cars[slot], 25, &mut rand);
        cam.direct(&t, &spots, &cars, range, &t.views.demo, t.views.demo_count, 25, &mut rand);
        m.call(0x8003_69e4, &[25]).unwrap();
        let original = Camera::read(&Ram(&mut m.bus.ram), at(0));
        tried += 1;
        if original.mode == Some(hwtr_game::camera::ViewMode::Other(3)) && cam.director_ms > 3900 {
            cut_to_spot += 1;
        }
        if original == cam && m.bus.read_u32(SEED) == rand.seed {
            same += 1;
        } else if first_miss.is_none() {
            first_miss = Some(format!("round {round} (mode {mode}):\n original {original:?}\n port     {cam:?}"));
        }
    }
    eprintln!("{cut_to_spot} cuts to a trackside camera");
    assert_eq!(same, tried, "{same} of {tried} the same; first miss: {}", first_miss.unwrap_or_default());
    assert!(cut_to_spot > 0, "the director chose a trackside camera at least once");
}
