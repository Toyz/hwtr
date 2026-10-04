//! Making collision objects, and what zones do to the cars in them.

use super::object::{CollisionObject, Kind, RefSet};
use super::scp::Scp;
use super::world::{Collision, ObjectId};
use crate::car::Car;
use crate::math::{Tables, add, apply_matrix_lv, div, fx, sub};

impl Collision {
    /// An empty world on the track `scp`.
    pub fn new(scp: Scp) -> Collision {
        let members = vec![RefSet::default(); scp.zones.len()];
        Collision { scp, members, ..Collision::default() }
    }

    /// 0x8004c478 with 0x8004c714 and 0x8004d798: a car's collision object.
    /// Its points are its wheels' mounts and the eight corners of its box,
    /// each in the zone it starts in; the object goes into those zones'
    /// member sets and the lists for its kind, and the zones' effects apply
    /// to the car.
    pub fn add_car(&mut self, t: &Tables, car: &mut Car) -> ObjectId {
        let kind = if car.state == 2 { Kind::PlayerCar } else { Kind::ComputerCar };
        let half = [car.width, car.length, car.height].map(|c| fx(c, 0x800));
        let wheels = car.wheels.iter().map(|w| sub(w.mount, car.origin));
        let corners =
            [(1, 1, 1), (1, 1, -1), (1, -1, 1), (1, -1, -1), (-1, 1, 1), (-1, 1, -1), (-1, -1, 1), (-1, -1, -1)]
                .map(|(x, y, z): (i32, i32, i32)| [half[0] * x, half[1] * y, half[2] * z]);
        let local: Vec<_> = wheels.chain(corners).collect();
        let centre = add(car.body.pos, car.body.centre);
        let point_zones =
            local.iter().map(|&p| self.scp.zone_at(add(apply_matrix_lv(&car.body.rot, p), centre))).collect();
        let margin = half.map(|c| c.wrapping_add(0x1000));
        let obj = CollisionObject {
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
