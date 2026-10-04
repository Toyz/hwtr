//! The collision world: the track, the objects in it, and which zones hold
//! which objects.

use super::object::{CollisionObject, Kind, RefSet};
use super::scp::Scp;
use crate::car::Car;
use crate::math::{Vec3, add, apply_matrix_lv, fx, sub};

/// An object, by its place in [`Collision::objects`].
pub type ObjectId = usize;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Collision {
    pub scp: Scp,
    pub objects: Vec<CollisionObject>,
    /// Each zone's objects.
    pub members: Vec<RefSet<ObjectId>>,
    /// The game's object lists (0x800d265c to 0x800d2670), newest first:
    /// every object; those that move; players' cars; all cars; those that
    /// hit walls; computer cars.
    pub all: RefSet<ObjectId>,
    pub moving: RefSet<ObjectId>,
    pub players: RefSet<ObjectId>,
    pub cars: RefSet<ObjectId>,
    pub walls: RefSet<ObjectId>,
    pub computers: RefSet<ObjectId>,
    /// Counts collision steps (0x800d2654).
    pub step: u32,
}

/// Bit 4 of an object's flags: its lead point changed zone.
pub const ZONE_CHANGED: u32 = 16;

impl Collision {
    /// 0x8004e47c: every object's points moved with its body, and the
    /// bodies' sleep: a car with all its wheels down, slower than 0x17fff
    /// in/s, turning slower than 10° a second and with no pedal pressed for
    /// 16 steps falls asleep; other bodies the same at 0x5ffff in/s and 90°
    /// a second for 40.
    pub fn update_points(&mut self, cars: &mut [Car]) {
        let order: Vec<ObjectId> = self.all.iter().collect();
        for id in order {
            let obj = &mut self.objects[id];
            match obj.kind.byte() {
                0 | 5 => continue,
                2 => {
                    // A fixed object: its points from its own pose.
                    place(obj);
                    continue;
                }
                _ => {}
            }
            let Some(slot) = obj.car else { continue };
            let car = &mut cars[slot as usize];
            let body = &mut car.body;
            if body.asleep != 0 {
                continue;
            }
            obj.centre = add(body.pos, body.centre);
            obj.rot = body.rot;
            place(obj);
            let ten_degrees = fx(0xa000, 0x3244) / 180;
            let still = if obj.kind == Kind::PlayerCar {
                if car.grounded != car.wheels.len() as u8 {
                    continue;
                }
                body.speed <= 0x1_7fff && body.spin_rate < ten_degrees && car.accel == 0 && car.brake == 0
            } else {
                body.speed <= 0x5_ffff && body.spin_rate < fx(0x5_a000, 0x3244) / 180
            };
            if !still {
                body.sleep_count = 0;
                continue;
            }
            body.sleep_count = body.sleep_count.wrapping_add(1);
            let limit = if obj.kind == Kind::PlayerCar { 16 } else { 40 };
            if (body.sleep_count as u32) >= limit {
                body.asleep = 1;
            }
        }
    }

    /// 0x800515e0: follows each moving object's points from zone to zone.
    /// A point that has crossed a portal of its zone (tested only while the
    /// object's centre is within its radius of the portal) moves to the zone
    /// beyond. The object is kept in the member set of every zone its points
    /// are in; a car whose lead point changes zone is flagged so the zone's
    /// effects apply, and a computer car is also kept in its lead zone's
    /// neighbours.
    pub fn track_zones(&mut self, cars: &[Car]) {
        let order: Vec<ObjectId> = self.moving.iter().collect();
        for id in order {
            let obj = &self.objects[id];
            if obj.car.is_some_and(|slot| cars[slot as usize].body.asleep != 0) {
                continue;
            }
            let old = obj.point_zones.clone();
            let mut new = old.clone();
            for zone_id in obj.zones.iter() {
                let zone = self.scp.zones[zone_id as usize];
                let origin = zone.origin();
                let centre = sub(obj.centre, origin);
                for plane in self.scp.planes_of(&zone).iter().filter(|p| p.is_portal()) {
                    let d = plane.distance(centre);
                    if obj.points.len() == 1 {
                        if d < 0 {
                            new[0] = plane.target;
                        }
                        continue;
                    }
                    if d >= obj.radius {
                        continue;
                    }
                    for (k, p) in obj.points.iter().enumerate() {
                        if old[k] == zone_id && plane.distance(sub(*p, origin)) < 0 {
                            new[k] = plane.target;
                        }
                    }
                }
            }
            for k in 0..old.len() {
                if new[k] == old[k] {
                    continue;
                }
                self.members[old[k] as usize].remove(id);
                self.objects[id].zones.remove(old[k]);
                self.objects[id].point_zones[k] = new[k];
                self.members[new[k] as usize].add(id);
                self.objects[id].zones.add(new[k]);
                let kind = self.objects[id].kind;
                if k != 0 || !kind.is_car() {
                    continue;
                }
                self.objects[id].flags |= ZONE_CHANGED;
                if kind == Kind::ComputerCar {
                    for target in self.portals(old[0]) {
                        self.members[target as usize].remove(id);
                    }
                    for target in self.portals(new[0]) {
                        self.members[target as usize].add(id);
                    }
                }
            }
        }
    }

