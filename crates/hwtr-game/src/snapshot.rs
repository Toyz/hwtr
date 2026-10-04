//! The snapshot module (0x8007fe48 sets it up, 0x8007feb0 takes one,
//! 0x80080020 puts one back): moments of the race kept for the results,
//! which show them two seconds each once the race stands still.
//!
//! A snapshot is taken at the start, at the end, at the top of a player's
//! big jump, half a second into a player's wreck and when two players'
//! cars touch; at most one a second (by the race clock), and only while
//! the 4096 bytes the original keeps them in have room. Each holds every
//! car's pose and every camera's, coarsely: positions in whole units, the
//! first two rows of each rotation in 127ths (the third their cross
//! product when put back).

use crate::camera::{Camera, ViewMode};
use crate::car::Car;
use crate::math::{Matrix, Vec3};

/// The original's room for snapshots, and what each part of one takes.
const BYTES: u32 = 4096;
const HEADER: u32 = 24;
const CAR: u32 = 24;
const CAMERA: u32 = 16;
/// The parts that save nothing still take a word each: the moving
/// volumes' (0x8006ba48), the track objects' (0x8007f6f8), and the loose
/// bodies' when none is flying (0x8007e6e0).
const EMPTY: u32 = 4;
/// The least time between two snapshots, ms.
const APART_MS: u32 = 1000;

/// A car as 0x8004a8fc keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarShot {
    /// Its draw flags (+0x8, the low half).
    pub flags_8: u16,
    pub wrecked: bool,
    /// The floor under it, if found: its normal, and its distance with the
    /// origin folded in (whole units).
    pub floor: Option<([i16; 3], i16)>,
    pub rot: [[i8; 3]; 2],
    pub pos: [i16; 3],
}

/// A camera as 0x8003b610 keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraShot {
    pub mode: u8,
    pub car: u8,
    pub rot: [[i8; 3]; 2],
    pub pos: [i16; 3],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub cars: Vec<CarShot>,
    pub cameras: Vec<CameraShot>,
}

/// The snapshots taken (0x800d2718 counts them), the bytes they fill
/// (0x800d271c) and when the last was taken (0x800d2720).
#[derive(Clone, Debug, Default)]
pub struct Snapshots {
    pub shots: Vec<Snapshot>,
    used: u32,
    last: u32,
}

/// A rotation entry in 127ths, clamped.
fn pack_unit(v: i16) -> i8 {
    ((v as i32 * 127) >> 12).clamp(-127, 127) as i8
}

/// Back to 4.12, as the original divides: whole part, then the remainder.
fn unpack_unit(b: i8) -> i16 {
    let v = (b as i32) << 12;
    let d = 127 << 12;
    (((v / d) << 12) + (((v % d) << 12) / d)) as i16
}

fn pack_rot(m: &Matrix) -> [[i8; 3]; 2] {
    [m[0].map(pack_unit), m[1].map(pack_unit)]
}

fn fx16(a: i16, b: i16) -> i32 {
    (a as i32 * b as i32) >> 12
}

/// The first two rows back, the third their cross product.
fn unpack_rot(r: &[[i8; 3]; 2]) -> Matrix {
    let a = r[0].map(unpack_unit);
    let b = r[1].map(unpack_unit);
    let c = [
        (fx16(a[1], b[2]) - fx16(b[1], a[2])) as i16,
        (fx16(b[0], a[2]) - fx16(a[0], b[2])) as i16,
        (fx16(a[0], b[1]) - fx16(b[0], a[1])) as i16,
    ];
    [a, b, c]
}

fn pack_pos(p: Vec3) -> [i16; 3] {
    p.map(|c| (c >> 12) as i16)
}

fn unpack_pos(p: [i16; 3]) -> Vec3 {
    p.map(|c| (c as i32) << 12)
}

fn fx(a: i32, b: i32) -> i32 {
    ((a as i64 * b as i64) >> 12) as i32
}

impl CarShot {
    fn take(car: &Car) -> CarShot {
        let ground = &car.ground;
        let floor = ground.floor.found.then(|| {
            let n = ground.floor.normal;
            let along = (0..3).fold(0i32, |s, i| s.wrapping_add(fx(n[i], ground.origin[i])));
            (n.map(|c| c as i16), (ground.floor.d.wrapping_sub(along) >> 12) as i16)
        });
        CarShot {
            flags_8: car.flags_8 as u16,
            wrecked: car.wrecked,
            floor,
            rot: pack_rot(&car.body.rot),
            pos: pack_pos(car.body.pos),
        }
    }

