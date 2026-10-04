//! The race camera (`camera_load` 0x80036830, the step 0x800369e4): one per
//! player, following its car at the view the player picked.
//!
//! The chase views put a target point behind and above the car, along its
//! travel (or its heading when slow), and pull the camera toward it on a
//! spring (0x80039d54); the camera then looks at the car from no farther
//! than the target's offset. The mounted views ride on the car.

use crate::car::{Car, Tuning};
use crate::collision::Collision;
use crate::collision::scp::Keyframe;
use crate::math::{Matrix, Tables, Vec3, add, column, cross, div_fx, dot, fx, sub};
use crate::rand::Rand;

/// How a view follows its car.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    /// Fixed to the car (the bumper view).
    Mounted,
    /// On a spring behind it.
    Chase,
    /// Modes 2 to 4 (replays and the like), not yet ported.
    Other(u8),
}

impl ViewMode {
    fn from_byte(b: u8) -> ViewMode {
        match b {
            0 => ViewMode::Mounted,
            1 => ViewMode::Chase,
            b => ViewMode::Other(b),
        }
    }
}

/// One of the views a player cycles through: how it follows, and where it
/// sits in the car's axes (across, along, up), aiming `lift` above the car.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
    pub mode: ViewMode,
    pub offset: Vec3,
    pub lift: i32,
}

/// The views for one player (0x800be94c), two (0x800be9c4) and the attract
/// demo (0x800bea3c), five each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Views {
    pub one: [View; 5],
    pub two: [View; 5],
    pub demo: [View; 5],
}

impl Default for Views {
    fn default() -> Views {
        let view = View { mode: ViewMode::Chase, offset: [0; 3], lift: 0 };
        Views { one: [view; 5], two: [view; 5], demo: [view; 5] }
    }
}

impl Views {
    pub const ONE: u32 = 0x800b_e94c;
    pub const TWO: u32 = 0x800b_e9c4;
    pub const DEMO: u32 = 0x800b_ea3c;

    /// The tables from a byte reader over the executable.
    pub fn read(byte: &impl Fn(u32) -> u8) -> Views {
        let word = |a: u32| i32::from_le_bytes([byte(a), byte(a + 1), byte(a + 2), byte(a + 3)]);
        let table = |at: u32| -> [View; 5] {
            std::array::from_fn(|k| {
                let e = at + 24 * k as u32;
                View {
                    mode: ViewMode::from_byte(byte(e)),
                    offset: [word(e + 4), word(e + 8), word(e + 12)],
                    lift: word(e + 20),
                }
            })
        };
        Views { one: table(Self::ONE), two: table(Self::TWO), demo: table(Self::DEMO) }
    }
}

/// What a camera step looks at besides its camera and car.
#[derive(Clone, Copy)]
pub struct Surroundings<'a> {
    pub tables: &'a Tables,
    pub tuning: &'a Tuning,
    /// The views the race offers (the table for its players), and how many.
    pub views: &'a [View; 5],
    pub count: u8,
    /// Under way: the chase views spring (before, they snap).
    pub racing: bool,
    /// The race clock, and the track's flyby.
    pub time: u32,
    pub flyby: &'a [Keyframe],
    /// The track, to keep the camera inside it.
    pub collision: &'a Collision,
}

/// A player's camera.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Camera {
    /// The car it follows (by slot) and the view picked.
    pub car: u8,
    pub view: u8,
    pub mode: Option<ViewMode>,
    pub pos: Vec3,
    pub vel: Vec3,
    /// Its axes as columns: right, forward (toward what it looks at), up.
    pub rot: Matrix,
    /// The field of view, radians.
    pub fov: i32,
    /// Placed exactly this step, not sprung (a new view, the start).
    pub snap: bool,
    /// The view button, this step and last.
    pub button: bool,
    pub button_before: bool,
    /// How far a boost has pulled the chase views back.
    pub zoom: i32,
    /// Milliseconds of shake left (a wreck sets 250).
    pub shake: u32,
    /// Milliseconds left of the sweep that opens a race.
    pub intro_ms: i32,
    /// Flying over the track before the race (the SCP's flyby).
    pub flyby: bool,
}

/// The spring's limits: speed (in/s), acceleration (in/s²), and the share
/// of the gap closed in a second.
const SPRING_SPEED: i32 = 30_000 << 12;
const SPRING_ACCEL: i32 = 10_000 << 12;

/// How long the opening sweep takes, ms.
pub const INTRO_MS: i32 = 5000;

impl Camera {
    /// `camera_load`'s camera for player `slot`, at the first view.
    pub fn new(slot: u8, views: &[View; 5]) -> Camera {
        Camera {
            car: slot,
            mode: Some(views[0].mode),
            fov: fx(0x3244, 0x800),
            snap: true,
            ..Camera::default()
        }
    }

