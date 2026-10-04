//! The computer cars' driver (`fakeai_load`, 0x800797f0, and the step
//! 0x8007265c): a computer car does not run the car physics. Each follows the
//! track's route (the BLD's program of keypoints and branches) with a
//! kinematic model of its own: a place along the route, a lateral offset
//! that swerves about, a speed from its pace and the race's rubber band, an
//! orientation built from the ground and its travel and turned by the spin
//! knocks give it, and stunts over jumps. The result is written into the
//! car's body for drawing and for the collision code, whose knocks come
//! back as shoves (0x8007937c).

use crate::car::Car;
use crate::line::BestLine;
use crate::math::{Matrix, Tables, Vec3, add, cross, div_fx, dot, fx, gte_mul, mul_16_64, mul_64_16, sub, transpose};
use crate::rand::Rand;

/// The AI's own random numbers (0x800d2710): a linear congruential
/// generator apart from the game's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AiRand {
    pub seed: u32,
}

impl AiRand {
    pub fn draw(&mut self) -> u32 {
        self.seed = self.seed.wrapping_mul(0x19_660d).wrapping_add(0x3c6e_f35f);
        self.seed
    }

    /// The next number modulo `n`.
    pub fn below(&mut self, n: u32) -> u32 {
        self.draw() % n
    }
}

/// TUNING.PRM's settings for the computer cars.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AiTuning {
    /// Pairs scaled between by the difficulty: the field's pace (percent),
    /// the rubber band's lowest and highest factor (percent), and the lead
    /// it keeps over the player and how far either way it lets that drift
    /// (tenths); the extra lead on the last lap (tenths).
    pub pace: [u8; 2],
    pub rubber_min: [u8; 2],
    pub rubber_max: [u8; 2],
    pub lead: [u8; 2],
    pub band: [u8; 2],
    pub last_lap: u8,
    /// The chance (percent) a car tries a harder stunt.
    pub stunt_skill: [u8; 2],
    /// A knock's effect on the swerve and the pace: sideways, the pace's
    /// ceiling and floor (percent; the floor less 100), forward, backward,
    /// from below and from above.
    pub knock_side: u8,
    pub knock_pace_max: u8,
    pub knock_pace_min: u8,
    pub knock_forward: u8,
    pub knock_back: u8,
    pub knock_under: u8,
    pub knock_over: u8,
    /// How far a car keeps from the route's edges, and its swerves' limit.
    pub edge: u8,
    pub swerve: u8,
    /// Hundredths of a second between rubber-band adjustments, and the
    /// adjustments (percent): ahead, settling while ahead, settling while
    /// behind, behind.
    pub rubber_every: u8,
    pub rubber_ahead: u8,
    pub rubber_settle_up: u8,
    pub rubber_settle_down: u8,
    pub rubber_behind: u8,
}

impl AiTuning {
    pub fn from_prm(at: &impl Fn(u32) -> u8) -> AiTuning {
        AiTuning {
            pace: [at(0x00), at(0x01)],
            rubber_min: [at(0x02), at(0x03)],
            rubber_max: [at(0x0c), at(0x0d)],
            lead: [at(0x04), at(0x05)],
            band: [at(0x2e), at(0x2f)],
            last_lap: at(0x34),
            stunt_skill: [at(0x06), at(0x07)],
            knock_side: at(0x12),
            knock_pace_max: at(0x13),
            knock_pace_min: at(0x14),
            knock_forward: at(0x15),
            knock_back: at(0x16),
            knock_under: at(0x17),
            knock_over: at(0x18),
            edge: at(0x19),
            swerve: at(0x1a),
            rubber_every: at(0x1b),
            rubber_ahead: at(0x1c),
            rubber_settle_up: at(0x1d),
            rubber_settle_down: at(0x1e),
            rubber_behind: at(0x1f),
        }
    }
}

/// A choice at a route's branch: the routes (bits of a car's handling flags)
/// it suits, bits 29 and 30 for a car swerved right or left, and its weight
/// for each route.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Choice {
    pub mask: u32,
    pub weights: [u8; 16],
}

/// One instruction of the route (0x8007b388).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// A keypoint at `pos`, with the route's widths either side, its flags
    /// (bit 1: a jump) and its distance to go (in hundredths of a lap).
    Point {
        pos: Vec3,
        widths: (i32, i32),
        flags: u16,
        distance: u16,
    },
    /// A keypoint `delta` from the last.
    Step {
        delta: Vec3,
        widths: (i32, i32),
    },
    Jump(u16),
    /// The choices, and where each goes.
    Branch(Vec<(Choice, u16)>),
    /// The time a jump lasts, in hundredths.
    Air(u8),
}

impl Op {
    /// The instruction at `at` in `stream`, and its length; None past the
    /// end or for an unknown opcode.
    pub fn decode(stream: &[u8], at: u16) -> Option<(Op, u16)> {
        let b = stream.get(at as usize..)?;
        let byte = |k: usize| b.get(k).copied();
        let word = |k: usize| Some(i32::from_le_bytes([byte(k)?, byte(k + 1)?, byte(k + 2)?, byte(k + 3)?]));
        let half = |k: usize| Some(u16::from_le_bytes([byte(k)?, byte(k + 1)?]));
        match byte(0)? {
            0 => Some((
                Op::Point {
                    pos: [word(1)?, word(5)?, word(9)?],
                    widths: ((byte(13)? as i32) << 12, (byte(14)? as i32) << 12),
                    flags: half(15)?,
                    distance: half(17)?,
                },
                19,
            )),
            1 => Some((
                Op::Step {
                    delta: [half(1)?, half(3)?, half(5)?].map(|h| (h as i16 as i32) << 7),
                    widths: ((byte(7)? as i32) << 12, (byte(8)? as i32) << 12),
                },
                9,
            )),
            2 => Some((Op::Jump(half(1)?), 3)),
            3 => {
                let n = byte(1)? as usize;
                let masks: Option<Vec<u32>> = (0..n).map(|k| word(2 + 4 * k).map(|w| w as u32)).collect();
                let tos: Option<Vec<u16>> = (0..n).map(|k| half(2 + 4 * n + 2 * k)).collect();
                let at = 2 + 6 * n;
                let weights: Option<Vec<[u8; 16]>> =
                    (0..n).map(|k| b.get(at + 16 * k..at + 16 * k + 16).map(|w| w.try_into().unwrap())).collect();
                let choices = masks?
                    .into_iter()
                    .zip(tos?)
                    .zip(weights?)
                    .map(|((mask, to), weights)| (Choice { mask, weights }, to))
                    .collect();
                Some((Op::Branch(choices), ((4 * n + 2 * n + 2 + 16 * n) & 254) as u16))
            }
            4 => Some((Op::Air(byte(1)?), 2)),
            _ => None,
        }
    }
}

