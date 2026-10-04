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

/// The unlock pickups (UNCAR1, UNCAR2) taken by cars in slots 0 to 2, each
/// made a player's under full physics: the cars the original notes (0x800d0eaa to
/// 0x800d0eb0, from the track's two at 0x800d0ea6 and 0x800d0ea8) against
/// `PowerUps::unlocked`.
#[test]
fn the_unlock_pickups_note_cars_as_the_original_does() {
    use hwtr_game::powerup::PowerUps;
    use hwtr_hle::original::car::CAR_SIZE;
    let Some(exe) = common::exe() else { return };
    let Some(mut m) = common::state(&exe, "desert1-race") else { return };
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work/big");
    let start = m.bus.ram.clone();
    let mut noted = 0;
    for name in ["UNCAR1", "UNCAR2"] {
        let Ok(bytes) = std::fs::read(root.join(format!("DESERT1BIG/{name}PUP"))) else { continue };
        let def = PowerUp::parse(&bytes).expect("the power-up parses");
        for slot in 0..3u32 {
            m.bus.ram.copy_from_slice(&start);
            let at = CARS + CAR_SIZE * slot;
            let unlockable = [17u8 + slot as u8, 30 + slot as u8];
            let mut car = {
                let mut ram = Ram(&mut m.bus.ram);
                for (k, &b) in bytes.iter().enumerate() {
                    ram.set_u8(PUP + k as u32, b);
                }
                ram.set_i16(0x800d_0ea6, unlockable[0] as i16);
                ram.set_i16(0x800d_0ea8, unlockable[1] as i16);
                for k in 0..4 {
                    ram.set_i16(0x800d_0eaa + 2 * k, -1);
                }
                let mut car = Car::read(&ram, at);
                // A player's, under full physics (as only such a car takes
                // a power-up).
                car.flags |= 1;
                car.state = 2;
                car.write(&mut ram, at);
                car
            };
            let mut ours = PowerUps::new(&[], vec![def.clone()], 3, unlockable);
            ours.apply(0, &mut car, 0);
            m.call(0x8006_5520, &[PUP, at]).unwrap();
            let ram = Ram(&mut m.bus.ram);
            let note = |a: u32| match ram.i16(a) {
                -1 => None,
                v => Some(v as u8),
            };
            // The original keeps them by car then player.
            let theirs = [[note(0x800d_0eaa), note(0x800d_0eae)], [note(0x800d_0eac), note(0x800d_0eb0)]];
            assert_eq!(ours.unlocked, theirs, "{name} by car {slot}");
            noted += ours.unlocked.iter().flatten().flatten().count();
        }
    }
    assert_eq!(noted, 4, "each pickup notes its car for both players' slots");
}
