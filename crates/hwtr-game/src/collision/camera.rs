//! Keeping a camera inside the track (0x8005d094).

use super::object::Kind;
use super::world::Collision;
use crate::math::{Tables, Vec3, add, dot, fx, sub};

/// How far inside the track's sides a camera stays, inches (4.12).
const MARGIN: i32 = 48 << 12;

impl Collision {
    /// The zone `point` is in, found by walking through the portals it is
    /// beyond from the zone `car`'s object is in (a player's car: its third
    /// point's; another: its newest zone), stopping when it would walk
    /// back. None after 100 zones.
    fn zone_of_point(&self, car: u8, point: Vec3) -> Option<u16> {
        let obj = self.objects.iter().find(|o| o.car == Some(car))?;
        let mut zone = if obj.kind == Kind::PlayerCar {
            *obj.point_zones.get(2)?
        } else {
            obj.zones.iter().next()?
        };
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