    /// 0x800369e4 for one camera, a step of `dt_ms`, following `car`: the
    /// flyby while it lasts, else its view's mode, then shake and the view
    /// button.
    pub fn step(&mut self, world: &Surroundings, car: &mut Car, dt_ms: u32, rand: &mut Rand) {
        let Surroundings { tables: t, views, count, time, flyby, collision, .. } = *world;
        if self.flyby && self.fly(t, flyby, time) {
            return;
        }
        let rate = div_fx(1000 << 12, (dt_ms as i32) << 12);
        let view = views[self.view as usize % 5];
        match self.mode {
            Some(ViewMode::Mounted) => self.mount(t, collision, car, view),
            Some(ViewMode::Chase) => self.chase(world, car, view, dt_ms, rate),
            other => tracing::trace!("camera mode {other:?}: not yet ported"),
        }
        if dt_ms < self.shake {
            for k in 0..3 {
                let jolt = (rand.below(self.shake) as i32 - 125) << 12;
                self.pos[k] = self.pos[k].wrapping_add(div_fx(jolt, 10 << 12));
            }
            self.shake -= dt_ms;
        } else {
            self.shake = 0;
        }
        self.snap = false;
        if self.button != self.button_before && self.button {
            self.view = (self.view + 1) % count.max(1);
            self.mode = Some(views[self.view as usize % 5].mode);
            self.snap = true;
            if self.intro_ms != 0 {
                car.grounded = car.wheels.len() as u8;
                self.intro_ms = 0;
            }
        }
        self.button_before = self.button;
    }

    /// 0x8003acbc: the flyby, `time` milliseconds into the race: 200 ms
    /// between keyframes, the eye and the target each moving straight
    /// between them; the camera looks level-sided from the eye to the
    /// target. False when the track has no flyby.
    pub fn fly(&mut self, t: &Tables, path: &[Keyframe], time: u32) -> bool {
        let Some(last) = (path.len() as u32).checked_sub(1).filter(|_| !path.is_empty()) else { return false };
        let span = last.wrapping_mul(200);
        if span == 0 {
            return false;
        }
        let clamped = time.min(span);
        let at = time.wrapping_mul(last) / span;
        let (from, to) = (at.min(last) as usize, (at + 1).min(last) as usize);
        let progress = div_fx((last.wrapping_mul(clamped) << 12) as i32, (span << 12) as i32);
        let f = progress.wrapping_sub((from as i32) << 12);
        let mix = |a: Vec3, b: Vec3| [0, 1, 2].map(|k| fx(a[k], 0x1000 - f).wrapping_add(fx(b[k], f)));
        let eye = mix(path[from].eye, path[to].eye);
        let target = mix(path[from].target, path[to].target);
        let look = t.normalize(sub(target, eye));
        let right = t.normalize([look[1], look[0].wrapping_neg(), 0]);
        let up = t.normalize(cross(right, look));
        self.pos = eye;
        for (i, row) in self.rot.iter_mut().enumerate() {
            *row = [right[i] as i16, look[i] as i16, up[i] as i16];
        }
        true
    }

    /// Mode 0: the camera rides on the car at the view's offset, its axes
    /// the car's.
    fn mount(&mut self, t: &Tables, collision: &Collision, car: &Car, view: View) {
        let body = &car.body;
        let centre = add(body.pos, body.centre);
        let off = body.rot.map(|row| dot(row.map(|c| c as i32), view.offset));
        self.pos = add(centre, off);
        self.vel = body.vel;
        self.rot = body.rot;
        self.fov = fx(0x3244, 0x800);
        collision.keep_inside(t, self.car, &mut self.pos);
    }

