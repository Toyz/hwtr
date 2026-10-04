//! Objects against objects: cars against cars, and moving objects against
//! the others in their zones (0x8004e938). Boxes close enough are tested for
//! overlap along fifteen axes (0x8004ed4c); overlapping, the pair gets a
//! contact on the face or edge they met by, and both are pushed out
//! (0x8005001c). collision_update then answers each pair with an impulse
//! between the two bodies (0x8006f20c), after a car hit hard enough is
//! wrecked (0x8007e000).

use super::object::Kind;
use super::world::{Collision, ObjectId, body_of, body_of_mut};
use crate::car::Car;
use crate::math::{Matrix, Tables, Vec3, add, cross, div_fx, dot, fx, sub};
use crate::rand::Rand;

/// At most this many pairs a step.
pub const MAX_PAIRS: usize = 16;

/// A contact between two objects: the point, and the normal from `b`
/// toward `a` (the way an impulse pushes `a`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pair {
    pub a: ObjectId,
    pub b: ObjectId,
    pub point: Vec3,
    pub normal: Vec3,
}

/// Which kinds of object meet (0x800bea8c), by kind byte.
const MEETS: [[bool; 7]; 7] = {
    let t = [
        [0, 1, 0, 1, 1, 0, 1],
        [1, 1, 1, 1, 1, 0, 1],
        [0, 1, 0, 1, 1, 0, 1],
        [1, 1, 1, 0, 0, 1, 1],
        [1, 1, 1, 0, 0, 1, 1],
        [0, 0, 0, 1, 1, 0, 0],
        [1, 1, 1, 1, 1, 0, 0],
    ];
    let mut out = [[false; 7]; 7];
    let mut i = 0;
    while i < 7 {
        let mut j = 0;
        while j < 7 {
            out[i][j] = t[i][j] != 0;
            j += 1;
        }
        i += 1;
    }
    out
};

fn col(m: &Matrix, j: usize) -> Vec3 {
    [m[0][j] as i32, m[1][j] as i32, m[2][j] as i32]
}

/// A box: centre, half sizes, axes.
#[derive(Clone, Copy)]
struct Boxed {
    centre: Vec3,
    half: Vec3,
    rot: Matrix,
}

/// 0x8004ed4c: the first of fifteen axes (`a`'s three, `b`'s three, the
/// nine across them) along which the boxes are apart, from 1; 0 when they
/// overlap. Each axis passed leaves its overlap (negative) in `depths`, at
/// its number.
/// [`apart`] for two boxes (centre, half, rotation), from no depths.
pub fn apart_of(a: (Vec3, Vec3, Matrix), b: (Vec3, Vec3, Matrix)) -> (u8, [i32; 16]) {
    let mut depths = [0; 16];
    let axis =
        apart(&Boxed { centre: a.0, half: a.1, rot: a.2 }, &Boxed { centre: b.0, half: b.1, rot: b.2 }, &mut depths);
    (axis, depths)
}

