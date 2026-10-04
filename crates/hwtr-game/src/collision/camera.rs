//! Keeping a camera inside the track (0x8005d094), and the way round a
//! loop (0x8005e984).

use super::object::Kind;
use super::world::Collision;
use crate::math::{Tables, Vec3, add, div_fx, dot, fx, sub};

/// How far inside the track's sides a camera stays, inches (4.12).
const MARGIN: i32 = 48 << 12;

/// A loop's zone flags: both bits.
const LOOP: u16 = 0x9000;

impl Collision {
    /// 0x8005e984: with car `car`'s lead point in a loop's zone, the way
    /// round the loop there and which way is up, each of length 1. None
    /// outside a loop.
    ///
    /// The way round leans from the first side portal's normal (portals
    /// whose normal is less than half up) to the second's, reversed, by
    /// how far the point is from each. Up is the zone's solid planes'
    /// normals and those of the zone beyond its last floor or ceiling
    /// portal, each weighted by one less its share of the summed distances,
    /// so the nearest counts most. (With no such portal the original reads
    /// a zone at address 0; the port leaves that zone out.)
    pub fn loop_axes(&self, t: &Tables, car: u8) -> Option<(Vec3, Vec3)> {
        let obj = self.objects.iter().find(|o| o.car == Some(car))?;
        let zone = self.scp.zones.get(*obj.point_zones.first()? as usize)?;
        if zone.flags & LOOP != LOOP {
            return None;
        }
        let lead = *obj.points.first()?;
        let d = sub(lead, zone.origin());
        let (mut first, mut second, mut beyond) = (None, None, None);
        for plane in self.scp.planes_of(zone).iter().filter(|p| p.is_portal()) {
            let n = plane.normal();
            if n[2].wrapping_abs() < 2048 {
                if first.is_none() {
                    first = Some((plane.distance(d), n));
                } else if second.is_none() {
                    second = Some((plane.distance(d), n.map(i32::wrapping_neg)));
                }
            } else {
                beyond = self.scp.zones.get(plane.target as usize);
            }
        }
        let (to_first, a) = first.unwrap_or_default();
        let (to_second, b) = second.unwrap_or_default();
        let share = div_fx(to_second, to_first.wrapping_add(to_second));
        let rest = 0x1000 - share;
        let way = [0, 1, 2].map(|k| fx(a[k], share).wrapping_add(fx(b[k], rest)));
        let way = t.normalize(way);
        // Up: the solid planes of both zones, nearest weighing most.
        let zones: Vec<_> = std::iter::once(zone).chain(beyond).map(|z| (z, sub(lead, z.origin()))).collect();
        let solid =
            |z: &super::scp::Zone| self.scp.planes_of(z).iter().filter(|p| !p.is_portal()).copied().collect::<Vec<_>>();
        let total = zones
            .iter()
            .flat_map(|(z, d)| solid(z).into_iter().map(move |p| p.distance(*d)))
            .fold(0i32, i32::wrapping_add);
        let mut up = [0; 3];
        for (z, d) in &zones {
            for p in solid(z) {
                let w = 0x1000 - div_fx(p.distance(*d), total);
                up = add(up, p.normal().map(|c| fx(c, w)));
            }
        }
        Some((way, t.normalize(up)))
    }

    /// The zone `point` is in, found by walking through the portals it is
    /// beyond from the zone `car`'s object is in (a player's car: its third
    /// point's; another: its newest zone), stopping when it would walk
    /// back. None after 100 zones.
    fn zone_of_point(&self, car: u8, point: Vec3) -> Option<u16> {
        let obj = self.objects.iter().find(|o| o.car == Some(car))?;
        let mut zone = if obj.kind == Kind::PlayerCar { *obj.point_zones.get(2)? } else { obj.zones.iter().next()? };
        let mut previous = None;
        for _ in 0..100 {
            let z = self.scp.zones[zone as usize];
            let p = sub(point, z.origin());
            let crossed = self.scp.planes_of(&z).iter().find(|plane| plane.is_portal() && plane.distance(p) < 0);
            let Some(portal) = crossed else { return Some(zone) };
            let back = previous == Some(portal.target);
            previous = Some(zone);
            zone = portal.target;
            if back {
                return Some(zone);
            }
        }
        None
    }

    /// 0x8005d094: moves `point` (near car `car`) to at least [`MARGIN`]
    /// inside the zone it is in: off a road zone's floor, walls and roof,
    /// or a plane zone's planes. Whether it moved.
    pub fn keep_inside(&self, t: &Tables, car: u8, point: &mut Vec3) -> bool {
        let Some(zone) = self.zone_of_point(car, *point) else { return false };
        let z = self.scp.zones[zone as usize];
        let p = sub(*point, z.origin());
        let mut push = [0; 3];
        let mut moved = false;
        let mut away = |from: Vec3, n: Vec3, sign: i32| {
            let d = dot(from, n);
            if d < MARGIN {
                let step = n.map(|c| fx(c, MARGIN.wrapping_sub(d)));
                push = if sign > 0 { add(push, step) } else { sub(push, step) };
                moved = true;
            }
        };
        if z.is_road() {
            let (low, high) = self.road_edges(&z, p);
            let across = t.normalize(sub(low[1], low[0]));
            let up = t.normalize(sub(high[0], low[0]));
            let from_left = sub(p, low[0]);
            let to_right_top = sub(high[1], p);
            away(from_left, across, 1);
            away(from_left, up, 1);
            away(to_right_top, across, -1);
            away(to_right_top, up, -1);
        } else {
            for plane in self.scp.planes_of(&z).iter().filter(|plane| !plane.is_portal()) {
                let d = plane.distance(p);
                if d < MARGIN {
                    push = add(push, plane.normal().map(|c| fx(c, MARGIN.wrapping_sub(d))));
                    moved = true;
                }
            }
        }
        *point = add(*point, push);
        moved
    }
}
