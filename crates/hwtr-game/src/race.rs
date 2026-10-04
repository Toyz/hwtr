//! The race as the front end sets it up: the track, the cars and who drives
//! them, the difficulty (the record `race_load`, 0x80033304, takes; at
//! 0x80138c94 in the original).

/// Who drives a car.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Driver {
    Computer,
    PlayerOne,
    PlayerTwo,
    /// A value the game does not use; kept as it was.
    Other(u8),
}

impl Driver {
    pub fn from_byte(b: u8) -> Driver {
        match b {
            0 => Driver::Computer,
            1 => Driver::PlayerOne,
            2 => Driver::PlayerTwo,
            b => Driver::Other(b),
        }
    }

    pub fn byte(self) -> u8 {
        match self {
            Driver::Computer => 0,
            Driver::PlayerOne => 1,
            Driver::PlayerTwo => 2,
            Driver::Other(b) => b,
        }
    }

    pub fn is_player(self) -> bool {
        matches!(self, Driver::PlayerOne | Driver::PlayerTwo)
    }
}

/// One car in the race.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entrant {
    /// The car's file name (up to 8 characters: "spltimg", "mongoose").
    pub name: String,
    pub driver: Driver,
    /// The car's number in the game's car table.
    pub car_id: u8,
    /// Which player's pad drives it.
    pub player: u8,
    /// Its place on the start grid.
    pub grid: u8,
}

/// The worlds, as the front end names them (0x800c5c24).
pub const WORLDS: [&str; 4] = ["Desert", "Glacial", "Haunted", "Volcano"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaceSetup {
    /// Bit 6: the mirrored track.
    pub flags: u32,
    /// "Desert", "Glacial", "Volcano", "Haunted", and which of its three.
    pub track: String,
    pub track_number: u8,
    /// Laps to run, and checkpoints a lap.
    pub laps: u8,
    pub checkpoints: u8,
    /// Bit 2: cars at a third of their size; bit 5: wheels at half.
    pub options: u32,
    /// A race against the clock (flag 4): its limit, ms.
    pub time_limit: u32,
    pub cars: Vec<Entrant>,
    /// 0 to 255.
    pub difficulty: u8,
}
use crate::car::{Car, Controls, EngineSpec, Handling, Tuning};
use crate::camera::{Camera, Surroundings};
use crate::car::Respawn;
use crate::car::stunt::Award;
use crate::car::update::Drive;
use crate::hud::Hud;
use crate::laps::{Course, LapEvent, Laps};
use crate::line::BestLine;
use crate::math::div_fx;
use crate::collision::world::Step;
use crate::collision::{Collision, Scp};
use crate::rand::Rand;
use crate::math::Tables;

/// One race step: the game logic runs at 40 steps a second (`race_frame`,
/// 0x80033ed8), whatever the display does.
pub const STEP_MS: i32 = 25;

/// Where a race is (0x800d0de9).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    /// The flyby over the track, then the countdown; the cars wait.
    #[default]
    Starting,
    /// Under way.
    Racing,
    /// Over: the results.
    Finished,
}

/// What happened in a race that the sounds and the HUD show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaceEvent {
    /// The countdown calls 3, 2, 1 (sounds 6 to 8).
    Count(u8),
    /// The start (sound 9).
    Go,
    /// Car `car` passed a checkpoint.
    Lap { car: u8, event: LapEvent },
    /// The race is over; the results follow.
    Finish,
    /// The results have been up four seconds: the race stands still.
    Results,
}

/// A car's result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Standing {
    pub car: u8,
    /// Its race time (0 if it did not finish) and best lap, milliseconds.
    pub time: u32,
    pub best: u32,
    /// The championship points its place earns.
    pub points: u8,
}

/// The front end's actions race_frame reads besides the driving: held on a
/// player's pad.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Buttons {
    /// Action 18 (Cross): skips the flyby, leaves the results.
    pub accept: bool,
    /// Action 26 (Start): leaves the results.
    pub start: bool,
}

