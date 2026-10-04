//! Laps: the checkpoints a car must pass in order, its lap times, and when
//! its race is over (0x8006137c, with the rules `gameflow_load`, 0x80061728,
//! takes from the race and the checkpoints `collision_load`, 0x8004c334,
//! finds on the track).

use crate::collision::Scp;
use crate::math::{div_fx, fx};
use crate::race::RaceSetup;
use crate::rand::Rand;

/// The race's lap rules, and where the track's checkpoints are.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Course {
    /// Laps to run, and checkpoints a lap (the last is the finish line).
    pub laps: u8,
    pub checkpoints: u8,
    /// Race flag 2: no checkpoint sounds, and no laps counted.
    pub quiet: bool,
    /// Race flag 4: the laps never run out.
    pub endless: bool,
    /// Race flag 8: each lap timed from the last crossing of the line, the
    /// lap count staying at one.
    pub flying: bool,
    /// The lap's length in the units of a car's lap distance (the best
    /// line's header).
    pub lap_length: i32,
    /// Where each checkpoint begins: the least lap distance of its zones,
    /// the lap length if it has none.
    pub starts: Vec<i32>,
}

impl Course {
    /// The rules of `setup` on the track `scp`, a lap `lap_length` long.
    pub fn new(setup: &RaceSetup, scp: &Scp, lap_length: i32) -> Course {
        let mut starts = vec![lap_length; setup.checkpoints as usize];
        for zone in scp.zones.iter().filter(|z| z.flags & 1 != 0) {
            let distance = zone.distance as i32 * 10;
            if let Some(start) = (zone.param as usize).checked_sub(1).and_then(|k| starts.get_mut(k))
                && (distance as u32) < *start as u32
            {
                *start = distance;
            }
        }
        Course {
            laps: setup.laps,
            checkpoints: setup.checkpoints,
            quiet: setup.flags & 2 != 0,
            endless: setup.flags & 4 != 0,
            flying: setup.flags & 8 != 0,
            lap_length,
            starts,
        }
    }

    /// Where the checkpoint after the `passed` passed this lap begins.
    pub fn next_start(&self, passed: u8) -> i32 {
        self.starts.get(passed as usize).copied().unwrap_or(0)
    }
}

/// What passing a checkpoint did, for the sounds and the HUD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LapEvent {
    /// A checkpoint out of order (sound 11).
    WrongWay,
    /// The next checkpoint (sound 10).
    Checkpoint,
    /// The finish line after them all (sound 12): the lap's time, and
    /// whether it is the car's best.
    Lap { time: u32, best: bool },
}

/// A car's progress through its laps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Laps {
    /// When its race started, by the race clock.
    pub start: u32,
    /// Laps completed.
    pub done: u8,
    /// The checkpoints passed this lap, and how many.
    pub passed: [bool; 10],
    pub passed_count: u8,
    /// The race clock at the end of each lap.
    pub ends: Vec<u32>,
    /// Its best lap time, 0 before it has one.
    pub best: u32,
    /// Its laps are all run: it drives on by itself.
    pub finished: bool,
    /// Its place in the race, from 0 (0x800408cc).
    pub place: u8,
}

impl Laps {
    /// 0x80061264 for one car, at `time` when the race starts: nothing
    /// passed, no laps; a flying lap starts timing now.
    pub fn new(course: &Course, time: u32) -> Laps {
        Laps { start: time, ends: if course.flying { vec![time] } else { Vec::new() }, ..Laps::default() }
    }

    /// 0x8006137c: the car drives into checkpoint `number` (from 1, the last
    /// the finish line) at `time`. A checkpoint counts when every one before
    /// it this lap has; the finish line then ends the lap, timing it, and
    /// after the race's laps the car's race. None when nothing happened.
    pub fn pass(&mut self, course: &Course, number: u8, time: u32) -> Option<LapEvent> {
        if self.finished {
            return None;
        }
        let at = number.wrapping_sub(1);
        let in_order = self.passed.iter().take(at as usize).all(|&p| p);
        if !in_order {
            // Driving back into the checkpoint just passed, or the finish
            // line from the grid, is not the wrong way.
            let behind = (self.passed_count == 0 && at as u32 == (course.checkpoints as u32).wrapping_sub(1))
                || at == self.passed_count.wrapping_sub(1);
            return (!course.quiet && !behind).then_some(LapEvent::WrongWay);
        }
        if self.passed.get(at as usize).copied().unwrap_or(false) {
            return None;
        }
        let last = (course.checkpoints as u32).wrapping_sub(1);
        if (at as u32) < last {
            if let Some(p) = self.passed.get_mut(at as usize) {
                *p = true;
            }
            self.passed_count = self.passed_count.wrapping_add(1);
            return Some(LapEvent::Checkpoint);
        }
        if at as u32 != last {
            return None;
        }
        if course.quiet {
            // No laps are counted: the line only starts the checkpoints again.
            self.passed_count = 0;
            self.passed = [false; 10];
            return None;
        }
        // The lap before the first ends at the race's start (the clock
        // starts from 0 then).
        let from = if course.flying { self.ends.first() } else { self.ends.last() };
        let lap = time.wrapping_sub(from.copied().unwrap_or(0));
        let best = self.best == 0 || lap < self.best;
        if best {
            self.best = lap;
        }
        if course.flying {
            self.ends = vec![time];
            self.done = 1;
        } else {
            self.ends.push(time);
            self.done = self.done.wrapping_add(1);
            if !course.endless && self.done == course.laps {
                self.finished = true;
            }
        }
        self.passed_count = 0;
        self.passed = [false; 10];
        Some(LapEvent::Lap { time: lap, best })
    }

