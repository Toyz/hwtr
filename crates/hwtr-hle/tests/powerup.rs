//! Power-ups against the original: each of the shipped ones taken
//! (0x80065520) and lost again (0x80065ecc) by a player's car in a race,
//! the whole car compared after each.

mod common;

use hwtr_game::car::Car;
use hwtr_game::powerup::{PowerUp, apply, remove};
use hwtr_hle::original::car::CARS;
use hwtr_hle::original::{InMemory, Ram};

/// Free memory well below the stack, for the power-up's 180 bytes.
const PUP: u32 = 0x801f_8000;

#[test]
fn power_ups_change_a_car_as_the_original_does() {
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work/big");
    let mut tried = 0;
    for (track, name) in [
        ("DESERT1", "HANDLING"),
        ("DESERT1", "GYRO"),
        ("DESERT1", "BRAKES"),
        ("DESERT1", "STICKY"),
        ("DESERT1", "TURBO"),
        ("DESERT1", "UNCAR1"),
        ("DESERT2", "STEEL"),
        ("DESERT2", "RUBBER"),
        ("DESERT2", "4X4"),
    ] {
        let Ok(bytes) = std::fs::read(root.join(format!("{track}BIG/{name}PUP"))) else { continue };
        let def = PowerUp::parse(&bytes).expect("the power-up parses");
        let start = Car::read(&Ram(&mut m.bus.ram), CARS);
        assert_eq!(start.state, 2, "car 0 is a player's");
        {
            let mut ram = Ram(&mut m.bus.ram);
            for (k, &b) in bytes.iter().enumerate() {
                ram.set_u8(PUP + k as u32, b);
            }
        }
        let mut ours = start.clone();
        apply(&def, &mut ours);
        ours.turbo_given = false;
        m.call(0x8006_5520, &[PUP, CARS]).unwrap();
        let theirs = Car::read(&Ram(&mut m.bus.ram), CARS);
        assert_eq!(ours, theirs, "{name} taken");
        if name == "HANDLING" {
            assert_ne!(theirs.handling, start.handling, "handling changes the car");
        }
        remove(&def, &mut ours);
        m.call(0x8006_5ecc, &[PUP, CARS]).unwrap();
        let theirs = Car::read(&Ram(&mut m.bus.ram), CARS);
        assert_eq!(ours, theirs, "{name} lost");
        // Back as it was for the next.
        start.write(&mut Ram(&mut m.bus.ram), CARS);
        tried += 1;
    }
    assert!(tried >= 9, "{tried} tried");
}
