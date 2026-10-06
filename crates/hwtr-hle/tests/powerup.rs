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

const RACES: [&str; 11] = [
    "desert1", "desert2", "desert3", "glacial1", "glacial2", "glacial3", "haunted2", "haunted3", "volcano1", "volcano2",
    "volcano3",
];

/// What of the power-ups the original keeps, to compare: each pickup's
/// name, number, power-up and state, each car's held power-ups, and the
/// cars noted.
type Kept = (Vec<(String, u16, u8, u32, bool)>, Vec<Vec<hwtr_game::powerup::Held>>, [[Option<u8>; 2]; 2]);

fn kept(p: &hwtr_game::powerup::PowerUps) -> Kept {
    (
        p.pickups.iter().map(|q| (q.name.clone(), q.index, q.number, q.taken_at, q.out)).collect(),
        p.held.clone(),
        p.unlocked,
    )
}

/// 0x800671a8 over each track's pickups: the power-ups `load_defs` keeps,
/// in the order the original's list holds them (the order "Random" draws
/// from).
#[test]
fn power_ups_load_in_the_originals_order() {
    use hwtr_hle::original::powerup;
    let Some(exe) = common::exe() else { return };
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work/big");
    let mut checked = 0;
    for track in RACES {
        let Some(mut m) = common::state(&exe, &format!("{track}-race")) else { continue };
        let ram = Ram(&mut m.bus.ram);
        let names: Vec<String> = (0..ram.i32(powerup::PICKUP_COUNT) as u32).map(|k| powerup::pickup(&ram, k).name).collect();
        let theirs: Vec<String> = powerup::defs(&ram).into_iter().map(|(_, d)| d.name).collect();
        let big = root.join(format!("{}BIG", track.to_uppercase()));
        let ours: Vec<String> = hwtr_game::powerup::load_defs(names.iter().map(String::as_str), |name| {
            std::fs::read(big.join(format!("{}PUP", name.to_uppercase()))).ok().and_then(|b| PowerUp::parse(&b))
        })
        .into_iter()
        .map(|d| d.name)
        .collect();
        assert_eq!(ours, theirs, "{track}: pickups {names:?}");
        checked += (theirs.len() > 1) as u32;
    }
    assert!(checked >= 5, "{checked} tracks with more than one power-up");
}

/// Pickups taken (0x80067f98) by any car, out or not, "Random" ones too,
/// and the frames after (0x80068130: power-ups running out, pickups back),
/// over the race's clock, on every track: the cars, the pickups, what each
/// car holds, the cars noted and the random seed compared after each.
#[test]
fn pickups_are_taken_and_come_back_as_in_the_original() {
    use hwtr_game::rand::Rand;
    use hwtr_hle::original::car::CAR_SIZE;
    use hwtr_hle::original::powerup;
    use hwtr_hle::original::rand::SEED;
    let Some(exe) = common::exe() else { return };
    let mut rng = common::Rng(0x0b1c_4b5);
    let (mut taken, mut expired, mut back, mut random) = (0, 0, 0, 0);
    for track in RACES {
        let Some(mut m) = common::state(&exe, &format!("{track}-race")) else { continue };
        let mut ours = powerup::read(&Ram(&mut m.bus.ram), 6);
        if ours.pickups.is_empty() {
            continue;
        }
        let mut now = Ram(&mut m.bus.ram).i32(powerup::CLOCK) as u32;
        for round in 0..150 {
            let what = format!("{track} round {round}");
            // A car through a pickup, put out or not.
            let k = rng.below(ours.pickups.len() as u32);
            if rng.below(3) != 0 {
                ours.pickups[k as usize].out = true;
                powerup::write_pickup(&mut Ram(&mut m.bus.ram), k, &ours.pickups[k as usize]);
            }
            let slot = rng.below(6);
            let at = CARS + CAR_SIZE * slot;
            let mut car = Car::read(&Ram(&mut m.bus.ram), at);
            let mut rand = Rand { seed: Ram(&mut m.bus.ram).i32(SEED) as u32 };
            random += (ours.pickups[k as usize].out && ours.pickups[k as usize].name.eq_ignore_ascii_case("random")) as u32;
            let got = ours.take(&mut car, k as usize, now, |n| rand.below(n));
            // The port's own event for a turbo given (its sound and HUD).
            car.turbo_given = false;
            let pickup = powerup::pickup_at(&Ram(&mut m.bus.ram), k);
            let r = m.call(0x8006_7f98, &[at, pickup]).unwrap();
            let ram = Ram(&mut m.bus.ram);
            assert_eq!(got.is_some(), r & 0xff != 0, "{what}: taken");
            assert_eq!(Car::read(&ram, at), car, "{what}: car {slot} after taking pickup {k}");
            assert_eq!(kept(&powerup::read(&ram, 6)), kept(&ours), "{what}: after taking pickup {k}");
            assert_eq!(ram.i32(SEED) as u32, rand.seed, "{what}: seed");
            taken += got.is_some() as u32;
            // Time on, and a frame's step.
            now = now.wrapping_add(rng.below(6000));
            Ram(&mut m.bus.ram).set_i32(powerup::CLOCK, now as i32);
            let mut cars: Vec<Car> = (0..6).map(|s| Car::read(&Ram(&mut m.bus.ram), CARS + CAR_SIZE * s)).collect();
            let before = kept(&ours);
            ours.step(&mut cars, now);
            m.call(0x8006_8130, &[]).unwrap();
            let ram = Ram(&mut m.bus.ram);
            for (s, c) in cars.iter().enumerate() {
                assert_eq!(Car::read(&ram, CARS + CAR_SIZE * s as u32), *c, "{what}: car {s} after the step");
            }
            assert_eq!(kept(&powerup::read(&ram, 6)), kept(&ours), "{what}: after the step at {now}");
            let after = kept(&ours);
            expired += before.1.iter().zip(&after.1).map(|(b, a)| b.len() - a.len()).sum::<usize>();
            back += before.0.iter().zip(&after.0).filter(|(b, a)| !b.4 && a.4).count();
        }
    }
    eprintln!("{taken} taken, {expired} run out, {back} back, {random} random");
    assert!(taken > 300 && expired > 50 && back > 50 && random > 10, "{taken} taken, {expired} run out, {back} back, {random} random");
}
