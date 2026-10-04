//! A player's wreck throws its wheels (0x8007c9b0): each becomes a body of
//! its own, a collision object of kind 6, flung from its mount at the car's
//! speed and spin plus a random push out and up, rolling as it was. Eight
//! at most fly at once (0x801323f4, 680 bytes each); they are stepped after
//! the collision (0x8007c894), drawn as the car's own wheels where they are
//! (0x8007c8fc), and taken away when the car is put back on the road
//! (0x8007da78).

use crate::body::Body;
use crate::car::Car;
use crate::math::{Tables, Vec3, add, column, div_fx, fx, mul_16_64, mul_64_16, sub, transpose};
use crate::rand::Rand;

/// How many wheels may fly at once.
pub const SLOTS: usize = 8;
/// A flying wheel's mass.
pub const MASS: i32 = 2122;
/// The step a flying wheel's body takes, seconds (4.12): 25 ms.
pub const DT: i32 = 102;

/// One flying wheel: the car and wheel it came from, its body, its box
/// (half its width, half its diameter twice) and reach, and its collision
/// object once made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlyingWheel {
    pub car: u8,
    pub wheel: u8,
    pub body: Body,
    pub half: Vec3,
    pub radius: i32,
    pub object: Option<usize>,
}

fn mph() -> i32 {
    div_fx(176 << 12, 10 << 12)
}

/// 0x8007c9b0 for wheel `k` of `car`: its body (mass 2122, a box of half
/// its width by twice its diameter each way) at its mount turned with the
/// car, moving at the car's speed and its spin at the wheel, plus a push
/// along the wheel's direction from the car of up to 19 mph and up of up
/// to 29 mph (two random draws), and rolling about its axle at the wheel's
/// spin.
pub fn throw(t: &Tables, car: &Car, k: usize, rand: &mut Rand) -> FlyingWheel {
    let w = &car.wheels[k];
    let size = [div_fx(w.width, 0x2000), fx(w.diameter, 0x2000), fx(w.diameter, 0x2000)];
    let mut body = Body::new(MASS, size, [0; 3]);
    let rot = car.body.rot;
    body.rot = rot;
    let local = sub(w.mount, car.origin);
    let turned: Vec3 =
        std::array::from_fn(|i| (0..3).fold(0i32, |s, j| s.wrapping_add(fx(rot[i][j] as i32, local[j]))));
    body.pos = add(turned, car.body.pos);
    let r = sub(w.world, car.body.pos);
    let len = t.length(r);
    let dir = r.map(|c| div_fx(c, len));
    let out = fx((rand.below(20) as i32) << 12, mph());
    let mut push = dir.map(|c| fx(c, out));
    push[2] = fx((rand.below(30) as i32) << 12, mph());
    let s = car.body.spin;
    let at_wheel = [
        fx(s[1], r[2]).wrapping_sub(fx(r[1], s[2])),
        fx(r[0], s[2]).wrapping_sub(fx(s[0], r[2])),
        fx(s[0], r[1]).wrapping_sub(fx(r[0], s[1])),
    ];
    body.vel = add(add(car.body.vel, at_wheel), push);
    body.momentum = body.vel.map(|c| fx(c, body.mass));
    body.spin = column(&rot, 0).map(|c| fx(c, w.spin_rate));
    let inertia = mul_64_16(&mul_16_64(&rot, &body.inertia), &transpose(&rot));
    let spin = body.spin;
    body.ang_momentum =
        inertia.map(|row| (0..3).fold(0i64, |s, j| s.wrapping_add(row[j].wrapping_mul(spin[j] as i64) >> 12)));
    FlyingWheel {
        car: car.slot,
        wheel: k as u8,
        body,
        half: [fx(w.width, 0x800), fx(w.diameter, 0x800), fx(w.diameter, 0x800)],
        radius: fx(w.diameter, 0x800),
        object: None,
    }
}

/// 0x8007c9b0: a player's car's wheels thrown, in order, each into the
/// first free slot of `table`; none once the slots run out.
pub fn throw_all(t: &Tables, car: &Car, table: &mut [Option<FlyingWheel>; SLOTS], rand: &mut Rand) {
    for k in 0..car.wheels.len() {
        let Some(slot) = table.iter().position(Option::is_none) else { return };
        table[slot] = Some(throw(t, car, k, rand));
    }
}

/// 0x8007c894: each flying wheel's body stepped 25 ms and its rotation
/// squared up (0x80025be4).
pub fn step(t: &Tables, table: &mut [Option<FlyingWheel>; SLOTS]) {
    for f in table.iter_mut().flatten() {
        f.body.integrate(t, DT);
        f.body.rot = t.orthonormalize(&f.body.rot);
    }
}
