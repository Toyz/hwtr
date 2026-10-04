//! The car: its rigid body, wheels, engine and handling, and the physics
//! step that turns pedals and steering into forces (`car_update`,
//! 0x8003fbe4, for cars under full physics).
//!
//! Units are the game's: inches, seconds, radians, all 4.12 fixed point and
//! multiplied with GCC's `(a * b) >> 12` ([`fx`]). See
//! `docs/engine/car-object.md`.

mod controls;
pub mod handling;
pub mod righting;
pub mod wreck;
pub mod impact;
pub mod stunt;
pub mod update;
mod load;

pub use controls::Controls;

pub use handling::{Axle, EngineSpec, Handling};

use crate::body::Body;
use crate::math::{Matrix, Tables, Vec3, add, apply_matrix_lv, column, cross, div, div_fx, divdi3, dot, fx, sub};

/// 1 and ½, 4.12.
const ONE: i32 = 0x1000;
const HALF: i32 = 0x800;
/// π, 4.12.
const PI: i32 = 0x3244;
/// Gravity, inches a second squared (386).
const G: i32 = 0x18_2000;
/// Inches in a foot, and seconds in a minute.
const TWELVE: i32 = 0xc000;
const SIXTY: i32 = 0x3_c000;
/// Sea-level air density in slugs per cubic foot, times 1000: the game
/// computes it as `0x949000 / 1000` and divides the product by 1000 again.
const AIR: i32 = 0x94_9000 / 1000;
/// The surface that drags a car along the ground.
const DRAGGING_SURFACE: u8 = 6;

/// Where a car is put back on the road.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Respawn {
    pub pos: Vec3,
    pub rot: Matrix,
    /// The zone it is in; None until one is saved.
    pub zone: Option<u16>,
}

/// The stick axes armed for turning the car in the air.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Armed {
    pub along: bool,
    pub across: bool,
}

impl Armed {
    pub fn any(self) -> bool {
        self.along || self.across
    }
}

/// One wheel.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Wheel {
    /// Mount point, body space (from the handling).
    pub mount: Vec3,
    pub diameter: i32,
    /// On the rear axle; steering; driven.
    pub rear: bool,
    pub steers: bool,
    pub driven: bool,
    /// Rolling direction, world space, unit length.
    pub heading: Vec3,
    /// Mount point, world space.
    pub world: Vec3,
    /// Touching the ground.
    pub on_ground: bool,
    /// The kind of ground under it.
    pub surface: u8,
    pub normal: Vec3,
    pub contact: Vec3,
    /// The body's velocity at the contact point.
    pub contact_vel: Vec3,
    /// The tyre's friction, scaling its grip.
    pub friction: i32,
    /// The spring's force along the ground normal.
    pub spring: i32,
    /// Set when the tyre's force passed its grip.
    pub slipping: bool,
    /// How far the spring is compressed (negative: extended), from the
    /// collision with the ground.
    pub compression: i32,
    /// How fast it turns, radians a second, for drawing.
    pub spin_rate: i32,
}


/// The engine and gearbox. Speeds of rotation are revolutions a minute.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Engine {
    pub rpm: i32,
    /// The rpm over the overall ratio: the driven wheels' speed.
    pub wheel_rpm: i32,
    /// The car rolls backwards along its forward axis.
    pub reverse: bool,
    /// The gear in use, from 0.
    pub gear: u8,
    /// The overall ratio in use (gear times final drive), negative in reverse
    /// on the ground.
    pub ratio: i32,
    /// Force at the driven wheels.
    pub drive: i64,
    pub redline: i32,
    pub idle: i32,
    /// How many forward gears (at most six).
    pub gears: i32,
    pub final_drive: i32,
    pub gear_ratios: [i32; 6],
    pub reverse_ratio: i32,
    /// Foot-pounds.
    pub peak_torque: i32,
    /// From idle to redline, 255 at the peak.
    pub torque_curve: [u8; 17],
}

impl Engine {
    pub fn in_reverse(&self) -> bool {
        self.reverse
    }

    /// The ratio of the gear in use, before the final drive.
    fn gear_ratio(&self) -> i32 {
        if self.in_reverse() { self.reverse_ratio } else { self.gear_ratios[self.gear as usize] }
    }

