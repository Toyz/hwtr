//! Making collision objects, and what zones do to the cars in them.

use super::object::{CollisionObject, Kind, RefSet};
use super::scp::Scp;
use super::world::{Collision, ObjectId};
use crate::car::Car;
use crate::math::{Matrix, Tables, Vec3, add, apply_matrix_lv, div, fx, sub};

impl Collision {
    /// An empty world on the track `scp`.
    pub fn new(scp: Scp) -> Collision {
        let members = vec![RefSet::default(); scp.zones.len()];
        Collision { scp, members, ..Collision::default() }
    }

    /// 0x8004c478 with 0x8004c714 (0x8004ccc4 for a computer car) and
    /// 0x8004d798: a car's collision object. A player's points are its
    /// wheels' mounts and the eight corners of its box (a computer car's,
    /// its centre alone),
    /// each in the zone it starts in; the object goes into those zones'
    /// member sets and the lists for its kind, and the zones' effects apply
    /// to the car.
    pub fn add_car(&mut self, t: &Tables, car: &mut Car) -> ObjectId {
        let kind = if car.state == 2 { Kind::PlayerCar } else { Kind::ComputerCar };
        let half = [car.width, car.length, car.height].map(|c| fx(c, 0x800));
        let centre = add(car.body.pos, car.body.centre);
        let (local, margin): (Vec<Vec3>, Vec3) = if kind == Kind::ComputerCar {
            // 0x8004ccc4: a computer car is one point at its centre, its box
            // its own size.
            (vec![[0; 3]], half)
        } else {
            let wheels = car.wheels.iter().map(|w| sub(w.mount, car.origin));
            let corners =
                [(1, 1, 1), (1, 1, -1), (1, -1, 1), (1, -1, -1), (-1, 1, 1), (-1, 1, -1), (-1, -1, 1), (-1, -1, -1)]
                    .map(|(x, y, z): (i32, i32, i32)| [half[0] * x, half[1] * y, half[2] * z]);
            (wheels.chain(corners).collect(), half.map(|c| c.wrapping_add(0x1000)))
        };
        let point_zones =
            local.iter().map(|&p| self.scp.zone_at(add(apply_matrix_lv(&car.body.rot, p), centre))).collect();
        let obj = CollisionObject {
            id: self.objects.len() as u16,
            kind,
            flags: 0,
            centre: [0; 3],
            rot: [[0; 3]; 3],
            half: margin,
            points: vec![[0; 3]; local.len()],
            local,
            point_zones,
            radius: t.length(margin),
            zones: RefSet::default(),
            stamp: 0,
            contact_point: [0; 3],
            car: Some(car.slot),
            paired: 0,
            heft: 0,
            pickup: None,
            volume: None,
        };
        let id = self.objects.len();
        self.objects.push(obj);
        let zones = self.objects[id].point_zones.clone();
        for &z in &zones {
            self.members[z as usize].add(id);
            self.objects[id].zones.add(z);
        }
        if kind == Kind::ComputerCar {
            let lead = self.scp.zones[zones[0] as usize];
            let targets: Vec<u16> =
                self.scp.planes_of(&lead).iter().filter(|p| p.is_portal()).map(|p| p.target).collect();
            for target in targets {
                self.members[target as usize].add(id);
            }
        }
        self.all.add(id);
        self.moving.add(id);
        if kind == Kind::PlayerCar {
            self.players.add(id);
            self.walls.add(id);
        } else {
            self.computers.add(id);
        }
        self.cars.add(id);
        for &z in &zones {
            self.zone_effects(car, z, None);
        }
        id
    }

    /// 0x80067418 with 0x8004cedc and 0x8004d798: pickup `number`'s object,
    /// a box of half `size` each way at `pos` with one point, its centre, in
    /// the zone there; it stays put (not among the moving).
    pub fn add_pickup(&mut self, t: &Tables, number: u16, pos: Vec3, size: i32) -> ObjectId {
        let half = [fx(size, 0x800); 3];
        let zone = self.scp.zone_at(pos);
        let id = self.objects.len();
        self.objects.push(CollisionObject {
            id: id as u16,
            kind: Kind::Other(5),
            flags: 0,
            centre: pos,
            rot: [[0x1000, 0, 0], [0, 0x1000, 0], [0, 0, 0x1000]],
            half,
            local: vec![[0; 3]],
            points: vec![pos],
            point_zones: vec![zone],
            radius: t.length(half),
            zones: RefSet::default(),
            stamp: 0,
            contact_point: [0; 3],
            car: None,
            paired: 0,
            heft: 0,
            pickup: Some(number),
            volume: None,
        });
        self.members[zone as usize].add(id);
        self.objects[id].zones.add(zone);
        self.all.add(id);
        id
    }

