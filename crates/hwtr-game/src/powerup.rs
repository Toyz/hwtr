//! Power-ups (0x80067418 on): the track's pickups and what they do. Each
//! pickup names a power-up (`<name>.PUP`, 180 bytes) or "Random"; a car
//! driving through one that is out (its collision box, kind 5) takes it,
//! losing any it had, and the pickup comes back ten seconds later
//! (0x80068130). A power-up scales the car's handling and engine by its
//! factors for its duration (0x80065520, undone by 0x80065ecc) and may set
//! flags: strong brakes, all-terrain (4x4), steel and rubber (in the cars'
//! knocks), gyro; a turbo; or, for the unlock pickups, a car the race
//! unlocks (0x80067708).

use crate::car::Car;
use crate::math::{Vec3, div_fx, fx};

/// A power-up's definition (`.PUP`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowerUp {
    /// Its file name ("turbo") and what the HUD calls it ("Turbo!").
    pub name: String,
    pub label: String,
    /// Seconds it lasts (+0x2c).
    pub seconds: i32,
    /// +0x30, kept with each car's power-up.
    pub kind: u8,
    /// +0x34: 1 strong brakes, 2 a callback no power-up uses, 4 a turbo, 8
    /// all-terrain, 16 steel (and skill × 2.5), 32 rubber, 64 gyro; 1 << 24
    /// and 1 << 25 the track's first and second car unlocked.
    pub flags: u32,
    /// Factors, 4.12, from +0x38: the handling's mass, centre of gravity
    /// along and up, brake grip and bias, drag, downforce and its scale
    /// front and rear, the air power's yaw, pitch and roll, the front
    /// axle's stiffness, grip and damping in and out, the rear's, and the
    /// engine's redline, idle and peak torque.
    pub factors: [i32; 24],
    /// A kick at taking it, in the car's axes, times its mass (+0x98).
    pub kick: Vec3,
    /// The steering lock's factor (+0xa8) and what is added to the drive
    /// and steer layout's four bytes (+0xac).
    pub steer_lock: i32,
    pub layout: [u8; 4],
    /// +0xb0: kept as long as the car has it (never runs out).
    pub instant: bool,
}

pub const PUP_SIZE: usize = 180;

impl PowerUp {
    pub fn parse(b: &[u8]) -> Option<PowerUp> {
        if b.len() < PUP_SIZE {
            return None;
        }
        let word = |at: usize| i32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        let text = |at: usize, n: usize| b[at..at + n].iter().take_while(|&&c| c != 0).map(|&c| c as char).collect();
        Some(PowerUp {
            name: text(0, 12),
            label: text(12, 32),
            seconds: word(0x2c),
            kind: b[0x30],
            flags: word(0x34) as u32,
            factors: std::array::from_fn(|k| word(0x38 + 4 * k)),
            kick: [word(0x98), word(0x9c), word(0xa0)],
            steer_lock: word(0xa8),
            layout: [b[0xac], b[0xad], b[0xae], b[0xaf]],
            instant: b[0xb0] != 0,
        })
    }

    /// 0x80067dfc: what the HUD shows of it (0x892): 1 turbo, 2 brakes, 3
    /// handling, 4 gyro, 5 sticky, 6 random, 7 stealth, 8 boing, 9 steel, 10
    /// rubber, 11 4x4, 12 points; 0 anything else.
    pub fn icon(&self) -> u8 {
        icon_of(&self.name)
    }
}

/// 0x80067090: names alike ignoring case.
fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// 0x800672a0, 0x80067dfc: a power-up's number by its name.
pub fn icon_of(name: &str) -> u8 {
    const NAMES: [(&str, u8); 12] = [
        ("turbo", 1),
        ("brakes", 2),
        ("handling", 3),
        ("gyro", 4),
        ("sticky", 5),
        ("random", 6),
        ("stealth", 7),
        ("boing", 8),
        ("steel", 9),
        ("rubber", 10),
        ("points", 12),
        ("4x4", 11),
    ];
    NAMES.iter().find(|(n, _)| same(n, name)).map_or(0, |&(_, k)| k)
}

/// A pickup on the track (0x800d0e9c, 24 bytes): its name, its number on
/// the track, its power-up's number (0: it never comes back), when it was
/// last taken (race time), whether it is out, and where it is.
#[derive(Clone, Debug)]
pub struct Pickup {
    pub name: String,
    pub index: u16,
    pub number: u8,
    pub taken_at: u32,
    pub out: bool,
    pub pos: Vec3,
    /// The world object drawn for it, if any.
    pub object: Option<usize>,
}

/// A power-up a car has (12 bytes on the car's list at 0x8012ff04): which,
/// since when (race time), and whether it never runs out.
#[derive(Clone, Copy, Debug)]
pub struct Held {
    pub power_up: usize,
    pub since: u32,
    pub instant: bool,
}

