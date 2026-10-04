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
