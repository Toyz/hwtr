//! The track's moving objects (0x8007eff4 loads them, 0x8007f17c moves
//! them once a frame): each object's keys, evenly spaced over a round of
//! `period` ms, the pose between two keys blended (position straight,
//! orientation by slerp, 0x8006b1a8 and 0x8006ad78). The pickups turn this
//! way, and windmills, UFOs and the like move.

use crate::math::{Matrix, Tables, Vec3, div_fx, fx, quat_to_matrix, transpose};

/// One object's animation: its keys (position, 20.12, and quaternion, 4.12)
/// and its round, and where it is in it.
#[derive(Clone, Debug)]
pub struct ObjectAnim {
    pub object: usize,
    pub period: u32,
    pub keys: Vec<(Vec3, [i32; 4])>,
    /// Milliseconds into the round (0x800d0ffc +0x10).
    pub time: u32,
    /// Run only when a trigger starts it (+0x14, 0x8007f628), how much of
    /// its round it has left to run (+0x18) and the trigger that started it
    /// (+0x1c).
    pub triggered: bool,
    pub left: u32,
    pub trigger: u16,
}

/// What a trigger's firing sounds: an animation's world sound where it
/// stands, or effect 27 for a player's car.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fired {
    Sound { anim: usize, sound: u8 },
    Effect27,
}

/// What the animations' steps did to the triggers' looped sounds: those
/// whose animation still runs, and those whose animation came to rest.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Running {
    pub moving: Vec<u16>,
    pub stopped: Vec<u16>,
}

/// The objects' animations and the clock they last ran to (0x800d1000:
/// the race clock plus the time before the start).
#[derive(Clone, Debug, Default)]
pub struct WorldAnims {
    pub anims: Vec<ObjectAnim>,
    last: u32,
}

impl WorldAnims {
    pub fn new(anims: Vec<ObjectAnim>) -> WorldAnims {
        WorldAnims { anims, last: 0 }
    }

    /// 0x8007f17c at clock `now`: every animation on by the time since the
    /// last, but a triggered one only by what it has left to run, until it
    /// comes to rest.
    pub fn step(&mut self, now: u32) -> Running {
        let mut out = Running::default();
        let dt = now.wrapping_sub(self.last);
        self.last = now;
        if dt == 0 {
            return out;
        }
        for a in &mut self.anims {
            if !a.triggered {
                a.time = a.time.wrapping_add(dt);
            } else if a.left != 0 {
                let run = a.left.min(dt);
                a.time = a.time.wrapping_add(run);
                a.left -= run;
                if a.left == 0 { out.stopped.push(a.trigger) } else { out.moving.push(a.trigger) }
            } else {
                continue;
            }
            // The pose that follows (0x8007f2a4) keeps the time within a
            // round.
            if a.period != 0 {
                a.time %= a.period;
            }
        }
        out
    }

    /// 0x8007f628: animation `k` runs only when triggered.
    pub fn set_triggered(&mut self, k: usize) -> bool {
        match self.anims.get_mut(k) {
            Some(a) => {
                a.triggered = true;
                true
            }
            None => false,
        }
    }

    /// 0x8006a200: trigger number `trigger` (`t`) fired by a car (a
    /// player's if `player`). Unless it is off, it starts its animations
    /// (the second only with the first) toward its key; each one started
    /// sounds its world sound (looped ones once), or else, for the first, a
    /// player's car hears effect 27.
    pub fn fire(&mut self, t: &crate::collision::scp::Trigger, trigger: u16, player: bool) -> Vec<Fired> {
        let mut out = Vec::new();
        if t.flags & 1 != 0 || t.flags & 2 == 0 {
            return out;
        }
        for k in 0..2 {
            if k == 1 && t.flags & 4 == 0 {
                break;
            }
            let anim = t.anims[k] as usize;
            if !self.start(anim, t.frame, trigger) {
                continue;
            }
            if t.flags & 0x38 != 0 {
                out.push(Fired::Sound { anim, sound: t.sounds[k] });
            } else if player && k == 0 {
                out.push(Fired::Effect27);
            }
        }
        out
    }

    /// 0x8007f670: trigger `trigger` starts animation `k` toward key
    /// `frame` (at most its last), unless it is still running: it is given
    /// that share of its round to run. Whether it started.
    pub fn start(&mut self, k: usize, frame: u16, trigger: u16) -> bool {
        let Some(a) = self.anims.get_mut(k) else { return false };
        if a.left != 0 {
            return false;
        }
        let last = (a.keys.len() as u32).wrapping_sub(1);
        let frame = (frame as u32).min(last);
        a.left = frame.wrapping_mul(a.period).checked_div(last).unwrap_or(0);
        a.trigger = trigger;
        true
    }

    /// 0x8007f2a4: animation `k`'s object's pose now: its rotation (the
    /// object record's, the quaternion's matrix transposed, 0x80021508) and
    /// position (20.12).
    pub fn pose(&mut self, t: &Tables, k: usize) -> Option<(Matrix, Vec3)> {
        let a = self.anims.get_mut(k)?;
        if a.period == 0 || a.keys.is_empty() {
            return None;
        }
        a.time %= a.period;
        let last = a.keys.len() as u32 - 1;
        let seg = ((a.time as u64 * last as u64) / a.period as u64) as u32;
        let (from, to) = (a.keys[seg as usize], a.keys[(seg as usize + 1).min(a.keys.len() - 1)]);
        let part = (last.wrapping_mul(a.time).wrapping_sub(seg.wrapping_mul(a.period)) as i32) << 12;
        let f = div_fx(part, (a.period as i32) << 12);
        let pos = lerp(f, from.0, to.0);
        let q = slerp(t, f, from.1, to.1);
        Some((transpose(&quat_to_matrix(q)), pos))
    }
}

/// 0x8006b1a8: `a` to `b` by `f` (4.12).
fn lerp(f: i32, a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| fx(a[i], 4096 - f).wrapping_add(fx(b[i], f)))
}

/// 0x8006ad78: quaternion `a` to `b` by `f` on the shorter arc; nearly the
/// same, straight.
fn slerp(t: &Tables, f: i32, mut a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    let mut dot = (0..4).fold(0i32, |s, i| s.wrapping_add(fx(a[i], b[i])));
    if dot < 0 {
        a = a.map(i32::wrapping_neg);
        dot = -dot;
    }
    let (wa, wb) = if 4096 / 1000 < 4096 - dot {
        let theta = t.acos(dot);
        let s = t.sin(theta);
        (div_fx(t.sin(fx(4096 - f, theta)), s), div_fx(t.sin(fx(f, theta)), s))
    } else {
        (4096 - f, f)
    };
    std::array::from_fn(|i| fx(wa, a[i]).wrapping_add(fx(wb, b[i])))
}
