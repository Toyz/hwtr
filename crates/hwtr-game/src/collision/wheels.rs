//! Players' wheels on the ground (0x80051bc0).

use super::scp::Section;
use super::world::Collision;
use crate::car::Car;
use crate::math::{Tables, Vec3, add, column, cross, div_fx, dot, fx, sub};

/// A wheel's deepest touch with the ground this step.
#[derive(Clone, Copy)]
struct Touch {
    depth: i32,
    normal: Vec3,
    surface: u8,
}

impl Collision {
    /// 0x80051bc0: where each player's wheel meets the ground. Every zone
    /// its wheel point is in is tried: a road zone's surface, interpolated
    /// between its cross-sections, and a plane zone's ground planes, each
    /// only where it faces the car's up axis within about 25°. The deepest
    /// wins; a wheel that reaches the ground within its spring's extension
    /// stands on it, with its spring's force (preload plus stiffness times
    /// compression, the compression held to the spring's travel), the
    /// ground's normal, kind and friction, and the contact point a radius
    /// down from the wheel. Asleep cars, and cars out of the race's
    /// collision, are left as they are.
    pub fn wheels(&self, t: &Tables, cars: &mut [Car]) {
        for id in self.players.iter() {
            let obj = &self.objects[id];
            let Some(slot) = obj.car else { continue };
            let car = &mut cars[slot as usize];
            if car.body.asleep != 0 || car.unknown_62c != 0 {
                continue;
            }
            let up = column(&car.body.rot, 2);
            let radius = |car: &Car, k: usize| fx(car.wheels[k].diameter, 0x800);
            let mut touch: Vec<Touch> = car
                .wheels
                .iter()
                .map(|w| Touch {
                    depth: car.extension[w.is_rear() as usize].wrapping_neg(),
                    normal: [0; 3],
                    surface: 0,
                })
                .collect();
            let wheel_points = car.wheels.len().min(obj.points.len());
            // Road zones.
            for zone_id in obj.zones.iter() {
                let zone = self.scp.zones[zone_id as usize];
                if !zone.is_road() {
                    continue;
                }
                let origin = zone.origin();
                let ends = &self.scp.planes[zone.first_plane as usize..zone.first_plane as usize + 2];
                let first = zone.param as usize;
                let (near, far) = (self.scp.sections[first], self.scp.sections[first + 1]);
                for k in 0..wheel_points {
                    if obj.point_zones[k] != zone_id {
                        continue;
                    }
                    let p = sub(obj.points[k], origin);
                    let Some((n, dist)) = road_surface(t, p, ends[0].distance(p), ends[1].distance(p), &near, &far)
                    else {
                        continue;
                    };
                    let facing = dot(up, n);
                    if facing <= div_fx(0x9000, 0xa000) {
                        continue;
                    }
                    let depth = div_fx(dist.wrapping_sub(radius(car, k)), facing).wrapping_neg();
                    if touch[k].depth < depth {
                        touch[k] = Touch { depth, normal: n, surface: zone.surface };
                    }
                }
            }
            // Plane zones' ground.
            for zone_id in obj.zones.iter() {
                let zone = self.scp.zones[zone_id as usize];
                if zone.is_road() {
                    continue;
                }
                let origin = zone.origin();
                let centre = sub(obj.centre, origin);
                for plane in self.scp.planes_of(&zone) {
                    if !plane.is_ground() || plane.distance(centre) >= obj.radius {
                        continue;
                    }
                    let n = plane.normal();
                    for k in 0..wheel_points {
                        if obj.point_zones[k] != zone_id {
                            continue;
                        }
                        let d = plane.distance(sub(obj.points[k], origin));
                        let facing = dot(up, n);
                        if facing <= 0x1000 - div_fx(0x1000, 0xa000) {
                            continue;
                        }
                        let depth = div_fx(d.wrapping_sub(radius(car, k)), facing).wrapping_neg();
                        if touch[k].depth < depth {
                            touch[k] = Touch { depth, normal: n, surface: plane.kind };
                        }
                    }
                }
            }
            // The springs.
            let preload = car.spring_preload;
            for (k, wheel) in car.wheels.iter_mut().enumerate() {
                let rear = wheel.is_rear();
                let extension = car.extension[rear as usize];
                let mut depth = touch[k].depth;
                if extension.wrapping_neg() < depth {
                    depth = depth.min(car.handling.axle(rear).travel);
                    let spring = preload.wrapping_add(fx(depth, car.handling.axle(rear).stiffness));
                    wheel.ground = 1;
                    wheel.surface = touch[k].surface;
                    wheel.normal = touch[k].normal;
                    let friction = t.surface_friction.get(touch[k].surface as usize).copied().unwrap_or(0);
                    wheel.friction = div_fx((friction as i32) << 12, 100 << 12);
                    wheel.spring = spring.max(0);
                    let down = fx(wheel.diameter, -0x800);
                    wheel.contact = add(obj.points[k], touch[k].normal.map(|c| fx(c, down)));
                } else {
                    wheel.ground = 0;
                }
                wheel.compression = depth;
            }
        }
    }
}

/// A road zone's surface under the point `p` (relative to the zone's
/// origin), given its distances `a` and `b` to the zone's two end planes:
/// the cross-section there, between the near and far ones by `b / (a + b)`;
/// the point's place across it; the surface's normal (the cross product of
/// the across and along directions, the across one scaled down a hundred
/// times first) and the point's height above it.
fn road_surface(t: &Tables, p: Vec3, a: i32, b: i32, near: &Section, far: &Section) -> Option<(Vec3, i32)> {
    let along_t = div_fx(b, a.wrapping_add(b));
    let mix = |x: Vec3, y: Vec3, k: i32| [0, 1, 2].map(|i| fx(x[i], k).wrapping_add(fx(y[i], 0x1000 - k)));
    let (p0, p1) = (near.edges[0].pos(), near.edges[1].pos());
    let (p2, p3) = (far.edges[0].pos(), far.edges[1].pos());
    let left = mix(p0, p2, along_t);
    let right = mix(p1, p3, along_t);
    let hundredth = div_fx(0x1000, 100 << 12);
    let across = sub(right, left).map(|c| fx(c, hundredth));
    let from_left = dot(across, sub(p, left));
    let across_t = div_fx(from_left, from_left.wrapping_add(dot(across, sub(right, p))));
    let along = [0, 1, 2].map(|i| {
        fx(p2[i].wrapping_sub(p0[i]), 0x1000 - across_t).wrapping_add(fx(p3[i].wrapping_sub(p1[i]), across_t))
    });
    let n = cross(across, along);
    let len = t.length(n);
    let n = n.map(|c| div_fx(c, len));
    Some((n, dot(n, sub(p, left))))
}