    /// 0x8004aef8's part for one car: its pose, floor and draw flags back,
    /// and what was moving about it stilled.
    fn put_back(&self, car: &mut Car) {
        car.reset_grace_ms = 0;
        car.flags_8 = self.flags_8 as i32 | 8;
        tracing::trace!(
            "car {}: the wreck's smoke {} (0x80029e10 / 0x80029f04): not yet ported",
            car.slot,
            if self.wrecked { "again" } else { "cleared" }
        );
        car.ground.floor.found = self.floor.is_some();
        if let Some((normal, d)) = self.floor {
            car.ground.floor.normal = normal.map(i32::from);
            car.ground.origin = [0; 3];
            car.ground.floor.d = (d as i32) << 12;
        }
        car.body.rot = unpack_rot(&self.rot);
        car.body.pos = unpack_pos(self.pos);
        car.steer = 0;
        car.body.asleep = false;
        for wheel in &mut car.wheels {
            wheel.spin_rate = 0;
            wheel.compression = 0;
        }
    }
}

impl CameraShot {
    fn take(c: &Camera) -> CameraShot {
        let mode = match c.mode {
            Some(ViewMode::Mounted) | None => 0,
            Some(ViewMode::Chase) => 1,
            Some(ViewMode::Other(b)) => b,
        };
        CameraShot { mode, car: c.car, rot: pack_rot(&c.rot), pos: pack_pos(c.pos) }
    }

    /// 0x8003bafc's part for one camera.
    fn put_back(&self, c: &mut Camera) {
        c.rot = unpack_rot(&self.rot);
        c.pos = unpack_pos(self.pos);
        c.mode = Some(match self.mode {
            0 => ViewMode::Mounted,
            1 => ViewMode::Chase,
            b => ViewMode::Other(b),
        });
        c.shake = 0;
        c.car = self.car;
    }
}

impl Snapshots {
    pub fn len(&self) -> usize {
        self.shots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.shots.is_empty()
    }

    /// 0x8007feb0 at race clock `time`: a snapshot, unless the last was
    /// under a second ago or there is no room for every part of it.
    /// `mid_step` is a car caught mid-step, and its rotation then.
    pub fn take(&mut self, time: u32, cars: &[Car], cameras: &[Camera], mid_step: Option<(usize, Matrix)>) {
        if !self.shots.is_empty() && time.wrapping_sub(self.last) < APART_MS {
            return;
        }
        let Some(mut room) = (BYTES - self.used).checked_sub(HEADER) else { return };
        let mut size = 0;
        for need in [CAR * cars.len() as u32, EMPTY, EMPTY, EMPTY, CAMERA * cameras.len() as u32] {
            if need == 0 || room < need {
                return;
            }
            room -= need;
            size += need;
        }
        self.shots.push(Snapshot {
            cars: cars
                .iter()
                .enumerate()
                .map(|(k, car)| {
                    let mut shot = CarShot::take(car);
                    if let Some((_, rot)) = mid_step.filter(|&(slot, _)| slot == k) {
                        shot.rot = pack_rot(&rot);
                    }
                    shot
                })
                .collect(),
            cameras: cameras.iter().map(CameraShot::take).collect(),
        });
        self.used += HEADER + size;
        self.last = time;
    }

    /// 0x80080020: snapshot `k`'s cars and cameras back (the loose bodies,
    /// none kept, are not touched).
    pub fn put_back(&self, k: usize, cars: &mut [Car], cameras: &mut [Camera]) {
        let Some(shot) = self.shots.get(k) else { return };
        for (s, car) in shot.cars.iter().zip(cars.iter_mut()) {
            s.put_back(car);
        }
        for (s, c) in shot.cameras.iter().zip(cameras.iter_mut()) {
            s.put_back(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_round_trip_coarsely() {
        assert_eq!(pack_unit(4096), 127);
        assert_eq!(pack_unit(-4096), -127);
        assert_eq!(unpack_unit(127), 4096);
        assert_eq!(unpack_unit(-127), -4096);
        assert_eq!(unpack_unit(64), ((64i32 << 12) / 127) as i16);
        assert_eq!(pack_unit(-1), -1);
    }

    #[test]
    fn identity_comes_back() {
        let m: Matrix = [[4096, 0, 0], [0, 4096, 0], [0, 0, 4096]];
        assert_eq!(unpack_rot(&pack_rot(&m)), m);
    }
}