fn apart(a: &Boxed, b: &Boxed, depths: &mut [i32; 16]) -> u8 {
    let ca = [col(&a.rot, 0), col(&a.rot, 1), col(&a.rot, 2)];
    let cb = [col(&b.rot, 0), col(&b.rot, 1), col(&b.rot, 2)];
    // b's axes in a's, each kept in 16 bits.
    let r: [[i32; 3]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| dot(ca[i], cb[j]) as i16 as i32));
    let d = sub(b.centre, a.centre);
    let t = [dot(ca[0], d), dot(ca[1], d), dot(ca[2], d)];
    let w: [[i32; 3]; 3] = std::array::from_fn(|i| std::array::from_fn(|j| r[i][j].wrapping_abs() + 4));
    let (ha, hb) = (a.half, b.half);
    let f = fx;
    let tests: [(i32, i32); 15] = [
        (t[0], ha[0] + f(hb[0], w[0][0]) + f(hb[1], w[0][1]) + f(hb[2], w[0][2])),
        (
            f(t[0], r[0][0]) + f(t[1], r[1][0]) + f(t[2], r[2][0]),
            hb[0] + f(ha[0], w[0][0]) + f(ha[1], w[1][0]) + f(ha[2], w[2][0]),
        ),
        (t[1], ha[1] + f(hb[0], w[1][0]) + f(hb[1], w[1][1]) + f(hb[2], w[1][2])),
        (
            f(t[0], r[0][1]) + f(t[1], r[1][1]) + f(t[2], r[2][1]),
            hb[1] + f(ha[0], w[0][1]) + f(ha[1], w[1][1]) + f(ha[2], w[2][1]),
        ),
        (t[2], ha[2] + f(hb[0], w[2][0]) + f(hb[1], w[2][1]) + f(hb[2], w[2][2])),
        (
            f(t[0], r[0][2]) + f(t[1], r[1][2]) + f(t[2], r[2][2]),
            hb[2] + f(ha[0], w[0][2]) + f(ha[1], w[1][2]) + f(ha[2], w[2][2]),
        ),
        (
            f(t[2], r[1][0]) - f(t[1], r[2][0]),
            f(ha[1], w[2][0]) + f(ha[2], w[1][0]) + f(hb[1], w[0][2]) + f(hb[2], w[0][1]),
        ),
        (
            f(t[2], r[1][1]) - f(t[1], r[2][1]),
            f(ha[1], w[2][1]) + f(ha[2], w[1][1]) + f(hb[0], w[0][2]) + f(hb[2], w[0][0]),
        ),
        (
            f(t[2], r[1][2]) - f(t[1], r[2][2]),
            f(ha[1], w[2][2]) + f(ha[2], w[1][2]) + f(hb[0], w[0][1]) + f(hb[1], w[0][0]),
        ),
        (
            f(t[0], r[2][0]) - f(t[2], r[0][0]),
            f(ha[0], w[2][0]) + f(ha[2], w[0][0]) + f(hb[1], w[1][2]) + f(hb[2], w[1][1]),
        ),
        (
            f(t[0], r[2][1]) - f(t[2], r[0][1]),
            f(ha[0], w[2][1]) + f(ha[2], w[0][1]) + f(hb[0], w[1][2]) + f(hb[2], w[1][0]),
        ),
        (
            f(t[0], r[2][2]) - f(t[2], r[0][2]),
            f(ha[0], w[2][2]) + f(ha[2], w[0][2]) + f(hb[0], w[1][1]) + f(hb[1], w[1][0]),
        ),
        (
            f(t[1], r[0][0]) - f(t[0], r[1][0]),
            f(ha[0], w[1][0]) + f(ha[1], w[0][0]) + f(hb[1], w[2][2]) + f(hb[2], w[2][1]),
        ),
        (
            f(t[1], r[0][1]) - f(t[0], r[1][1]),
            f(ha[0], w[1][1]) + f(ha[1], w[0][1]) + f(hb[0], w[2][2]) + f(hb[2], w[2][0]),
        ),
        (
            f(t[1], r[0][2]) - f(t[0], r[1][2]),
            f(ha[0], w[1][2]) + f(ha[1], w[0][2]) + f(hb[0], w[2][1]) + f(hb[1], w[2][0]),
        ),
    ];
    for (k, (dist, reach)) in tests.into_iter().enumerate() {
        let gap = dist.wrapping_abs().wrapping_sub(reach);
        if gap > 0 {
            return k as u8 + 1;
        }
        depths[k + 1] = gap;
    }
    0
}

impl Collision {
    fn boxed(&self, id: ObjectId) -> Boxed {
        let o = &self.objects[id];
        Boxed { centre: o.centre, half: o.half, rot: o.rot }
    }

    fn body_asleep(&self, id: ObjectId, cars: &[Car]) -> Option<bool> {
        body_of(&self.flying, cars, &self.objects[id]).map(|b| b.asleep)
    }

    /// Near enough to test: on every axis within half as much again as the
    /// two radii.
    fn near(&self, a: ObjectId, b: ObjectId) -> bool {
        let (oa, ob) = (&self.objects[a], &self.objects[b]);
        let d = sub(ob.centre, oa.centre).map(i32::wrapping_abs);
        let r = oa.radius.wrapping_add(ob.radius);
        d[0].max(d[1]).max(d[2]) < r.wrapping_add(fx(r, 2048))
    }