/// A car's place on the route (the record +0x08 to +0x9b, advanced by
/// 0x8007b830).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    /// The point on the route now.
    pub target: Vec3,
    /// The route's width to the left and right of it.
    pub left: i32,
    pub right: i32,
    /// How far the route turns at the keypoint just passed (a 409th of a
    /// radian a unit).
    pub turn: i32,
    /// The keypoint's flags (bit 1: a jump).
    pub flags: u16,
    /// Distance to go, and a jump's time left.
    pub distance: i32,
    pub air: i32,
    /// The choices of the branch waiting, and the one taken (none yet).
    pub choices: Vec<Choice>,
    pub choice: Option<usize>,
    /// The keypoints passed and next, and their widths.
    pub from: Vec3,
    pub to: Vec3,
    pub from_widths: (i32, i32),
    pub to_widths: (i32, i32),
    /// Where in the route, the segment's length and the way along it.
    pub at: u16,
    pub length: i32,
    pub along: i32,
}

impl Line {
    /// 0x8007b6b0: on the route at `at` (a grid place's start), which must
    /// be a keypoint.
    pub fn start(stream: &[u8], at: u16) -> Option<Line> {
        let (Op::Point { pos, widths, flags, distance }, len) = Op::decode(stream, at)? else { return None };
        Some(Line {
            target: pos,
            left: widths.0,
            right: widths.1,
            flags,
            distance: fx((distance as i32) << 12, 409),
            at: at.wrapping_add(len),
            from: pos,
            to: pos,
            from_widths: widths,
            to_widths: widths,
            ..Line::default()
        })
    }

    /// 0x8007b830: `step` further along. Past the segment's end the route is
    /// read on to the next keypoint; a branch with no choice made yet stops
    /// there (false) for the caller to choose. `ground` is the surface's
    /// normal under the car, if any, to measure the route's turn against.
    pub fn advance(&mut self, t: &Tables, stream: &[u8], step: i32, ground: Option<Vec3>, mirrored: bool) -> bool {
        if stream.is_empty() {
            return false;
        }
        if self.length < self.along.wrapping_add(step) {
            let keypoint = loop {
                let Some((op, len)) = Op::decode(stream, self.at) else { return false };
                match op {
                    Op::Branch(choices) => match self.choice.take() {
                        None => {
                            self.choices = choices.into_iter().map(|(c, _)| c).collect();
                            return false;
                        }
                        Some(k) => self.at = choices.get(k).map_or(0, |c| c.1),
                    },
                    Op::Jump(to) => self.at = to,
                    Op::Air(p) => {
                        self.at = self.at.wrapping_add(len);
                        let a = fx((p as i32) << 12, self.length / 1000);
                        self.air = a.wrapping_sub(self.along.wrapping_sub(self.length) / 1000);
                    }
                    op @ (Op::Point { .. } | Op::Step { .. }) => {
                        self.at = self.at.wrapping_add(len);
                        break op;
                    }
                }
            };
            let old = self.length;
            self.length = 0x6_4000;
            self.along = self.along.wrapping_sub(old);
            let before = self.from;
            self.from = self.to;
            let widths = match keypoint {
                Op::Point { pos, widths, flags, distance } => {
                    self.to = pos;
                    self.flags = flags;
                    let a = fx((distance as i32) << 12, self.length / 1000);
                    self.distance = a.wrapping_sub(self.along / 1000);
                    widths
                }
                Op::Step { delta, widths } => {
                    self.to = add(self.from, delta);
                    widths
                }
                _ => unreachable!(),
            };
            self.from_widths = self.to_widths;
            self.to_widths = widths;
            self.turn = 0;
            if let Some(n) = ground {
                let flat = |v: Vec3| {
                    let k = fx(v[0], n[0]).wrapping_add(fx(v[1], n[1])).wrapping_add(fx(v[2], n[2]));
                    sub(v, n.map(|c| fx(c, k)))
                };
                let a = flat(sub(self.from, before));
                let b = flat(sub(self.to, self.from));
                let (la, lb) = (t.length(a), t.length(b));
                if la >= 0x1001 && lb >= 0x1001 {
                    let c = div_fx(dot(a, b), fx(la, lb)).clamp(-4096, 4096);
                    let mut turn = t.acos(c);
                    if dot(cross(a, b), n) < 0 {
                        turn = -turn;
                    }
                    self.turn = div_fx(turn, 409);
                }
            }
        }
        self.along = self.along.wrapping_add(step);
        let k = div_fx(self.along, self.length);
        let mix = |a: i32, b: i32| fx(a, 4096 - k).wrapping_add(fx(b, k));
        self.target = [0, 1, 2].map(|i| mix(self.from[i], self.to[i]));
        self.left = mix(self.from_widths.0, self.to_widths.0);
        self.right = mix(self.from_widths.1, self.to_widths.1);
        let d = step / 1000;
        self.distance = self.distance.wrapping_sub(d);
        if self.flags & 2 != 0 {
            self.air = self.air.wrapping_sub(d);
        }
        if mirrored {
            self.target[0] = self.target[0].wrapping_neg();
        }
        true
    }
}

