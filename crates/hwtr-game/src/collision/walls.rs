//! Keeping bodies' points inside the track (0x80054964), and the contacts
//! that leaves for the impulses.

use super::object::Kind;
use super::world::{Collision, ObjectId, Step};
use crate::car::Car;
use crate::car::righting::Righting;
use crate::math::{Tables, Vec3, add, column, cross, div_fx, dot, fx, sub};

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

/// 0x80035a88: a contact's volume from the car's speed: its mph (20 to
/// 100) over 100.
pub fn contact_volume(speed: i32) -> i32 {
    div_fx(fx(speed, 232).clamp(0x1_4000, 0x6_4000), 0x6_4000)
}

/// How much of a point's inward speed a contact gives back: half, and 1.3
/// for a flying wheel (0x8006dc08, kind 6).
const BOUNCE: i32 = 0x800;
const WHEEL_BOUNCE: i32 = div_fx_const(0xd000, 0xa000);

/// [`crate::math::div_fx`] for small constants.
const fn div_fx_const(a: i32, b: i32) -> i32 {
    ((a / b) << 12) + ((a % b) << 12) / b
}

impl Collision {
    /// The contacts' part of `collision_update` (0x8004de6c). For a
    /// player's car not yet wrecked: at the step's first contact the contact
    /// timers run, then each contact may wreck it (hard enough, 0x8007db14)
    /// and, against the floor, works to right it (0x80046ac0), and the race
    /// clock is noted. Then each pushes its body back with an impulse,
    /// sliding against the surface's friction ([`contact_friction`]); the
    /// object remembers the step and the point. The step's first contact is
    /// heard ([`super::world::Hit::Track`]) and a player's throws a spark.
    /// A player's car's first contact of the step jolts its pad
    /// ([`Collision::wall_jolt`]).
    pub fn contact_impulses(&mut self, t: &Tables, cars: &mut [Car], step: &mut Step) {
        for c in self.contacts.clone() {
            let obj = &mut self.objects[c.object];
            // A flying wheel's contact only pushes its body back.
            if obj.car.is_none() {
                if let Some(f) = obj.flying.and_then(|k| self.flying.get_mut(k as usize)?.as_mut()) {
                    obj.stamp = self.step;
                    obj.contact_normal = c.normal;
                    let friction = contact_friction(t, c.surface, false);
                    f.body.impulse(t, c.point, c.normal, WHEEL_BOUNCE, friction);
                }
                continue;
            }
            let Some(slot) = obj.car else { continue };
            let car = &mut cars[slot as usize];
            let first = obj.stamp != self.step;
            let (kind, half) = (obj.kind, obj.half);
            // 0x80035a88: the step's first contact is heard, louder the
            // faster the car goes.
            if first && !self.hushed {
                self.hits.push(super::world::Hit::Track {
                    slot,
                    surface: c.surface,
                    volume: contact_volume(car.body.speed),
                });
            }
            let was_wrecked = car.wrecked;
            if obj.kind == Kind::PlayerCar && !was_wrecked {
                if first {
                    car.contact_timers(step.clock);
                    if car.flags & 1 != 0 {
                        wall_jolt(&mut self.jolts, &mut self.jolt_wait, slot, car, c.normal);
                    }
                }
                if car.hard_impact(c.normal) {
                    car.wreck_throwing(false, step.rand, Some((t, &mut self.flying)));
                }
                if car.ground.floor.found
                    && dot(c.normal, car.ground.floor.normal) > 3547
                    && car.right_itself(step.tuning, step.rand) == Righting::Wreck
                {
                    car.wreck_throwing(false, step.rand, Some((t, &mut self.flying)));
                }
                car.contact_time = step.time;
            }
            obj.stamp = self.step;
            obj.contact_normal = c.normal;
            let friction = contact_friction(t, c.surface, obj.kind == Kind::ComputerCar && !was_wrecked);
            car.body.impulse(t, c.point, c.normal, BOUNCE, friction);
            // 0x8002e9f8: a player's car's first contact of the step throws
            // a spark.
            if first
                && kind == Kind::PlayerCar
                && let Some(s) =
                    crate::effects::contact_spark(step.rand, t, &car.body, half, c.point, c.normal, c.surface)
            {
                self.sparks.push(s);
            }
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
                // A car's body, or a flying wheel's, taken out while its
                // points are pressed.
                let mut flying = obj.flying.and_then(|k| self.flying.get_mut(k as usize)?.take());
                let (body, mut flags, wheels) = match (obj.car, flying.as_mut()) {
                    (Some(slot), _) => {
                        let car = &mut cars[slot as usize];
                        (&mut car.body, Some(&mut car.flags), car.wheels.len())
                    }
                    (None, Some(f)) => (&mut f.body, None, 0),
                    _ => continue,
                };
                if body.asleep {
                    if let (Some(k), Some(f)) = (self.objects[id].flying, flying) {
                        self.flying[k as usize] = Some(f);
                    }
                    continue;
                }
                let player = obj.kind == Kind::PlayerCar;
                let first = if player { wheels } else { 0 };
                // A flying wheel is a ball: each side meets its point
                // nearest it, its reach in from the centre.
                let ball = (obj.kind.byte() == 6).then_some(obj.radius);
                let nearest = |p: Vec3, n: Vec3| match ball {
                    Some(r) => sub(p, n.map(|c| fx(c, r))),
                    None => p,
                };
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
                            let shape = ball.map_or(Shape::Point, Shape::Ball);
                            let sides = self.road_sides(t, &zone, sub(point, origin), &shape);
                            for (side, (d, n)) in sides.into_iter().enumerate() {
                                if d <= 0 {
                                    let surface = if side == 0 { 2 } else { 1 };
                                    let through = flags.as_deref_mut().filter(|_| player);
                                    self.press(id, body, through, nearest(point, n), n, d, surface);
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
                            let point = nearest(point, plane.normal());
                            let d = plane.distance(sub(point, origin));
                            if d > 0 {
                                continue;
                            }
                            if player && let Some(f) = flags.as_deref_mut() {
                                match plane.kind {
                                    3 => *f |= HIT_KIND_3,
                                    4 => *f |= HIT_KIND_4,
                                    _ => {}
                                }
                            }
                            let through = flags.as_deref_mut().filter(|_| player);
                            self.press(id, body, through, point, plane.normal(), d, plane.kind);
                        }
                    }
                }
                if let (Some(k), Some(f)) = (self.objects[id].flying, flying) {
                    self.flying[k as usize] = Some(f);
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

    /// A ball or a box has every side's distance and normal, less its
    /// reach toward the side.
    fn road_sides(&self, t: &Tables, zone: &super::scp::Zone, p: Vec3, shape: &Shape) -> [(i32, Vec3); 4] {
        let (low, high) = self.road_edges(zone, p);
        // Floor (through the right edge), left wall, roof, right wall.
        let sides = [(low[1], high[1]), (low[0], low[1]), (high[0], low[0]), (high[1], high[0])];
        sides.map(|(base, toward)| {
            let v = sub(toward, base);
            let w = sub(p, base);
            let measure = dot(v, w);
            if measure > 0 && matches!(shape, Shape::Point) {
                return (measure, [0; 3]);
            }
            let len = t.length(v);
            let n = v.map(|c| div_fx(c, len));
            (dot(n, w).wrapping_sub(shape.reach(n)), n)
        })
    }

    /// 0x800572f0: each computer car wrecked or past the line, and awake,
    /// against the sides of the zone its point is in, as a box: its half
    /// width and length and its origin's height along its axes. Each side
    /// meets the box's corner deepest toward it, found by turning each
    /// axis against the side's normal; the axes stay turned for the next
    /// side, so a side square to an axis meets the corner the last one
    /// did. A plane zone's planes (but its portals) come first for every
    /// car, then a road zone's four sides, as in [`Collision::walls`]. A
    /// plane of kind 3 or 4 marks the car as a player's does, and one gone
    /// through flags it rather than stopping it.
    pub fn computer_walls(&mut self, t: &Tables, cars: &mut [Car]) {
        for road in [false, true] {
            for id in self.computers.iter().collect::<Vec<_>>() {
                let obj = &self.objects[id];
                let Some(slot) = obj.car else { continue };
                let car = &mut cars[slot as usize];
                if car.body.asleep || !(car.wrecked || car.laps.finished) {
                    continue;
                }
                let Some(&zone_id) = obj.point_zones.first() else { continue };
                let zone = self.scp.zones[zone_id as usize];
                if zone.is_road() != road {
                    continue;
                }
                let reach = [obj.half[0], obj.half[1], car.origin[2]];
                let mut axes: [Vec3; 3] = std::array::from_fn(|k| column(&car.body.rot, k).map(|c| fx(c, reach[k])));
                let (centre, origin) = (obj.centre, zone.origin());
                let corner = |axes: &mut [Vec3; 3], n: Vec3| {
                    for a in axes.iter_mut() {
                        if dot(n, *a) > 0 {
                            *a = a.map(i32::wrapping_neg);
                        }
                    }
                    axes.iter().fold(centre, |p, &a| add(p, a))
                };
                if road {
                    let sides = self.road_sides(t, &zone, sub(centre, origin), &Shape::Box(axes));
                    for (side, (d, n)) in sides.into_iter().enumerate() {
                        if d <= 0 {
                            let point = corner(&mut axes, n);
                            let surface = if side == 0 { 2 } else { 1 };
                            self.press(id, &mut car.body, Some(&mut car.flags), point, n, d, surface);
                        }
                    }
                    continue;
                }
                for plane in self.scp.planes_of(&zone).to_vec() {
                    if plane.is_portal() {
                        continue;
                    }
                    let point = corner(&mut axes, plane.normal());
                    let d = plane.distance(sub(point, origin));
                    if d > 0 {
                        continue;
                    }
                    match plane.kind {
                        3 => car.flags |= HIT_KIND_3,
                        4 => car.flags |= HIT_KIND_4,
                        _ => {}
                    }
                    self.press(id, &mut car.body, Some(&mut car.flags), point, plane.normal(), d, plane.kind);
                }
            }
        }
    }

    /// A point of object `id` (its `body`) `d` deep (d <= 0) into a
    /// surface with normal `n`: a contact if it moves inward, then the body
    /// pushed back out, unless it has gone right through: then the car's
    /// `through` flags are marked, or without them the body stops.
    #[allow(clippy::too_many_arguments)]
    fn press(
        &mut self,
        id: ObjectId,
        body: &mut crate::body::Body,
        through: Option<&mut i32>,
        point: Vec3,
        n: Vec3,
        d: i32,
        surface: u8,
    ) {
        let r = sub(point, body.pos);
        let vel = add(body.vel, cross(body.spin, r));
        if dot(vel, n) < 0 && self.contacts.len() < MAX_CONTACTS {
            self.contacts.push(Contact { object: id, point, normal: n, surface });
        }
        let depth = d.wrapping_neg();
        if depth > THROUGH {
            match through {
                Some(f) => *f |= THROUGH_WALL,
                None => body.asleep = true,
            }
            return;
        }
        body.pos = add(body.pos, n.map(|c| fx(c, depth)));
    }
}

/// What meets a zone's sides: a point, a ball of some reach (a flying
/// wheel), or a box of three half axes (a computer car out of the race).
pub(crate) enum Shape {
    Point,
    Ball(i32),
    Box([Vec3; 3]),
}

impl Shape {
    /// How far the shape reaches toward a side of normal `n`: a box the
    /// sum of its axes' lengths along it.
    fn reach(&self, n: Vec3) -> i32 {
        match self {
            Shape::Point => 0,
            Shape::Ball(r) => *r,
            Shape::Box(axes) => axes.iter().fold(0i32, |s, &a| s.wrapping_add(dot(n, a).wrapping_abs())),
        }
    }
}

/// 0x8005fd4c: a player's car striking a wall jolts its pad by how
/// fast it goes into it (the speed over 2304, times 255, over 4096; at
/// most 255), unless it jolted in the last ten frames.
pub fn wall_jolt(jolts: &mut Vec<(u8, u8)>, waits: &mut [i32; 2], slot: u8, car: &Car, normal: Vec3) {
    let Some(wait) = waits.get_mut(slot as usize) else { return };
    if *wait != 0 {
        return;
    }
    let into = (0..3).fold(0i32, |s, i| s.wrapping_add(fx(car.body.vel[i], normal[i]))).wrapping_neg();
    if into <= 0 {
        return;
    }
    let level = (fx(0xf_f000, into / 2304) >> 12).clamp(0, 255) as u8;
    jolts.push((slot, level));
    *wait = 10;
}