    /// 0x8004e938: this step's pairs, from the cars against each other, then
    /// each moving object against the objects in its zones that it meets.
    pub fn find_pairs(&mut self, t: &Tables, cars: &mut [Car]) {
        let car_list: Vec<ObjectId> = self.cars.iter().collect();
        for (i, &a) in car_list.iter().enumerate() {
            if self.objects[a].flags & 1 != 0 {
                continue;
            }
            for &b in &car_list[i + 1..] {
                if self.objects[b].flags & 1 != 0 || !self.near(a, b) {
                    continue;
                }
                self.test_pair(t, a, b, cars);
            }
        }
        let moving: Vec<ObjectId> = self.moving.iter().collect();
        for a in moving {
            if self.body_asleep(a, cars) == Some(true) || self.objects[a].flags & 1 != 0 {
                continue;
            }
            let zones: Vec<u16> = self.objects[a].zones.iter().collect();
            for z in zones {
                let members: Vec<ObjectId> = self.members[z as usize].iter().collect();
                for b in members {
                    if b == a || self.objects[b].flags & 1 != 0 {
                        continue;
                    }
                    if self.body_asleep(b, cars) != Some(true) && self.objects[b].paired == self.step {
                        continue;
                    }
                    let (ka, kb) = (self.objects[a].kind.byte() as usize, self.objects[b].kind.byte() as usize);
                    if !MEETS.get(ka).and_then(|r| r.get(kb)).copied().unwrap_or(false) || !self.near(a, b) {
                        continue;
                    }
                    if kb == 5 {
                        let mut depths = self.depths;
                        let axis = apart(&self.boxed(a), &self.boxed(b), &mut depths);
                        self.depths = depths;
                        if axis != 0 {
                            self.separations.insert((self.objects[a].id, self.objects[b].id), axis);
                        } else if let (Some(car), Some(pickup)) = (self.objects[a].car, self.objects[b].pickup) {
                            // 0x80067f98, taken after the step.
                            self.pickups_touched.push((car, pickup));
                        }
                        continue;
                    }
                    self.test_pair(t, a, b, cars);
                }
            }
            self.objects[a].paired = self.step;
        }
    }

    fn test_pair(&mut self, t: &Tables, a: ObjectId, b: ObjectId, cars: &mut [Car]) {
        let mut depths = self.depths;
        let axis = apart(&self.boxed(a), &self.boxed(b), &mut depths);
        self.depths = depths;
        if axis != 0 {
            self.separations.insert((self.objects[a].id, self.objects[b].id), axis);
        } else {
            self.make_pair(t, a, b, cars);
        }
    }

