//! Keeping bodies' points inside the track (0x80054964), and the contacts
//! that leaves for the impulses.

use super::object::Kind;
use super::world::{Collision, ObjectId, Step};
use crate::car::Car;
use crate::car::righting::Righting;
use crate::math::{Tables, Vec3, add, cross, div_fx, dot, fx, sub};

/// A point of a body pressing into a surface, moving into it: the impulse
/// stage pushes the body back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Contact {
    pub object: ObjectId,
    /// The point, world space.
    pub point: Vec3,
    /// The surface's normal, out of it.
    pub normal: Vec3,
    /// The surface: a plane's kind, or 2 for a road's floor and 1 for its
    /// other sides. Indexes the restitution table.
    pub surface: u8,
}

/// At most this many contacts a step.
pub const MAX_CONTACTS: usize = 16;

/// A point deeper than this has gone through: the car is flagged to be put
/// back on the road, anything else stops.
const THROUGH: i32 = 0x3_2000;

/// Car flags this stage sets.
pub const HIT_KIND_3: i32 = 0x2000;
pub const HIT_KIND_4: i32 = 0x1000;
pub const THROUGH_WALL: i32 = 0x800;

/// A contact's friction: the surface table's percentage (0x800beac0), four
/// times that for a computer car still racing.
pub fn contact_friction(t: &Tables, surface: u8, computer: bool) -> i32 {
    let percent = t.surface_friction.get(surface as usize).copied().unwrap_or(0) as i32;
    let friction = div_fx(percent << 12, 100 << 12);
    if computer { fx(friction, 0x4000) } else { friction }
}

/// The share of a point's inward speed a contact takes away.
const BOUNCE: i32 = 0x800;

impl Collision {
    /// The contacts' part of `collision_update` (0x8004de6c). For a
    /// player's car not yet wrecked: at the step's first contact the contact
    /// timers run, then each contact may wreck it (hard enough, 0x8007db14)
    /// and, against the floor, works to right it (0x80046ac0), and the race
    /// clock is noted. Then each pushes its body back with an impulse,
    /// sliding against the surface's friction ([`contact_friction`]); the
    /// object remembers the step and the point. The hit's sounds, sparks and
    /// rumble are not yet ported.
    pub fn contact_impulses(&mut self, t: &Tables, cars: &mut [Car], step: &mut Step) {
        for c in self.contacts.clone() {
            let obj = &mut self.objects[c.object];
            let Some(slot) = obj.car else { continue };
            let car = &mut cars[slot as usize];
            let first = obj.stamp != self.step;
            if first {
                tracing::trace!("contact sound for car {slot}: not yet ported");
            }
            let was_wrecked = car.wrecked;
            if obj.kind == Kind::PlayerCar && !was_wrecked {
                if first {
                    car.contact_timers(step.clock);
                }
                if car.hard_impact(c.normal) {
                    car.wreck(false, step.rand);
                }
                if car.ground.floor.found
                    && dot(c.normal, car.ground.floor.normal) > 3547
                    && car.right_itself(step.tuning, step.rand) == Righting::Wreck
                {
                    car.wreck(false, step.rand);
                }
                car.contact_time = step.time;
            }
            obj.stamp = self.step;
            obj.contact_point = c.point;
            let friction = contact_friction(t, c.surface, obj.kind == Kind::ComputerCar && !was_wrecked);
            car.body.impulse(t, c.point, c.normal, BOUNCE, friction);
        }
    }