/// A stunt over a jump (+0x220): which way it turns, and how far.
pub mod stunt {
    /// Deciding: no stunt this jump.
    pub const NONE: u16 = 1;
    pub const UNDER_WAY: u16 = 2;
    /// About the car's sideways, up or forward axis.
    pub const FLIP: u16 = 4;
    pub const SPIN: u16 = 8;
    pub const ROLL: u16 = 16;
    /// The other way.
    pub const BACKWARD: u16 = 32;
    /// Twice or three times round.
    pub const DOUBLE: u16 = 64;
    pub const TRIPLE: u16 = 128;
    /// Half a turn more, and a wreck on landing.
    pub const BOTCHED: u16 = 256;
}

/// A computer car's driver (a record of 592 bytes at 0x801315f4).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Driver {
    /// The car it drives (its slot).
    pub car: u8,
    pub line: Line,
    /// It has a route to drive.
    pub active: bool,
    /// Where the car is, and was a step ago; the route's point under it now
    /// and a step ago (raised to the car's ride height).
    pub pos: Vec3,
    pub prev_pos: Vec3,
    pub base: Vec3,
    pub prev_base: Vec3,
    pub rot: Matrix,
    /// The last three up and travel directions, the next to replace, and
    /// their averages.
    pub ups: [Vec3; 3],
    pub up_at: u32,
    pub travels: [Vec3; 3],
    pub travel_at: u32,
    pub up: Vec3,
    pub travel: Vec3,
    /// The swerve: offset from the route, where it heads, its speed and
    /// acceleration; a knock from behind turns it about.
    pub swerve: i32,
    pub swerve_to: i32,
    pub swerve_speed: i32,
    pub swerve_accel: i32,
    pub swerve_flip: bool,
    /// Pace (percent less 100), the step's distance, and the start's
    /// acceleration from rest.
    pub pace: i32,
    pub speed: i32,
    pub launch: i32,
    /// The race left to run (for the rubber band).
    pub progress: i32,
    /// A knock's drift and the shove still to come into it.
    pub drift: Vec3,
    pub shove: Vec3,
    /// Spin (64 bits) and the spin knocks still to give.
    pub spin: [i64; 3],
    pub spin_shove: Vec3,
    /// Knocks' effects on the swerve and the pace: 2 while they last.
    pub knocked: i32,
    pub knock_pace: i32,
    pub knock_swerve: i32,
    /// The rubber band: its limits, factor and when it was last set.
    pub rubber_min: i32,
    pub rubber_max: i32,
    pub rubber: i32,
    pub rubber_at: u32,
    /// The lean the route's turns ask for, the lean, and which way it moved.
    pub slip: i32,
    pub lean: i32,
    pub lean_way: i32,
    /// A stunt over a jump, its time, and the car's turn when it began.
    pub stunt: u16,
    pub stunt_ms: i32,
    pub stunt_rot: Matrix,
    /// Percent chance of trying a harder stunt.
    pub skill: i32,
    /// Has been over a jump; not yet off on its own.
    pub jumped: bool,
    pub racing: bool,
    /// The route where the car last drove steadily (kept in the car's
    /// record at +0x7d0), for a reset.
    pub respawn_line: Line,
}

/// The race's settings for the computer cars (0x800d26dc to 0x800d270c).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ai {
    pub drivers: Vec<Driver>,
    pub rand: AiRand,
    /// The lap's length (thousandths), and the laps.
    pub lap: i32,
    pub laps: u8,
    /// The pace scale, the rubber band's limits, the lead kept and the band
    /// either side of it, the difficulty's share and the last lap's extra
    /// lead.
    pub pace: i32,
    pub rubber_min: i32,
    pub rubber_max: i32,
    pub lead: i32,
    pub share: i32,
    pub band: i32,
    pub last_lap: i32,
    /// The players' cars the band holds to (the first, its slot), and how
    /// many players there are.
    pub leader: Option<u8>,
    pub players: u8,
    /// The track is mirrored.
    pub mirrored: bool,
}

/// `lo + (hi - lo) * difficulty / 255`, from bytes (0x8007977c).
fn between(lo: u8, hi: u8, difficulty: u8) -> i32 {
    let span = ((hi as i32) << 12) - ((lo as i32) << 12);
    fx(((difficulty as i32) << 12) / 255, span).wrapping_add((lo as i32) << 12)
}

/// The car's state for the AI's driving.
const DRIVEN: u8 = 1;

fn driving(car: &Car, d: &Driver) -> bool {
    !car.wrecked && !car.laps.finished && car.state == DRIVEN && d.active
}

impl Ai {
    /// `fakeai_load` (0x800797f0): the race's settings from the tuning and
    /// the difficulty (a cup race "tcup" has its own), a driver record for
    /// each of six cars.
    pub fn new(tuning: &crate::car::Tuning, difficulty: u8, laps: u8, lap_length: i32, cup: bool, seed: u32) -> Ai {
        let a = &tuning.ai;
        let pct = |v: i32| div_fx(v, 100 << 12);
        let tenth = |v: i32| div_fx(v, 10 << 12);
        let lerp = |p: [u8; 2]| between(p[0], p[1], difficulty);
        let mut ai = if cup {
            Ai {
                pace: 4096,
                rubber_min: pct(100 << 12),
                rubber_max: pct(0x6_8000),
                share: 4096,
                band: 0x2000,
                last_lap: 0,
                lead: div_fx(0x4000, 0xa000),
                ..Ai::default()
            }
        } else {
            Ai {
                pace: pct(lerp(a.pace)),
                rubber_min: pct(lerp(a.rubber_min)),
                rubber_max: pct(lerp(a.rubber_max)),
                lead: tenth(lerp(a.lead)),
                share: ((difficulty as i32) << 12) / 255,
                band: tenth(lerp(a.band)),
                last_lap: tenth(between(a.last_lap, 0, difficulty)),
                ..Ai::default()
            }
        };
        ai.rand = AiRand { seed };
        ai.laps = laps;
        ai.drivers = (0..6).map(|k| Driver { car: k, ..Driver::default() }).collect();
        ai.lap = (lap_length << 12) / 1000;
        ai
    }