    /// 0x8005001c: the contact between overlapping objects `a` and `b`. The
    /// axis they were last apart along (or, never seen apart, apart along a
    /// step ago) gives the normal: a face of either, or an edge of each;
    /// with none, the line between their centres. On a face, the contact is
    /// the other's corner nearest it; else midway between the centres. Both
    /// are pushed out by the overlap along that axis, half each if both
    /// move.
    fn make_pair(&mut self, t: &Tables, a: ObjectId, b: ObjectId, cars: &mut [Car]) {
        let (ia, ib) = (self.objects[a].id, self.objects[b].id);
        let corners = |o: &super::object::CollisionObject| -> Vec<Vec3> {
            let skip = if o.kind == Kind::PlayerCar {
                o.car.and_then(|s| cars.get(s as usize)).map_or(0, |c| c.wheels.len())
            } else {
                0
            };
            o.points.iter().skip(skip).copied().collect()
        };
        let (pa, pb) = (corners(&self.objects[a]), corners(&self.objects[b]));
        let clear = self.objects[a]
            .car
            .and_then(|s| cars.get(s as usize))
            .is_some_and(|c| self.objects[a].kind == Kind::PlayerCar && (c.all_terrain || c.handling.all_terrain));
        let mut axis = self.separations.get(&(ia, ib)).copied().unwrap_or(0);
        if axis == 0
            && let Some(vel) = body_of(&self.flying, cars, &self.objects[a]).map(|b| b.vel)
        {
            let mut was = self.boxed(a);
            was.centre = sub(was.centre, vel);
            let mut depths = self.depths;
            axis = apart(&was, &self.boxed(b), &mut depths);
            self.depths = depths;
            if axis == 0 {
                self.separations.remove(&(ia, ib));
            } else {
                self.separations.insert((ia, ib), axis);
            }
        }
        let (ba, bb) = (self.boxed(a), self.boxed(b));
        let mut n = if axis == 0 {
            sub(ba.centre, bb.centre)
        } else {
            let (ra, rb) = (&ba.rot, &bb.rot);
            let (u, w) = match axis {
                1 => (col(ra, 1), col(ra, 2)),
                2 => (col(rb, 1), col(rb, 2)),
                3 => (col(ra, 2), col(ra, 0)),
                4 => (col(rb, 2), col(rb, 0)),
                5 => (col(ra, 0), col(ra, 1)),
                6 => (col(rb, 0), col(rb, 1)),
                k => (col(ra, ((k - 7) / 3) as usize), col(rb, ((k - 7) % 3) as usize)),
            };
            cross(u, w)
        };
        if t.length(n) < div_fx(4096, 0xa000) {
            return;
        }
        n = t.normalize(n);
        if dot(sub(bb.centre, ba.centre), n) < 0 {
            n = n.map(i32::wrapping_neg);
        }
        // The nearest of `points` to the plane through `at` with normal `n`.
        let nearest = |points: &[Vec3], at: Vec3, n: Vec3| {
            let off = dot(n, at).wrapping_neg();
            let mut best = (0x3e_8000, at);
            for &p in points.iter().take(8) {
                let d = dot(p, n).wrapping_add(off).wrapping_abs();
                if d < best.0 {
                    best = (d, p);
                }
            }
            best.1
        };
        match axis {
            1 | 3 | 5 => {
                let h = ba.half[(axis as usize - 1) / 2];
                let face = add(ba.centre, n.map(|c| fx(c, h)));
                let point = nearest(&pb, face, n);
                self.push_pair(Pair { a: b, b: a, point, normal: n });
            }
            2 | 4 | 6 => {
                let back = n.map(i32::wrapping_neg);
                let h = bb.half[(axis as usize - 1) / 2];
                let face = add(bb.centre, back.map(|c| fx(c, h)));
                let point = nearest(&pa, face, back);
                self.push_pair(Pair { a, b, point, normal: back });
            }
            _ => {
                let point = add(ba.centre, sub(bb.centre, ba.centre).map(|c| fx(c, 2048)));
                self.push_pair(Pair { a: b, b: a, point, normal: n });
            }
        }
        // Pushed apart by the overlap.
        let depth = self.depths.get(axis as usize).copied().unwrap_or(0);
        let mut push = n.map(|c| fx(c, depth));
        let (ka, kb) = (self.objects[a].kind.byte(), self.objects[b].kind.byte());
        let fixed = |k: u8| k == 0 || k == 2;
        if !fixed(ka) && !fixed(kb) {
            push = push.map(|c| fx(c, 2048));
        }
        let (fb, heft) = (self.objects[b].flags, self.objects[b].heft);
        if !fixed(ka)
            && fb & 4 == 0
            && !(fb & 2 != 0 && (heft < 10_000 || clear))
            && let Some(body) = body_of_mut(&mut self.flying, cars, &self.objects[a])
        {
            move_body(body, push);
        }
        let push = push.map(i32::wrapping_neg);
        if !fixed(kb)
            && let Some(body) = body_of_mut(&mut self.flying, cars, &self.objects[b])
        {
            move_body(body, push);
        }
    }

    fn push_pair(&mut self, p: Pair) {
        if self.pairs.len() < MAX_PAIRS {
            self.pairs.push(p);
        }
    }

    /// collision_update's pair loop: each pair's hit checked for a wreck on
    /// either side (0x8007e000), then the impulse between them (0x8006f20c),
    /// steel and rubber cars marked on their objects for it.
    pub fn pair_impulses(&mut self, t: &Tables, tuning: &crate::car::Tuning, cars: &mut [Car], rand: &mut Rand) {
        for k in 0..self.pairs.len() {
            self.pair_impulses_one(t, tuning, cars, rand, k);
        }
    }