    /// Mode 1: the chase view.
    fn chase(&mut self, world: &Surroundings, car: &Car, view: View, dt_ms: u32, rate: i32) {
        let Surroundings { tables: t, tuning, racing, flyby, collision, .. } = *world;
        let body = &car.body;
        let rot = body.rot;
        let (car_forward, car_up) = (column(&rot, 1), column(&rot, 2));
        let banked = car.flags & 4 != 0;
        if car.flags & 16 != 0 {
            tracing::trace!("the camera on loops (0x8005e984): not yet ported");
        }
        let mut centre = add(body.pos, body.centre);
        // Which way the car goes: its travel when fast, its heading (or the
        // way from the camera) when slow; level unless the track banks.
        let mut travel = body.vel;
        if !banked {
            travel[2] = 0;
        }
        let mph = div_fx(176 << 12, 10 << 12);
        let slow = fx((tuning.camera_travel_mph as i32) << 12, mph);
        let along = if t.length(travel) < slow {
            let mut dir = if car.grounded != 0 || self.intro_ms != 0 || self.snap {
                car_forward
            } else {
                sub(centre, self.pos)
            };
            dir[2] = 0;
            if dir[0].wrapping_abs().wrapping_add(dir[1].wrapping_abs()) > 0 { dir } else { car_up }
        } else {
            travel
        };
        let forward = t.normalize(along);
        let (right, up) = if banked {
            let mut right = cross(forward, car_up);
            if right.iter().fold(0i32, |s, c| s.wrapping_add(c.wrapping_abs())) <= 0 {
                right = [0x1000, 0, 0];
            }
            let right = t.normalize(right);
            (right, cross(right, forward))
        } else {
            ([forward[1], forward[0].wrapping_neg(), 0], [0, 0, 0x1000])
        };
        centre = add(centre, up.map(|c| fx(c, view.lift)));
        // A boost pulls the view back as the car nears the boost's speed.
        let mut pull = 0;
        if let Some(target) = car.boost {
            let span = fx(30 << 12, mph);
            pull = div_fx(body.speed.wrapping_sub(target).wrapping_add(span), span);
        }
        self.zoom = self.zoom.wrapping_add(div_fx(pull.wrapping_sub(self.zoom), rate));
        let scale = fx(self.zoom, 0x800).wrapping_add(0x1000);
        let offset = [fx(view.offset[0], scale), fx(view.offset[1], scale), view.offset[2]];
        let mut target = centre;
        for (axis, amount) in [(right, offset[0]), (forward, offset[1]), (up, offset[2])] {
            target = add(target, axis.map(|c| fx(c, amount)));
        }
        collision.keep_inside(t, self.car, &mut target);
        let reach = t.length(sub(target, centre));
        let length = t.length(offset);
        if racing && !body.asleep && !self.snap && self.intro_ms == 0 {
            // Behind the car's back: rise over it rather than through it.
            if dot(sub(self.pos, centre), sub(target, centre)) < 0 {
                target[2] = target[2].wrapping_add(0x7_8000);
            }
            target = self.spring(t, target, rate);
        }
        let mut look = t.normalize(sub(centre, target));
        let up = if car.flags & 16 != 0 { [0, 0, 0x1000] } else { up };
        let mut right = cross(look, up);
        if right == [0; 3] {
            look = [0, 0, -0x1000];
            right = [0x1000, 0, 0];
        }
        let right = t.normalize(right);
        let up = cross(right, look);
        let distance = reach.min(length);
        let pos = sub(centre, look.map(|c| fx(c, distance)));
        if self.intro_ms != 0 {
            self.sweep(t, flyby, pos, centre);
            self.intro_ms = self.intro_ms.wrapping_sub(dt_ms as i32);
            return;
        }
        // Snapped, it takes the car's travel (level unless banked).
        self.vel = if self.snap { travel } else { sub(pos, self.pos).map(|c| fx(c, rate)) };
        self.pos = pos;
        for (i, row) in self.rot.iter_mut().enumerate() {
            *row = [right[i] as i16, look[i] as i16, up[i] as i16];
        }
        self.fov = fx(0x3244, 0x800);
    }

    /// The sweep that opens a race: over [`INTRO_MS`] the eye eases (a
    /// half sine) from the flyby's last keyframe to the chase view's `pos`,
    /// and what it looks at from the keyframe's target to the car's
    /// `centre`.
    fn sweep(&mut self, t: &Tables, flyby: &[Keyframe], pos: Vec3, centre: Vec3) {
        let Some(from) = flyby.last() else { return };
        let done = (INTRO_MS - self.intro_ms) * 0x1000 / INTRO_MS;
        let ease = fx(t.sin(fx(done - 0x800, 0x3244)) + 0x1000, 0x800);
        let mix = |a: Vec3, b: Vec3| [0, 1, 2].map(|k| fx(a[k], 0x1000 - ease).wrapping_add(fx(b[k], ease)));
        let eye = mix(from.eye, pos);
        let look = t.normalize(sub(mix(from.target, centre), eye));
        let right = t.normalize([look[1], look[0].wrapping_neg(), 0]);
        let up = cross(right, look);
        self.pos = eye;
        for (i, row) in self.rot.iter_mut().enumerate() {
            *row = [right[i] as i16, look[i] as i16, up[i] as i16];
        }
    }

    /// 0x80039d54: the camera moves toward `target`, its velocity heading
    /// for a tenth of the gap each `rate`th of a second (at most
    /// [`SPRING_SPEED`]), changing by at most [`SPRING_ACCEL`] while it
    /// speeds up; the new position.
    fn spring(&mut self, t: &Tables, target: Vec3, rate: i32) -> Vec3 {
        let tenth = div_fx(0x1000, 10 << 12);
        let lengths = |v: Vec3| t.length(v);
        let mut want = sub(target, self.pos).map(|c| fx(fx(c, tenth), rate));
        let speed = lengths(want);
        if SPRING_SPEED < speed {
            let k = div_fx(SPRING_SPEED, speed);
            want = want.map(|c| fx(c, k));
        }
        let mut accel = sub(want, self.vel).map(|c| fx(c, rate));
        if lengths(self.vel) < lengths(want) {
            let a = lengths(accel);
            if SPRING_ACCEL < a {
                let k = div_fx(SPRING_ACCEL, a);
                accel = accel.map(|c| fx(c, k));
            }
        }
        let per = div_fx(0x1000, rate);
        self.vel = add(self.vel, accel.map(|c| fx(c, per)));
        add(self.pos, self.vel.map(|c| fx(c, per)))
    }
}