    /// The zones `zone`'s portals lead to.
    fn portals(&self, zone: u16) -> Vec<u16> {
        let z = self.scp.zones[zone as usize];
        self.scp.planes_of(&z).iter().filter(|p| p.is_portal()).map(|p| p.target).collect()
    }
}

/// An object's world points from its local ones, its rotation and centre.
fn place(obj: &mut CollisionObject) {
    let (rot, centre) = (obj.rot, obj.centre);
    obj.points = obj.local.iter().map(|&p| add(apply_matrix_lv(&rot, p), centre)).collect::<Vec<Vec3>>();
}

/// Where the original keeps the world's lists, and the codec.
pub mod layout {
    use super::{Collision, ObjectId};
    use crate::collision::object::layout::read_list;
    use crate::collision::object::{CollisionObject, RefSet};
    use crate::collision::scp::Scp;
    use crate::ram::Ram;

    pub const SCP: u32 = 0x800d_2658;
    pub const ALL: u32 = 0x800d_265c;
    pub const MOVING: u32 = 0x800d_2660;
    pub const PLAYERS: u32 = 0x800d_2664;
    pub const CARS: u32 = 0x800d_2668;
    pub const WALLS: u32 = 0x800d_266c;
    pub const COMPUTERS: u32 = 0x800d_2670;
    pub const STEP: u32 = 0x800d_2654;

    /// The SCP's bytes as loaded, and where.
    pub fn scp_bytes(ram: &Ram) -> (u32, Vec<u8>) {
        let at = ram.i32(SCP) as u32;
        let sizes = [20, 24, 12, 20, 12, 32];
        let len = 240 + (0..6).map(|k| ram.i32(at + 4 * k) as usize * sizes[k as usize]).sum::<usize>();
        (at, (0..len as u32).map(|k| ram.u8(at + k)).collect())
    }

    impl Collision {
        /// The world as the original has it, with the objects' addresses.
        pub fn read(ram: &Ram) -> (Collision, Vec<u32>) {
            let (scp_at, bytes) = scp_bytes(ram);
            let scp = Scp::parse(&bytes).expect("the loaded SCP parses");
            let all = read_list(ram, ALL);
            let addresses: Vec<u32> = all.iter().collect();
            let id = |a: u32| -> ObjectId { addresses.iter().position(|&x| x == a).expect("object in the list") };
            let objects = addresses.iter().map(|&o| CollisionObject::read(ram, o)).collect();
            let set =
                |head: u32| RefSet { entries: read_list(ram, head).entries.iter().map(|&(a, n)| (id(a), n)).collect() };
            let members = (0..scp.zones.len() as u32).map(|z| set(scp_at + 240 + 20 * z + 16)).collect();
            let world = Collision {
                scp,
                objects,
                members,
                all: set(ALL),
                moving: set(MOVING),
                players: set(PLAYERS),
                cars: set(CARS),
                walls: set(WALLS),
                computers: set(COMPUTERS),
                step: ram.i32(STEP) as u32,
            };
            (world, addresses)
        }

        /// Writes the objects' own fields back (the lists are compared, not
        /// written).
        pub fn write(&self, ram: &mut Ram, addresses: &[u32]) {
            for (obj, &at) in self.objects.iter().zip(addresses) {
                obj.write(ram, at);
            }
            ram.set_i32(STEP, self.step as i32);
        }
    }
}

impl Collision {
    /// The part of `collision_update` (0x8004de6c) ported so far: the
    /// points, the zones, the players' wheels, and the effects of the zones
    /// cars' lead points entered.
    pub fn update(&mut self, t: &crate::math::Tables, cars: &mut [Car]) {
        self.step = self.step.wrapping_add(1);
        self.update_points(cars);
        self.track_zones(cars);
        self.wheels(t, cars);
        for id in self.cars.iter().collect::<Vec<_>>() {
            if self.objects[id].flags & ZONE_CHANGED == 0 {
                continue;
            }
            if let Some(slot) = self.objects[id].car {
                let zone = self.objects[id].point_zones[0];
                self.zone_effects(&mut cars[slot as usize], zone, false);
            }
            self.objects[id].flags &= !ZONE_CHANGED;
        }
    }
}