    /// Pair `k`'s part of [`Collision::pair_impulses`].
    pub fn pair_impulses_one(
        &mut self,
        t: &Tables,
        tuning: &crate::car::Tuning,
        cars: &mut [Car],
        rand: &mut Rand,
        k: usize,
    ) {
        self.pair_sound(t, cars, rand, k);
        // Each side, a player's car's pad jolted (0x8005fed0), then the
        // crash check (0x8007e000).
        let p = self.pairs[k];
        for (side, other) in [(p.a, p.b), (p.b, p.a)] {
            if self.objects[side].car.is_some() {
                self.pair_jolt(t, side, other, tuning, cars);
                self.crash(t, side, other, cars, rand);
            }
        }
        // A steel car's object hits without moving (32), a rubber car's
        // throws the other off (64); both of a kind cancel out.
        let p = self.pairs[k];
        let mark = |s: Option<u8>| {
            s.and_then(|s| cars.get(s as usize)).map_or(0, |c| (c.steel as u32) << 5 | (c.rubber as u32) << 6)
        };
        let (ma, mb) = (mark(self.objects[p.a].car), mark(self.objects[p.b].car));
        self.objects[p.a].flags |= ma;
        self.objects[p.b].flags |= mb;
        for bit in [32, 64] {
            if self.objects[p.a].flags & bit != 0 && self.objects[p.b].flags & bit != 0 {
                self.objects[p.a].flags &= !bit;
                self.objects[p.b].flags &= !bit;
            }
        }
        self.pair_impulse_only(t, tuning, cars, k);
        for side in [p.a, p.b] {
            if self.objects[side].car.is_some() {
                self.objects[side].flags &= !96;
            }
        }
        // 0x8004e428: two players' cars touching make a snapshot.
        let player =
            |o: usize| self.objects[o].car.and_then(|s| cars.get(s as usize)).is_some_and(|c| c.flags & 1 != 0);
        if player(p.a) && player(p.b) {
            self.players_touched = true;
        }
    }

    /// 0x80035c7c: pair `k` heard on the later car of the two, if either
    /// is one, when the two meet faster than 10 mph; the crash's tone is
    /// drawn there and then (0x80016a18).
    pub fn pair_sound(&mut self, t: &Tables, cars: &[Car], rand: &mut Rand, k: usize) {
        if self.hushed {
            return;
        }
        let p = self.pairs[k];
        let (ca, cb) = (self.objects[p.a].car, self.objects[p.b].car);
        let Some(slot) = ca.max(cb) else { return };
        let vel = |id: ObjectId| body_of(&self.flying, cars, &self.objects[id]).map_or([0; 3], |b| b.vel);
        if let Some(hit) = crash_hit(t, slot, sub(vel(p.a), vel(p.b)), rand) {
            self.hits.push(hit);
        }
    }

    /// 0x8005fed0: a player's car meeting `other` jolts its pad. A prop it
    /// knocks or one that lifts it gives a set jolt (TUNING +0x30, +0x31);
    /// unless it is a heavy prop (weight over 10000) and the car is not
    /// all-terrain, then, as for anything else, the jolt goes by how fast
    /// they meet (as a wall's), at most every twenty frames.
    pub fn pair_jolt(
        &mut self,
        t: &Tables,
        side: ObjectId,
        other: ObjectId,
        tuning: &crate::car::Tuning,
        cars: &[Car],
    ) {
        let Some(slot) = self.objects[side].car else { return };
        let Some(car) = cars.get(slot as usize) else { return };
        if car.flags & 1 == 0 {
            return;
        }
        let o = &self.objects[other];
        let armoured = car.all_terrain || car.handling.all_terrain;
        let heavy = o.flags & 2 != 0 && o.heft > 10000;
        if (!heavy || armoured) && o.flags & 6 != 0 {
            let level = if o.flags & 2 != 0 { tuning.knock_jolt } else { tuning.lift_jolt };
            self.jolts.push((slot, level));
            return;
        }
        let Some(wait) = self.jolt_wait.get_mut(slot as usize) else { return };
        if *wait != 0 {
            return;
        }
        let theirs = body_of(&self.flying, cars, o).map_or([0; 3], |b| b.vel);
        let speed = t.length(sub(car.body.vel, theirs));
        if speed <= 0 {
            return;
        }
        let q = (speed / 2304).min(4096);
        let level = (fx(0xf_f000, q) >> 12).clamp(0, 255) as u8;
        self.jolts.push((slot, level));
        *wait = 20;
    }

    /// Pair `k`'s crash checks, either side.
    pub fn pair_crashes(&mut self, t: &Tables, cars: &mut [Car], rand: &mut Rand, k: usize) {
        let p = self.pairs[k];
        for (side, other) in [(p.a, p.b), (p.b, p.a)] {
            if self.objects[side].car.is_some() {
                self.crash(t, side, other, cars, rand);
            }
        }
    }