    /// 0x80061824 for a computer car short of the line when the race ends,
    /// at race clock `now`: the laps it has still to run are timed for it,
    /// and it has finished. This lap ends after the race it has left to
    /// run (`progress`, the driver's), less the laps after this one, at
    /// its `pace`; each later lap takes a lap's length at its pace, plus up
    /// to a twentieth of that length at random.
    pub fn estimate(&mut self, progress: i32, laps: u8, lap_length: i32, pace: i32, now: u32, rand: &mut Rand) {
        let done = self.done as usize;
        let left = fx(progress.wrapping_abs(), 1000 << 12) >> 12;
        let after = (laps as i32).wrapping_sub(done as i32 + 1).wrapping_mul(lap_length);
        let rest = if (left as u32) < after as u32 { 0 } else { left.wrapping_sub(after) };
        let end = now.wrapping_add((div_fx(rest << 12, pace) >> 12) as u32);
        self.set_end(done, end);
        let first = if done == 0 { end } else { end.wrapping_sub(self.ends[done - 1]) };
        self.note_best(first);
        for k in done + 1..laps as usize {
            let lap = ((div_fx(lap_length << 12, pace) >> 12) as u32).wrapping_add(rand.below(lap_length as u32 / 20));
            self.set_end(k, lap.wrapping_add(self.ends[k - 1]));
            self.note_best(lap);
        }
        self.finished = true;
        self.done = laps;
    }

    fn set_end(&mut self, k: usize, time: u32) {
        if self.ends.len() <= k {
            self.ends.resize(k + 1, 0);
        }
        self.ends[k] = time;
    }

    fn note_best(&mut self, lap: u32) {
        if self.best == 0 || lap < self.best {
            self.best = lap;
        }
    }
}

/// 0x8005c9b4, after the places each step, for a player's car `car` in
/// `zone` (the first of its object's zones), a step of `dt_ms`: with every
/// wheel down, moving forward at over 10 mph in a zone without flag 0x400,
/// the portal its velocity points most squarely out of leads to the zone
/// beyond; heading into a zone with a greater distance starts the timer
/// (`dt_ms`), which runs on while the zone beyond is not nearer, and stops
/// otherwise or on any of the other conditions failing. Over half a second
/// and the car is going the wrong way.
pub fn watch_way(scp: &crate::collision::scp::Scp, car: &mut crate::car::Car, zone: Option<u16>, dt_ms: u32) {
    use crate::math::{column, div_fx, dot, fx};
    if car.grounded as usize == car.wheels.len() {
        let forward = dot(column(&car.body.rot, 1), car.body.vel);
        let mph = div_fx(0xb_0000, 0xa000);
        let beyond = zone.and_then(|z| scp.zones.get(z as usize)).filter(|z| z.flags & 0x400 == 0).and_then(|z| {
            if fx(0xa000, mph) >= forward {
                return None;
            }
            let mut best: Option<(u16, i32)> = None;
            for k in 0..z.plane_count as usize {
                let Some(p) = scp.planes.get(z.first_plane as usize + k) else { continue };
                if p.kind != 0 {
                    continue;
                }
                let d = dot(car.body.vel, p.normal());
                if best.is_none_or(|(_, b)| d < b) {
                    best = Some((p.target, d));
                }
            }
            let (target, _) = best?;
            Some((z.distance, scp.zones.get(target as usize)?.distance))
        });
        car.wrong_way_ms = match beyond {
            Some((here, there)) if car.wrong_way_ms != 0 => {
                if there < here {
                    0
                } else {
                    car.wrong_way_ms.wrapping_add(dt_ms)
                }
            }
            Some((here, there)) => {
                if here < there {
                    dt_ms
                } else {
                    0
                }
            }
            None => 0,
        };
    }
    car.wrong_way = car.wrong_way_ms > 500;
}