/// When the countdown calls 3, 2 and 1, and starts the race, after the
/// flyby: milliseconds.
const COUNTDOWN: [(u32, u8); 3] = [(1200, 3), (2550, 2), (3800, 1)];
const GO_MS: u32 = 5000;
/// The longest race: past this the race ends, milliseconds.
const LONGEST_MS: u32 = 0x1b_773f;
/// How long the results run before the race stands still, and before they
/// leave by themselves.
const RESULTS_MS: u32 = 4000;
const RESULTS_LEAVE_MS: u32 = 30_000;
/// The championship points for first to sixth.
const POINTS: [u8; 6] = [10, 8, 7, 6, 5, 4];

/// A race in progress: what is ported of it so far. The players' cars
/// drive on the track; computer cars and the rest follow.
pub struct Race {
    pub setup: RaceSetup,
    pub tables: Tables,
    pub tuning: Tuning,
    pub cars: Vec<Car>,
    pub collision: Collision,
    /// The game's random numbers.
    pub rand: Rand,
    /// The race clock (0x800d0e34), 25 ms a step.
    pub time: u32,
    /// The system clock (0x800d240c), which the host advances 17 ms a
    /// vertical blank.
    pub clock: u32,
    /// One camera a player.
    pub cameras: Vec<Camera>,
    /// The track's best line, where reset cars are put back.
    pub line: BestLine,
    /// The last stunt landed by each car, for the HUD.
    pub stunts: Vec<Option<Award>>,
    pub phase: Phase,
    /// When the flyby ends and the countdown starts, by the race clock
    /// (0x800d2600).
    pub countdown_from: u32,
    /// The last number the countdown called.
    pub called: Option<u8>,
    /// How long the race ran before the start (0x800d0e38), when the clock
    /// went back to 0.
    pub before_start: u32,
    /// The results: when they began, by the system clock (0x800d25e8); the
    /// race standing still (0x800d2607); Cross let go since (0x800d2609);
    /// the standings, best first.
    pub results_from: u32,
    pub frozen: bool,
    pub accept_released: bool,
    pub standings: Vec<Standing>,
    /// Done: back to the front end (0x800d261c).
    pub over: bool,
    /// What happened since the host last took them.
    pub events: Vec<RaceEvent>,
    /// The HUD, and the cars by place (0x800d264c).
    pub hud: Hud,
    pub order: Vec<u8>,
}

/// How many of the views a player cycles through (the fifth, the side view,
/// is not offered).
const VIEWS_OFFERED: u8 = 4;

impl Race {
    /// Starts the race `setup` on the track `scp`, with each car's handling
    /// (in the setup's order). Only the players' cars are built so far.
    pub fn new(
        setup: RaceSetup,
        scp: Scp,
        line: BestLine,
        handling: &[(Handling, EngineSpec)],
        tables: Tables,
        tuning: Tuning,
    ) -> Race {
        let course = Course::new(&setup, &scp, line.lap_length);
        let mut collision = Collision::new(scp);
        collision.course = course;
        let mut cars = Vec::new();
        for (slot, entrant) in setup.cars.iter().enumerate() {
            let Some((h, spec)) = handling.get(slot) else { break };
            let grid = collision.scp.grid[entrant.grid as usize % collision.scp.grid.len()];
            let mut car = Car::load(slot as u8, entrant, &setup, (h, spec), grid, &tuning);
            if entrant.driver.is_player() {
                collision.add_car(&tables, &mut car);
            } else {
                // Computer cars wait on the grid until their driving is
                // ported: no collision object, but their zone's effects (the
                // lap distance their place counts from).
                tracing::trace!("car {slot}: computer cars (0x80040494, 0x8007c6fc) not yet ported");
                let zone = collision.scp.zone_at(crate::math::add(car.body.pos, car.body.centre));
                collision.zone_effects(&mut car, zone, None);
            }
            cars.push(car);
        }
        // The cameras fly over the track first (0x8003ac30); the countdown
        // starts when the flyby ends (0x8003abec).
        let mut cameras: Vec<Camera> =
            cars.iter().filter(|c| c.flags & 3 != 0).map(|c| Camera::new(c.slot, &tables.views.one)).collect();
        for camera in &mut cameras {
            camera.flyby = true;
        }
        let countdown_from = (collision.scp.flyby.len() as u32).saturating_sub(1) * 200;
        let stunts = vec![None; cars.len()];
        let hud = Hud::new(&setup, tables.meter.clone());
        let order = (0..cars.len() as u8).collect();
        Race {
            setup,
            tables,
            tuning,
            cars,
            collision,
            rand: Rand::default(),
            time: 0,
            clock: 0,
            cameras,
            line,
            stunts,
            phase: Phase::Starting,
            countdown_from,
            called: None,
            before_start: 0,
            results_from: 0,
            frozen: false,
            accept_released: false,
            standings: Vec::new(),
            over: false,
            events: Vec::new(),
            hud,
            order,
        }
    }

