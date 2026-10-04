//! A race's result as the front end takes it (0x8009b0a4 and what it
//! calls): the records it sets (best laps, stunt scores), the cars and
//! tracks it unlocks, and which of those are new, to announce.

use super::*;

/// What a race leaves the front end (its 292-byte record at 0x80138d14):
/// how it ended, each car's result by slot, and what it unlocked (the
/// first player's cars 0-31 and 32 on and tracks, then the second's:
/// record +4, +8, +0x14, +0xc, +0x10, +0x18).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaceResult {
    pub end: RaceEnd,
    pub cars: Vec<CarResult>,
    pub unlocks: [u32; 6],
}

impl RaceResult {
    /// A race that ended without results (it would not load, or was left).
    pub fn ended(end: RaceEnd) -> RaceResult {
        RaceResult { end, cars: Vec::new(), unlocks: [0; 6] }
    }
}

/// A car's result (44 bytes from +0x18 a car): its race time, best lap,
/// laps run (+0x40), the points its place earns (+0x42), its stunt score
/// (+0x44).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CarResult {
    pub time: u32,
    pub best: u32,
    pub laps: u8,
    pub points: u8,
    pub score: u32,
}

/// What was newly unlocked, to announce (0x800d27d0 to 0x800d27e8): each
/// player's cars (two words) and tracks, and a cup.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Unlocked {
    pub cars: [[u32; 2]; 2],
    pub tracks: [u32; 2],
    pub cup: u8,
}

impl Unlocked {
    pub fn any(&self) -> bool {
        self.cars.iter().flatten().any(|&b| b != 0) || self.tracks.iter().any(|&b| b != 0) || self.cup != 0
    }
}

/// 0x80099d18: the lowest bit set in `bits`, taken out; None if none.
pub(super) fn take_lowest(bits: &mut u32) -> Option<u8> {
    if *bits == 0 {
        return None;
    }
    let k = bits.trailing_zeros();
    *bits &= !(1 << k);
    Some(k as u8)
}

impl Front {
    /// The track the race just run was, 0 to 11 (world × 3 + number - 1).
    fn raced_track(&self) -> Option<usize> {
        let s = self.last_race.as_ref()?;
        let world = self.tables.worlds.iter().position(|w| *w == s.track)?;
        Some(world * 3 + s.track_number as usize - 1)
    }

    /// 0x8009b0a4: the records a finished race sets (0x8009af74 the stunt
    /// scores, when the race counts them; 0x8009aff8 the best laps of
    /// players who ran every lap), then its unlocks into the players, the
    /// new ones noted.
    pub(super) fn take_result(&mut self, r: &RaceResult) {
        let setup = self.last_race.clone();
        if r.end == RaceEnd::Finished
            && let (Some(setup), Some(track)) = (setup, self.raced_track())
        {
            for k in 0..self.people.min(2) as usize {
                let Some(car) = r.cars.get(k).copied() else { continue };
                let name = self.players[k].name.clone();
                if setup.flags & 2 != 0 {
                    insert(
                        &mut self.card.high_scores,
                        track,
                        &name,
                        car.score,
                        |new, old| new > old,
                        self.strings.get(84),
                    );
                }
                if car.laps == setup.laps {
                    insert(
                        &mut self.card.best_times,
                        track,
                        &name,
                        car.best,
                        |new, old| new < old,
                        self.strings.get(84),
                    );
                }
            }
        }
        let u = r.unlocks;
        for k in 0..2 {
            let p = &mut self.players[k];
            let (c0, c1, t) = (u[3 * k], u[3 * k + 1], u[3 * k + 2]);
            self.unlocked.cars[k] = [c0 & !p.cars[0], c1 & !p.cars[1]];
            self.unlocked.tracks[k] = t & !p.tracks;
            p.cars[0] |= c0;
            p.cars[1] |= c1;
            p.tracks |= t;
        }
    }
}

/// 0x8009aadc, 0x8009ac64, 0x8009adec: `name` and `value` into the five
/// lines of a table from `track` × 5: in the first empty line, or where it
/// beats the line there (the rest moving down, the last dropping off).
pub(super) fn insert(
    table: &mut [card::Score],
    track: usize,
    name: &str,
    value: u32,
    beats: impl Fn(u32, u32) -> bool,
    empty: &str,
) {
    let Some(lines) = table.get_mut(track * 5..track * 5 + 5) else { return };
    for i in 0..5 {
        if lines[i].name == empty {
            lines[i] = card::Score { name: name.chars().take(11).collect(), value };
            return;
        }
        if beats(value, lines[i].value) {
            lines[i..].rotate_right(1);
            lines[i] = card::Score { name: name.chars().take(11).collect(), value };
            return;
        }
    }
}
