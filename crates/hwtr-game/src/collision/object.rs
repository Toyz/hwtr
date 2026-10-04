//! A body's collision object: its points (a car's wheels and the corners of
//! its box) and the zones they are in.

use crate::math::{Matrix, Vec3};

/// A set with a count per member, newest first: the game's zone lists
/// (0x8005c864 adds, 0x8005c8f0 removes). Adding a member already there
/// counts it again; removing it takes one count away, and the last takes it
/// out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RefSet<T> {
    /// (member, count), newest first.
    pub entries: Vec<(T, u16)>,
}

impl<T: PartialEq + Copy> RefSet<T> {
    pub fn add(&mut self, v: T) {
        match self.entries.iter_mut().find(|(m, _)| *m == v) {
            Some((_, n)) => *n = n.wrapping_add(1),
            None => self.entries.insert(0, (v, 1)),
        }
    }

    pub fn remove(&mut self, v: T) {
        if let Some(k) = self.entries.iter().position(|(m, _)| *m == v) {
            let n = &mut self.entries[k].1;
            *n = n.wrapping_sub(1);
            if *n == 0 {
                self.entries.remove(k);
            }
        }
    }

    /// The members, newest first.
    pub fn iter(&self) -> impl Iterator<Item = T> + '_ {
        self.entries.iter().map(|(m, _)| *m)
    }
}

/// What a collision object belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// 3: a car under full physics (a player's).
    PlayerCar,
    /// 4: a computer car.
    ComputerCar,
    /// Others, not yet ported.
    Other(u8),
}

impl Kind {
    pub fn from_byte(b: u8) -> Kind {
        match b {
            3 => Kind::PlayerCar,
            4 => Kind::ComputerCar,
            b => Kind::Other(b),
        }
    }

    pub fn byte(self) -> u8 {
        match self {
            Kind::PlayerCar => 3,
            Kind::ComputerCar => 4,
            Kind::Other(b) => b,
        }
    }

    pub fn is_car(self) -> bool {
        matches!(self, Kind::PlayerCar | Kind::ComputerCar)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollisionObject {
    pub kind: Kind,
    /// Bit 4: its lead point changed zone this step.
    pub flags: u32,
    /// The centre of its box (the body's position plus its centre of
    /// mass), world space, and its rotation.
    pub centre: Vec3,
    pub rot: Matrix,
    /// Half its box, an inch larger each way, and the word after it.
    pub half: Vec3,
    pub half_pad: i32,
    /// Its points in the body's axes from the centre, and in the world.
    pub local: Vec<Vec3>,
    pub points: Vec<Vec3>,
    /// The zone each point is in.
    pub point_zones: Vec<u16>,
    /// The length of `half`: nothing farther than this from the centre
    /// touches it.
    pub radius: i32,
    /// The zones its points are in.
    pub zones: RefSet<u16>,
    /// The collision step it was last processed in.
    pub stamp: u32,
    /// The car it belongs to, by slot.
    pub car: Option<u8>,
}

/// Where the original keeps the objects and their lists, and the codec.
pub mod layout {
    use super::{CollisionObject, Kind, RefSet};
    use crate::car::layout::{CAR_SIZE, CARS};
    use crate::ram::Ram;

    pub const FLAGS: u32 = 0x04;
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

    impl CollisionObject {
        pub fn read(ram: &Ram, o: u32) -> CollisionObject {
            let n = ram.u8(o + POINT_COUNT) as u32;
            let (local, points, zones_at) =
                (ram.i32(o + LOCAL) as u32, ram.i32(o + POINTS) as u32, ram.i32(o + POINT_ZONES) as u32);
            let car_at = ram.i32(o + CAR) as u32;
            let car = (CARS..CARS + 8 * CAR_SIZE).contains(&car_at).then(|| ((car_at - CARS) / CAR_SIZE) as u8);
            let list = read_list(ram, o + ZONES);
            CollisionObject {
                kind: Kind::from_byte(ram.u8(o + KIND)),
                flags: ram.i32(o + FLAGS) as u32,
                centre: ram.vec3(o + CENTRE),
                rot: ram.matrix(o + ROT),
                half: ram.vec3(o + HALF),
                half_pad: ram.i32(o + HALF + 12),
                local: (0..n).map(|k| ram.vec3(local + 16 * k)).collect(),
                points: (0..n).map(|k| ram.vec3(points + 16 * k)).collect(),
                point_zones: (0..n).map(|k| ram.i16(zones_at + 2 * k) as u16).collect(),
                radius: ram.i32(o + RADIUS),
                zones: RefSet { entries: list.entries.into_iter().map(|(z, c)| (z as u16, c)).collect() },
                stamp: ram.i32(o + STAMP) as u32,
                car,
            }
        }

        /// Writes the object's own fields and arrays back over the original's
        /// (its zone list is compared, not written: the original's nodes come
        /// from a pool).
        pub fn write(&self, ram: &mut Ram, o: u32) {
            ram.set_i32(o + FLAGS, self.flags as i32);
            ram.set_vec3(o + CENTRE, self.centre);
            ram.set_matrix(o + ROT, &self.rot);
            ram.set_vec3(o + HALF, self.half);
            ram.set_i32(o + HALF + 12, self.half_pad);
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
        }
    }
}
