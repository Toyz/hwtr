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
    /// Its number, in the order objects were made.
    pub id: u16,
    pub kind: Kind,
    /// Bit 4: its lead point changed zone this step.
    pub flags: u32,
    /// The centre of its box (the body's position plus its centre of
    /// mass), world space, and its rotation.
    pub centre: Vec3,
    pub rot: Matrix,
    /// Half its box, an inch larger each way.
    pub half: Vec3,
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
    /// The collision step it last touched the track in, and where.
    pub stamp: u32,
    pub contact_point: Vec3,
    /// The car it belongs to, by slot.
    pub car: Option<u8>,
    /// The collision step it last looked for objects to meet.
    pub paired: u32,
    /// A prop's weight, for the knocks it takes and gives.
    pub heft: u32,
}
