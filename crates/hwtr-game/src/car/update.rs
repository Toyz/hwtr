//! A player's car's step (`car_update` 0x8003fbe4), and the timers every
//! car runs first (`cars_update` 0x8004064c).

use super::stunt::Award;
use super::{Armed, Car, Tuning};
use crate::math::{Tables, column, div_fx, dot, fx};
use crate::rand::Rand;

/// What a car's step needs besides the car.
pub struct Drive<'a> {
    pub tables: &'a Tables,
    pub tuning: &'a Tuning,
    pub rand: &'a mut Rand,
    /// The race clock (ms) and the step (seconds, 4.12).
    pub time: u32,
    pub dt: i32,
    /// The race counts stunt points; the turbo never runs out (a cheat).
    pub scoring: bool,
    pub endless_turbo: bool,
}

/// What a step leaves for the race to do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stepped {
    /// A stunt was landed (for the HUD).
    pub stunt: Option<Award>,
    /// A snapshot is due (0x8007feb0), with the car's rotation as it was
    /// then: the original takes it mid-step, before squaring it up.
    pub snapshot: Option<crate::math::Matrix>,
}

fn mph(n: i32) -> i32 {
    fx(n << 12, div_fx(176 << 12, 10 << 12))
}

impl Car {
    /// `cars_update`'s timers, `dt_ms` a step: a wreck's (its line at half a
    /// second, a reset asked for at three), the reset's grace (true when it
    /// ends), and a boost's end, 30 mph short of its speed.
    pub fn run_timers(&mut self, dt_ms: u32) -> bool {
        if self.wrecked {
            self.wreck_ms = self.wreck_ms.wrapping_add(dt_ms);
            if self.wreck_ms >= 500 && self.wreck_line {
                self.say(0);
                self.wreck_line = false;
            }
            if self.wreck_ms >= 3000 {
                self.reset_requested = true;
            }
        }
        let mut grace_over = false;
        if self.reset_grace_ms != 0 {
            if dt_ms < self.reset_grace_ms {
                self.reset_grace_ms -= dt_ms;
            } else {
                self.reset_grace_ms = 0;
                grace_over = true;
            }
        }
        if let Some(speed) = self.boost
            && self.body.speed.wrapping_add(mph(30)) < speed
        {
            // iface_general+0xd8 (0x8002aff4): the boost flame out.
            self.flame_out.0 = Some(());
            self.boost = None;
        }
        grace_over
    }

    /// `car_update`'s first question: is the car to be put back on the
    /// road? Asked to; or wrecked a second with the reset held; or the
    /// reset held, out of its grace, and not all its wheels down or under
    /// 10 mph. The caller resets it (and clears the request).
    pub fn wants_reset(&self) -> bool {
        if self.reset_requested {
            return true;
        }
        if self.wrecked {
            return self.reset_held && self.wreck_ms >= 1000;
        }
        self.reset_held
            && self.reset_grace_ms == 0
            && (self.grounded as usize != self.wheels.len() || self.body.speed < mph(10))
    }

    /// 0x80049440: a turbo, with two wheels down, not wrecked and at least
    /// 2 mph (10 just after a reset): the car leaps to 130 mph along its
    /// travel (its velocity must have no zero component), and is boosted
    /// toward that speed.
    pub fn turbo(&mut self, t: &Tables) -> bool {
        if self.grounded < 2 || self.wrecked || self.body.speed < mph(2) {
            return false;
        }
        if self.reset_grace_ms != 0 && self.body.speed < mph(10) {
            return false;
        }
        let vel = self.body.vel;
        if vel.contains(&0) {
            return false;
        }
        let top = mph(130);
        let dir = t.normalize(vel).map(|c| fx(c, top));
        self.body.momentum = dir.map(|c| fx(c, self.body.mass));
        // 0x80049950: a player's turbo sounds (effect 1).
        if self.flags & 1 != 0 {
            self.sound(1, 1);
        }
        self.turbo_fired.0 = Some(());
        self.boost = Some(top);
        self.body.speed = top;
        true
    }

    /// 0x80049a8c: a boost pad. Faster than 2 mph and more than 5 short of
    /// 130, the car's momentum is scaled to 130 mph; a car not boosting
    /// already is boosted, a player's with the turbo's sound.
    pub fn boost_pad(&mut self) {
        let top = mph(130);
        let speed = self.body.speed;
        if top.wrapping_sub(speed) < mph(5) || speed < mph(2) {
            return;
        }
        let scale = div_fx(top, speed);
        self.body.momentum = self.body.momentum.map(|c| fx(c, scale));
        if self.boost.is_none() {
            if self.flags & 1 != 0 {
                self.sound(1, 1);
            }
            self.turbo_fired.0 = Some(());
            self.boost = Some(top);
            self.body.speed = top;
        }
    }

