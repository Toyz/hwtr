//! The computer cars' driver against the original's.

mod common;

use hwtr_game::car::Car;
use hwtr_game::math::Tables;
use hwtr_game::rand::Rand;
use hwtr_hle::original::ai::{ai, write_ai};
use hwtr_hle::original::car::{CAR_COUNT, CAR_SIZE, CARS};
use hwtr_hle::original::rand::SEED;
use hwtr_hle::original::{InMemory, Ram};

const STATES: [&str; 3] = ["desert1-race", "desert1-drive", "desert1-speed"];
/// The race clock, and where `bestline_load` keeps the route.
const TIME: u32 = 0x800d_0e34;
const ROUTE: u32 = 0x8013_23ec;
const ROUTE_LEN: u32 = 0x8013_23dc;

fn addresses(ram: &Ram) -> Vec<u32> {
    (0..ram.i32(CAR_COUNT) as u32).map(|k| CARS + k * CAR_SIZE).collect()
}

fn cars(ram: &Ram) -> Vec<Car> {
    addresses(ram).iter().map(|&a| Car::read(ram, a)).collect()
}

#[test]
fn ai_steps_match_the_original() {
    let Some(exe) = common::exe() else { return };
    let mut checked = 0;
    let mut rng = common::Rng(0xa1a1_0000_0000_0001);
    let mut seen = std::collections::BTreeMap::<&str, u32>::new();
    for name in STATES {
        let Some(mut m) = common::state(&exe, name) else { return };
        let t = hwtr_hle::original::tables(&m.bus.ram);
        let _ = Tables::from_exe(&exe);
        let (tuning, stream) = {
            let ram = Ram(&mut m.bus.ram);
            let at = ram.i32(ROUTE) as u32;
            let len = ram.i16(ROUTE_LEN) as u16 as u32;
            (hwtr_hle::original::car::tuning(&ram), (0..len).map(|k| ram.u8(at + k)).collect::<Vec<u8>>())
        };
        for step in 0..3000 {
            // Now and then a knock to a computer car, as the collision code
            // gives one, handed to its driver by both.
            if rng.below(6) == 0 {
                let mut ram = Ram(&mut m.bus.ram);
                let at = addresses(&ram);
                let k = 1 + rng.below(at.len() as u32 - 1) as usize;
                let mut car = Car::read(&ram, at[k]);
                car.body.momentum = [0; 3].map(|_| (rng.word() as i32) >> (10 + rng.below(8)));
                if rng.below(2) == 0 {
                    car.body.ang_momentum = [0; 3].map(|_| ((rng.word() as i32) >> (12 + rng.below(8))) as i64);
                }
                car.write(&mut ram, at[k]);
                let mut port_ai = ai(&ram, &at);
                let mut port_cars = cars(&ram);
                port_ai.handoff(&mut port_cars, &tuning);
                m.call(0x8007_937c, &[]).unwrap();
                let ram = Ram(&mut m.bus.ram);
                assert_eq!(ai(&ram, &at), port_ai, "{name} step {step}: the handoff");
                assert_eq!(cars(&ram), port_cars, "{name} step {step}: the handoff's cars");
                *seen.entry("knocks").or_default() += 1;
            }
            let ram = Ram(&mut m.bus.ram);
            let at = addresses(&ram);
            let mut port_ai = ai(&ram, &at);
            let mut port_cars = cars(&ram);
            let mut rand = Rand { seed: ram.i32(SEED) as u32 };
            let time = ram.i32(TIME) as u32;
            let before: Vec<bool> = port_cars.iter().map(|c| c.wrecked).collect();
            // Read back through the codecs, as the original will see them.
            let mut copy = m.bus.ram.clone();
            {
                let mut r = Ram(&mut copy);
                write_ai(&mut r, &at, &port_ai);
                for (c, &a) in port_cars.iter().zip(&at) {
                    c.write(&mut r, a);
                }
            }
            assert_eq!(ai(&Ram(&mut copy), &at), port_ai, "{name} step {step}: the codec round trip");
            port_ai.step(&t, &tuning, &stream, &mut port_cars, 25, time, &mut rand);
            m.call(0x8007_265c, &[25]).unwrap();
            let ram = Ram(&mut m.bus.ram);
            let original = ai(&ram, &at);
            let original_cars = cars(&ram);
            for (k, (o, p)) in original.drivers.iter().zip(&port_ai.drivers).enumerate() {
                assert_eq!(o, p, "{name} step {step}: driver {k}");
            }
            for (k, (o, p)) in original_cars.iter().zip(&port_cars).enumerate() {
                assert_eq!(o, p, "{name} step {step}: car {k}");
            }
            assert_eq!(original.rand, port_ai.rand, "{name} step {step}: the AI's seed");
            // A wreck crumples the car's model (0x8002e574, not yet ported),
            // drawing random numbers the port does not.
            let wrecks = original_cars.iter().zip(&before).filter(|(c, w)| c.wrecked && !**w).count();
            if wrecks == 0 {
                assert_eq!(ram.i32(SEED) as u32, rand.seed, "{name} step {step}: the game's seed");
            } else {
                *seen.entry("wrecks (seed not compared)").or_default() += wrecks as u32;
            }
            checked += 1;
            for d in &original.drivers {
                if d.stunt & 2 != 0 {
                    *seen.entry("stunt frames").or_default() += 1;
                }
                if d.line.flags & 2 != 0 {
                    *seen.entry("air frames").or_default() += 1;
                }
                if d.knocked == 2 {
                    *seen.entry("knocked frames").or_default() += 1;
                }
                if d.rubber != 4096 {
                    *seen.entry("banded frames").or_default() += 1;
                }
                if !d.line.choices.is_empty() {
                    *seen.entry("past a branch").or_default() += 1;
                }
            }
        }
    }
    eprintln!("{checked} steps; {seen:?}");
    assert!(checked > 0);
}