/// The race's power-ups: the definitions its pickups use, the pickups, each
/// car's, and the cars the race has unlocked (0x800d0eaa to 0x800d0eb0: each
/// player's first and second).
#[derive(Clone, Debug, Default)]
pub struct PowerUps {
    pub defs: Vec<PowerUp>,
    pub pickups: Vec<Pickup>,
    pub held: Vec<Vec<Held>>,
    /// The track's two cars to unlock (0x800d0ea6, 0x800d0ea8).
    pub unlockable: [u8; 2],
    pub unlocked: [[Option<u8>; 2]; 2],
    /// The pickups taken or back since the host last looked, for the
    /// pickups' objects.
    pub changed: bool,
}

/// A pickup comes back after this long, ms.
const BACK_MS: u32 = 10_001;

impl PowerUps {
    /// 0x80067418 (with 0x800671a8): the pickups, each with the power-up
    /// its name gives loaded once ("Random" loads none), for `cars` cars;
    /// `unlockable` the track's two cars (the tables at 0x800c5cd0 and
    /// 0x800c5cdc).
    pub fn new(
        spots: &[(String, Vec3, Option<usize>)],
        defs: Vec<PowerUp>,
        cars: usize,
        unlockable: [u8; 2],
    ) -> PowerUps {
        let pickups = spots
            .iter()
            .enumerate()
            .map(|(k, (name, pos, object))| Pickup {
                name: name.clone(),
                index: k as u16,
                number: icon_of(name),
                taken_at: 0,
                out: true,
                pos: *pos,
                object: *object,
            })
            .collect();
        PowerUps { defs, pickups, held: vec![Vec::new(); cars], unlockable, unlocked: [[None; 2]; 2], changed: true }
    }

    /// 0x80067b64: the power-up a pickup gives: its own, or for "Random" any
    /// loaded but the unlocks, by `pick` (0x80067ac4: one of `n` at random).
    fn power_up_for(&self, pickup: usize, mut pick: impl FnMut(u32) -> u32) -> Option<usize> {
        let name = &self.pickups.get(pickup)?.name;
        let wanted = if same(name, "random") {
            if self.defs.is_empty() {
                return None;
            }
            loop {
                let k = pick(self.defs.len() as u32) as usize;
                let n = &self.defs[k].name;
                if !same(n, "uncar1") && !same(n, "uncar2") {
                    break n.clone();
                }
            }
        } else {
            name.clone()
        };
        self.defs.iter().position(|d| same(&d.name, &wanted))
    }

    /// 0x80067f98: car `car` drives through pickup `pickup` at race time
    /// `now`: if it is out, the car loses its power-ups and takes the
    /// pickup's (applied unless it never runs out), the HUD shows it (not a
    /// turbo), a player hears it (sound 24, true here), and the pickup goes.
    pub fn take(&mut self, car: &mut Car, pickup: usize, now: u32, pick: impl FnMut(u32) -> u32) -> Option<bool> {
        if !self.pickups.get(pickup)?.out {
            return None;
        }
        let def = self.power_up_for(pickup, pick)?;
        tracing::debug!(
            "car {} took pickup {pickup} ({}): {}",
            car.slot,
            self.pickups[pickup].name,
            self.defs[def].name
        );
        self.drop_all(car);
        let held = &mut self.held[car.slot as usize];
        let (instant, icon) = (self.defs[def].instant, self.defs[def].icon());
        if instant {
            if let Some(k) = held.iter().position(|h| h.instant) {
                let h = held.remove(k);
                remove(&self.defs[h.power_up], car);
            }
        }
        self.held[car.slot as usize].push(Held { power_up: def, since: now, instant });
        if !instant {
            self.apply(def, car, now);
        }
        car.power_up = if icon == 1 { 0 } else { icon };
        let p = &mut self.pickups[pickup];
        p.taken_at = now;
        p.out = false;
        self.changed = true;
        Some(car.flags & 3 != 0)
    }

    /// 0x80067d50: every power-up car `car` has, undone and gone.
    pub fn drop_all(&mut self, car: &mut Car) {
        let held = std::mem::take(&mut self.held[car.slot as usize]);
        for h in held {
            remove(&self.defs[h.power_up], car);
        }
    }

    /// 0x800679b0 for each car, then the pickups taken ten seconds ago back
    /// (those that give a power-up by name).
    pub fn step(&mut self, cars: &mut [Car], now: u32) {
        for car in cars.iter_mut() {
            let slot = car.slot as usize;
            let held = &mut self.held[slot];
            if held.is_empty() {
                car.power_up = 0;
                continue;
            }
            let mut k = 0;
            while k < held.len() {
                let h = held[k];
                let d = &self.defs[h.power_up];
                if h.instant {
                    held[k].since = now;
                    k += 1;
                } else if (d.seconds as u32).wrapping_mul(1000) < now.wrapping_sub(h.since) {
                    remove(d, car);
                    held.remove(k);
                } else {
                    k += 1;
                }
            }
        }
        for p in &mut self.pickups {
            if !p.out && now.wrapping_sub(p.taken_at) >= BACK_MS && p.number != 0 {
                p.out = true;
                self.changed = true;
            }
        }
    }