    /// 0x80079e58: the driver for computer car `car` from grid place
    /// `grid`, which takes the car's place and turn, its pace and skill.
    #[allow(clippy::too_many_arguments)]
    pub fn add(
        &mut self,
        car: &mut Car,
        route: &BestLine,
        grid: usize,
        difficulty: u8,
        laps: u8,
        tuning: &crate::car::Tuning,
        time: u32,
    ) {
        let a = &tuning.ai;
        let skill = between(a.stunt_skill[0], a.stunt_skill[1], difficulty) >> 12;
        let body = &mut car.body;
        let rot = body.rot;
        let col = |j: usize| [rot[0][j] as i32, rot[1][j] as i32, rot[2][j] as i32];
        let mut d = Driver {
            car: car.slot,
            skill,
            pos: body.pos,
            prev_pos: body.pos,
            base: body.pos,
            prev_base: body.pos,
            rot,
            ..Driver::default()
        };
        body.ang_momentum = [0; 3];
        body.gravity = 0;
        body.gravity_dir = [0, 0, -0x1000];
        body.momentum = [0; 3];
        body.speed = 0;
        match route.starts.get(grid).and_then(|&at| Line::start(&route.stream, at)) {
            None => d.active = false,
            Some(line) => {
                d.line = line;
                d.active = true;
                self.laps = laps;
                d.ups = [col(2); 3];
                d.up_at = 0;
                d.travels = [col(1); 3];
                d.travel_at = 0;
                d.swerve_to = ((self.rand.below(201) as i32) - 100) << 12;
                d.pace = fx(0x6_4000, car.handling.ai_pace) - 0x6_4000;
                d.pace = fx(d.pace + 0x6_4000, self.pace) - 0x6_4000;
                d.rubber_min = self.rubber_min;
                d.rubber_max = self.rubber_max;
                d.rubber = 4096;
                d.rubber_at = time;
                d.racing = true;
                d.progress = fx((self.laps as i32) << 12, self.lap);
            }
        }
        let slot = car.slot as usize;
        if let Some(r) = self.drivers.get_mut(slot) {
            *r = d;
        }
    }
}

/// 0.99 and 0.95 a step, as the game divides them out.
fn k99() -> i32 {
    div_fx(0x6_3000, 0x6_4000)
}

fn k95() -> i32 {
    div_fx(0x5_f000, 0x6_4000)
}

fn col(m: &Matrix, j: usize) -> Vec3 {
    [m[0][j] as i32, m[1][j] as i32, m[2][j] as i32]
}

fn set_cols(cols: [Vec3; 3]) -> Matrix {
    std::array::from_fn(|i| std::array::from_fn(|j| cols[j][i] as i16))
}

fn scale(v: Vec3, k: i32) -> Vec3 {
    v.map(|c| fx(c, k))
}

impl Ai {
    /// 0x80079cc4, at the race's start: the computer cars slowed by the
    /// difficulty's share of the first car's catch-up (unless player one is
    /// a computer car, or in a cup race), and the players found: the band
    /// holds to the first.
    pub fn start(&mut self, cars: &[Car], cup: bool) {
        if !cup
            && let Some(first) = cars.first()
            && first.state != DRIVEN
        {
            let k = 4096 - fx(self.share, 4096 - first.handling.ai_rubber);
            for d in self.drivers.iter_mut().take(cars.len()) {
                d.pace = fx(d.pace + 0x6_4000, k) - 0x6_4000;
            }
        }
        let players: Vec<u8> = cars.iter().filter(|c| c.state != DRIVEN).map(|c| c.slot).take(2).collect();
        self.leader = players.first().copied();
        self.players = players.len() as u8;
    }

    /// 0x8007a958: car `slot` put back on the road (0x80041384): its driver
    /// back where it was last steady on the route, still.
    pub fn reset(&mut self, car: &Car) {
        let Some(d) = self.drivers.get_mut(car.slot as usize) else { return };
        d.line = d.respawn_line.clone();
        (d.swerve, d.swerve_speed, d.swerve_accel, d.launch) = (0, 0, 0, 0);
        let at = d.line.target;
        (d.pos, d.prev_pos, d.base, d.prev_base) = (at, at, at, at);
        let rot = car.body.rot;
        d.rot = rot;
        d.ups = [col(&rot, 2); 3];
        d.travels = [col(&rot, 1); 3];
        d.up_at = 0;
        d.travel_at = 0;
        d.up = col(&rot, 2);
        // The game's own slip: the travel too from the up axis.
        d.travel = col(&rot, 2);
        (d.shove, d.drift, d.spin_shove, d.spin) = ([0; 3], [0; 3], [0; 3], [0; 3]);
        (d.knocked, d.knock_pace, d.knock_swerve, d.slip, d.lean) = (0, 0, 0, 0, 0);
    }

    /// 0x80040494, a computer car's part of cars_update: a reset asked for;
    /// where it last drove steadily on the road kept for the next; and its
    /// body run as a body once wrecked or finished.
    pub fn car_update(&mut self, t: &Tables, car: &mut Car, zone: Option<u16>, dt: i32) -> bool {
        let mut reset = false;
        if car.reset_requested {
            reset = true;
            car.reset_requested = false;
        }
        let steady = car.grounded == car.wheels.len() as u8
            && car.body.rot[2][2] > 4033
            && !car.wrecked
            && car.flags & 0x20 != 0;
        if steady || car.respawn.zone.is_none() {
            car.respawn.pos = car.body.pos;
            car.respawn.rot = car.body.rot;
            car.respawn.zone = zone;
            if let Some(d) = self.drivers.get_mut(car.slot as usize) {
                d.respawn_line = d.line.clone();
            }
        }
        if car.wrecked || car.laps.finished {
            car.body.integrate(t, dt);
            car.body.rot = t.orthonormalize(&car.body.rot);
        }
        reset
    }