    /// 0x80041384: puts car `slot` back on the road: player one's at the
    /// first clear point of the best line from where it was along the lap
    /// (when the line has one and it has a heading), others at their saved
    /// respawn point; still, unwrecked, its timers and forces cleared, its
    /// object in the respawn zone, a free turbo if it had none, and two
    /// seconds' grace from other cars.
    pub fn reset_car(&mut self, slot: usize) {
        let others: Vec<_> = self
            .cars
            .iter()
            .enumerate()
            .filter(|&(k, _)| k != slot)
            .map(|(_, c)| crate::math::add(c.body.pos, c.body.centre))
            .collect();
        let t = &self.tables;
        let car = &mut self.cars[slot];
        if car.flags & 1 != 0
            && let Some(point) = self.line.free_point(car.lap_distance as u32, &others)
        {
            let forward = point.heading;
            let right = crate::math::cross(forward, [0, 0, 0x1000]);
            if div_fx(0x1000, 10 << 12) < t.length(right) {
                let right = t.normalize(right);
                let up = crate::math::cross(right, forward);
                let mut rot = [[0i16; 3]; 3];
                for (i, row) in rot.iter_mut().enumerate() {
                    *row = [right[i] as i16, forward[i] as i16, up[i] as i16];
                }
                car.respawn = Respawn { pos: point.pos, rot, zone: Some(point.zone) };
            }
        }
        let body = &mut car.body;
        body.pos = car.respawn.pos;
        body.rot = car.respawn.rot;
        body.asleep = false;
        body.sleep_count = 0;
        car.wrecked = false;
        car.wreck_ms = 0;
        car.flags &= !0x3800;
        tracing::trace!("car {slot}: the wreck's debris cleared, the reset's effects, not yet ported");
        body.ang_momentum = [0; 3];
        body.torque = [0; 3];
        body.momentum = [0; 3];
        body.vel = [0; 3];
        body.spin = [0; 3];
        body.force = [0; 3];
        if let Some(zone) = car.respawn.zone {
            self.collision.move_to_zone(slot as u8, zone, car);
        }
        car.airborne = false;
        if car.flags & 1 != 0
            && let Some(camera) = self.cameras.iter_mut().find(|c| c.car as usize == slot)
        {
            camera.snap = true;
        }
        car.righting = [0; 3];
        if car.turbos == 0 {
            car.add_turbos(1);
        }
        tracing::trace!("car {slot}: its power-ups ended, not yet ported");
        car.reset_grace_ms = 2000;
        if let Some(obj) = self.collision.objects.iter_mut().find(|o| o.car == Some(slot as u8)) {
            obj.flags |= 1;
        }
        for wheel in &mut car.wheels {
            wheel.on_ground = false;
        }
        car.air_lock.active = false;
    }

    /// One step of `STEP_MS` (race_frame's inner loop), with each player's
    /// controls (in car order): before the results the controls go to the
    /// cars; once under way the cars and the collision step; the cameras
    /// step until the race stands still; the race clock always runs.
    pub fn step(&mut self, controls: &[Controls]) {
        let dt = (STEP_MS << 12) / 1000;
        if self.phase != Phase::Finished {
            for (car, c) in self.cars.iter_mut().zip(controls) {
                car.apply_controls(c, &self.tuning);
            }
        }
        if !self.frozen {
            let racing = self.phase != Phase::Starting;
            if racing {
                self.cars_update(dt);
                let mut step =
                    Step { tuning: &self.tuning, rand: &mut self.rand, time: self.time, clock: self.clock };
                self.collision.update(&self.tables, &mut self.cars, &mut step);
                let laps = self.collision.lap_events.iter().filter(|(slot, _)| {
                    self.cars.get(*slot as usize).is_some_and(|c| c.flags & 1 != 0) && !self.collision.course.quiet
                });
                let laps: Vec<_> = laps.copied().collect();
                for (car, event) in laps {
                    if let LapEvent::Lap { time, best } = event
                        && let Some(player) = self.cameras.iter().position(|c| c.car == car)
                    {
                        self.hud.lap_done(player, time, best, self.time);
                    }
                    self.events.push(RaceEvent::Lap { car, event });
                }
                tracing::trace!("the race's effects and debris (0x8006b754, 0x8007c894): not yet ported");
            }
            let around = Surroundings {
                tables: &self.tables,
                tuning: &self.tuning,
                views: &self.tables.views.one,
                count: VIEWS_OFFERED,
                racing,
                time: self.time,
                flyby: &self.collision.scp.flyby,
                collision: &self.collision,
            };
            for (camera, c) in self.cameras.iter_mut().zip(controls) {
                camera.button = c.view;
                if let Some(car) = self.cars.get_mut(camera.car as usize) {
                    camera.step(&around, car, STEP_MS as u32, &mut self.rand);
                }
            }
        }
        self.time = self.time.wrapping_add(STEP_MS as u32);
    }

