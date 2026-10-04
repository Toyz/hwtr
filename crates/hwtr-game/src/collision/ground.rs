//! The ground under each car (0x800536b4).

use super::object::Kind;
use super::world::Collision;
use crate::car::{Car, GroundPlane};
use crate::math::{Tables, Vec3, div_fx, dot, fx, sub};

impl Collision {
    /// 0x800536b4: the planes near each car: the nearest surface, and the
    /// nearest floor (one facing up against gravity). A player's car takes
    /// them from its last grounded wheel with a floor under it; other cars,
    /// and players with none, from the zones its points are in: a road
    /// zone's surface under its centre, a plane zone's planes. A road zone
    /// with flag 0x2000 turns gravity toward its surface (the loops), as
    /// does a wheel's ground where the car's zone holds gravity (car flag
    /// 0x400). The best distances so far start at 0 once and run on across
    /// the cars: a car's first candidate takes its place regardless, but a
    /// player's car whose wheel gave it a nearest surface and no floor
    /// measures its zones' against the last car's best.
    pub fn ground(&self, t: &Tables, cars: &mut [Car]) {
        let mut search = vec![false; cars.len()];
        for id in self.cars.iter() {
            let obj = &self.objects[id];
            let Some(slot) = obj.car else { continue };
            let car = &mut cars[slot as usize];
            if car.body.asleep {
                continue;
            }
            car.ground.floor.found = false;
            car.ground.nearest.found = false;
            search[slot as usize] = true;
            if obj.kind != Kind::PlayerCar {
                continue;
            }
            for k in (0..car.wheels.len()).rev() {
                let wheel = &car.wheels[k];
                if !wheel.on_ground {
                    continue;
                }
                let n = wheel.normal;
                let plane = GroundPlane { found: true, normal: n, d: dot(n, wheel.contact).wrapping_neg() };
                car.ground.nearest = plane;
                if car.flags & 0x400 != 0 || n[2] > 0x800 {
                    car.ground.floor = plane;
                    car.ground.origin = [0; 3];
                    if car.flags & 0x400 != 0 {
                        car.body.gravity_dir = n.map(i32::wrapping_neg);
                    }
                    search[slot as usize] = false;
                    break;
                }
            }
        }
        let (mut nearest_d, mut floor_d) = (0i32, 0i32);
        for id in self.cars.iter() {
            let obj = &self.objects[id];
            let Some(slot) = obj.car else { continue };
            let car = &mut cars[slot as usize];
            if car.body.asleep || !search[slot as usize] {
                continue;
            }
            for zone_id in obj.zones.iter() {
                let zone = self.scp.zones[zone_id as usize];
                let origin = zone.origin();
                let c = sub(obj.centre, origin);
                let mut consider = |car: &mut Car, d: i32, normal: Vec3, plane_d: i32, facing_up: bool| {
                    if !car.ground.nearest.found || d < nearest_d {
                        car.ground.nearest = GroundPlane { found: true, normal, d: plane_d };
                        nearest_d = d;
                    }
                    if (!car.ground.floor.found || d < floor_d) && facing_up {
                        floor_d = d;
                        car.ground.floor = GroundPlane { found: true, normal, d: plane_d };
                        car.ground.origin = origin;
                    }
                };
                if zone.is_road() {
                    let ends = &self.scp.planes[zone.first_plane as usize..zone.first_plane as usize + 2];
                    let (a, b) = (ends[0].distance(c), ends[1].distance(c));
                    let along = div_fx(b, a.wrapping_add(b));
                    let first = zone.param as usize;
                    let (near, far) = (self.scp.sections[first].edges[1], self.scp.sections[first + 1].edges[1]);
                    let mix =
                        |x: Vec3, y: Vec3| [0, 1, 2].map(|i| fx(x[i], along).wrapping_add(fx(y[i], 0x1000 - along)));
                    let p = mix(near.pos(), far.pos());
                    let up = mix(near.up(), far.up());
                    let v = sub(up, p);
                    let len = t.length(v);
                    let n = v.map(|c| div_fx(c, len));
                    let d = dot(n, sub(c, p));
                    let facing_up = dot(n, car.body.gravity_dir).wrapping_neg() > 0x800;
                    consider(car, d, n, dot(p, n).wrapping_neg(), facing_up);
                    if zone.flags & 0x2000 != 0 {
                        car.body.gravity_dir = n.map(i32::wrapping_neg);
                    }
                } else {
                    for plane in self.scp.planes_of(&zone) {
                        if plane.is_portal() || plane.kind == 1 {
                            continue;
                        }
                        let n = plane.normal();
                        consider(car, plane.distance(c), n, (plane.d as i32) << 12, n[2] > 0x800);
                    }
                }
            }
        }
        for id in self.cars.iter() {
            let Some(slot) = self.objects[id].car else { continue };
            let car = &mut cars[slot as usize];
            if car.body.asleep || !car.ground.floor.found || !car.ground.nearest.found {
                continue;
            }
            let floor = car.ground.floor;
            car.ground.nearest.normal = floor.normal;
            car.ground.nearest.d = floor.d.wrapping_sub(dot(floor.normal, car.ground.origin));
        }
    }
}