    /// 0x80048fa4: a launcher, throwing a car faster than 2 mph and going
    /// its way along `heading` (256ths of a turn about the vertical) at
    /// `speed_mph` (130 for 0). With all its wheels down and facing that
    /// way, the car is also turned level onto it. A player's sounds
    /// (effect 45), and the car is boosted.
    pub fn launch(&mut self, t: &Tables, speed_mph: u16, heading: u8) {
        let target = if speed_mph != 0 { mph(speed_mph as i32) } else { mph(130) };
        if self.body.speed < mph(2) {
            return;
        }
        let degrees = ((heading as u32 * 360) >> 8) as i32;
        let angle = fx(degrees << 12, 12868) / 180;
        let (s, c) = (t.sin(angle), t.cos(angle));
        let way = [c, s, 0];
        if dot(self.body.vel, way) < 0 {
            return;
        }
        if self.grounded == self.wheels.len() as u8 && dot(column(&self.body.rot, 1), way) > 0 {
            let (s, c) = (s as i16, c as i16);
            self.body.rot = [[s, c, 0], [c.wrapping_neg(), s, 0], [0, 0, 0x1000]];
        }
        let push = fx(target, self.body.mass);
        self.body.momentum = way.map(|v| fx(v, push));
        if self.flags & 1 != 0 {
            self.sound(45, 1);
        }
        self.turbo_fired.0 = Some(());
        self.boost = Some(target);
        self.body.speed = target;
    }

    /// The rest of `car_update` once any reset is done: the turbo button,
    /// the respawn point saved while driving well, the strong brakes, the
    /// forces, the air control, the step's motion, the stunt watch, the
    /// brake lights and the stuck timer. `zone` is the zone its object is
    /// in, and whether that is its only one.
    pub fn update(&mut self, drive: &mut Drive, zone: (Option<u16>, bool)) -> Stepped {
        let t = drive.tables;
        let mut stepped = Stepped::default();
        let wheels = self.wheels.len() as u8;
        if !self.wrecked {
            if self.turbo_held && !self.turbo_before {
                if drive.endless_turbo {
                    self.turbo(t);
                } else if self.turbos == 0 {
                    self.sound(44, 0);
                } else if self.turbo(t) {
                    self.turbos -= 1;
                }
                self.turbo_hint = true;
            }
            self.turbo_before = self.turbo_held;
            let level = self.grounded == wheels && self.body.rot[2][2] > 4033 && zone.1;
            let driving = level && self.flags & 32 != 0 && mph(5) < self.body.speed;
            if driving || self.respawn.zone.is_none() {
                self.respawn.pos = self.body.pos;
                self.respawn.rot = self.body.rot;
                self.respawn.zone = zone.0;
            }
            if self.strong_brakes
                && (self.handbrake || div_fx(0x2000, 10 << 12) < self.brake)
                && self.grounded == wheels
                && dot(self.body.vel, column(&self.body.rot, 1)) > 0
            {
                let k = div_fx(0x9000, 10 << 12);
                self.body.momentum = self.body.momentum.map(|c| fx(c, k));
            }
            if !self.body.asleep {
                self.physics(t, drive.tuning);
            }
            if self.grounded == 0 {
                self.air_control();
            } else {
                self.air_armed = Armed::default();
                self.air_lock.active = false;
            }
        }
        let player_one = self.flags & 1 != 0;
        let big_air = player_one
            && self.airborne
            && self.air_ms > 500
            && self.body.vel[2] > 0
            && drive.time.wrapping_sub(self.contact_time) >= 501;
        self.body.integrate(t, drive.dt);
        // A snapshot for the results, one time in two: at the top of a
        // big jump, and half a second into a wreck.
        if big_air && self.body.vel[2] < 0 && drive.rand.below(2) == 0 {
            stepped.snapshot = Some(self.body.rot);
        }
        if player_one && self.wrecked && self.wreck_ms == 500 && drive.rand.below(2) == 0 {
            stepped.snapshot = Some(self.body.rot);
        }
        if !self.body.asleep {
            self.body.rot = t.orthonormalize(&self.body.rot);
            self.spin_wheels();
        }
        if !self.wrecked {
            if player_one {
                stepped.stunt = self.watch_stunt(drive.dt, drive.scoring, &t.stunts, drive.rand);
            }
            if self.accel != 0 || self.brake != 0 {
                self.body.asleep = false;
            }
        }
        let pressing = if self.engine.reverse { self.accel } else { self.brake };
        if pressing > 0 && !self.wrecked {
            self.flags_8 |= 4;
        } else {
            self.flags_8 &= !4;
        }
        let stuck = self.accel >= 2049
            && self.steer.wrapping_abs() >= 2049
            && self.grounded == wheels
            && self.body.speed < mph(5)
            && drive.time.wrapping_sub(self.contact_time) < 100;
        self.stuck_ms = if stuck { self.stuck_ms + 25 } else { 0 };
        if self.stuck_ms > 500 {
            self.unstick();
        }
        stepped
    }

    /// 0x8004b478: a car stuck half a second turns where it stands: two
    /// opposite sideways forces, the steering times the square of its
    /// width and length, act at its mass's distance ahead of and behind
    /// its centre, for the next step's sums.
    pub fn unstick(&mut self) {
        let b = &mut self.body;
        let centre = crate::math::add(b.pos, b.centre);
        let ahead = column(&b.rot, 1).map(|c| fx(c, fx(b.mass, 4096)));
        let push = fx(fx(self.width, self.width).wrapping_add(fx(self.length, self.length)), self.steer);
        let side = column(&b.rot, 0).map(|c| fx(c, push));
        b.apply_force(crate::math::add(centre, ahead), side);
        let (behind, back) = (ahead.map(|c| fx(c, -4096)), side.map(|c| fx(c, -4096)));
        b.apply_force(crate::math::add(centre, behind), back);
    }
}
