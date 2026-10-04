//! Laps: the checkpoints a car must pass in order, its lap times, and when
//! its race is over (0x8006137c, with the rules `gameflow_load`, 0x80061728,
//! takes from the race and the checkpoints `collision_load`, 0x8004c334,
//! finds on the track).

use crate::collision::Scp;
use crate::race::RaceSetup;

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
}