    /// The torque curve at the current rpm: 17 points from idle to redline.
    fn torque_fraction(&self) -> i32 {
        let along = div_fx(self.rpm.wrapping_sub(self.idle), self.redline.wrapping_sub(self.idle));
        let point = (fx(along, 16 << 12) >> 12).clamp(0, 16) as usize;
        div((self.torque_curve[point] as i32) << 12, 255).0
    }
}

/// How hard the stick turns the car in the air, about each axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AirPower {
    pub pitch: i32,
    pub roll: i32,
    pub yaw: i32,
}

/// A body axis held to a direction in the air (see [`Body::align`]); kept
/// as the game stores it, active or not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AxisLock {
    /// Held.
    pub active: bool,
    pub axis: i32,
    pub dir: Vec3,
}

/// A plane, `n · p + d` (world coordinates unless said otherwise). The
/// words after the game's normals and origin often hold uninitialised stack
/// and are not modelled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GroundPlane {
    /// Found this step.
    pub found: bool,
    pub normal: Vec3,
    pub d: i32,
}

/// The ground near a car, found each step (0x800536b4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ground {
    /// The floor (ground facing up against gravity): its plane relative to
    /// `origin` (a zone's origin, or zero).
    pub floor: GroundPlane,
    pub origin: Vec3,
    /// The nearest surface of any kind (a wheel's ground, the road, a wall).
    pub nearest: GroundPlane,
}

/// One set of tuning bytes, chosen by car flag bit 7, 8 or 9.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TuningSet {
    /// Percentages scaling the downforce.
    pub downforce_front: u8,
    pub downforce_rear: u8,
    /// Tenths added to the front grip.
    pub grip_front: u8,
}

/// Settings shared by every car, from TUNING.PRM.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tuning {
    /// A speed in mph added to a grounded car's for its downforce.
    pub downforce_mph: u8,
    pub sets: [TuningSet; 3],
    /// The drag of the dragging surface, in percent.
    pub surface_drag: u8,
    /// The range a car's skill is scaled into by the difficulty, in
    /// percent: for computer cars and for players' (low, high).
    pub computer_skill: [u8; 2],
    pub player_skill: [u8; 2],
    /// Above this speed (mph) the steering is reduced, from the first
    /// percentage at that speed toward the second at top speed.
    pub steer_mph: u8,
    pub steer_percent: [u8; 2],
    /// Milliseconds a car lies on its side, or on its nose or tail, before
    /// it is turned back (0x80046ac0).
    pub right_side_ms: u8,
    pub right_end_ms: u8,
    /// On its roof: the percentage of times it stays there (and is wrecked
    /// after `wreck_roof_ms` tens of milliseconds), else it is turned back
    /// after `right_roof_ms`.
    pub stay_roof_percent: u8,
    pub right_roof_ms: u8,
    pub wreck_roof_tens: u8,
    /// Below this speed (mph) the chase camera follows the car's heading,
    /// above it its travel.
    pub camera_travel_mph: u8,
}

/// Where TUNING.PRM keeps each setting.
mod prm {
    pub const DOWNFORCE_MPH: u32 = 10;
    pub const SETS: u32 = 33;
    pub const SURFACE_DRAG: u32 = 51;
    pub const COMPUTER_SKILL: u32 = 14;
    pub const STEER_MPH: u32 = 8;
    pub const STEER_PERCENT: [u32; 2] = [0x3a, 9];
    pub const PLAYER_SKILL: u32 = 16;
    pub const RIGHT_SIDE_MS: u32 = 0x35;
    pub const RIGHT_END_MS: u32 = 0x36;
    pub const RIGHT_ROOF_MS: u32 = 0x37;
    pub const STAY_ROOF_PERCENT: u32 = 0x38;
    pub const WRECK_ROOF_TENS: u32 = 0x39;
    pub const CAMERA_TRAVEL_MPH: u32 = 0x0b;
}