    /// Pair `k`'s impulse.
    pub fn pair_impulse_only(&mut self, t: &Tables, tuning: &crate::car::Tuning, cars: &mut [Car], k: usize) -> i32 {
        let p = self.pairs[k];
        self.pair_impulse(t, &p, tuning, cars)
    }

    /// 0x8007e000: the car of object `side`, hit by `other` faster (relative
    /// to it) than its skill allows, is wrecked. A steel car wrecks any car
    /// it hits, a rubber one any computer car; a steel or rubber car is not
    /// wrecked by speed. A prop that falls over (flag 2) or a bump (flag 4)
    /// is knocked (0x8006b958).
    fn crash(&mut self, t: &Tables, side: ObjectId, other: ObjectId, cars: &mut [Car], rand: &mut Rand) {
        let Some(slot) = self.objects[side].car else { return };
        let o = &self.objects[other];
        if o.kind.byte() == 6 {
            return;
        }
        if let Some(oc) = o.car.and_then(|s| cars.get(s as usize))
            && (oc.steel || (oc.rubber && self.objects[side].kind == Kind::ComputerCar))
        {
            if let Some(c) = cars.get_mut(slot as usize) {
                c.wreck_throwing(false, rand, Some((t, &mut self.flying)));
            }
            return;
        }
        if o.flags & 8 != 0 {
            if let Some(c) = cars.get_mut(slot as usize) {
                c.wreck_throwing(false, rand, Some((t, &mut self.flying)));
            }
            return;
        }
        let clear = cars.get(slot as usize).is_some_and(|c| c.all_terrain || c.handling.all_terrain);
        let (o_flags, o_heft) = (o.flags, o.heft);
        let car_vel = cars.get(slot as usize).map_or([0; 3], |c| c.body.vel);
        if o_flags & 2 != 0 {
            // Knocked over, unless heavy (10000 on) and the car not
            // all-terrain: then it stands like a wall.
            if o_heft < 10_000 || clear {
                self.knock(other, car_vel, rand);
                return;
            }
        } else if o_flags & 4 != 0 {
            // A bump: a car not steel and of little skill is lifted by its
            // weight; knocked either way, then the speed counts as for any.
            if let Some(c) = cars.get_mut(slot as usize) {
                let skill = c.handling.skill.clamp(1024, 0x4000);
                if !c.steel && skill < div_fx(0x2_7000, 0xa000) {
                    let lift = div_fx(fx((o_heft as i32) << 12, 0x10_8000), 0x6_4000);
                    c.body.momentum[2] =
                        c.body.momentum[2].wrapping_add(((lift as i64 * c.body.mass as i64) >> 12) as i32);
                }
            }
            self.knock(other, car_vel, rand);
        }
        let o = &self.objects[other];
        let other_vel = o.car.and_then(|s| cars.get(s as usize)).map_or([0; 3], |c| c.body.vel);
        let Some(c) = cars.get_mut(slot as usize) else { return };
        if c.steel || c.rubber {
            return;
        }
        let skill = c.handling.skill.clamp(1024, 0x4000);
        let rel = sub(c.body.vel, other_vel);
        let speed = t.length(rel);
        let mph = div_fx(0xb_0000, 0xa000);
        let base = fx(0x7_8000, mph);
        let (low, span, from, over) = if skill < 4096 {
            (fx(-0x2_8000, mph), fx(0, mph).wrapping_sub(fx(-0x2_8000, mph)), skill - 1024, 3072)
        } else {
            (fx(0, mph), fx(0x8_c000, mph).wrapping_sub(fx(0, mph)), skill - 4096, 0x3000)
        };
        let k = div_fx(span, over);
        if base.wrapping_add(low.wrapping_add(fx(from, k))) < speed {
            c.wreck_throwing(false, rand, Some((t, &mut self.flying)));
        }
    }

    /// 0x8006b958: world object `id` knocked by a car going `vel`: it is out
    /// of the collision (flag 1), and its volume knocked (its object goes
    /// flying, with its sound) for the race to show.
    fn knock(&mut self, id: ObjectId, vel: Vec3, rand: &mut Rand) {
        let o = &mut self.objects[id];
        o.flags |= 1;
        if let Some(v) = o.volume {
            self.knocked.push((v, vel));
            // 0x80020824: the prop's debris, its random numbers drawn now.
            let (flags, quads) = self.volume_fx.get(v as usize).copied().unwrap_or((0, 0));
            self.prop_draws.push(crate::effects::PropDraws::take(rand, v, vel, flags, quads as usize));
        }
    }