    /// 0x80054964: every point of a wall-hitting object (past a player's
    /// wheels, which the wheel stage handles) that has gone through a side
    /// of its zone is pushed back out along the side's normal, the body
    /// moving with it, and logged as a contact if it is moving inward. The
    /// sides are a plane zone's planes (but its portals), first for every
    /// object, then a road zone's four: its floor, walls and roof, between
    /// the edges of its sections where the point is along it.
    pub fn walls(&mut self, t: &Tables, cars: &mut [Car]) {
        for road in [false, true] {
            for id in self.walls.iter().collect::<Vec<_>>() {
                let obj = &self.objects[id];
                let Some(slot) = obj.car else { continue };
                let car = &mut cars[slot as usize];
                if car.body.asleep {
                    continue;
                }
                let player = obj.kind == Kind::PlayerCar;
                let first = if player { car.wheels.len() } else { 0 };
                for zone_id in obj.zones.iter().collect::<Vec<_>>() {
                    let zone = self.scp.zones[zone_id as usize];
                    if zone.is_road() != road {
                        continue;
                    }
                    let obj = &self.objects[id];
                    let origin = zone.origin();
                    let points: Vec<Vec3> = (first..obj.points.len())
                        .filter(|&k| obj.point_zones[k] == zone_id)
                        .map(|k| obj.points[k])
                        .collect();
                    if road {
                        for point in points {
                            for (side, (d, n)) in self.road_sides(t, &zone, sub(point, origin)).into_iter().enumerate()
                            {
                                if d <= 0 {
                                    let surface = if side == 0 { 2 } else { 1 };
                                    self.press(id, car, point, n, d, surface);
                                }
                            }
                        }
                        continue;
                    }
                    let centre = sub(obj.centre, origin);
                    let radius = obj.radius;
                    for plane in self.scp.planes_of(&zone).to_vec() {
                        if plane.is_portal() || plane.distance(centre) >= radius {
                            continue;
                        }
                        for &point in &points {
                            let d = plane.distance(sub(point, origin));
                            if d > 0 {
                                continue;
                            }
                            if player {
                                match plane.kind {
                                    3 => car.flags |= HIT_KIND_3,
                                    4 => car.flags |= HIT_KIND_4,
                                    _ => {}
                                }
                            }
                            self.press(id, car, point, plane.normal(), d, plane.kind);
                        }
                    }
                }
            }
        }
    }

    /// The four sides of a road zone at `p` (relative to the zone's origin):
    /// each its distance and inward normal. A side the point is well inside
    /// of has only a positive measure, not its distance, and no normal.
    /// A road zone's cross-section at `p` (relative to the zone's origin),
    /// between its two sections by how far along `p` is: the left and right
    /// edges, and the points above them.
    pub(crate) fn road_edges(&self, zone: &super::scp::Zone, p: Vec3) -> ([Vec3; 2], [Vec3; 2]) {
        let ends = &self.scp.planes[zone.first_plane as usize..zone.first_plane as usize + 2];
        let (a, b) = (ends[0].distance(p), ends[1].distance(p));
        let along = div_fx(b, a.wrapping_add(b));
        let (near, far) = (&self.scp.sections[zone.param as usize], &self.scp.sections[zone.param as usize + 1]);
        let mix = |x: Vec3, y: Vec3| [0, 1, 2].map(|i| fx(x[i], along).wrapping_add(fx(y[i], 0x1000 - along)));
        let low = [0, 1].map(|e| mix(near.edges[e].pos(), far.edges[e].pos()));
        let high = [0, 1].map(|e| mix(near.edges[e].up(), far.edges[e].up()));
        (low, high)
    }

    fn road_sides(&self, t: &Tables, zone: &super::scp::Zone, p: Vec3) -> [(i32, Vec3); 4] {
        let (low, high) = self.road_edges(zone, p);
        // Floor (through the right edge), left wall, roof, right wall.
        let sides = [(low[1], high[1]), (low[0], low[1]), (high[0], low[0]), (high[1], high[0])];
        sides.map(|(base, toward)| {
            let v = sub(toward, base);
            let w = sub(p, base);
            let measure = dot(v, w);
            if measure > 0 {
                return (measure, [0; 3]);
            }
            let len = t.length(v);
            let n = v.map(|c| div_fx(c, len));
            (dot(n, w), n)
        })
    }

    /// A point of object `id` (of `car`) `d` deep (d <= 0) into a surface
    /// with normal `n`: a contact if it moves inward, then the body pushed
    /// back out, unless it has gone right through.
    fn press(&mut self, id: ObjectId, car: &mut Car, point: Vec3, n: Vec3, d: i32, surface: u8) {
        let body = &mut car.body;
        let r = sub(point, body.pos);
        let vel = add(body.vel, cross(body.spin, r));
        if dot(vel, n) < 0 && self.contacts.len() < MAX_CONTACTS {
            self.contacts.push(Contact { object: id, point, normal: n, surface });
        }
        let depth = d.wrapping_neg();
        if depth > THROUGH {
            if self.objects[id].kind == Kind::PlayerCar {
                car.flags |= THROUGH_WALL;
            } else {
                body.asleep = true;
            }
            return;
        }
        body.pos = add(body.pos, n.map(|c| fx(c, depth)));
    }
}
