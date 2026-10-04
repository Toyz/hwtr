//! The original's memory as object: where it keeps it, read into and written
//! from the port's types.

use super::car::{CAR_SIZE, CARS};
use super::{InMemory, Ram};
use hwtr_game::collision::object::{CollisionObject, Kind, RefSet};

pub const ID: u32 = 0x00;
pub const FLAGS: u32 = 0x04;
pub const PAIRED: u32 = 0x70;
pub const HEFT: u32 = 0x8c;
pub const CENTRE: u32 = 0x0c;
pub const HALF: u32 = 0x1c;
pub const ROT: u32 = 0x2c;
pub const POINT_COUNT: u32 = 0x4c;
pub const LOCAL: u32 = 0x50;
pub const POINTS: u32 = 0x54;
pub const POINT_ZONES: u32 = 0x58;
pub const RADIUS: u32 = 0x5c;
pub const KIND: u32 = 0x60;
pub const BODY: u32 = 0x64;
pub const CAR: u32 = 0x68;
pub const ZONES: u32 = 0x74;
pub const STAMP: u32 = 0x78;
pub const CONTACT_POINT: u32 = 0x7c;
/// A car's pointer to its collision object.
pub const CAR_OBJECT: u32 = 0x908;

/// A list's members, newest first: nodes of { value, previous, next,
/// count (u16 at +12) } from the head at `head`.
pub fn read_list(ram: &Ram, head: u32) -> RefSet<u32> {
    let mut entries = Vec::new();
    let mut node = ram.i32(head) as u32;
    while node != 0 && entries.len() < 4096 {
        entries.push((ram.i32(node) as u32, ram.i32(node + 12) as u16));
        node = ram.i32(node + 8) as u32;
    }
    RefSet { entries }
}

impl InMemory for CollisionObject {
    fn read(ram: &Ram, o: u32) -> CollisionObject {
        let n = ram.u8(o + POINT_COUNT) as u32;
        let (local, points, zones_at) =
            (ram.i32(o + LOCAL) as u32, ram.i32(o + POINTS) as u32, ram.i32(o + POINT_ZONES) as u32);
        let car_at = ram.i32(o + CAR) as u32;
        let car = (CARS..CARS + 8 * CAR_SIZE).contains(&car_at).then(|| ((car_at - CARS) / CAR_SIZE) as u8);
        let list = read_list(ram, o + ZONES);
        CollisionObject {
            id: ram.i32(o + ID) as u16,
            kind: Kind::from_byte(ram.u8(o + KIND)),
            flags: ram.i32(o + FLAGS) as u32,
            centre: ram.vec3(o + CENTRE),
            rot: ram.matrix(o + ROT),
            half: ram.vec3(o + HALF),
            local: (0..n).map(|k| ram.vec3(local + 16 * k)).collect(),
            points: (0..n).map(|k| ram.vec3(points + 16 * k)).collect(),
            point_zones: (0..n).map(|k| ram.i16(zones_at + 2 * k) as u16).collect(),
            radius: ram.i32(o + RADIUS),
            zones: RefSet { entries: list.entries.into_iter().map(|(z, c)| (z as u16, c)).collect() },
            stamp: ram.i32(o + STAMP) as u32,
            contact_point: ram.vec3(o + CONTACT_POINT),
            car,
            paired: ram.i32(o + PAIRED) as u32,
            heft: ram.i32(o + HEFT) as u32,
            pickup: None,
            volume: None,
        }
    }

    /// Writes the object's own fields and arrays back over the original's
    /// (its zone list is compared, not written: the original's nodes come
    /// from a pool).
    fn write(&self, ram: &mut Ram, o: u32) {
        ram.set_i32(o + FLAGS, self.flags as i32);
        ram.set_vec3(o + CENTRE, self.centre);
        ram.set_matrix(o + ROT, &self.rot);
        ram.set_vec3(o + HALF, self.half);
        let (local, points, zones_at) =
            (ram.i32(o + LOCAL) as u32, ram.i32(o + POINTS) as u32, ram.i32(o + POINT_ZONES) as u32);
        for (k, p) in self.local.iter().enumerate() {
            ram.set_vec3(local + 16 * k as u32, *p);
        }
        for (k, p) in self.points.iter().enumerate() {
            ram.set_vec3(points + 16 * k as u32, *p);
        }
        for (k, z) in self.point_zones.iter().enumerate() {
            ram.set_i16(zones_at + 2 * k as u32, *z as i16);
        }
        ram.set_i32(o + RADIUS, self.radius);
        ram.set_u8(o + KIND, self.kind.byte());
        ram.set_i32(o + STAMP, self.stamp as i32);
        ram.set_vec3(o + CONTACT_POINT, self.contact_point);
        ram.set_i32(o + PAIRED, self.paired as i32);
    }
}
