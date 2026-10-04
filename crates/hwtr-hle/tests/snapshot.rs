//! The results' snapshots against the original: one taken of a race under
//! way (0x8007feb0), byte for byte, and put back (0x80080020) over cars and
//! a camera moved since, the whole of each compared.

mod common;

use hwtr_game::camera::Camera;
use hwtr_game::car::Car;
use hwtr_game::snapshot::{CameraShot, CarShot, Snapshots};
use hwtr_hle::original::car::{CARS, CAR_SIZE};
use hwtr_hle::original::{InMemory, Ram, camera};

/// The snapshots' buffer, and their count and bytes used.
const BUFFER: u32 = 0x8013_5a18;
const COUNT: u32 = 0x800d_2718;
const USED: u32 = 0x800d_271c;

fn car_bytes(s: &CarShot) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend(s.flags_8.to_le_bytes());
    b.push(s.wrecked as u8);
    b.push(s.floor.is_some() as u8);
    let (normal, d) = s.floor.unwrap_or_default();
    for v in normal {
        b.extend(v.to_le_bytes());
    }
    b.extend(d.to_le_bytes());
    b.extend(s.rot.iter().flatten().map(|&v| v as u8));
    for v in s.pos {
        b.extend(v.to_le_bytes());
    }
    b
}

fn camera_bytes(s: &CameraShot) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend((s.mode as u16).to_le_bytes());
    b.extend((s.car as u16).to_le_bytes());
    b.extend(s.rot.iter().flatten().map(|&v| v as u8));
    for v in s.pos {
        b.extend(v.to_le_bytes());
    }
    b
}

#[test]
fn snapshots_keep_and_put_back_what_the_original_does() {
    let Some(exe) = common::exe() else { return };
    for state in ["desert1-drive", "desert1-air"] {
        let Some(mut m) = common::state(&exe, state) else { return };
        let (cars, cameras): (Vec<Car>, Vec<Camera>) = {
            let ram = Ram(&mut m.bus.ram);
            let n = ram.i32(hwtr_hle::original::car::CAR_COUNT) as u32;
            let k = ram.u8(camera::COUNT) as u32;
            ((0..n).map(|i| Car::read(&ram, CARS + i * CAR_SIZE)).collect(), (0..k).map(|i| Camera::read(&ram, camera::at(i))).collect())
        };
        m.call(0x8007_fe48, &[]).unwrap();
        m.call(0x8007_feb0, &[]).unwrap();
        let mut ours = Snapshots::default();
        ours.take(0, &cars, &cameras, None);
        let shot = &ours.shots[0];
        let ram = Ram(&mut m.bus.ram);
        assert_eq!(ram.i32(COUNT), 1, "{state}: one taken");
        let used = ram.i32(USED) as u32;
        assert_eq!(used, 24 + 24 * cars.len() as u32 + 12 + 16 * cameras.len() as u32, "{state}: its size");
        let theirs: Vec<u8> = (24..used).map(|k| ram.u8(BUFFER + k)).collect();
        let mut mine: Vec<u8> = shot.cars.iter().flat_map(car_bytes).collect();
        mine.extend([0; 12]);
        mine.extend(shot.cameras.iter().flat_map(camera_bytes));
        assert_eq!(mine, theirs, "{state}: the snapshot's bytes");

        // Everything moved on, then the snapshot put back.
        let mut moved = cars.clone();
        for car in &mut moved {
            car.body.pos = car.body.pos.map(|c| c.wrapping_add(123_456));
            car.body.rot = [[0, 4096, 0], [-4096, 0, 0], [0, 0, 4096]];
            car.steer = 777;
            car.body.asleep = true;
            car.reset_grace_ms = 99;
            car.flags_8 = 0x1_0003;
            for wheel in &mut car.wheels {
                wheel.compression = 55;
                wheel.spin_rate = 66;
            }
        }
        let mut moved_cameras = cameras.clone();
        for c in &mut moved_cameras {
            c.pos = c.pos.map(|v| v.wrapping_sub(999_999));
            c.shake = 250;
            c.car = 3;
        }
        {
            let mut ram = Ram(&mut m.bus.ram);
            for (i, car) in moved.iter().enumerate() {
                car.write(&mut ram, CARS + i as u32 * CAR_SIZE);
            }
            for (i, c) in moved_cameras.iter().enumerate() {
                c.write(&mut ram, camera::at(i as u32));
            }
        }
        m.call(0x8008_0020, &[0]).unwrap();
        ours.put_back(0, &mut moved, &mut moved_cameras);
        let ram = Ram(&mut m.bus.ram);
        for (i, car) in moved.iter().enumerate() {
            assert_eq!(*car, Car::read(&ram, CARS + i as u32 * CAR_SIZE), "{state}: car {i} put back");
        }
        for (i, c) in moved_cameras.iter().enumerate() {
            assert_eq!(*c, Camera::read(&ram, camera::at(i as u32)), "{state}: camera {i} put back");
        }
    }
}
