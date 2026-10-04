//! The track's own sounds (12 records at 0x8011aab0): the sources its
//! world names, keyed looping as the race's sound starts (0x800350d4); the
//! two spots that knocks and triggers sound at (0x80128ef4, 0x80036270);
//! and their part of the mixer (0x80017928).

use crate::engines::{Bank, Change, Engines, doppler, level};
use crate::math::{Tables, Vec3, fx};

/// A world sound's record (36 bytes): the voice it plays on (+0), its tone
/// (+2), bank (+4: 2 the track's, 0 the effects), note (+6), program (+8),
/// whether it loops (+12), where it is (+16, 20.12) and its level (+32,
/// 4.12).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldSound {
    pub voice: Option<usize>,
    pub tone: u8,
    pub bank: u8,
    pub note: u16,
    pub program: u8,
    pub looped: bool,
    pub pos: Vec3,
    pub level: i32,
}

/// One of the two spots (8 bytes): its world sound record, whether it
/// loops, and when it was last keyed (the system clock, ms).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Spot {
    pub record: u8,
    pub looped: bool,
    pub time: u32,
}

/// A track sound as the race keeps it (0x800d0df4, 16 bytes): its world
/// sound record, its flags (1: it follows an animation), the animation and
/// its sound.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Source {
    pub record: u8,
    pub flags: u32,
    pub anim: usize,
    pub sound: u32,
}

/// One of the track's sounds as its world lists it: where, its flags, the
/// sound and its volume (0-255).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceDef {
    pub pos: Vec3,
    pub flags: u32,
    pub sound: u32,
    pub volume: u32,
}

/// The number of world sound records.
pub const RECORDS: usize = 12;

impl Engines {
    /// 0x80016c98: the first world sound record with no voice (255 if
    /// none; nothing is marked, so two calls give the same one).
    pub fn world_alloc(&self) -> u8 {
        self.world.iter().position(|w| w.voice.is_none()).map_or(255, |k| k as u8)
    }

    /// 0x80016d0c: record `k` placed at `pos`.
    pub fn world_place(&mut self, k: u8, pos: Vec3) {
        if let Some(w) = self.world.get_mut(k as usize) {
            w.pos = pos;
        }
    }

    /// 0x80016e2c: record `k`'s level (4.12).
    pub fn world_level(&mut self, k: u8, level: i32) {
        if let Some(w) = self.world.get_mut(k as usize) {
            w.level = level;
        }
    }

    /// 0x80019abc with its first argument 1: a voice for a looping world
    /// sound, from the engines' count to the first effect voice: the first
    /// not held (0x8011aca0), else the first one playing something less
    /// important, let go.
    fn loop_voice(&self, importance: u8) -> Option<(usize, bool)> {
        let mut weaker = None;
        for v in self.cars.len()..self.first {
            if !self.held[v] {
                return Some((v, false));
            }
            if self.importance[v] < importance && weaker.is_none() {
                weaker = Some(v);
            }
        }
        weaker.map(|v| (v, true))
    }

    /// 0x80016e54: sound `id` keyed on record `k` at `importance`. Below 10
    /// it is the track's bank's program 0, tone `id`; 10 to 16 are effects
    /// bank tones; any other keys nothing. A looping one (not already
    /// playing) is keyed silent on a voice of its own for the mixer to
    /// raise; a one-off is heard where the record is, at its level.
    pub fn world_key(
        &mut self,
        t: &Tables,
        k: u8,
        id: u32,
        importance: u8,
        looped: bool,
        alive: &dyn Fn(usize) -> bool,
    ) -> Vec<Change> {
        let mut out = Vec::new();
        let s = k as usize;
        if s >= RECORDS {
            return out;
        }
        if looped && self.world[s].voice.is_some_and(alive) {
            return out;
        }
        let picked = if looped { self.loop_voice(importance) } else { self.voice(importance, alive) };
        if let Some((v, true)) = picked {
            out.push(Change::KeyOff { voice: v });
        }
        let w = &mut self.world[s];
        w.note = id.wrapping_add(60) as u16;
        let tone = if id < 10 {
            w.bank = 2;
            w.program = 0;
            id as u8
        } else {
            w.bank = 0;
            let (tone, program, note) = match id - 10 {
                0 => (0, 7, 60),
                1 => (8, 3, 68),
                2 => (0, 4, 60),
                3 | 4 => (1, 4, 61),
                5 => (2, 4, 50),
                6 => (2, 4, 62),
                _ => return out,
            };
            w.program = program;
            w.note = note;
            tone
        };
        let Some((voice, _)) = picked else { return out };
        if voice == 0 {
            return out;
        }
        let w = self.world[s];
        let bank = if w.bank == 2 { Bank::Track } else { Bank::Effects };
        if looped {
            self.held[voice] = true;
            self.importance[voice] = importance;
            out.push(Change::KeyOn {
                voice,
                bank,
                program: w.program,
                tone,
                note: w.note as u8,
                fine: 64,
                left: 0,
                right: 0,
            });
            let w = &mut self.world[s];
            w.voice = Some(voice);
            w.looped = true;
            w.tone = tone;
        } else {
            let [l, r] = level(t, &self.listener, w.pos, self.volume);
            let side = |v: u8| (fx(w.level, (v as i32) << 12) >> 12) as i16 as u8;
            out.push(Change::KeyOn {
                voice,
                bank,
                program: w.program,
                tone,
                note: w.note as u8,
                fine: 64,
                left: side(l),
                right: side(r),
            });
        }
        out
    }

