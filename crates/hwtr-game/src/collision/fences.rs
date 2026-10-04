//! The zones' fences against the players' cars (0x8005a4cc, 0x8005a548):
//! a car's box overlapping a fence is pushed straight out, and while it
//! moves into the fence the two ends of the fence inside the box become
//! contacts for the impulse stage.

use super::scp::Fence;
use super::walls::{Contact, MAX_CONTACTS};
use super::world::{Collision, ObjectId};
use crate::car::Car;
use crate::math::{Matrix, Vec3, div_fx, fx, sub};

/// `v` in the axes of `m` (its columns): each term rounded as the game does.
fn into(m: &Matrix, v: Vec3) -> Vec3 {
    let c = |j: usize| [m[0][j], m[1][j], m[2][j]].map(|e| e as i32);
    [0, 1, 2].map(|j| {
        let a = c(j);
        fx(a[0], v[0]).wrapping_add(fx(a[1], v[1])).wrapping_add(fx(a[2], v[2]))
    })
}

/// `v` out of the axes of `m`, back into the world.
fn out_of(m: &Matrix, v: Vec3) -> Vec3 {
    m.map(|row| fx(row[0] as i32, v[0]).wrapping_add(fx(row[1] as i32, v[1])).wrapping_add(fx(row[2] as i32, v[2])))
}

/// 0x8005bcec: whether a box (`centre`, axes `rot`, half sizes `half`) and
/// a fence are apart along one of seven axes: the box's three, the fence's,
/// and the three across both (a margin of 4 on the fence's components).
/// Each axis passed leaves its overlap in `depths` (the objects' test's
/// scratch, at the slots 0x8005bcec writes).
fn apart(centre: Vec3, rot: &Matrix, half: Vec3, f: &Fence, depths: &mut [i32; 16]) -> bool {
    let along = into(rot, f.along);
    let gap = into(rot, sub(f.centre, centre));
    let [ax, ay, az] = along;
    let [gx, gy, gz] = gap;
    let [hx, hy, hz] = half;
    let l = f.half;
    let [wx, wy, wz] = along.map(|c| c.wrapping_abs().wrapping_add(4));
    let tests = [
        (gx, hx.wrapping_add(fx(l, wx))),
        (
            fx(gx, ax).wrapping_add(fx(gy, ay).wrapping_add(fx(gz, az))),
            l.wrapping_add(fx(hx, wx).wrapping_add(fx(hy, wy).wrapping_add(fx(hz, wz)))),
        ),
        (gy, hy.wrapping_add(fx(l, wy))),
        (gz, hz.wrapping_add(fx(l, wz))),
        (fx(gz, ay).wrapping_sub(fx(gy, az)), fx(hy, wz).wrapping_add(fx(hz, wy))),
        (fx(gx, az).wrapping_sub(fx(gz, ax)), fx(hx, wz).wrapping_add(fx(hz, wx))),
        (fx(gy, ax).wrapping_sub(fx(gx, ay)), fx(hx, wy).wrapping_add(fx(hy, wx))),
    ];
    const SLOTS: [usize; 7] = [1, 2, 3, 5, 7, 10, 13];
    for (&(d, r), slot) in tests.iter().zip(SLOTS) {
        let gap = d.wrapping_abs().wrapping_sub(r);
        if gap > 0 {
            return true;
        }
        depths[slot] = gap;
    }
    false
}

impl Collision {
    /// 0x8005a4cc: each player's car, awake and across more than one zone,
    /// against the fences of the zones it is in.
    pub fn fences(&mut self, cars: &mut [Car]) {
        for id in self.players.iter().collect::<Vec<_>>() {
            let Some(slot) = self.objects[id].car else { continue };
            let Some(car) = cars.get_mut(slot as usize) else { continue };
            if car.body.asleep || self.objects[id].zones.entries.len() < 2 {
                continue;
            }
            self.fence_car(id, car);
        }
    }

    /// 0x8005a548: car object `id` against its zones' fences. The fence is
    /// cut to the box; the box moves out along its axis of least overlap;
    /// moving into the fence's normal, the cut ends are contacts.
    fn fence_car(&mut self, id: ObjectId, car: &mut Car) {
        let (centre, rot, half) = {
            let o = &self.objects[id];
            (o.centre, o.rot, o.half)
        };
        let zones: Vec<u16> = self.objects[id].zones.iter().collect();
        for zone in zones {
            let z = self.scp.zones[zone as usize];
            for k in 0..z.fence_count as usize {
                let Some(&f) = self.scp.fences.get(z.first_fence as usize + k) else { continue };
                let mut depths = self.depths;
                let separate = apart(centre, &rot, half, &f, &mut depths);
                self.depths = depths;
                if separate {
                    continue;
                }
                let p = into(&rot, sub(f.centre, centre));
                let d = into(&rot, f.along);
                let reach = d.map(|c| fx(c, f.half));
                let mut a = [0, 1, 2].map(|i| p[i].wrapping_add(reach[i]));
                let mut b = [0, 1, 2].map(|i| p[i].wrapping_sub(reach[i]));
                // Cut the fence to each face of the box, replacing the end
                // that was outside.
                for axis in 0..3 {
                    let h = half[axis];
                    for (bound, inside) in
                        [(h, (|v: i32, h: i32| v < h) as fn(i32, i32) -> bool), (h.wrapping_neg(), |v, h| h < v)]
                    {
                        let a_in = inside(a[axis], bound);
                        if a_in == inside(b[axis], bound) {
                            continue;
                        }
                        let t = div_fx(bound.wrapping_sub(p[axis]), d[axis]);
                        let q = [0, 1, 2].map(|i| p[i].wrapping_add(fx(d[i], t)));
                        if a_in {
                            b = q;
                        } else {
                            a = q;
                        }
                    }
                }
                // How far in from each pair of faces.
                let depth = |axis: usize| {
                    let (lo, hi) = (
                        if b[axis] < a[axis] { b[axis] } else { a[axis] },
                        if a[axis] < b[axis] { b[axis] } else { a[axis] },
                    );
                    let h = half[axis];
                    let (near, far) = (h.wrapping_sub(lo), hi.wrapping_add(h));
                    (if far < near { far } else { near }, near, far)
                };
                let (dx, dy, dz) = (depth(0), depth(1), depth(2));
                if dx.0 <= 0 || dy.0 <= 0 || dz.0 <= 0 {
                    continue;
                }
                let (axis, (_, near, far)) = if dx.0 < dy.0 && dx.0 < dz.0 {
                    (0, dx)
                } else if dy.0 < dz.0 {
                    (1, dy)
                } else {
                    (2, dz)
                };
                let push = if near < far { near.wrapping_neg() } else { far };
                let body = &mut car.body;
                let way = [0, 1, 2].map(|i| fx(body.rot[i][axis] as i32, push));
                for ((pos, centre), way) in body.pos.iter_mut().zip(body.centre).zip(way) {
                    *pos = pos.wrapping_add(centre).wrapping_add(way).wrapping_sub(centre);
                }
                let n = f.normal;
                let into_it =
                    fx(n[0], body.vel[0]).wrapping_add(fx(n[1], body.vel[1])).wrapping_add(fx(n[2], body.vel[2]));
                if into_it >= 0 {
                    continue;
                }
                for end in [a, b] {
                    if self.contacts.len() >= MAX_CONTACTS {
                        break;
                    }
                    let point = out_of(&rot, end);
                    let point = [0, 1, 2].map(|i| point[i].wrapping_add(centre[i]));
                    self.contacts.push(Contact { object: id, point, normal: n, surface: 1 });
                }
            }
        }
    }
}