    /// 0x8006b2a8 with 0x8004cedc and 0x8004d798: world volume `number`'s
    /// object: kind 2 if it follows its object (volume flag 8), else 0 (no
    /// track has a body of its own, kind 1); knocked over (flag 1 to 2),
    /// lifting a car (2 to 4), wrecking one (64 to 8); a box half `size`
    /// each way at `centre` turned by `rot`, one point at its centre (eight, its corners,
    /// for flag 16), in that point's zone. One that follows its object moves.
    pub fn add_world_object(&mut self, t: &Tables, number: u16, flags: u32, centre: Vec3, rot: Matrix, size: Vec3, heft: u32) -> ObjectId {
        let kind = if flags & 8 != 0 {
            Kind::Other(2)
        } else if flags & 16 != 0 {
            Kind::Other(1)
        } else {
            Kind::Other(0)
        };
        let mut obj_flags = 0;
        if flags & 1 != 0 {
            obj_flags |= 2;
        }
        if flags & 2 != 0 {
            obj_flags |= 4;
        }
        if flags & 64 != 0 {
            obj_flags |= 8;
        }
        let half = size.map(|c| fx(c, 0x800));
        let local: Vec<Vec3> = if flags & 16 != 0 {
            [(1, 1, 1), (1, 1, -1), (1, -1, 1), (1, -1, -1), (-1, 1, 1), (-1, 1, -1), (-1, -1, 1), (-1, -1, -1)]
                .map(|(x, y, z): (i32, i32, i32)| [half[0] * x, half[1] * y, half[2] * z])
                .to_vec()
        } else {
            vec![[0; 3]]
        };
        let points: Vec<Vec3> = local.iter().map(|&p| add(apply_matrix_lv(&rot, p), centre)).collect();
        let point_zones: Vec<u16> = points.iter().map(|&p| self.scp.zone_at(p)).collect();
        let id = self.objects.len();
        self.objects.push(CollisionObject {
            id: id as u16,
            kind,
            flags: obj_flags,
            centre,
            rot,
            half,
            local,
            points,
            point_zones: point_zones.clone(),
            radius: t.length(half),
            zones: RefSet::default(),
            stamp: 0,
            contact_point: [0; 3],
            car: None,
            paired: 0,
            heft,
            pickup: None,
            volume: Some(number),
        });
        for z in point_zones {
            self.members[z as usize].add(id);
            self.objects[id].zones.add(z);
        }
        self.all.add(id);
        if kind != Kind::Other(0) {
            self.moving.add(id);
        }
        id
    }

    /// 0x8005c3a4: what the zone `zone` does to a car entering it: track
    /// flags into the car's, the lap distance, gravity's direction (reset to
    /// straight down unless the zone keeps it) and strength (scaled by the
    /// zone's parameter where it says so). A checkpoint zone counts toward
    /// the car's laps when the car drove in, at race time `driven` (not when
    /// it is put there). Power-ups and special zones are not yet ported.
    pub fn zone_effects(&self, car: &mut Car, zone: u16, driven: Option<u32>) -> Option<crate::laps::LapEvent> {
        if car.wrecked {
            return None;
        }
        let z = self.scp.zones[zone as usize];
        let f = z.flags;
        let bit = |zone_bit: u16, car_bit: i32, flags: &mut i32| {
            if f & zone_bit != 0 { *flags |= car_bit } else { *flags &= !car_bit }
        };
        let event = match driven {
            Some(time) if f & 1 != 0 => car.laps.pass(&self.course, z.param as u8, time),
            _ => None,
        };
        bit(0x10, 1, &mut car.flags_8);
        // The race's flag 0x80 (0x800d2678) also sets bit 1; not yet ported.
        bit(0x8, 2, &mut car.flags_8);
        car.flags &= !0x1c;
        match f & 0x9000 {
            0x9000 => car.flags |= 16,
            0x8000 => car.flags |= 8,
            0x1000 => car.flags |= 4,
            _ => {}
        }
        bit(0x40, 0x20, &mut car.flags);
        bit(0x80, 0x40, &mut car.flags);
        bit(0x4, 0x80, &mut car.flags);
        bit(0x100, 0x100, &mut car.flags);
        bit(0x200, 0x200, &mut car.flags);
        if f & 0x400 != 0 {
            car.flags |= 0x4000;
        } else {
            car.flags &= !0x4000;
            // Short of the next checkpoint, the zone is on the lap ahead:
            // the distance carries on past the lap's length.
            let distance = (z.distance as i32) * 10;
            let ahead = (distance as u32) < self.course.next_start(car.laps.passed_count) as u32;
            car.lap_distance = if ahead { distance.wrapping_add(self.course.lap_length) } else { distance };
        }
        if f & 0x2000 != 0 {
            car.flags |= 0x400;
        } else {
            car.body.gravity_dir = [0, 0, -0x1000];
            car.flags &= !0x400;
        }
        car.body.gravity = if f & 0x20 != 0 { fx(0x18_2000, div((z.param as i32) << 12, 256).0) } else { 0x18_2000 };
        if f & 0x4000 != 0 && car.state == 2 {
            tracing::trace!("zone {zone}: special zone (not yet ported)");
        }
        if f & 2 != 0 {
            tracing::trace!("zone {zone}: power-up {} (not yet ported)", z.param);
        }
        event
    }
}

impl Collision {
    /// 0x8004c5f4: puts all of car `slot`'s object's points in `zone`
    /// (leaving their old zones), then applies the zone as at the start.
    pub fn move_to_zone(&mut self, slot: u8, zone: u16, car: &mut Car) {
        let Some(id) = self.objects.iter().position(|o| o.car == Some(slot)) else { return };
        for k in 0..self.objects[id].point_zones.len() {
            let old = self.objects[id].point_zones[k];
            self.members[old as usize].remove(id);
            self.objects[id].zones.remove(old);
        }
        for k in 0..self.objects[id].point_zones.len() {
            self.objects[id].point_zones[k] = zone;
            self.members[zone as usize].add(id);
            self.objects[id].zones.add(zone);
        }
        self.zone_effects(car, zone, None);
    }
}