    /// 0x80016d5c: record `k` stopped: a looping one let go (0x8001a8dc),
    /// another keyed off, if playing; its level 0 and its voice forgotten.
    pub fn world_stop(&mut self, k: u8, alive: &dyn Fn(usize) -> bool) -> Vec<Change> {
        let mut out = Vec::new();
        let s = k as usize;
        if s >= RECORDS {
            return out;
        }
        let w = self.world[s];
        if let Some(v) = w.voice.filter(|&v| alive(v)) {
            if w.looped {
                out.push(self.let_go(v));
            } else {
                out.push(Change::KeyOff { voice: v });
            }
        }
        self.world[s].level = 0;
        self.world[s].voice = None;
        out
    }

    /// 0x80017208: record `k`'s voice silenced, if it plays (it keeps
    /// playing; the mixer sets its volume again).
    pub fn world_mute(&self, k: u8, alive: &dyn Fn(usize) -> bool) -> Option<Change> {
        let v = self.world.get(k as usize)?.voice.filter(|&v| alive(v))?;
        Some(Change::Volume { voice: v, left: 0, right: 0 })
    }

    /// The mixer's pass over the world sounds (0x80017928 after the
    /// cars): each one playing at its level where it is, a looping one at
    /// three fifths, and bent by the listener's speed.
    pub fn world_mixer(&self, t: &Tables, alive: &dyn Fn(usize) -> bool) -> Vec<Change> {
        let mut out = Vec::new();
        for w in &self.world {
            let Some(voice) = w.voice.filter(|&v| alive(v)) else { continue };
            let [l, r] = level(t, &self.listener, w.pos, self.volume);
            let fine = doppler(t, self.listener.vel, [0; 3], w.note);
            let (l, r) = if w.looped { (l as i32 * 3 / 5, r as i32 * 3 / 5) } else { (l as i32, r as i32) };
            let side = |v: i32| (fx(w.level, v << 12) >> 12) as i16 as u8;
            out.push(Change::Volume { voice, left: side(l), right: side(r) });
            let bank = if w.bank == 2 { Bank::Track } else { Bank::Effects };
            out.push(Change::Bend { voice, bank, program: w.program, bend: (w.note as i32 + fine) as i16 as i32 });
        }
        out
    }

    /// 0x800350d4's track sounds: each source given a record, placed, at
    /// its volume, and keyed looping (importance 0); one that follows an
    /// animation takes the one whose second trigger word names it (`links`,
    /// 0x8007f5e0: the first, else 0). Then the two spots take the next
    /// free record, the same one (0x80036174).
    pub fn start_world(
        &mut self,
        t: &Tables,
        defs: &[SourceDef],
        links: &[usize],
        alive: &dyn Fn(usize) -> bool,
    ) -> Vec<Change> {
        let mut out = Vec::new();
        self.sources.clear();
        for (k, d) in defs.iter().enumerate() {
            let record = self.world_alloc();
            let anim = if d.flags & 1 != 0 { links.get(k).copied().unwrap_or(0) } else { 0 };
            self.sources.push(Source { record, flags: d.flags, anim, sound: d.sound });
            self.world_level(record, ((d.volume << 12) / 255) as i32);
            self.world_place(record, d.pos);
            out.extend(self.world_key(t, record, d.sound, 0, true, alive));
        }
        self.spots = [Spot::default(); 2];
        for s in &mut self.spots {
            s.record = self.world.iter().position(|w| w.voice.is_none()).map_or(255, |k| k as u8);
        }
        out
    }