    /// 0x8007265c: a step of `dt_ms` for every computer car.
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        t: &Tables,
        tuning: &crate::car::Tuning,
        stream: &[u8],
        cars: &mut [Car],
        dt_ms: i32,
        time: u32,
        rand: &mut Rand,
    ) {
        let a = tuning.ai;
        let fp = (dt_ms << 12) / 1000;
        let mirrored = self.mirrored;
        let rng = &mut self.rand;

        // A finished car is let go as a body, with the momentum it had.
        for (d, c) in self.drivers.iter_mut().zip(cars.iter_mut()) {
            if d.racing && c.laps.finished {
                let r = rng.below(5) as i32;
                d.racing = false;
                let k = div_fx(c.body.mass, fp);
                c.body.momentum = scale(sub(d.pos, d.prev_pos), k);
                let spin = fx((r - 4) << 12, c.body.mass);
                // Written as three 32-bit words over the 64-bit momentum: the
                // first component's two halves and the second's low half.
                let [x, y, z] = scale(c.ground.nearest.normal, spin);
                let am = &mut c.body.ang_momentum;
                am[0] = (x as u32 as i64) | ((y as i64) << 32);
                am[1] = (am[1] & !0xffff_ffff) | z as u32 as i64;
            }
        }

        // The step's speed.
        for (d, c) in self.drivers.iter_mut().zip(cars.iter()) {
            if !driving(c, d) {
                continue;
            }
            d.prev_pos = d.pos;
            d.prev_base = d.base;
            d.knock_pace = fx(d.knock_pace, k99());
            d.knock_swerve = fx(d.knock_swerve, k99());
            d.speed = fx(dt_ms << 12, div_fx(d.pace + 0x6_4000, 0x6_4000));
            d.speed = fx(d.speed, d.rubber);
            d.speed = fx(d.speed, div_fx(d.knock_pace + 0x6_4000, 0x6_4000));
            if d.launch < 4096 {
                d.launch += div_fx(0x5000, 0x6_4000);
                d.speed = fx(d.speed, d.launch);
            }
            if d.speed <= 0xcfff {
                d.speed = 0xd000;
            }
        }

        // Along the route, choosing at its branches; the base raised to the
        // car's ride height. The branch weights' counts carry on from car to
        // car through the step, as the game keeps them.
        let (mut plain, mut weight) = (0i32, 0i32);
        for (d, c) in self.drivers.iter_mut().zip(cars.iter()) {
            if !driving(c, d) {
                continue;
            }
            let ground = c.ground.nearest.found.then_some(c.ground.nearest.normal);
            let mut tries = 0;
            while !d.line.advance(t, stream, d.speed, ground, mirrored) {
                tries += 1;
                if tries > 64 {
                    tracing::warn!("car {}: the route's branch has no way on", d.car);
                    break;
                }
                let choices = &d.line.choices;
                let mut kind = [0u8; 2];
                let mut wt = [0i32; 2];
                for (i, ch) in choices.iter().enumerate().take(2) {
                    if ch.mask as u16 != 0 {
                        kind[i] = 2;
                    } else {
                        kind[i] = 1;
                        plain += 1;
                    }
                }
                if plain == 0 {
                    continue;
                }
                let mask = |i: usize| choices.get(i).map_or(0, |c| c.mask);
                for (i, kd) in kind.iter_mut().enumerate() {
                    if *kd == 2 && (c.handling.flags & (mask(i) & 0xffff)) == 0 {
                        *kd = 0;
                    }
                }
                for i in 0..2 {
                    if kind[i] == 0 {
                        continue;
                    }
                    if (d.swerve > 0 && mask(i) & 0x2000_0000 != 0) || (d.swerve < 0 && mask(i) & 0x4000_0000 != 0) {
                        wt[i] += 20;
                    }
                    if kind[i] == 2 {
                        let mut best = 0i32;
                        for b in 0..16 {
                            if c.handling.flags & (1 << b) & mask(i) != 0 {
                                let w = choices.get(i).map_or(0, |ch| ch.weights[b]) as i32;
                                if best < w {
                                    best = w;
                                }
                            }
                        }
                        wt[i] += best;
                    }
                }
                weight += wt[0] + wt[1];
                weight = weight.min(255);
                let rest = 255 - weight;
                for i in 0..2 {
                    if kind[i] == 1 {
                        wt[i] += rest / plain;
                    }
                }
                let mut r = rng.below(255) as i32;
                let mut choice = 0;
                for (i, &w) in wt.iter().enumerate() {
                    if r < w {
                        choice = i;
                        break;
                    }
                    r -= w;
                }
                d.line.choice = Some(choice);
            }
            let ride = c.handling.front.ride_height.max(c.handling.rear.ride_height);
            let h = c.origin[2].wrapping_add(ride).wrapping_sub(0x2_4000);
            d.base = add(d.line.target, scale(col(&d.rot, 2), h));
        }

        // The swerve.
        for (d, c) in self.drivers.iter_mut().zip(cars.iter()) {
            if !driving(c, d) || d.line.flags & 2 != 0 {
                continue;
            }
            match d.knocked {
                2 => {
                    if d.knock_swerve.wrapping_abs() < 4096 {
                        d.knock_swerve = 0;
                        d.knocked = 0;
                    }
                    if d.swerve_flip {
                        d.swerve_flip = false;
                        d.swerve_to = d.swerve_to.wrapping_neg();
                    }
                }
                0 if d.swerve.wrapping_sub(d.swerve_to).wrapping_abs() < 4096 => {
                    d.swerve_to = ((rng.below(201) as i32) - 100) << 12;
                    let limit = (a.swerve as i32) << 12;
                    if d.swerve.wrapping_abs() >= limit {
                        d.swerve = if d.swerve < 0 { -limit } else { limit };
                    }
                }
                _ => {}
            }
            let m = (a.edge as i32) << 12;
            let want = d.swerve_to.wrapping_add(d.knock_swerve).min(d.line.left - m).max(m - d.line.right);
            d.swerve_accel = want.wrapping_sub(d.swerve.wrapping_add(d.swerve_speed)).clamp(-0xc_8000, 0xc_8000);
            d.swerve_speed = d.swerve_speed.wrapping_add(fx(fp, d.swerve_accel)).clamp(-0x2_8000, 0x2_8000);
            d.swerve = d.swerve.wrapping_add(fx(fp, d.swerve_speed));
        }

        // Placed off the route by the swerve, and drifted by knocks.
        for (d, c) in self.drivers.iter_mut().zip(cars.iter()) {
            if !driving(c, d) {
                continue;
            }
            d.swerve = d.swerve.min(d.line.left).max(d.line.right.wrapping_neg());
            d.pos = add(d.base, scale(col(&d.rot, 0), d.swerve));
            let k_mass = div_fx(fp, c.handling.mass);
            d.drift = add(d.drift, scale(d.shove, k_mass));
            d.drift = scale(d.drift, k95());
            d.shove = scale(d.shove, k95());
            d.pos = add(d.pos, d.drift);
        }

        // Kept within the route's widths across the ground.
        for (d, c) in self.drivers.iter_mut().zip(cars.iter()) {
            if !driving(c, d) || d.stunt & stunt::UNDER_WAY != 0 || d.line.flags & 2 != 0 {
                continue;
            }
            let nrm = c.ground.nearest.normal;
            let off = sub(d.pos, d.base);
            let mut flat = sub(off, scale(nrm, dot(nrm, off)));
            let side = t.normalize(cross(sub(d.prev_base, d.base), nrm));
            let across = dot(side, flat);
            let right = d.line.right.wrapping_neg();
            if across < right {
                flat = scale(flat, div_fx(right, across));
            } else if d.line.left < across {
                flat = scale(flat, div_fx(d.line.left, across));
            }
            d.pos = add(d.base, flat);
        }

        // The body's world inertia, then the spin the knocks give.
        for (d, c) in self.drivers.iter().zip(cars.iter_mut()) {
            if !driving(c, d) {
                continue;
            }
            let rot = c.body.rot;
            c.body.inv_inertia_world = mul_64_16(&mul_16_64(&rot, &c.body.inv_inertia), &transpose(&rot));
        }
        for (d, c) in self.drivers.iter_mut().zip(cars.iter()) {
            if !driving(c, d) {
                continue;
            }
            let m = &c.body.inv_inertia_world;
            let w: Vec3 = std::array::from_fn(|i| {
                let s = (0..3).fold(0i64, |s, j| s.wrapping_add(m[i][j].wrapping_mul(d.spin_shove[j] as i64) >> 20));
                (s >> 8) as i32
            });
            for (s, w) in d.spin.iter_mut().zip(w) {
                *s = s.wrapping_add(((w as i64) << 8).wrapping_mul(fp as i64) >> 12);
            }
            d.spin_shove = scale(d.spin_shove, k95());
            for s in &mut d.spin {
                *s = s.wrapping_mul(k95() as i64) >> 12;
            }
        }

        // Into the car: its place, travel, speed and engine.
        for (d, c) in self.drivers.iter_mut().zip(cars.iter_mut()) {
            if c.laps.finished {
                c.accel = 0;
                c.engine.rpm = c.engine.idle;
            }
            if !driving(c, d) {
                continue;
            }
            c.body.pos = d.pos;
            c.body.vel = scale(sub(d.base, d.prev_base), div_fx(4096, fp));
            c.body.speed = t.length(c.body.vel);
            if 0x90_0000 < c.body.speed {
                c.body.vel = scale(c.body.vel, div_fx(0x90_0000, c.body.speed));
                c.body.speed = 0x90_0000;
            }
            let span = c.engine.redline.wrapping_sub(c.engine.idle);
            c.engine.rpm = c.engine.idle.wrapping_add(fx(c.body.speed / 2288, span));
            c.accel = div_fx(c.engine.rpm.wrapping_sub(c.engine.idle), span);
            c.grounded = if d.line.flags & 2 != 0 { 0 } else { c.wheels.len() as u8 };
        }

        // The lean the route's turns ask for, followed at its own pace.
        for (d, c) in self.drivers.iter_mut().zip(cars.iter()) {
            if !driving(c, d) {
                continue;
            }
            let x = fx(fx(c.body.speed / 2304, div_fx(c.body.mass, 0xa000)), fx(d.line.turn, fp));
            d.slip = fx(x, 0x3_2000).clamp(-1429, 1429);
            let (fast, slow) = (fx(fp, div_fx(0x4_1000, 0x6_4000)), fx(fp, div_fx(0xf000, 0x6_4000)));
            let rising = if d.lean < 0 { fast } else { slow };
            let falling = if d.lean < 0 { slow } else { fast };
            if d.lean < d.slip {
                if d.lean_way > 0 {
                    d.lean = d.lean.wrapping_add(rising);
                }
                d.lean_way = 1;
                if d.slip < d.lean {
                    d.lean = d.slip;
                }
            } else if d.slip < d.lean {
                if d.lean_way < 0 {
                    d.lean = d.lean.wrapping_sub(falling);
                }
                d.lean_way = -1;
                if d.lean < d.slip {
                    d.lean = d.slip;
                }
            }
        }

        // The up and travel directions, smoothed over three steps.
        for (d, c) in self.drivers.iter_mut().zip(cars.iter()) {
            if !driving(c, d) {
                continue;
            }
            let airborne = d.line.flags & 2 != 0;
            let stunting = d.stunt & stunt::UNDER_WAY != 0;
            let v = c.body.vel;
            d.travel = if stunting {
                col(&c.body.rot, 1)
            } else if v.iter().all(|c| c.wrapping_abs() < 4096) {
                col(&d.rot, 1)
            } else {
                v
            };
            d.up = if stunting || airborne {
                col(&c.body.rot, 2)
            } else if c.ground.nearest.found {
                c.ground.nearest.normal
            } else {
                [0, 0, 4096]
            };
            d.travels[d.travel_at as usize % 3] = d.travel;
            d.travel_at = if d.travel_at + 1 < 3 { d.travel_at + 1 } else { 0 };
            d.ups[d.up_at as usize % 3] = d.up;
            d.up_at = if d.up_at + 1 < 3 { d.up_at + 1 } else { 0 };
            if !stunting && !airborne {
                let third = div_fx(4096, 0x3000);
                d.travel = d.travels.iter().fold([0; 3], |s, v| add(s, scale(*v, third)));
                d.up = d.ups.iter().fold([0; 3], |s, v| add(s, scale(*v, third)));
            }
        }

        // The orientation: built from them, turned by the spin, made
        // orthonormal, leant into the turn; into the car.
        for (d, c) in self.drivers.iter_mut().zip(cars.iter_mut()) {
            if !driving(c, d) || d.stunt & stunt::UNDER_WAY != 0 || d.line.flags & 2 != 0 {
                continue;
            }
            if d.up == [0; 3] || d.travel == [0; 3] {
                continue;
            }
            let up = t.normalize(d.up);
            let travel = t.normalize(d.travel);
            let right = cross(travel, up);
            let forward = cross(up, right);
            d.rot = set_cols([right, forward, up]);
        }
        for (d, c) in self.drivers.iter_mut().zip(cars.iter()) {
            if !driving(c, d) || d.stunt & stunt::UNDER_WAY != 0 || d.line.flags & 2 != 0 {
                continue;
            }
            let [wx, wy, wz] = d.spin;
            let w = [[0, wz.wrapping_neg(), wy], [wz, 0, wx.wrapping_neg()], [wy.wrapping_neg(), wx, 0]];
            let turn = mul_64_16(&w, &d.rot);
            for (row, trow) in d.rot.iter_mut().zip(turn) {
                for (e, x) in row.iter_mut().zip(trow) {
                    *e = e.wrapping_add((x >> 8) as i16);
                }
            }
        }
        for (d, c) in self.drivers.iter_mut().zip(cars.iter()) {
            if !driving(c, d) || d.stunt & stunt::UNDER_WAY != 0 || d.line.flags & 2 != 0 {
                continue;
            }
            let c0 = t.normalize(col(&d.rot, 0));
            let c2 = t.normalize(cross(c0, col(&d.rot, 1)));
            let c1 = t.normalize(cross(c2, c0));
            d.rot = set_cols([c0, c1, c2]);
        }
        for (d, c) in self.drivers.iter_mut().zip(cars.iter_mut()) {
            if !driving(c, d) || d.stunt & stunt::UNDER_WAY != 0 || d.line.flags & 2 != 0 {
                continue;
            }
            let v0 = d.lean.wrapping_shl(12);
            let hi = ((v0 as i64 * 0xa2f9_6525u32 as i32 as i64) >> 32) as i32;
            let angle = ((hi.wrapping_add(v0) >> 14).wrapping_sub(v0 >> 31)) & 4095;
            let mut lean: Matrix = [[4096, 0, 0], [0, 4096, 0], [0, 0, 4096]];
            t.turn_rows(angle, &mut lean);
            let identity: Matrix = [[4096, 0, 0], [0, 4096, 0], [0, 0, 4096]];
            d.rot = gte_mul(&d.rot, &gte_mul(&identity, &lean));
            c.body.rot = d.rot;
        }

        // Stunts over jumps: chosen as one begins, by the car's skill and its
        // air control; a botched one wrecks the car as it lands.
        for (d, c) in self.drivers.iter_mut().zip(cars.iter_mut()) {
            if !driving(c, d) {
                continue;
            }
            if d.line.flags & 2 == 0 {
                if d.stunt & stunt::BOTCHED != 0 {
                    c.wreck(false, rand);
                }
                d.stunt = 0;
                continue;
            }
            d.jumped = true;
            if d.stunt != 0 {
                continue;
            }
            let roll = rng.below(100) as i32;
            d.stunt_ms = d.line.air;
            let mut which = rng.below(3);
            let skilled = roll < d.skill;
            if skilled && which == 1 {
                which = (rng.draw() & 1) << 1;
            }
            let (axis, power) = match which {
                0 => (stunt::FLIP, c.handling.air_power.pitch),
                1 => (stunt::SPIN, c.handling.air_power.yaw),
                _ => (stunt::ROLL, c.handling.air_power.roll),
            };
            let (mut once, mut twice, mut thrice) = (0, 0, 0);
            if power != 0 {
                let need = |turns: i32| div_fx(div_fx(turns, 0xa000), power);
                once = need(0xf000);
                twice = need(0x1_9000);
                thrice = need(0x2_3000);
            }
            if once < d.stunt_ms {
                if rng.below(2) == 0 || skilled {
                    d.stunt = axis | stunt::UNDER_WAY;
                    if skilled {
                        d.stunt |= stunt::BOTCHED;
                    }
                    if rng.below(2) == 0 {
                        d.stunt |= stunt::BACKWARD;
                    }
                    if twice < d.stunt_ms {
                        if rng.below(2) == 0 {
                            d.stunt |= stunt::DOUBLE;
                        } else if thrice < d.stunt_ms && rng.below(2) == 0 {
                            d.stunt |= stunt::TRIPLE;
                        }
                    }
                    d.stunt_rot = c.body.rot;
                } else {
                    d.stunt = stunt::NONE;
                }
            } else {
                d.stunt = stunt::NONE;
            }
        }
        for (d, c) in self.drivers.iter_mut().zip(cars.iter_mut()) {
            if !driving(c, d) || d.stunt & stunt::UNDER_WAY == 0 {
                continue;
            }
            let done = 4096 - div_fx(d.line.air, d.stunt_ms);
            let ease = t.cos(fx(done, 0x3244));
            let mut angle = fx(fx(4096 - ease, 2048), fx(0x2000, 0x3244));
            if d.stunt & stunt::BACKWARD != 0 {
                angle = -angle;
            }
            let mut turns = if d.stunt & stunt::TRIPLE != 0 {
                0x3000
            } else if d.stunt & stunt::DOUBLE != 0 {
                0x2000
            } else {
                4096
            };
            if d.stunt & stunt::BOTCHED != 0 {
                turns += 2048;
            }
            angle = fx(angle, turns);
            let (cs, sn) = (t.cos(angle), t.sin(angle));
            let mut m = [[4096i32, 0, 0], [0, 4096, 0], [0, 0, 4096]];
            if d.stunt & stunt::FLIP != 0 {
                m[1][1] = cs;
                m[1][2] = -sn;
                m[2][1] = sn;
                m[2][2] = cs;
            } else if d.stunt & stunt::SPIN != 0 {
                m[0][0] = cs;
                m[0][1] = -sn;
                m[1][0] = sn;
                m[1][1] = cs;
            } else if d.stunt & stunt::ROLL != 0 {
                m[0][0] = cs;
                m[0][2] = sn;
                m[2][0] = -sn;
                m[2][2] = cs;
            }
            let m = m.map(|row| row.map(|v| v as i16 as i32));
            let r = d.stunt_rot;
            c.body.rot = std::array::from_fn(|i| {
                std::array::from_fn(|j| (0..3).fold(0i32, |s, q| s.wrapping_add(fx(r[i][q] as i32, m[q][j]))) as i16)
            });
        }

        // The wheels: steered by the route's turn against the lean, spun by
        // the speed.
        for (d, c) in self.drivers.iter().zip(cars.iter_mut()) {
            if !driving(c, d) {
                continue;
            }
            if c.grounded != 0 {
                let lock = div_fx(0x3244, 0x6000);
                let s = div_fx(d.line.turn, 0x2000).wrapping_sub(fx(d.lean, 0x2400));
                c.steer = s.min(lock).max(-lock).wrapping_neg();
                if c.handling.rear_steers {
                    c.steer = fx(c.steer, 1024);
                }
            } else {
                c.steer = 0;
            }
            let speed = c.body.speed;
            for w in &mut c.wheels {
                w.spin_rate = fx(0x2000, div_fx(speed, w.diameter));
            }
        }

        // The rubber band, now and then: slowed while ahead of the player by
        // more than the band, hurried while behind, settling between.
        let leader = self.leader.map(|s| s as usize);
        let progress: Vec<i32> = self.drivers.iter().map(|d| d.progress).collect();
        for (d, c) in self.drivers.iter_mut().zip(cars.iter()) {
            if !driving(c, d) {
                continue;
            }
            let every = (a.rubber_every as u32) * 10;
            if every >= time.wrapping_sub(d.rubber_at) {
                continue;
            }
            d.rubber_at = time;
            if self.players != 1 {
                d.rubber = 4096;
                continue;
            }
            let lead = leader.and_then(|l| progress.get(l)).copied().unwrap_or(0);
            let mut gap = lead.wrapping_sub(d.progress);
            if (c.laps.done as i32) >= self.laps as i32 - 1 {
                gap = gap.wrapping_add(self.last_lap);
            }
            let step = |p: u8| div_fx((p as i32) << 12, 0x6_4000);
            if self.lead + self.band < gap {
                d.rubber -= step(a.rubber_ahead);
            } else if gap < self.lead - self.band {
                d.rubber += step(a.rubber_behind);
            } else if d.rubber > 0 {
                d.rubber -= step(a.rubber_settle_down);
            } else {
                d.rubber += step(a.rubber_settle_up);
            }
            d.rubber = d.rubber.max(d.rubber_min).min(d.rubber_max);
        }
    }

    /// 0x8007937c, after the step: each car's race left to run (a player's
    /// from its lap distance); the knocks the collision code gave computer
    /// cars' bodies taken into their drivers: momentum (less its part into
    /// the ground) as a shove and a change of pace and swerve, spin as a spin
    /// shove.
    pub fn handoff(&mut self, cars: &mut [Car], tuning: &crate::car::Tuning) {
        for (d, c) in self.drivers.iter_mut().zip(cars.iter()) {
            if c.state != DRIVEN {
                d.line.distance = (c.lap_distance << 12) / 1000;
            }
            let left = (self.laps as i32 - 1 - c.laps.done as i32) << 12;
            d.progress = fx(left, self.lap).wrapping_add(d.line.distance);
        }
        let a = tuning.ai;
        for (d, c) in self.drivers.iter_mut().zip(cars.iter_mut()) {
            if c.state != DRIVEN || c.wrecked || c.laps.finished {
                continue;
            }
            if c.body.momentum != [0; 3] {
                let nrm = c.ground.nearest.normal;
                let into = dot(nrm, c.body.momentum);
                c.body.momentum = sub(c.body.momentum, scale(nrm, into));
                d.shove = add(c.body.momentum, d.shove);
                knock(d, c, &a);
                c.body.momentum = [0; 3];
            }
            if c.body.ang_momentum != [0; 3] {
                // Each component's low word.
                for (s, m) in d.spin_shove.iter_mut().zip(c.body.ang_momentum) {
                    *s = (m as i32).wrapping_add(*s);
                }
                c.body.ang_momentum = [0; 3];
            }
        }
    }
}

