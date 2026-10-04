//! The wrong-way watch (0x8005c9b4) against the original: player one's car
//! turned and sent any way at any speed, its timer anywhere, in the states
//! of a race under way; the whole car compared after one call.

mod common;

use hwtr_game::car::Car;
use hwtr_game::math::Tables;
use hwtr_hle::original::car::CARS;
use hwtr_hle::original::{InMemory, Ram};

#[test]
fn wrong_way_matches_the_original() {
    let Some(exe) = common::exe() else { return };
    let t = Tables::from_exe(&exe);
    let mut rng = common::Rng(0x0bad_cafe_0000_0086);
    let (mut tried, mut same, mut flagged) = (0, 0, 0);
    let mut first_miss = None;
    for name in ["desert1-drive", "desert1-speed", "desert1-race"] {
        let Some(mut m) = common::state(&exe, name) else { return };
        let start = m.bus.ram.clone();
        for round in 0..300 {
            m.bus.ram.copy_from_slice(&start);
            let mut car = Car::read(&Ram(&mut m.bus.ram), CARS);
            let a = rng.below(4096) as i32;
            let (s, c) = (t.sin(a * 0x3244 / 2048), t.sin(a * 0x3244 / 2048 + 0x1922));
            car.body.rot = [[c as i16, -s as i16, 0], [s as i16, c as i16, 0], [0, 0, 4096]];
            let speed = (rng.below(120) as i32) << 12;
            let dir = rng.below(4096) as i32 * 0x3244 / 2048;
            car.body.vel = [hwtr_game::math::fx(t.sin(dir + 0x1922), speed) * 18, hwtr_game::math::fx(t.sin(dir), speed) * 18, 0];
            car.wrong_way_ms = [0, 0, 25, 480, 490, 600][rng.below(6) as usize];
            if rng.below(8) == 0 {
                car.grounded = 0;
            }
            car.write(&mut Ram(&mut m.bus.ram), CARS);
            let mut car = Car::read(&Ram(&mut m.bus.ram), CARS);
            let (world, _) = hwtr_hle::original::world::collision(&Ram(&mut m.bus.ram));
            let zone = world.objects.iter().find(|o| o.car == Some(0)).and_then(|o| o.zones.iter().next());
            hwtr_game::laps::watch_way(&world.scp, &mut car, zone, 25);
            m.call(0x8005_c9b4, &[CARS, 25]).unwrap();
            let original = Car::read(&Ram(&mut m.bus.ram), CARS);
            tried += 1;
            flagged += original.wrong_way as u32;
            if original == car {
                same += 1;
            } else if first_miss.is_none() {
                first_miss = Some(format!(
                    "{name} round {round}: original {} ms {}, port {} ms {}",
                    original.wrong_way_ms, original.wrong_way, car.wrong_way_ms, car.wrong_way
                ));
            }
        }
    }
    eprintln!("{flagged} flagged wrong way");
    assert_eq!(same, tried, "{same} of {tried} the same; first miss: {}", first_miss.unwrap_or_default());
    assert!(flagged > 0, "some going the wrong way");
}