    /// 0x80036270: sound `id` at `pos` and `volume` (0-255) on the spot
    /// keyed longest ago that is not looping, the system clock `now`
    /// marking it; looping if `looped`. None if the race's sound is shut
    /// or no spot is free.
    #[allow(clippy::too_many_arguments)]
    pub fn spot_sound(
        &mut self,
        t: &Tables,
        now: u32,
        pos: Vec3,
        id: u32,
        volume: u32,
        looped: bool,
        hushed: bool,
        alive: &dyn Fn(usize) -> bool,
    ) -> (Option<u8>, Vec<Change>) {
        let mut out = Vec::new();
        if hushed {
            return (None, out);
        }
        let mut oldest = now;
        let mut pick = None;
        for (k, s) in self.spots.iter().enumerate() {
            if !s.looped && s.time < oldest {
                oldest = s.time;
                pick = Some(k);
            }
        }
        let Some(k) = pick.filter(|_| oldest != now) else { return (None, out) };
        let record = self.spots[k].record;
        out.extend(self.world_mute(record, alive));
        self.world_place(record, pos);
        self.world_level(record, ((volume << 12) / 255) as i32);
        out.extend(self.world_key(t, record, id, 1, looped, alive));
        self.spots[k].time = now;
        if looped {
            self.spots[k].looped = true;
        }
        (Some(k as u8), out)
    }

    /// 0x80036474: spot `k`'s record moved to `pos`.
    pub fn spot_place(&mut self, k: u8, pos: Vec3) {
        if let Some(s) = self.spots.get(k as usize).copied() {
            self.world_place(s.record, pos);
        }
    }

    /// 0x80036424: spot `k` silenced and free again.
    pub fn spot_stop(&mut self, k: u8, alive: &dyn Fn(usize) -> bool) -> Option<Change> {
        let s = self.spots.get(k as usize).copied()?;
        let c = self.world_mute(s.record, alive);
        let spot = &mut self.spots[k as usize];
        spot.looped = false;
        spot.time = 0;
        c
    }

    /// 0x8006a200's looping sound: trigger `trigger`'s `k`th animation's
    /// spot kept for it to follow.
    pub fn set_trigger_spot(&mut self, trigger: u16, k: u8, spot: u8) {
        let t = trigger as usize;
        if self.trigger_spots.len() <= t {
            self.trigger_spots.resize(t + 1, [0; 2]);
        }
        if let Some(s) = self.trigger_spots[t].get_mut(k as usize) {
            *s = spot;
        }
    }

    /// 0x8006a4cc: trigger `trigger`'s spots (not 255) moved where its
    /// animations are (`at`; none for an animation it does not run).
    pub fn follow_trigger(&mut self, trigger: u16, at: [Option<Vec3>; 2]) {
        let spots = self.trigger_spots.get(trigger as usize).copied().unwrap_or([0; 2]);
        for (spot, pos) in spots.into_iter().zip(at) {
            if spot != 255
                && let Some(pos) = pos
            {
                self.spot_place(spot, pos);
            }
        }
    }

    /// 0x8006a424: trigger `trigger`'s spots (not 255) silenced and freed
    /// (the trigger keeps their numbers).
    pub fn stop_trigger(&mut self, trigger: u16, both: bool, alive: &dyn Fn(usize) -> bool) -> Vec<Change> {
        let spots = self.trigger_spots.get(trigger as usize).copied().unwrap_or([0; 2]);
        let mut out = Vec::new();
        for (k, spot) in spots.into_iter().enumerate() {
            if spot != 255 && (k == 0 || both) {
                out.extend(self.spot_stop(spot, alive));
            }
        }
        out
    }

    /// 0x800354ac after the mixer: each source that follows an animation
    /// placed where it is (`at`, by animation).
    pub fn follow_sources(&mut self, at: &mut dyn FnMut(usize) -> Option<Vec3>) {
        for k in 0..self.sources.len() {
            let s = self.sources[k];
            if s.flags & 1 != 0
                && let Some(pos) = at(s.anim)
            {
                self.world_place(s.record, pos);
            }
        }
    }

    /// 0x80036758's world part: each source keyed again (nothing for one
    /// still playing).
    pub fn rekey_sources(&mut self, t: &Tables, alive: &dyn Fn(usize) -> bool) -> Vec<Change> {
        let mut out = Vec::new();
        for k in 0..self.sources.len() {
            let s = self.sources[k];
            out.extend(self.world_key(t, s.record, s.sound, 0, true, alive));
        }
        out
    }

    /// 0x80036634 and 0x800364cc's world part: every source silenced
    /// (0x80017208); the spots are left as they are.
    pub fn mute_sources(&self, alive: &dyn Fn(usize) -> bool) -> Vec<Change> {
        self.sources.iter().filter_map(|s| self.world_mute(s.record, alive)).collect()
    }
}