    /// The rest of race_frame (0x80033ed8), once a display frame after its
    /// steps, with the front end's buttons: the race's phase moves on.
    pub fn frame(&mut self, buttons: Buttons) {
        match self.phase {
            Phase::Starting => self.starting(buttons),
            Phase::Racing => {
                if self.race_over() {
                    self.finish();
                }
            }
            Phase::Finished => self.results(buttons),
        }
    }

    /// Before the start: the flyby until its time is up (Cross cuts it
    /// short), then the cameras sweep down to the cars (0x8003ac6c) while
    /// the countdown calls 3, 2, 1 (one a frame at most) and starts the
    /// race.
    fn starting(&mut self, buttons: Buttons) {
        if self.time >= self.countdown_from {
            for camera in self.cameras.iter_mut().filter(|c| c.flyby) {
                camera.intro_ms = crate::camera::INTRO_MS;
                camera.flyby = false;
            }
        } else if buttons.accept {
            self.countdown_from = self.time;
        }
        for (at, number) in COUNTDOWN {
            if self.time >= self.countdown_from.wrapping_add(at) && self.called.is_none_or(|n| n > number) {
                self.called = Some(number);
                self.events.push(RaceEvent::Count(number));
                return;
            }
        }
        if self.time >= self.countdown_from.wrapping_add(GO_MS) {
            self.go();
        }
    }

    /// The start (0x80033a78 with 0x80061264): the race clock goes back to
    /// 0 and every car's laps start.
    fn go(&mut self) {
        tracing::trace!("the replay's snapshots (0x8007feb0): not yet ported");
        self.events.push(RaceEvent::Go);
        self.before_start = self.time;
        self.time = 0;
        for car in &mut self.cars {
            car.laps = Laps::new(&self.collision.course, self.time);
        }
        self.phase = Phase::Racing;
        self.called = None;
    }

    /// 0x8003485c: the race is over past its longest time, or when every
    /// player's car has run its laps (each then coasts, 0x800617c8). (The
    /// races with a time limit are not yet ported.)
    fn race_over(&mut self) -> bool {
        if self.time > LONGEST_MS {
            return true;
        }
        let mut all = true;
        for car in self.cars.iter_mut().filter(|c| c.flags & 1 != 0) {
            if car.laps.finished {
                car.coast();
            } else {
                all = false;
            }
        }
        all
    }

    /// The race ends: the standings (0x80033aa0) and every racing car
    /// coasting.
    fn finish(&mut self) {
        tracing::trace!("the replay's snapshots (0x8007feb0): not yet ported");
        self.results_from = self.clock;
        self.accept_released = false;
        self.standings = self.standings();
        self.phase = Phase::Finished;
        self.events.push(RaceEvent::Finish);
        for car in self.cars.iter_mut().filter(|c| c.flags & 3 != 0) {
            car.coast();
        }
    }

    /// 0x80033aa0's standings: each car's race time (the end of its last
    /// lap) and best lap, ordered by time with those without one last, and
    /// the points for the first six with a time. (Computer cars short of the
    /// line, whose times the original estimates, are not yet ported.)
    fn standings(&self) -> Vec<Standing> {
        let laps = self.collision.course.laps as usize;
        let mut standings: Vec<Standing> = self
            .cars
            .iter()
            .map(|c| Standing {
                car: c.slot,
                time: laps.checked_sub(1).and_then(|k| c.laps.ends.get(k)).copied().unwrap_or(0),
                best: c.laps.best,
                points: 0,
            })
            .collect();
        standings.sort_by_key(|s| (s.time == 0, s.time));
        for (place, s) in standings.iter_mut().enumerate() {
            s.points = if s.time != 0 { POINTS.get(place).copied().unwrap_or(0) } else { 0 };
        }
        standings
    }