impl Tuning {
    /// TUNING.PRM, the race settings file (256 bytes).
    pub fn from_prm(b: &[u8]) -> Tuning {
        use prm::*;
        let at = |k: u32| b.get(k as usize).copied().unwrap_or(0);
        let set = |k: u32| {
            let o = SETS + 3 * k;
            TuningSet { downforce_front: at(o), downforce_rear: at(o + 1), grip_front: at(o + 2) }
        };
        Tuning {
            downforce_mph: at(DOWNFORCE_MPH),
            sets: [set(0), set(1), set(2)],
            surface_drag: at(SURFACE_DRAG),
            computer_skill: [at(COMPUTER_SKILL), at(COMPUTER_SKILL + 1)],
            player_skill: [at(PLAYER_SKILL), at(PLAYER_SKILL + 1)],
            steer_mph: at(STEER_MPH),
            steer_percent: STEER_PERCENT.map(at),
            right_side_ms: at(RIGHT_SIDE_MS),
            right_end_ms: at(RIGHT_END_MS),
            stay_roof_percent: at(STAY_ROOF_PERCENT),
            right_roof_ms: at(RIGHT_ROOF_MS),
            wreck_roof_tens: at(WRECK_ROOF_TENS),
            camera_travel_mph: at(CAMERA_TRAVEL_MPH),
        }
    }

    /// The set a car's flags choose, if any.
    pub fn for_flags(&self, flags: i32) -> Option<&TuningSet> {
        if flags & 0x80 != 0 {
            Some(&self.sets[0])
        } else if flags & 0x100 != 0 {
            Some(&self.sets[1])
        } else if flags & 0x200 != 0 {
            Some(&self.sets[2])
        } else {
            None
        }
    }
}

fn percent(p: u8) -> i32 {
    div_fx((p as i32) << 12, 100 << 12)
}

/// Downforce on each axle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Downforce {
    pub front: i32,
    pub rear: i32,
}