    /// 0x8006f20c: the impulse between the pair's two bodies, along the
    /// normal only, from their velocities at the point; how hard they bounce
    /// depends on the kinds (TUNING +42 to +45). An object without a body
    /// counts by its weight if it has one.
    fn pair_impulse(&mut self, _t: &Tables, p: &Pair, tuning: &crate::car::Tuning, cars: &mut [Car]) -> i32 {
        let (oa, ob) = (&self.objects[p.a], &self.objects[p.b]);
        let at_point = |o: &super::object::CollisionObject| -> Vec3 {
            body_of(&self.flying, cars, o).map_or([0; 3], |b| {
                let r = sub(p.point, b.pos);
                add(b.vel, cross(b.spin, r))
            })
        };
        let rel = sub(at_point(oa), at_point(ob));
        let n = p.normal;
        let inward = dot(n, rel);
        if inward > 0 {
            return 0;
        }
        let (ka, kb) = (oa.kind.byte(), ob.kind.byte());
        let index = match (ka, kb) {
            (3, 3) => 42,
            (4, 4) => 43,
            (3, 4) | (4, 3) => 44,
            _ => 45,
        };
        let bounce = div_fx((tuning.pair_bounce[index - 42] as i32) << 12, 0xa000);
        let wanted = fx(bounce.wrapping_neg(), inward);
        let give = |o: &super::object::CollisionObject| -> Option<i32> {
            match body_of(&self.flying, cars, o) {
                Some(b) => Some(b.give(sub(p.point, b.pos), n)),
                None if o.flags & 6 != 0 => (o.heft != 0).then(|| div_fx(0x18_2000, (o.heft as i32) << 12)),
                None => Some(0),
            }
        };
        let (Some(ga), Some(gb)) = (give(oa), give(ob)) else { return 0 };
        let size = div_fx(wanted, ga.wrapping_add(gb));
        let mut on_a = n.map(|c| fx(c, size));
        let mut on_b = on_a;
        let (fa, fb) = (oa.flags, ob.flags);
        if fa & 64 != 0 {
            on_b = on_b.map(|c| fx(c, if kb == 4 { 0xa000 } else { 0x4000 }));
        }
        if fb & 64 != 0 {
            on_a = on_a.map(|c| fx(c, if ka == 4 { 0xa000 } else { 0x4000 }));
        }
        if let Some(b) = body_of_mut(&mut self.flying, cars, &self.objects[p.a]) {
            if fa & 96 == 0 {
                let r = sub(p.point, b.pos);
                b.push(r, on_a);
            }
            b.settle();
        }
        if let Some(b) = body_of_mut(&mut self.flying, cars, &self.objects[p.b]) {
            if fb & 96 == 0 {
                let r = sub(p.point, b.pos);
                b.pull(r, on_b);
            }
            b.settle();
        }
        size
    }
}

/// A body moved by `by` and woken.
fn move_body(b: &mut crate::body::Body, by: Vec3) {
    b.pos = sub(add(add(b.pos, b.centre), by), b.centre);
    b.asleep = false;
}

/// 0x80035c7c's crash for car `slot`, the bodies meeting at `rel`: none
/// at 10 mph or less; else kind 2, 3 or 4 by 20 and 50 mph, its volume,
/// and the tone drawn for it (0x80016a18).
pub fn crash_hit(t: &Tables, slot: u8, rel: Vec3, rand: &mut Rand) -> Option<super::world::Hit> {
    let speed = t.length(rel);
    let mph = div_fx(0xb_0000, 0xa000);
    if speed <= fx(0xa000, mph) {
        return None;
    }
    let (kind, volume, tone) = if speed < fx(0x1_4000, mph) {
        (2, div_fx(speed, fx(0x1_4000, mph)), rand.below(4))
    } else if speed < fx(0x3_2000, mph) {
        (3, div_fx(speed, fx(0x3_2000, mph)), rand.below(2) + 4)
    } else {
        (4, speed / 2304, rand.below(2) + 6)
    };
    Some(super::world::Hit::Crash { slot, kind, volume, tone: tone as u8 })
}