/// 0x80078ef8: a knock along the car's axes: sideways moves its swerve,
/// forward and back its pace (back also turns its swerve about), from below
/// or above its pace.
fn knock(d: &mut Driver, c: &Car, a: &AiTuning) {
    let m = c.body.momentum;
    let along = |j: usize| dot(m, col(&c.body.rot, j));
    let (side, fwd, up) = (along(0), along(1), along(2));
    let b = |v: u8| (v as i32) << 12;
    if side > 0 {
        d.knock_swerve = (d.knock_swerve + b(a.knock_side)).min(0x5_0000);
    } else {
        d.knock_swerve = (d.knock_swerve - b(a.knock_side)).max(-0x5_0000);
    }
    let (max, min) = (b(a.knock_pace_max), (a.knock_pace_min as i32 - 100) << 12);
    if fwd > 0 {
        d.knock_pace = (d.knock_pace + b(a.knock_forward)).min(max);
    } else {
        d.knock_pace = (d.knock_pace - b(a.knock_back)).max(min);
        d.swerve_flip = true;
    }
    if up > 0 {
        d.knock_pace = (d.knock_pace - b(a.knock_over)).max(min);
    } else {
        d.knock_pace = (d.knock_pace + b(a.knock_under)).min(max);
    }
    d.knocked = 2;
}