    /// The results: after four seconds the race stands still (the replay
    /// that then plays is not yet ported); they leave after thirty, or on
    /// Cross pressed afresh, or on Start.
    fn results(&mut self, buttons: Buttons) {
        if !self.accept_released {
            self.accept_released = !buttons.accept;
        }
        let since = self.clock.wrapping_sub(self.results_from);
        if since <= RESULTS_MS {
            return;
        }
        if !self.frozen {
            self.events.push(RaceEvent::Results);
        }
        self.frozen = true;
        if since > RESULTS_LEAVE_MS || (self.accept_released && buttons.accept) || buttons.start {
            self.over = true;
        }
    }

    /// The countdown's number showing, if any.
    pub fn countdown(&self) -> Option<u8> {
        (self.phase == Phase::Starting).then_some(self.called).flatten()
    }

    /// `cars_update` (0x8004064c): each car's timers, then its update by its
    /// state (players' cars under full physics; computer cars not yet).
    fn cars_update(&mut self, dt: i32) {
        for slot in 0..self.cars.len() {
            if self.cars[slot].run_timers(STEP_MS as u32)
                && let Some(obj) = self.collision.objects.iter_mut().find(|o| o.car == Some(slot as u8))
            {
                obj.flags &= !1;
            }
            if self.cars[slot].state != 2 {
                tracing::trace!("car {slot}: state {} not yet ported", self.cars[slot].state);
                continue;
            }
            if self.cars[slot].wants_reset() {
                self.reset_car(slot);
                self.cars[slot].reset_requested = false;
            }
            let zone = self
                .collision
                .objects
                .iter()
                .find(|o| o.car == Some(slot as u8))
                .map_or((None, false), |o| (o.zones.iter().next(), o.zones.entries.len() == 1));
            let mut drive = Drive {
                tables: &self.tables,
                tuning: &self.tuning,
                rand: &mut self.rand,
                time: self.time,
                dt,
                scoring: self.setup.flags & 2 != 0,
                endless_turbo: false,
            };
            let before = self.cars[slot].turbos;
            let stepped = self.cars[slot].update(&mut drive, zone);
            if let Some(award) = stepped.stunt {
                if let Some(player) = self.cameras.iter().position(|c| c.car as usize == slot) {
                    self.hud.turbos_given(player, before, award.turbos, self.time);
                }
                self.stunts[slot] = Some(award);
            }
        }
        tracing::trace!("the computer cars' driving (0x80078ed8, 0x8007937c): not yet ported");
        self.rank();
        tracing::trace!("the players' 0x8005c9b4: not yet ported");
    }

    /// 0x800408cc: the cars' places: more laps run first, then the lesser
    /// lap distance, the order kept between steps.
    fn rank(&mut self) {
        let key = |c: &Car| (c.lap_distance as u32).wrapping_add((127u32.wrapping_sub(c.laps.done as u32)) << 24);
        let n = self.order.len();
        for i in 0..n.saturating_sub(1) {
            for j in i + 1..n {
                let (a, b) = (self.order[i] as usize, self.order[j] as usize);
                if key(&self.cars[b]) < key(&self.cars[a]) {
                    self.order.swap(i, j);
                }
            }
        }
        for (place, &slot) in self.order.iter().enumerate() {
            self.cars[slot as usize].laps.place = place as u8;
        }
    }

    /// What player `player`'s HUD draws now: the race HUD while the player
    /// races (the results' screen is not yet ported).
    pub fn hud(&mut self, player: usize) -> Vec<crate::hud::Sprite> {
        let Some(slot) = self.cameras.get(player).map(|c| c.car as usize) else { return Vec::new() };
        match self.cars.get(slot) {
            Some(car) if self.phase == Phase::Racing && !car.laps.finished => self.hud.draw(player, car, self.time),
            _ => Vec::new(),
        }
    }
}