    /// 0x80065520 for power-up `def`, with the cars it unlocks noted.
    fn apply(&mut self, def: usize, car: &mut Car, _now: u32) {
        let d = self.defs[def].clone();
        let [first, second] = apply(&d, car);
        if car.flags & 1 != 0 && (car.slot as usize) < 2 {
            let k = car.slot as usize;
            if first {
                self.unlocked[k][0] = Some(self.unlockable[0]);
            }
            if second {
                self.unlocked[k][1] = Some(self.unlockable[1]);
            }
        }
    }
}

/// 0x80065520: power-up `d` on `car` (only a car under full physics): a
/// kick, the factors, the flags; then the springs and wheels again. Whether
/// it unlocks the track's first car, and its second.
pub fn apply(d: &PowerUp, car: &mut Car) -> [bool; 2] {
    if car.state != 2 {
        return [false; 2];
    }
    let mass = car.body.mass;
    let v = d.kick.map(|c| fx(c, mass));
    let r = car.body.rot;
    for i in 0..3 {
        let push =
            fx(r[i][0] as i32, v[0]).wrapping_add(fx(r[i][1] as i32, v[1])).wrapping_add(fx(r[i][2] as i32, v[2]));
        car.body.momentum[i] = car.body.momentum[i].wrapping_add(push);
    }
    scale(car, d, true);
    if d.flags & 8 != 0 {
        car.all_terrain = true;
    }
    if d.flags & 16 != 0 {
        car.steel = true;
        car.handling.skill = fx(car.handling.skill, 0xa000);
    }
    if d.flags & 32 != 0 {
        car.rubber = true;
    }
    if d.flags & 64 != 0 {
        car.gyro = true;
    }
    if d.flags & 1 != 0 {
        car.strong_brakes = true;
    }
    if d.flags & 4 != 0 && car.turbos < 10 {
        car.turbo_given = true;
        car.turbos += 1;
    }
    let unlocks = [d.flags & 0x100_0000 != 0, d.flags & 0x200_0000 != 0];
    refit(car);
    unlocks
}

/// The factors multiplied in on taking (`take`), divided out on losing;
/// the layout's bytes added or taken away (no power-up changes them).
fn scale(car: &mut Car, d: &PowerUp, take: bool) {
    let op: fn(i32, i32) -> i32 = if take { fx } else { div_fx };
    let h = &mut car.handling;
    let f = &d.factors;
    h.steer_lock = op(h.steer_lock, d.steer_lock);
    let layout = [&mut h.front_driven, &mut h.front_steers, &mut h.rear_driven, &mut h.rear_steers];
    for (b, &add) in layout.into_iter().zip(&d.layout) {
        let v = (*b as u8).wrapping_add(if take { add } else { add.wrapping_neg() });
        *b = v != 0;
    }
    for (field, k) in [
        (&mut h.mass, 0),
        (&mut h.cg_along, 1),
        (&mut h.cg_up, 2),
        (&mut h.brake_grip, 3),
        (&mut h.brake_bias, 4),
        (&mut h.drag, 5),
        (&mut h.front.downforce, 6),
        (&mut h.rear.downforce, 7),
        (&mut h.front.downforce_scale, 8),
        (&mut h.rear.downforce_scale, 9),
        (&mut h.air_power.yaw, 10),
        (&mut h.air_power.pitch, 11),
        (&mut h.air_power.roll, 12),
        (&mut h.front.stiffness, 13),
        (&mut h.front.grip, 14),
        (&mut h.front.damp_in, 15),
        (&mut h.front.damp_out, 16),
        (&mut h.rear.stiffness, 17),
        (&mut h.rear.grip, 18),
        (&mut h.rear.damp_in, 19),
        (&mut h.rear.damp_out, 20),
    ] {
        *field = op(*field, f[k]);
    }
    let e = &mut car.engine;
    e.redline = op(e.redline, f[21]);
    e.idle = op(e.idle, f[22]);
    e.peak_torque = op(e.peak_torque, f[23]);
}

/// 0x80065ecc: power-up `d` undone on `car`: the factors divided back out,
/// the flags cleared; then the springs and wheels again.
pub fn remove(d: &PowerUp, car: &mut Car) {
    if car.state != 2 {
        return;
    }
    scale(car, d, false);
    if d.flags & 8 != 0 {
        car.all_terrain = false;
    }
    if d.flags & 16 != 0 {
        car.steel = false;
        car.handling.skill = div_fx(car.handling.skill, 0xa000);
    }
    if d.flags & 32 != 0 {
        car.rubber = false;
    }
    if d.flags & 64 != 0 {
        car.gyro = false;
    }
    if d.flags & 1 != 0 {
        car.strong_brakes = false;
    }
    refit(car);
}

/// 0x8004528c, then the body's mass and its inverse from the handling's.
fn refit(car: &mut Car) {
    car.refit_handling();
    car.body.mass = car.handling.mass;
    car.body.inv_mass = div_fx(0x1000, car.handling.mass);
}