impl Downforce {
    fn on(&self, rear: bool) -> i32 {
        if rear { self.rear } else { self.front }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Car {
    /// The car's place in the race's car array.
    pub slot: u8,
    /// Bit 0: player one's, bit 1: player two's; bits 7, 8 and 9 choose a
    /// [`TuningSet`].
    pub flags: i32,
    /// Bit 0: the zone's flag 0x10; bit 1: its flag 0x8 (or the race's
    /// flag 0x80). Meanings not yet known.
    pub flags_8: i32,
    /// Which player drives it.
    pub player: u8,
    /// Steering angle, radians; negative steers right.
    pub steer: i32,
    /// Pedals, 0 to 1. The brake is the throttle in reverse.
    pub accel: i32,
    pub brake: i32,
    /// The stick, -1 to 1: across (steering) and along.
    pub stick: [i32; 2],
    /// The handbrake is on: the rear wheels lose their
    /// sideways grip.
    pub handbrake: bool,
    pub body: Body,
    pub wheels: Vec<Wheel>,
    /// Wheels on the ground this step, and of those, the ones on a floor.
    pub grounded: u8,
    pub grounded_level: u8,
    pub engine: Engine,
    /// How many wheels are on the front axle, and on the rear.
    pub front_wheels: u8,
    pub rear_wheels: u8,
    /// The handling, from the car's CWH.
    pub handling: Handling,
    /// The point wheel mounts are measured from (the handling's, scaled for
    /// the race).
    pub origin: Vec3,
    pub width: i32,
    pub length: i32,
    pub height: i32,
    /// Each spring's force with the car at rest: its weight over its wheels.
    pub spring_preload: i32,
    /// How far each axle's springs extend: preload over stiffness, front and
    /// rear.
    pub extension: [i32; 2],
    /// A power-up (0x80065520) has made the dragging surface not drag it.
    pub all_terrain: bool,
    /// 2: full physics (players), 1: computer cars, 0: (not yet known).
    pub state: u8,
    /// Wrecked (0x8004619c), until put back on the road.
    pub wrecked: bool,
    /// Player one's wreck has a line to play (decided at random as it
    /// wrecks, played half a second on).
    pub wreck_line: bool,
    /// Milliseconds since it wrecked; at 3000 it is put back on the road.
    pub wreck_ms: u32,
    /// Asked to be put back on the road at its next update.
    pub reset_requested: bool,
    /// Milliseconds left after a reset during which it passes through other
    /// cars.
    pub reset_grace_ms: u32,
    /// The reset (action 9, R1) and the turbo (action 10, R2) held.
    pub reset_held: bool,
    pub turbo_held: bool,
    /// The turbo button last step.
    pub turbo_before: bool,
    /// A power-up (0x80065520) makes its brakes bite: braking while
    /// rolling forward takes a tenth of its momentum each step.
    pub strong_brakes: bool,
    /// Milliseconds it has been stuck (pressing on, steering hard, slow,
    /// touching something).
    pub stuck_ms: u32,
    /// Its checkpoints and laps; once they are run it drives itself, the
    /// controls ignored.
    pub laps: crate::laps::Laps,
    /// The distance along the lap of the zone the car is in, from the
    /// zone's distance (tenths).
    pub lap_distance: i32,
    pub ground: Ground,
    /// Milliseconds aloft, all told (counted by the stunt watch, never reset
    /// there), and the stunts' points.
    pub air_total_ms: u32,
    pub stunt_points: i32,
    /// The stunt watch (0x8003cb74): the angular velocity in the car's axes
    /// last step, each axis's turn since take-off and its turn when it last
    /// reversed, and the milliseconds aloft.
    pub stunt_spin: Vec3,
    pub stunt_turn: Vec3,
    pub stunt_peak: Vec3,
    pub air_ms: u32,
    /// Set while a player's car is off the ground, for its stunt
    /// (0x8003cb74); scraping a wall for half a second clears it.
    pub airborne: bool,
    /// The system clock at its last contact, and how long its contacts have
    /// run on with gaps of at most 100 ms (0x8003caec).
    pub contact_clock: u32,
    pub contact_ms: u32,
    /// The race clock at its last contact.
    pub contact_time: u32,
    /// Where a reset puts the car back (saved by `car_update` while it
    /// drives well, or found on the best line).
    pub respawn: Respawn,
    /// Turbos in hand, at most 10 (0x8003c850 adds, a boost spends one).
    pub turbos: u8,
    /// Set once the turbo has been used, or the ten-turbos hint played.
    pub turbo_hint: bool,
    /// A turbo or a boost zone is driving it toward this speed (in/s).
    pub boost: Option<i32>,
    /// The stick is turning the car in the air.
    pub air_control: bool,
    /// Which stick axes may turn the car in the air: each is armed once
    /// centred above 15 mph.
    pub air_armed: Armed,
    pub air_lock: AxisLock,
    /// Milliseconds the car has lain on its side, on its nose or tail, and
    /// on its roof (0x80046ac0).
    pub righting: [u32; 3],
    /// On its roof: whether it will be turned back (else it is wrecked), and
    /// which way (decided once, as it lands there).
    pub rights_itself: bool,
    pub roll_way: bool,
}

impl Car {

    fn axle(&self, rear: bool) -> &Axle {
        self.handling.axle(rear)
    }

    fn handbrake_on(&self) -> bool {
        self.handbrake
    }

    /// The pedal that drives: the accelerator, or the brake in reverse.
    fn throttle(&self) -> i32 {
        if self.engine.in_reverse() { self.brake } else { self.accel }
    }

    /// The pedal that brakes: the other one.
    fn braking(&self) -> i32 {
        if self.engine.in_reverse() { self.accel } else { self.brake }
    }

    /// 0x80040a90: places the wheels for this step. Each heading is the
    /// forward axis, turned by the steering angle for wheels that steer (the
    /// first two one way, the rest the other) and, on the ground, laid into
    /// the ground plane. Then the world mount point, and for a grounded wheel
    /// the body's velocity at the contact point (`v + ω × r`), counting
    /// grounded wheels and those on a floor.
    ///
    /// The original also copies four bytes of uninitialised stack into each
    /// heading's padding word; that is not modelled.
    pub fn place_wheels(&mut self, t: &Tables) {
        let Body { rot, pos, vel, spin, gravity_dir, .. } = self.body;
        let forward = column(&rot, 1);
        let angle = self.steer.wrapping_neg();
        let ahead = forward.map(|x| fx(x, t.cos(angle)));
        let across = column(&rot, 0).map(|x| fx(x, t.sin(angle)));
        let (front_heading, rear_heading) = (sub(ahead, across), add(ahead, across));
        let (mut grounded, mut level) = (0u8, 0u8);
        for (i, wheel) in self.wheels.iter_mut().enumerate() {
            wheel.heading = match (wheel.steers, i < 2) {
                (false, _) => forward,
                (true, true) => front_heading,
                (true, false) => rear_heading,
            };
            if wheel.on_ground {
                let (h, n) = (wheel.heading, wheel.normal);
                wheel.heading = t.normalize(sub(h, n.map(|x| fx(x, dot(h, n)))));
            }
            wheel.world = add(apply_matrix_lv(&rot, sub(wheel.mount, self.origin)), pos);
            if wheel.on_ground {
                wheel.contact_vel = add(vel, cross(spin, sub(wheel.contact, pos)));
                grounded = grounded.wrapping_add(1);
                if dot(wheel.normal, gravity_dir).wrapping_neg() > HALF {
                    level = level.wrapping_add(1);
                }
            }
        }
        self.grounded = grounded;
        self.grounded_level = level;
    }

    /// 0x80060138: the engine and gearbox. With driven wheels on the ground,
    /// the road speed under them (their mean contact point's velocity, in the
    /// mean ground plane, less its sideways part) turns them at
    /// `speed / (π d)` revolutions a second; through the overall ratio that
    /// is the rpm, in the lowest forward gear that keeps it under the redline
    /// (or reverse when rolling backwards). When no driven wheel grips, the
    /// engine revs to at least the throttle's share of the redline. The
    /// torque curve at that rpm, through the ratio and the wheel radius,
    /// times the throttle, is the drive force. Above the redline the rpm is
    /// held there with no drive; in the air the engine revs with the throttle
    /// and gives no drive either. The rpm never falls below idle.
    pub fn drivetrain(&mut self, t: &Tables) {
        let driven: Vec<&Wheel> = self.wheels.iter().filter(|w| w.driven && w.on_ground).collect();
        let diameter = driven.first().map_or(0, |w| w.diameter);
        if driven.is_empty() || diameter <= 0 {
            let throttle = self.throttle();
            let e = &mut self.engine;
            e.ratio = fx(e.final_drive, e.gear_ratio());
            e.rpm = fx(e.redline, throttle);
            e.drive = 0;
        } else {
            let gripping = driven.iter().filter(|w| !w.slipping).count();
            let (contact, normal) =
                driven.iter().fold(([0; 3], [0; 3]), |(c, n), w| (add(c, w.contact), add(n, w.normal)));
            let mean = div_fx(ONE, (driven.len() as i32) << 12);
            let contact = contact.map(|c| fx(c, mean));
            let normal = t.normalize(normal.map(|c| fx(c, mean)));
            // The road's speed under the wheels, along the car.
            let v = add(self.body.vel, cross(self.body.spin, sub(contact, self.body.pos)));
            let v = sub(v, normal.map(|c| fx(c, dot(v, normal))));
            let side = column(&self.body.rot, 0);
            let v = sub(v, side.map(|c| fx(c, dot(v, side))));
            self.engine.reverse = dot(v, column(&self.body.rot, 1)) < 0;
            let wheel_rpm = fx(SIXTY, div_fx(t.length(v), fx(diameter, PI)));
            let throttle = self.throttle();
            let e = &mut self.engine;
            if e.in_reverse() {
                let ratio = fx(e.final_drive, e.reverse_ratio);
                e.rpm = fx(ratio, wheel_rpm);
                e.ratio = ratio.wrapping_neg();
            } else {
                for (gear, &ratio) in e.gear_ratios.iter().enumerate().take(e.gears as u32 as usize) {
                    e.gear = gear as u8;
                    e.ratio = fx(e.final_drive, ratio);
                    e.rpm = fx(e.ratio, wheel_rpm);
                    if e.rpm < e.redline {
                        break;
                    }
                }
            }
            if gripping == 0 {
                e.rpm = e.rpm.max(fx(e.redline, throttle));
            }
            if e.redline < e.rpm {
                e.drive = 0;
                e.rpm = e.redline;
            } else {
                let torque = fx(fx(e.torque_fraction(), e.peak_torque), TWELVE);
                let at_axle = ((torque as i64) << 8).wrapping_mul(e.ratio as i64) >> 12;
                let force = divdi3(at_axle.wrapping_shl(20), (fx(diameter, HALF) as i64) << 8);
                e.drive = force.wrapping_mul(throttle as i64) >> 12;
            }
        }
        let e = &mut self.engine;
        e.wheel_rpm = div_fx(e.rpm, e.ratio);
        e.rpm = e.rpm.max(e.idle);
    }

    /// 0x80041af0: aerodynamics. Above a speed of 1, applies the drag
    /// `0.5 ρ v² Cd w h` (feet, from inches) against the velocity, at
    /// the body's centre. Returns the downforce on each axle from the same
    /// dynamic pressure, with the tuning's bonus speed while the car is on
    /// the ground and its percentages when the car's flags choose a set.
    pub fn aero(&mut self, tuning: &Tuning) -> Downforce {
        let speed = self.body.speed;
        if speed <= ONE {
            return Downforce::default();
        }
        let dir = self.body.vel.map(|v| div_fx(v, speed));
        let twelfth = div_fx(ONE, TWELVE);
        let feet = fx(speed, twelfth);
        let pressure = |v: i32| div(fx(HALF, fx(AIR, fx(v, v))), 1000).0;
        let area = fx(fx(self.width, twelfth), fx(self.height, twelfth));
        let drag = fx(pressure(feet), fx(area, self.handling.drag));
        let q = if self.grounded != 0 {
            let mph = div_fx(176 << 12, 10 << 12);
            let bonus = fx((tuning.downforce_mph as i32) << 12, mph);
            pressure(feet.wrapping_add(fx(bonus, twelfth)))
        } else {
            pressure(feet)
        };
        let on = |axle: &Axle| fx(q, fx(axle.downforce_scale, axle.downforce));
        let mut down = Downforce { front: on(&self.handling.front), rear: on(&self.handling.rear) };
        if let Some(set) = tuning.for_flags(self.flags) {
            down.front = fx(down.front, percent(set.downforce_front));
            down.rear = fx(down.rear, percent(set.downforce_rear));
        }
        let at = add(self.body.pos, self.body.centre);
        self.body.apply_force(at, dir.map(|d| fx(d, drag.wrapping_neg())));
        down
    }

    /// 0x800427a8: one step's forces. Places the wheels, runs the engine and
    /// the aerodynamics, then applies each wheel's force: its tyre and
    /// suspension on the ground ([`Car::tyre`]), its axle's share of the
    /// downforce in the air.
    pub fn physics(&mut self, t: &Tables, tuning: &Tuning) {
        let driven = self.wheels.iter().filter(|w| w.driven && w.on_ground).count() as i32;
        self.place_wheels(t);
        self.drivetrain(t);
        let down = self.aero(tuning);
        let share = Downforce {
            front: div_fx(down.front, (self.front_wheels as i32) << 12),
            rear: div_fx(down.rear, (self.rear_wheels as i32) << 12),
        };
        let rot = self.body.rot;
        let offset =
            [0, fx(self.handling.cg_along, fx(self.length, HALF)), fx(self.handling.cg_up, fx(self.height, HALF))];
        let cg = rot.map(|row| (0..3).fold(0i32, |s, k| s.wrapping_add(fx(row[k] as i32, offset[k]))));
        for i in 0..self.wheels.len() {
            let wheel = &self.wheels[i];
            let down = share.on(wheel.rear);
            let (at, force, slipping) = if wheel.on_ground {
                let (force, slipping) = self.tyre(t, tuning, wheel, down, driven);
                (sub(wheel.contact, cg), force, Some(slipping))
            } else if self.grounded != 0 {
                // Down the body, while other wheels touch the ground.
                (wheel.world, column(&rot, 2).map(|c| fx(c, down)), None)
            } else {
                // Under air control, about the centre of gravity's height.
                let at = if self.air_armed.any() && self.air_control {
                    let along = wheel.mount[1].wrapping_sub(self.origin[1]);
                    add(column(&rot, 1).map(|c| fx(c, along)), self.body.pos)
                } else {
                    add(column(&rot, 0).map(|c| fx(c, self.origin[0])), wheel.world)
                };
                (at, self.body.gravity_dir.map(|c| fx(c, down.wrapping_neg())), None)
            };
            if let Some(slipping) = slipping {
                self.wheels[i].slipping = slipping;
            }
            self.body.apply_force(at, force);
        }
    }

    /// A grounded wheel's force, and whether it slips:
    ///
    /// - sideways friction against the contact velocity across the heading
    ///   (all of it, for a rear wheel under the handbrake), `8 m / wheels`
    ///   per unit of speed;
    /// - braking along the heading ([`Car::brake_force`]);
    /// - the drive force, shared between the driven wheels on the ground;
    /// - damping along the normal ([`Car::damping`]) and the spring.
    ///
    /// The part across the normal is held within the grip, `(load -
    /// downforce) × grip × friction`; the wheel slips when it is cut. The
    /// dragging surface also drags the car along the ground.
    fn tyre(&self, t: &Tables, tuning: &Tuning, wheel: &Wheel, down: i32, driven: i32) -> (Vec3, bool) {
        let (n, cv, heading) = (wheel.normal, wheel.contact_vel, wheel.heading);
        let rear = wheel.rear;
        let across = sub(cv, n.map(|c| fx(c, dot(cv, n))));
        let slide = if rear && self.handbrake_on() {
            across
        } else {
            sub(across, heading.map(|c| fx(c, dot(across, heading))))
        };
        let per_wheel = fx(8 << 12, div_fx(self.handling.mass, (self.wheels.len() as i32) << 12));
        let mut f = slide.map(|c| fx(c.wrapping_neg(), per_wheel));
        let brake = self.brake_force(rear);
        f = add(f, heading.map(|c| fx(c, brake)));
        if wheel.driven {
            let share = div_fx((self.engine.drive >> 8) as i32, driven << 12);
            f = add(f, heading.map(|c| fx(c, share)));
        }
        f = add(f, self.damping(t, wheel));
        f = add(f, n.map(|c| fx(c, wheel.spring)));
        let load = dot(f, n);
        let normal = n.map(|c| fx(c, load));
        let mut tangent = sub(f, normal);
        let len = t.length(tangent);
        let limit = fx(load.wrapping_sub(down), fx(self.grip(rear, tuning), wheel.friction));
        let slipping = limit < len;
        if slipping {
            let scale = div_fx(limit, len);
            tangent = tangent.map(|c| fx(c, scale));
        }
        let mut force = add(normal, tangent);
        if wheel.surface == DRAGGING_SURFACE && !self.all_terrain && !self.handling.all_terrain {
            let (pct, k) = (percent(tuning.surface_drag), div_fx(4 << 12, 10 << 12));
            let x = cv.map(|c| fx(fx(fx(c, self.handling.mass.wrapping_neg()), pct), k));
            force = add(force, sub(x, n.map(|c| fx(c, dot(n, x)))));
        }
        (force, slipping)
    }

    /// The braking force along a wheel's heading: `m g μ` times the braking
    /// pedal and the axle's share, halved (and the rear's halved again with
    /// six wheels); against the motion, so positive in reverse.
    fn brake_force(&self, rear: bool) -> i32 {
        let mut brake = fx(fx(fx(self.handling.mass, G), self.handling.brake_grip), self.braking());
        if rear {
            brake = fx(brake, ONE.wrapping_sub(self.handling.brake_bias));
            if self.wheels.len() == 6 {
                brake = fx(brake, HALF);
            }
        } else {
            brake = fx(brake, self.handling.brake_bias);
        }
        brake = fx(brake, HALF);
        if self.engine.in_reverse() { brake } else { brake.wrapping_neg() }
    }

    /// Damping against the contact point's speed along the ground normal,
    /// with the axle's compression and rebound constants; in rebound no
    /// larger than the spring.
    fn damping(&self, t: &Tables, wheel: &Wheel) -> Vec3 {
        let axle = self.axle(wheel.rear);
        let speed = dot(wheel.contact_vel, wheel.normal);
        let along = wheel.normal.map(|c| fx(c, speed));
        if speed < 0 {
            return along.map(|c| fx(c, axle.damp_in.wrapping_neg()));
        }
        let damp = along.map(|c| fx(c, axle.damp_out.wrapping_neg()));
        let len = t.length(damp);
        if wheel.spring < len {
            let scale = div_fx(wheel.spring, len);
            damp.map(|c| fx(c, scale))
        } else {
            damp
        }
    }

    /// An axle's tyre grip. With a tuning set the front takes the rear's
    /// grip plus the set's tenths.
    fn grip(&self, rear: bool, tuning: &Tuning) -> i32 {
        match (rear, tuning.for_flags(self.flags)) {
            (true, _) => self.handling.rear.grip,
            (false, None) => self.handling.front.grip,
            (false, Some(set)) => self.handling.rear.grip.wrapping_add(div_fx((set.grip_front as i32) << 12, 10 << 12)),
        }
    }

    /// 0x80044fc4: how fast each wheel turns, for drawing. Rolling with grip:
    /// `2 v / d`, backwards when the car moves against the heading.
    /// Otherwise, wheels from the third on stop under the handbrake; a driven
    /// wheel turns with the engine (`wheel rpm × 2π / 60`) unless braked
    /// harder than driven, when it stops; an undriven wheel stops under more
    /// than half the brake and otherwise keeps turning.
    pub fn spin_wheels(&mut self) {
        let (vel, speed) = (self.body.vel, self.body.speed);
        let (braking, driving) = (self.braking(), self.throttle());
        let engine_rate = div_fx(fx(self.engine.wheel_rpm, fx(2 << 12, PI)), SIXTY);
        let handbrake = self.handbrake_on();
        for (i, wheel) in self.wheels.iter_mut().enumerate() {
            let rate = if wheel.on_ground && !wheel.slipping {
                let rate = fx(2 << 12, div_fx(speed, wheel.diameter));
                Some(if dot(vel, wheel.heading) < 0 { rate.wrapping_neg() } else { rate })
            } else if i >= 2 && handbrake {
                Some(0)
            } else if wheel.driven {
                Some(if driving < braking { 0 } else { engine_rate })
            } else if braking > HALF {
                Some(0)
            } else {
                None
            };
            if let Some(rate) = rate {
                wheel.spin_rate = rate;
            }
        }
    }
}

impl Car {
    /// 0x8003d71c: the stick turns the car in the air. Each stick axis is
    /// armed once centred (within ±0.2) while the car does over 15 mph, and
    /// then acts when pushed: across, it yaws the car (rolls it with the
    /// handbrake held); along, it pitches it. The strongest of the three
    /// wins, as a couple of opposite forces either side of the drag point,
    /// scaled by the car's size, mass and air power; that axis's
    /// perpendicular is then held (see [`Body::align`]) until another wins.
    /// The spin is damped while the stick acts.
    pub fn air_control(&mut self) {
        if self.air_lock.active {
            self.body.align(self.air_lock.axis as u8, self.air_lock.dir);
        }
        self.air_control = false;
        let dead = div_fx(2 << 12, 10 << 12);
        let centred = |v: i32| v < dead && -dead < v;
        let [across, along] = self.stick;
        let (across_centred, along_centred) = (centred(across), centred(along));
        if across_centred {
            self.air_armed.across = true;
        }
        if along_centred {
            self.air_armed.along = true;
        }
        let mph = div_fx(176 << 12, 10 << 12);
        if self.body.speed < fx(15 << 12, mph) {
            self.air_armed = Armed::default();
            self.air_lock.active = false;
        }
        let armed = self.air_armed;
        let acting = (!across_centred && armed.across) || (!along_centred && armed.along);
        if !acting {
            return;
        }
        self.air_control = true;
        self.body.damp_spin();
        let (mut roll, mut yaw) = if self.handbrake_on() { (across, 0) } else { (0, across) };
        let mut pitch = along;
        if !armed.across {
            (roll, yaw) = (0, 0);
        }
        if !armed.along {
            pitch = 0;
        }
        let half = div_fx(50 << 12, 100 << 12);
        let mass = self.body.mass;
        let force = |power: i32, input: i32| fx(half, fx(power, fx(mass, input)));
        let rot = self.body.rot;
        let col = |j: usize| column(&rot, j);
        let scale = |v: Vec3, k: i32| v.map(|c| fx(c, k));
        let square = |x: i32| fx(x, x);
        let (w2, l2, h2) = (square(self.width), square(self.length), square(self.height));
        let (r, y, p) = (roll.wrapping_abs(), yaw.wrapping_abs(), pitch.wrapping_abs());
        let (axis, push, arm) = if y < r && p < r {
            (1, scale(col(0), w2.wrapping_add(h2)), scale(col(2), force(self.handling.air_power.roll, roll)))
        } else if r < y && p < y {
            (2, scale(col(0), w2.wrapping_add(l2)), scale(col(1), force(self.handling.air_power.yaw, yaw)))
        } else if r < p && y < p {
            (0, scale(col(2), l2.wrapping_add(h2)), scale(col(1), force(self.handling.air_power.pitch, pitch)))
        } else {
            return;
        };
        let centre = add(self.body.pos, self.body.centre);
        self.body.apply_force(add(centre, arm), push);
        self.body.apply_force(sub(centre, arm), push.map(i32::wrapping_neg));
        if !self.air_lock.active || self.air_lock.axis != axis {
            self.air_lock = AxisLock { active: true, axis, dir: col(axis as usize) };
        }
    }
}

