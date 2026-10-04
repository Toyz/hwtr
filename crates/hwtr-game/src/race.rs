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
    /// A best line other than the track's (+13; "tcup" for the TwinMill
    /// Cup).
    pub best_line: Option<String>,
    /// The players' names (0x8009b5b8 copies them to 0x801399b0 for each
    /// race the players start), which the results show.
    pub names: [String; 2],
}
use crate::car::{Car, Controls, EngineSpec, Handling, Tuning};
use crate::camera::{Camera, Surroundings};
use crate::car::Respawn;
use crate::car::stunt::Award;
use crate::car::update::Drive;
use crate::ai::Ai;
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
    /// A player's car was wrecked (sound 29).
    Wreck { car: u8 },
    /// Any car was wrecked: its engine stops (0x80016004).
    Wrecked { car: u8 },
    /// A car was put back on the road: its engine starts (0x80015ebc).
    Reset { car: u8 },
    /// The pause menu came up (sound 14), and was left to go on.
    Pause,
    Resume,
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
    /// A player's car took a power-up (sound 24).
    PowerUp { car: u8 },
    /// A world object was knocked over, with its sound (0x80036270).
    Knock { sound: u8, at: crate::math::Vec3 },
}

/// A world volume as the race keeps it: the object drawn for it, whether
/// its box follows that object, its knock's sound, and its collision
/// object.
#[derive(Clone, Copy, Debug)]
pub struct WorldVolume {
    pub object: Option<usize>,
    pub follows: bool,
    pub sound: Option<u8>,
    pub collision: usize,
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
/// How long the results show each snapshot.
const SNAPSHOT_MS: u32 = 2000;
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
    /// The computer cars' drivers.
    pub ai: Ai,
    /// The race clock (0x800d0e34), 25 ms a step.
    pub time: u32,
    /// The system clock (0x800d240c), which the host advances 17 ms a
    /// vertical blank.
    pub clock: u32,
    /// The players' pads' motors, and whether each player has vibration on.
    pub motors: [crate::pad::Motors; 2],
    pub vibration: [bool; 2],
    /// The pause menu's makings, the menu while it is up, and Start's latch
    /// (0x800d260b: let go since the last pause) and the pause it asks for
    /// (0x800d260c).
    pub pause_kit: Option<crate::pause::PauseKit>,
    pub paused: Option<crate::pause::Pause>,
    /// The sound volumes (the settings', the pause menu changes them), the
    /// effects' level last set (0x800d24c8 from 0 to 255), and a new one
    /// for the sound to take.
    pub volumes: crate::pause::Volumes,
    pub effects_level: u8,
    pub effects_asked: Option<u8>,
    /// The CD's song (0x800d2484), which the pause menu's Boom Box shows
    /// and changes.
    pub song: u8,
    /// The CD asked for: the pause menu's, and its own as the menu comes
    /// up (0x80036634 holds the song) and goes (0x80036758 goes on).
    pub cd: Vec<crate::cd::CdAsk>,
    start_free: bool,
    pause_asked: bool,
    /// How the race ends for the front end (0x800d0de8): 1 run to the end,
    /// 2 restarted, 3 aborted.
    pub end: u8,
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
    /// The pickups and the cars' power-ups, and the track's moving objects.
    pub power_ups: crate::powerup::PowerUps,
    pub anims: crate::world_anim::WorldAnims,
    /// The world's collision volumes, and the objects knocked over (no
    /// longer drawn: 0x80020824 clears their visible bit).
    pub world_volumes: Vec<WorldVolume>,
    pub knocked: Vec<usize>,
    /// The moments kept for the results, and the results' words.
    pub snapshots: crate::snapshot::Snapshots,
    pub results_text: crate::hud::ResultsText,
    /// The world's trackside cameras, which the attract race cuts to.
    pub camera_spots: Vec<crate::camera::Spot>,
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
        let mut rand = Rand::default();
        // fakeai_load (0x800797f0), then the cars (cars_load, 0x8003beec).
        let mut ai = Ai::new(&tuning, setup.difficulty, setup.laps, line.lap_length, false, rand.below(0x4000_0000));
        ai.mirrored = setup.flags & 64 != 0;
        let mut cars = Vec::new();
        for (slot, entrant) in setup.cars.iter().enumerate() {
            let Some((h, spec)) = handling.get(slot) else { break };
            let grid = collision.scp.grid[entrant.grid as usize % collision.scp.grid.len()];
            let mut car = Car::load(slot as u8, entrant, &setup, (h, spec), grid, &tuning);
            if !entrant.driver.is_player() {
                // A computer car starts on its route's start point
                // (0x8007c6fc), raised by its ride height along its up axis.
                let start = line.starts.get(entrant.grid as usize).and_then(|&at| crate::ai::Op::decode(&line.stream, at));
                if let Some((crate::ai::Op::Point { pos, .. }, _)) = start {
                    let ride = car.handling.front.ride_height.max(car.handling.rear.ride_height);
                    let h = car.origin[2].wrapping_add(ride).wrapping_sub(0x2_4000);
                    let up = crate::math::column(&car.body.rot, 2).map(|c| crate::math::fx(c, h));
                    car.body.pos = crate::math::sub(crate::math::add(pos, up), car.body.centre);
                }
                ai.add(&mut car, &line, entrant.grid as usize, setup.difficulty, setup.laps, &tuning, 0);
            }
            collision.add_car(&tables, &mut car);
            cars.push(car);
        }
        // race_load (0x80033304): the collision's first step, then the
        // rubber band's players.
        {
            let mut step = Step { tuning: &tuning, rand: &mut rand, time: 0, clock: 0 };
            collision.update(&tables, &mut cars, &mut step);
        }
        ai.start(&cars, false);
        // The cameras fly over the track first (0x8003ac30); the countdown
        // starts when the flyby ends (0x8003abec).
        let mut cameras: Vec<Camera> =
            cars.iter().filter(|c| c.flags & 3 != 0).map(|c| Camera::new(c.slot, &tables.views.one)).collect();
        // camera_load (0x80036830): always one camera; with no players (the
        // attract race) it follows car 0 by the demo's views.
        if cameras.is_empty() {
            cameras.push(Camera::new(0, &tables.views.demo));
        }
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
            rand,
            ai,
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
            motors: Default::default(),
            vibration: [true; 2],
            pause_kit: None,
            paused: None,
            volumes: crate::pause::Volumes::default(),
            effects_level: 0,
            effects_asked: None,
            song: 0,
            cd: Vec::new(),
            start_free: true,
            pause_asked: false,
            end: 1,
            events: Vec::new(),
            hud,
            order,
            power_ups: crate::powerup::PowerUps::default(),
            anims: crate::world_anim::WorldAnims::default(),
            world_volumes: Vec::new(),
            knocked: Vec::new(),
            snapshots: Default::default(),
            results_text: Default::default(),
            camera_spots: Vec::new(),
        }
    }

    /// 0x8006b2a8: the world's collision volumes (flags, the object drawn
    /// for it, centre, rotation, size, weight, and the knock's sound bytes),
    /// each a collision object.
    pub fn set_volumes(&mut self, volumes: &[(u32, Option<usize>, crate::math::Vec3, crate::math::Matrix, crate::math::Vec3, u32, u8)]) {
        self.world_volumes = volumes
            .iter()
            .enumerate()
            .map(|(k, &(flags, object, centre, rot, size, heft, sound))| {
                let collision = self.collision.add_world_object(&self.tables, k as u16, flags, centre, rot, size, heft);
                WorldVolume { object, follows: flags & 8 != 0, sound: (flags & 4 != 0).then_some(sound), collision }
            })
            .collect();
    }

    /// 0x8006b754's moving volumes: each box where its object is now.
    fn volumes_follow(&mut self) {
        for v in &self.world_volumes {
            let (true, Some(object)) = (v.follows, v.object) else { continue };
            let Some(k) = self.anims.anims.iter().position(|a| a.object == object) else { continue };
            if let Some((rot, pos)) = self.anims.pose(&self.tables, k) {
                let o = &mut self.collision.objects[v.collision];
                o.centre = pos;
                o.rot = rot;
            }
        }
    }

    /// 0x80067418: the track's pickups (name, place, size, the world object
    /// drawn for it), the power-ups they use, and the track's two cars to
    /// unlock; each pickup's collision box made.
    pub fn set_power_ups(&mut self, spots: &[(String, crate::math::Vec3, i32, Option<usize>)], defs: Vec<crate::powerup::PowerUp>, unlockable: [u8; 2]) {
        let places: Vec<_> = spots.iter().map(|(n, p, _, o)| (n.clone(), *p, *o)).collect();
        self.power_ups = crate::powerup::PowerUps::new(&places, defs, self.cars.len(), unlockable);
        for (k, (_, pos, size, _)) in spots.iter().enumerate() {
            self.collision.add_pickup(&self.tables, k as u16, *pos, *size);
        }
        for car in &mut self.cars {
            car.power_up = 0;
        }
    }

    /// 0x80041384: puts car `slot` back on the road: player one's at the
    /// first clear point of the best line from where it was along the lap
    /// (when the line has one and it has a heading), others at their saved
    /// respawn point; still, unwrecked, its timers and forces cleared, its
    /// object in the respawn zone, a free turbo if it had none, and two
    /// seconds' grace from other cars.
    pub fn reset_car(&mut self, slot: usize) {
        self.events.push(RaceEvent::Reset { car: slot as u8 });
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
        self.power_ups.drop_all(car);
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
                self.volumes_follow();
                let mut step =
                    Step { tuning: &self.tuning, rand: &mut self.rand, time: self.time, clock: self.clock };
                self.collision.update(&self.tables, &mut self.cars, &mut step);
                if self.collision.players_touched {
                    self.snapshots.take(self.time, &self.cars, &self.cameras, None);
                }
                // 0x8004619c: a player's wreck jolts the pad of its port.
                for car in &mut self.cars {
                    let jolted = std::mem::take(&mut car.jolted);
                    if jolted {
                        self.events.push(RaceEvent::Wrecked { car: car.slot });
                    }
                    if jolted && car.flags & 1 != 0 {
                        self.events.push(RaceEvent::Wreck { car: car.slot });
                        let port = car.slot as usize;
                        if let Some(m) = self.motors.get_mut(port) {
                            m.jolt(self.vibration[port], self.tuning.wreck_jolt, self.clock);
                        }
                    }
                }
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
                // 0x80067f98: the pickups driven through.
                let touched = std::mem::take(&mut self.collision.pickups_touched);
                for (slot, pickup) in touched {
                    let Some(car) = self.cars.get_mut(slot as usize) else { continue };
                    let before = car.turbos;
                    let rand = &mut self.rand;
                    if let Some(player) = self.power_ups.take(car, pickup as usize, self.time, |n| rand.below(n)) {
                        if player {
                            self.events.push(RaceEvent::PowerUp { car: slot });
                        }
                        if std::mem::take(&mut car.turbo_given)
                            && let Some(p) = self.cameras.iter().position(|c| c.car == slot)
                        {
                            // 0x80065398: the meter shows the turbo coming.
                            self.hud.turbos_given(p, before, 1, self.time);
                        }
                    }
                }
                // 0x8006b958: what the cars knocked over.
                for (v, _vel) in std::mem::take(&mut self.collision.knocked) {
                    let Some(vol) = self.world_volumes.get(v as usize).copied() else { continue };
                    if let Some(object) = vol.object {
                        self.knocked.push(object);
                    }
                    if let Some(sound) = vol.sound {
                        let at = self.collision.objects[vol.collision].centre;
                        self.events.push(RaceEvent::Knock { sound, at });
                    }
                }
                tracing::trace!("the knocked objects' flight and the wrecks' debris (0x8002e27c, 0x8007c894): not yet ported");
            }
            let demo = self.demo_views();
            let (views, count) = if demo {
                (&self.tables.views.demo, self.tables.views.demo_count)
            } else {
                (&self.tables.views.one, VIEWS_OFFERED)
            };
            let around = Surroundings {
                tables: &self.tables,
                tuning: &self.tuning,
                views,
                count,
                racing,
                time: self.time,
                flyby: &self.collision.scp.flyby,
                collision: &self.collision,
                spots: &self.camera_spots,
                demo,
            };
            let range = self.tables.camera_range(&self.setup.track, self.setup.track_number);
            for (k, camera) in self.cameras.iter_mut().enumerate() {
                camera.button = controls.get(k).is_some_and(|c| c.view);
                if let Some(car) = self.cars.get_mut(camera.car as usize) {
                    camera.step(&around, car, STEP_MS as u32, &mut self.rand);
                }
                if demo {
                    camera.direct(&self.tables, &self.camera_spots, &self.cars, range, views, count, STEP_MS as u32, &mut self.rand);
                }
            }
            // 0x80068130: power-ups run out, pickups come back.
            self.power_ups.step(&mut self.cars, self.time);
        }
        self.time = self.time.wrapping_add(STEP_MS as u32);
    }

    /// The rest of race_frame (0x80033ed8), once a display frame after its
    /// steps, with the front end's buttons: the race's phase moves on.
    pub fn frame(&mut self, buttons: Buttons) {
        if self.paused.is_some() {
            return;
        }
        // 0x8007f17c: the track's objects move, by the race clock and the
        // time before the start, unless the race stands still.
        if !self.frozen {
            self.anims.step(self.time.wrapping_add(self.before_start));
        }
        self.rumble();
        match self.phase {
            Phase::Starting => self.starting(buttons),
            Phase::Racing => {
                if self.race_over() {
                    if self.demo() {
                        self.over = true;
                    } else {
                        self.finish();
                    }
                }
            }
            Phase::Finished => self.results(buttons),
        }
        // 0x800345dc: Start, pressed after being let go, asks for the pause
        // menu; it comes up before the end of the race.
        if self.start_free && buttons.start {
            self.pause_asked = true;
        }
        if !buttons.start {
            self.start_free = true;
        }
        if self.pause_asked && self.phase != Phase::Finished && !self.over && !self.demo()
            && let Some(kit) = &self.pause_kit
        {
            let mut pause = crate::pause::Pause::new(kit, 0);
            pause.song = self.song;
            pause.set_volumes(self.volumes, self.effects_level);
            self.paused = Some(pause);
            self.cd.push(crate::cd::CdAsk::Pause);
            self.events.push(RaceEvent::Pause);
        }
    }

    /// 0x80034770: a blank of the pause menu; when it is left, the race
    /// goes on, starts again (the front end sets it up anew) or is
    /// abandoned (before the start at once, else through the results).
    pub fn pause_tick(&mut self, pads: [crate::pad::PadState; 2]) {
        let Some(pause) = &mut self.paused else { return };
        let end = pause.tick(pads, self.clock);
        self.cd.append(&mut pause.cd);
        self.song = pause.song;
        self.volumes = pause.volumes;
        if pause.effects_changed {
            self.effects_level = pause.effects_level();
            self.effects_asked = Some(self.effects_level);
        }
        let Some(end) = end else { return };
        self.paused = None;
        // 0x800346fc: Start must be let go again.
        self.pause_asked = false;
        self.start_free = false;
        match end {
            crate::pause::PauseEnd::Continue => {
                self.cd.push(crate::cd::CdAsk::Resume);
                self.events.push(RaceEvent::Resume);
            }
            crate::pause::PauseEnd::Restart => {
                self.end = 2;
                self.over = true;
            }
            crate::pause::PauseEnd::Abort => {
                self.end = 3;
                if self.phase == Phase::Starting {
                    self.over = true;
                } else if self.phase == Phase::Racing {
                    self.finish();
                }
            }
        }
    }

    /// 0x8005fba8: while racing, each player's pad feels the ground under
    /// its car (0x80045ff4): the average roughness of the surfaces its
    /// wheels touch (0x800bea7c), and its speed.
    fn rumble(&mut self) {
        if self.phase != Phase::Racing {
            return;
        }
        for port in 0..2 {
            let Some(car) = self.cars.get(port) else { continue };
            if car.flags & 1 == 0 {
                continue;
            }
            let (roughness, speed) = car.road_feel(&self.tables.surface_rumble);
            self.motors[port].rumble(self.vibration[port], roughness, speed);
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
                self.hud.countdown.call(number);
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
        self.snapshots.take(self.time, &self.cars, &self.cameras, None);
        self.events.push(RaceEvent::Go);
        self.hud.countdown.call(0);
        self.before_start = self.time;
        self.time = 0;
        for car in &mut self.cars {
            car.laps = Laps::new(&self.collision.course, self.time);
        }
        self.phase = Phase::Racing;
        self.called = None;
    }

    /// 0x8003485c: the race is over past its longest time; against the
    /// clock (flag 4, 0x800d0de0 from race_load) once its limit is reached;
    /// otherwise when every player's car has run its laps (each then
    /// coasts, 0x800617c8).
    fn race_over(&mut self) -> bool {
        if self.time > LONGEST_MS {
            return true;
        }
        if self.setup.flags & 4 != 0 {
            return self.time >= self.setup.time_limit;
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
        self.snapshots.take(self.time, &self.cars, &self.cameras, None);
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

    /// The results: after four seconds the race stands still and shows its
    /// snapshots, two seconds each, round and round; they leave after
    /// thirty, or on Cross pressed afresh, or on Start.
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
        let shots = self.snapshots.len();
        if shots != 0 {
            let k = ((since - RESULTS_MS) / SNAPSHOT_MS) as usize % shots;
            self.snapshots.put_back(k, &mut self.cars, &mut self.cameras);
        }
        if since > RESULTS_LEAVE_MS || (self.accept_released && buttons.accept) || buttons.start {
            self.over = true;
        }
    }

    /// The attract race (setup flag 1, 0x800d2608): it ends as its time
    /// runs out, without results, and cannot be paused.
    pub fn demo(&self) -> bool {
        self.setup.flags & 1 != 0
    }

    /// No players' cameras: the demo's views (0x800d2632).
    fn demo_views(&self) -> bool {
        self.hud.players == 0
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
            if self.cars[slot].state == 1 {
                let zone = self.collision.objects.iter().find(|o| o.car == Some(slot as u8)).and_then(|o| o.zones.iter().next());
                if self.ai.car_update(&self.tables, &mut self.cars[slot], zone, dt) {
                    self.reset_car(slot);
                    self.ai.reset(&self.cars[slot]);
                }
                continue;
            }
            if self.cars[slot].state != 2 {
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
            if let Some(rot) = stepped.snapshot {
                self.snapshots.take(self.time, &self.cars, &self.cameras, Some((slot, rot)));
            }
            if let Some(award) = stepped.stunt {
                if let Some(player) = self.cameras.iter().position(|c| c.car as usize == slot) {
                    self.hud.turbos_given(player, before, award.turbos, self.time);
                }
                self.stunts[slot] = Some(award);
            }
        }
        self.ai.step(&self.tables, &self.tuning, &self.line.stream, &mut self.cars, STEP_MS, self.time, &mut self.rand);
        self.ai.handoff(&mut self.cars, &self.tuning);
        self.rank();
        for slot in 0..self.cars.len() {
            if self.cars[slot].flags & 1 != 0 {
                let zone = self.collision.objects.iter().find(|o| o.car == Some(slot as u8)).and_then(|o| o.zones.iter().next());
                crate::laps::watch_way(&self.collision.scp, &mut self.cars[slot], zone, STEP_MS as u32);
            }
        }
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

    /// What player `player`'s HUD draws now (0x80064b8c): the race HUD
    /// while the player races; once its car has run its laps, or the race
    /// is over, the results' table, drawn once over the whole screen when
    /// every player is done (0x8006452c); nothing before the start, or once
    /// the race is left.
    pub fn hud(&mut self, player: usize) -> Vec<crate::hud::Sprite> {
        if self.over {
            return Vec::new();
        }
        if self.hud.players == 0 {
            return match (&self.pause_kit, player) {
                (Some(kit), 0) => crate::hud::demo_mode(kit.style(), &self.results_text.demo, self.time),
                _ => Vec::new(),
            };
        }
        let done = |race: &Race, p: usize| {
            let car = race.cameras.get(p).and_then(|c| race.cars.get(c.car as usize));
            race.phase == Phase::Finished || (race.phase == Phase::Racing && car.is_some_and(|c| c.laps.finished))
        };
        let Some(slot) = self.cameras.get(player).map(|c| c.car as usize) else { return Vec::new() };
        if self.phase == Phase::Starting {
            return Vec::new();
        }
        if !done(self, player) {
            let Some(car) = self.cars.get(slot) else { return Vec::new() };
            let mut out = self.hud.draw(player, car, self.time);
            if let Some(kit) = &self.pause_kit {
                out.extend(self.hud.wrong_way(player, car, self.time, kit.style(), &self.results_text.wrong_way));
            }
            return out;
        }
        if player != 0 || !(0..self.cameras.len()).all(|p| done(self, p)) {
            return Vec::new();
        }
        let Some(kit) = &self.pause_kit else { return Vec::new() };
        let text = &self.results_text;
        let name = |car: &Car| self.setup.names[(car.slot != 0) as usize].clone();
        if self.setup.flags & 2 != 0 {
            // 0x80064294: the players by their stunt points, most first.
            let players = (self.hud.players as usize).min(self.cars.len());
            let swapped = players == 2 && (self.cars[0].stunt_points as u32) < (self.cars[1].stunt_points as u32);
            let mut lines: Vec<Option<crate::hud::ResultLine>> = self.cars[..players]
                .iter()
                .map(|c| Some(crate::hud::ResultLine { name: name(c), player: true, value: c.stunt_points.to_string() }))
                .collect();
            if swapped {
                lines.swap(0, 1);
            }
            tracing::trace!("the points' table lowering 0x800d0e54 to player one's lap time: not yet ported");
            return crate::hud::results_table(kit.style(), &text.points, &lines, false);
        }
        // 0x80064040: every car by its place, its race time (best lap
        // against the clock).
        let best = self.setup.flags & 4 != 0;
        let lines: Vec<Option<crate::hud::ResultLine>> = (0..self.setup.cars.len())
            .map(|k| {
                let s = self.standings.get(k)?;
                let car = self.cars.get(s.car as usize)?;
                let player = car.flags & 1 != 0;
                let name = if player {
                    name(car)
                } else {
                    let id = self.setup.cars.get(s.car as usize).map_or(0, |e| e.car_id);
                    text.cars.get(id as usize).cloned().unwrap_or_default()
                };
                let value = if best {
                    crate::hud::result_time(s.best, None)
                } else {
                    crate::hud::result_time(s.time, Some(&text.no_time))
                };
                Some(crate::hud::ResultLine { name, player, value })
            })
            .collect();
        crate::hud::results_table(kit.style(), if best { &text.best } else { &text.time }, &lines, true)
    }
}
